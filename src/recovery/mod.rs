use anyhow::{Context, Result, anyhow};
use colored::*;
use serde::Deserialize;
use std::fmt;
use std::io::{self, Write};

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, LlmClient};
use crate::config::{Config, RecoveryPreference};
use crate::planner::Plan;
use crate::validator::MissingCommand;

pub struct RecoveryEngine {
    client: AIClient,
    config: Config,
    available_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    pub command_alternatives: Vec<CommandAlternative>,
    pub installation_instructions: Vec<InstallationInstruction>,
    pub can_skip_step: bool,
    pub retry_possible: bool,
    pub recovery_preference: RecoveryPreference,
}

#[derive(Debug, Clone)]
pub struct CommandAlternative {
    pub command: String,
    pub description: String,
    pub confidence: f32,
}

impl fmt::Display for CommandAlternative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.command, self.description)
    }
}

#[derive(Debug, Clone)]
pub struct InstallationInstruction {
    pub command: String,
    pub install_commands: Vec<String>,
    pub package_managers: Vec<String>,
    pub confidence: f32,
}

impl fmt::Display for InstallationInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Install using: {}", self.install_commands.join(", "))
    }
}

#[derive(Debug, Clone)]
pub enum RecoveryChoice {
    UseAlternative(usize),
    InstallCommand(usize),
    SkipStep,
    AbortPlan,
    RetryOriginal,
}

#[derive(Debug, Deserialize)]
pub struct RecoveryResponse {
    alternatives: Vec<AlternativeResponse>,
    installation_instructions: Vec<InstallResponse>,
    can_skip: bool,
    original_goal_achievable: bool,
}

#[derive(Debug, Deserialize)]
struct AlternativeResponse {
    command: String,
    description: String,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct InstallResponse {
    command: String,
    description: String,
    platform: Option<String>,
    confidence: Option<f32>,
}

impl RecoveryEngine {
    /// Extract JSON content from markdown code blocks
    fn extract_json_from_markdown(content: &str) -> String {
        // First try to extract from ```json code blocks
        if let Some(start) = content.find("```json") {
            if let Some(end) = content[start + 7..].find("```") {
                let json_start = start + 7; // "```json".len()
                let json_end = start + 7 + end;
                return content[json_start..json_end].trim().to_string();
            }
        }

        // Try to extract from any ``` code blocks
        if let Some(start) = content.find("```") {
            if let Some(end) = content[start + 3..].find("```") {
                let json_start = start + 3;
                let json_end = start + 3 + end;
                return content[json_start..json_end].trim().to_string();
            }
        }

        // If no code blocks, return the content as-is (trimmed)
        content.trim().to_string()
    }

    pub fn new(config: &Config) -> Result<Self> {
        let client = AIClient::new(&config.llm)?;
        Ok(Self {
            client,
            config: config.clone(),
            available_tools: Vec::new(),
        })
    }

    /// Set the list of available tools for context
    pub async fn set_available_tools(&mut self) -> Result<()> {
        let validator = crate::validator::CommandValidator::new();
        self.available_tools = validator.get_available_tools().await;
        Ok(())
    }

    /// Generate recovery options for missing commands
    pub async fn generate_recovery_options(
        &mut self,
        missing: &MissingCommand,
        original_plan: &Plan,
        original_goal: &str,
    ) -> Result<RecoveryOptions> {
        if !self.should_attempt_recovery(missing) {
            return Ok(RecoveryOptions {
                command_alternatives: Vec::new(),
                installation_instructions: Vec::new(),
                can_skip_step: false,
                retry_possible: false,
                recovery_preference: RecoveryPreference::NeverRecover,
            });
        }

        // Ensure we have available tools context
        if self.available_tools.is_empty() {
            self.set_available_tools().await?;
        }

        match self.config.recovery.preference {
            RecoveryPreference::AlternativesFirst => {
                self.generate_alternatives_first(missing, original_plan, original_goal)
                    .await
            }
            RecoveryPreference::InstallationFirst => {
                self.generate_installation_first(missing, original_plan, original_goal)
                    .await
            }
            RecoveryPreference::SkipOnError => Ok(RecoveryOptions::skip_only()),
            RecoveryPreference::NeverRecover => Err(anyhow!("Recovery disabled by configuration")),
        }
    }

    pub fn should_attempt_recovery(&self, _missing: &MissingCommand) -> bool {
        if !self.config.recovery.enabled {
            return false;
        }

        match self.config.recovery.preference {
            RecoveryPreference::NeverRecover => false,
            _ => true,
        }
    }

    async fn generate_alternatives_first(
        &self,
        missing: &MissingCommand,
        original_plan: &Plan,
        original_goal: &str,
    ) -> Result<RecoveryOptions> {
        let recovery_prompt = self.build_recovery_prompt(missing, original_plan, original_goal);

        let request = ChatCompletionRequest {
            model: self.config.models.planner.clone(),
            messages: vec![ChatMessage {
                role: ChatMessageRole::User,
                content: recovery_prompt,
            }],
            max_tokens: Some(self.config.models.max_tokens),
            temperature: Some(0.3),
        };

        let response = self
            .client
            .chat_completion(request)
            .await
            .context("Failed to get recovery suggestions from AI")?;

        if let Some(choice) = response.choices.first() {
            let json_content = Self::extract_json_from_markdown(&choice.message.content);
            let recovery_response: RecoveryResponse = serde_json::from_str(&json_content)
                .with_context(|| {
                    format!(
                        "Failed to parse AI recovery response: {}",
                        choice.message.content
                    )
                })?;

            let mut options = self.convert_response_to_options(recovery_response)?;

            // For alternatives-first, ensure we have command alternatives
            if options.command_alternatives.is_empty() {
                // Fallback: try to generate simple alternatives
                options.command_alternatives =
                    self.generate_fallback_alternatives(&missing.command)?;
            }

            Ok(options)
        } else {
            Err(anyhow!("AI returned no recovery suggestions"))
        }
    }

    async fn generate_installation_first(
        &self,
        missing: &MissingCommand,
        original_plan: &Plan,
        original_goal: &str,
    ) -> Result<RecoveryOptions> {
        let install_prompt = self.build_installation_prompt(missing, original_plan, original_goal);

        let request = ChatCompletionRequest {
            model: self.config.models.planner.clone(),
            messages: vec![ChatMessage {
                role: ChatMessageRole::User,
                content: install_prompt,
            }],
            max_tokens: Some(self.config.models.max_tokens),
            temperature: Some(0.3),
        };

        let response = self
            .client
            .chat_completion(request)
            .await
            .context("Failed to get installation suggestions from AI")?;

        if let Some(choice) = response.choices.first() {
            let json_content = Self::extract_json_from_markdown(&choice.message.content);
            let recovery_response: RecoveryResponse = serde_json::from_str(&json_content)
                .with_context(|| {
                    format!(
                        "Failed to parse AI installation response: {}",
                        choice.message.content
                    )
                })?;

            let mut options = self.convert_response_to_options(recovery_response)?;

            // For installation-first, prioritize installation instructions
            if options.installation_instructions.is_empty() {
                options.installation_instructions =
                    self.generate_fallback_instructions(&missing.command)?;
            }

            Ok(options)
        } else {
            Err(anyhow!("AI returned no installation suggestions"))
        }
    }

    fn build_recovery_prompt(
        &self,
        missing: &MissingCommand,
        _original_plan: &Plan,
        original_goal: &str,
    ) -> String {
        format!(
            r#"The command '{}' is not available on this system.

Original goal: {}
Failed command line: {}

Available tools on this system: {}
Operating system: {}

Please suggest 2-3 alternative approaches to achieve the same goal, using only the available tools listed above.

For each suggestion:
1. Provide the exact command to run
2. Explain why it works as an alternative
3. Rate confidence (0.0-1.0) that it will achieve the same goal

Also include installation instructions for the missing command if available.

Respond in valid JSON format:
{{
  "alternatives": [
    {{
      "command": "alternate command",
      "description": "why this works as an alternative",
      "confidence": 0.9
    }}
  ],
  "installation_instructions": [
    {{
      "command": "brew install {}",
      "description": "install on macOS using Homebrew",
      "platform": "macos"
    }}
  ],
  "can_skip": false,
  "original_goal_achievable": true
}}"#,
            missing.command,
            original_goal,
            missing.failed_command_line,
            self.available_tools.join(", "),
            std::env::consts::OS,
            missing.command
        )
    }

    fn build_installation_prompt(
        &self,
        missing: &MissingCommand,
        _original_plan: &Plan,
        original_goal: &str,
    ) -> String {
        format!(
            r#"The command '{}' is not available on this system.

Original goal: {}
Failed command line: {}

Operating system: {}

Please provide installation instructions for the missing command and suggest alternative approaches.

Include:
1. Installation commands for this platform
2. Alternative commands that might achieve similar results
3. Whether the original goal can be achieved without the missing tool

Respond in valid JSON format:
{{
  "alternatives": [
    {{
      "command": "alternate command",
      "description": "why this works as an alternative",
      "confidence": 0.7
    }}
  ],
  "installation_instructions": [
    {{
      "command": "brew install {}",
      "description": "install on macOS using Homebrew",
      "platform": "macos"
    }}
  ],
  "can_skip": true,
  "original_goal_achievable": true
}}"#,
            missing.command,
            original_goal,
            missing.failed_command_line,
            std::env::consts::OS,
            missing.command
        )
    }

    fn convert_response_to_options(&self, response: RecoveryResponse) -> Result<RecoveryOptions> {
        let command_alternatives: Vec<CommandAlternative> = response
            .alternatives
            .into_iter()
            .map(|alt| CommandAlternative {
                command: alt.command,
                description: alt.description,
                confidence: alt.confidence,
            })
            .collect();

        let installation_instructions: Vec<InstallationInstruction> = response
            .installation_instructions
            .into_iter()
            .map(|inst| InstallationInstruction {
                command: inst.command.clone(),
                install_commands: vec![inst.command],
                package_managers: inst.platform.map(|p| vec![p]).unwrap_or_default(),
                confidence: inst.confidence.unwrap_or(0.8),
            })
            .collect();

        Ok(RecoveryOptions {
            command_alternatives,
            installation_instructions,
            can_skip_step: response.can_skip,
            retry_possible: response.original_goal_achievable,
            recovery_preference: self.config.recovery.preference,
        })
    }

    fn generate_fallback_alternatives(&self, missing_cmd: &str) -> Result<Vec<CommandAlternative>> {
        let mut alternatives = Vec::new();

        // Common fallbacks for missing commands
        match missing_cmd {
            "tar" => {
                if self.available_tools.contains(&"zip".to_string()) {
                    alternatives.push(CommandAlternative {
                        command: "zip -r archive.zip files".to_string(),
                        description: "Use zip for compression instead of tar".to_string(),
                        confidence: 0.8,
                    });
                }
                if self.available_tools.contains(&"gzip".to_string()) {
                    alternatives.push(CommandAlternative {
                        command: "gzip files".to_string(),
                        description: "Use gzip for individual file compression".to_string(),
                        confidence: 0.6,
                    });
                }
            }
            "curl" => {
                if self.available_tools.contains(&"wget".to_string()) {
                    alternatives.push(CommandAlternative {
                        command: "wget -O output.txt https://example.com".to_string(),
                        description: "Use wget instead of curl for downloading".to_string(),
                        confidence: 0.8,
                    });
                }
            }
            "git" => {
                alternatives.push(CommandAlternative {
                    command:
                        "echo 'Git is required for version control. Please install git first.'"
                            .to_string(),
                    description: "Git cannot be easily replaced - installation required"
                        .to_string(),
                    confidence: 0.1,
                });
            }
            _ => {
                alternatives.push(CommandAlternative {
                    command: format!(
                        "echo '{} command not found. Please install {} or find an alternative.'",
                        missing_cmd, missing_cmd
                    ),
                    description: "No suitable alternative found".to_string(),
                    confidence: 0.1,
                });
            }
        }

        Ok(alternatives)
    }

    fn generate_fallback_instructions(
        &self,
        missing_cmd: &str,
    ) -> Result<Vec<InstallationInstruction>> {
        let mut instructions = Vec::new();

        match std::env::consts::OS {
            "macos" => {
                instructions.push(InstallationInstruction {
                    command: missing_cmd.to_string(),
                    install_commands: vec![format!("brew install {}", missing_cmd)],
                    package_managers: vec!["brew".to_string()],
                    confidence: 0.8,
                });
            }
            "linux" => {
                instructions.push(InstallationInstruction {
                    command: missing_cmd.to_string(),
                    install_commands: vec![format!("sudo apt-get install {}", missing_cmd)],
                    package_managers: vec!["apt".to_string()],
                    confidence: 0.8,
                });
                instructions.push(InstallationInstruction {
                    command: missing_cmd.to_string(),
                    install_commands: vec![format!("sudo yum install {}", missing_cmd)],
                    package_managers: vec!["yum".to_string()],
                    confidence: 0.8,
                });
            }
            _ => {
                instructions.push(InstallationInstruction {
                    command: missing_cmd.to_string(),
                    install_commands: vec![format!(
                        "echo 'Please install {} using your system package manager'",
                        missing_cmd
                    )],
                    package_managers: vec!["generic".to_string()],
                    confidence: 0.5,
                });
            }
        }

        Ok(instructions)
    }

    /// Present recovery options to the user and get their choice
    pub async fn present_recovery_menu(
        &self,
        options: &RecoveryOptions,
        missing: &MissingCommand,
    ) -> Result<RecoveryChoice> {
        println!();
        println!("{}", "🔍 Command Not Found Recovery".bold().yellow());
        println!(
            "The command '{}' is not available on your system.",
            missing.command.bold()
        );

        if !options.command_alternatives.is_empty() {
            println!();
            println!(
                "🤖 AI generated {} alternative solutions:",
                options.command_alternatives.len()
            );

            for (i, alt) in options.command_alternatives.iter().enumerate() {
                println!(
                    "  [{}] {} ({:.0}% confidence)",
                    (i + 1).to_string().cyan(),
                    alt.command.green(),
                    alt.confidence
                );
                println!("      {}", alt.to_string());
                println!();
            }
        }

        if !options.installation_instructions.is_empty() {
            println!("📦 Installation options:");

            let base_offset = options.command_alternatives.len();
            for (i, inst) in options.installation_instructions.iter().enumerate() {
                println!(
                    "  [{}] Install {} ({})",
                    (base_offset + i + 1).to_string().cyan(),
                    missing.command.bold(),
                    inst.package_managers
                        .first()
                        .unwrap_or(&"unknown".to_string())
                        .dimmed()
                );
                println!("      {}", inst.to_string());
                println!();
            }
        }

        // Add skip, retry, and cancel options
        let total_options =
            options.command_alternatives.len() + options.installation_instructions.len();

        if options.can_skip_step {
            println!(
                "  [{}] Skip this step",
                (total_options + 1).to_string().cyan()
            );
        }

        if options.retry_possible {
            println!("  [retry] Retry original command",);
        }

        println!("  {}", "[abort] Cancel entire plan".red());

        let prompt_options = if options.can_skip_step {
            format!("[1-{}] or [skip/retry/abort]", total_options + 2)
        } else {
            format!("[1-{}] or [retry/abort]", total_options + 1)
        };
        print!("Your choice {}: ", prompt_options);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();

        self.parse_user_choice(choice, options, missing)
    }

    fn parse_user_choice(
        &self,
        choice: &str,
        options: &RecoveryOptions,
        _missing: &MissingCommand,
    ) -> Result<RecoveryChoice> {
        let choice_lower = choice.trim().to_lowercase();

        // Handle text-based choices
        if choice_lower == "skip" {
            if options.can_skip_step {
                return Ok(RecoveryChoice::SkipStep);
            } else {
                return Err(anyhow!("Skipping not allowed for this critical step"));
            }
        }
        if choice_lower == "retry" && options.retry_possible {
            return Ok(RecoveryChoice::RetryOriginal);
        }
        if choice_lower == "abort" {
            return Ok(RecoveryChoice::AbortPlan);
        }

        // Handle installation commands like "i1", "i2", etc.
        if choice_lower.starts_with('i') {
            let inst_num = choice_lower[1..]
                .parse::<usize>()
                .context("Please enter a valid installation number (e.g., i1, i2)")?;
            if inst_num > 0 && inst_num <= options.installation_instructions.len() {
                return Ok(RecoveryChoice::InstallCommand(inst_num - 1));
            } else {
                return Err(anyhow!(
                    "Invalid installation number. Please enter i1-i{}",
                    options.installation_instructions.len()
                ));
            }
        }

        // Handle numeric choices
        let choice_num = choice
            .parse::<usize>()
            .context("Please enter a valid number")?;

        let total_options =
            options.command_alternatives.len() + options.installation_instructions.len();

        // Check for command alternatives (1 to N where N = alternatives.len())
        if choice_num > 0 && choice_num <= options.command_alternatives.len() {
            return Ok(RecoveryChoice::UseAlternative(choice_num - 1));
        }

        // Check for installation instructions
        let install_start_idx = options.command_alternatives.len() + 1;
        let install_end_idx =
            options.command_alternatives.len() + options.installation_instructions.len();

        if (install_start_idx..=install_end_idx).contains(&choice_num) {
            let inst_idx = choice_num - install_start_idx;
            return Ok(RecoveryChoice::InstallCommand(inst_idx));
        }

        // Check for skip (only if can_skip_step is true)
        if options.can_skip_step && choice_num == total_options + 1 {
            return Ok(RecoveryChoice::SkipStep);
        }

        // Invalid numeric choice
        let max_choice = if options.can_skip_step {
            total_options + 1
        } else {
            total_options
        };
        Err(anyhow!(
            "Please enter a number between 1 and {}, or use text options (skip/retry/abort)",
            max_choice
        ))
    }

    /// Execute the user's recovery choice
    pub async fn execute_recovery(
        &mut self,
        choice: RecoveryChoice,
        context: &RecoveryContext,
    ) -> Result<RecoveryResult> {
        match choice {
            RecoveryChoice::UseAlternative(index) => {
                // Generate options to get the alternatives
                let options = self
                    .generate_recovery_options(
                        &context.missing_command,
                        &context.original_plan,
                        &context.original_goal,
                    )
                    .await?;
                if let Some(alternative) = options.command_alternatives.get(index) {
                    self.execute_alternative(alternative.clone(), context).await
                } else {
                    Ok(RecoveryResult::PlanAborted(
                        "Invalid alternative index".to_string(),
                    ))
                }
            }
            RecoveryChoice::InstallCommand(index) => {
                // Generate options to get the installation instructions
                let options = self
                    .generate_recovery_options(
                        &context.missing_command,
                        &context.original_plan,
                        &context.original_goal,
                    )
                    .await?;
                if let Some(instruction) = options.installation_instructions.get(index) {
                    self.execute_installation(instruction.clone(), context)
                        .await
                } else {
                    Ok(RecoveryResult::PlanAborted(
                        "Invalid installation index".to_string(),
                    ))
                }
            }
            RecoveryChoice::SkipStep => Ok(RecoveryResult::StepSkipped),
            RecoveryChoice::AbortPlan => Ok(RecoveryResult::PlanAborted(
                "User cancelled due to missing command".to_string(),
            )),
            RecoveryChoice::RetryOriginal => Ok(RecoveryResult::RetryRequested),
        }
    }

    async fn execute_alternative(
        &self,
        alternative: CommandAlternative,
        _context: &RecoveryContext,
    ) -> Result<RecoveryResult> {
        println!("🔄 Using alternative: {}", alternative.command.green());

        // Execute the alternative command
        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&alternative.command)
            .output()
            .await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Alternative command succeeded!");
                    if !output.stdout.is_empty() {
                        println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    }
                    Ok(RecoveryResult::AlternativeSucceeded(alternative))
                } else {
                    println!("❌ Alternative command failed");
                    if !output.stderr.is_empty() {
                        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
                    }
                    Ok(RecoveryResult::AlternativeFailed(alternative))
                }
            }
            Err(e) => {
                println!("❌ Failed to execute alternative: {}", e);
                Ok(RecoveryResult::AlternativeFailed(alternative))
            }
        }
    }

    async fn execute_installation(
        &self,
        instruction: InstallationInstruction,
        context: &RecoveryContext,
    ) -> Result<RecoveryResult> {
        if !self.config.recovery.auto_install {
            println!(
                "📦 Installation instructions for {}:",
                context.missing_command.command.bold()
            );
            println!("Command: {}", instruction.command.cyan());
            println!("Description: {}", instruction.to_string());

            print!("Execute this installation command? [y/N]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() != "y" {
                return Ok(RecoveryResult::InstallationCancelled);
            }
        }

        println!(
            "📦 Installing {}...",
            context.missing_command.command.bold()
        );

        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&instruction.command)
            .output()
            .await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Installation completed successfully!");
                    if !output.stdout.is_empty() {
                        println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    }
                    Ok(RecoveryResult::InstallationSucceeded(instruction))
                } else {
                    println!("❌ Installation failed");
                    if !output.stderr.is_empty() {
                        eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
                    }
                    Ok(RecoveryResult::InstallationFailed(instruction))
                }
            }
            Err(e) => {
                println!("❌ Failed to execute installation: {}", e);
                Ok(RecoveryResult::InstallationFailed(instruction))
            }
        }
    }
}

#[derive(Debug)]
pub struct RecoveryContext {
    pub missing_command: MissingCommand,
    pub original_plan: Plan,
    pub original_goal: String,
}

#[derive(Debug)]
pub enum RecoveryResult {
    AlternativeSucceeded(CommandAlternative),
    AlternativeFailed(CommandAlternative),
    InstallationSucceeded(InstallationInstruction),
    InstallationFailed(InstallationInstruction),
    InstallationCancelled,
    StepSkipped,
    PlanAborted(String),
    RetryRequested,
    RetryWithDifferentApproach,
}

impl RecoveryOptions {
    pub fn skip_only() -> Self {
        Self {
            command_alternatives: Vec::new(),
            installation_instructions: Vec::new(),
            can_skip_step: true,
            retry_possible: false,
            recovery_preference: RecoveryPreference::SkipOnError,
        }
    }
}

#[cfg(test)]
mod tests;
