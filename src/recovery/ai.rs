use anyhow::{Context, Result, anyhow};
use serde_json;

use crate::client::{ChatCompletionRequest, ChatMessage, ChatMessageRole, LlmClient};
use crate::planner::Plan;
use crate::validator::MissingCommand;

use super::types::{RecoveryEngine, RecoveryOptions, RecoveryResponse};
use super::utils;

impl RecoveryEngine {
    /// Extract JSON content from markdown code blocks
    pub fn extract_json_from_markdown(content: &str) -> String {
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

    /// Generate recovery options prioritizing installation instructions
    pub async fn generate_installation_first(
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
                    utils::generate_fallback_instructions(self, &missing.command)?;
            }

            if options.command_alternatives.is_empty() {
                options.command_alternatives =
                    utils::generate_fallback_alternatives(self, &missing.command)?;
            }

            Ok(options)
        } else {
            Err(anyhow!("AI returned no installation suggestions"))
        }
    }

    /// Build AI prompt for installation-focused recovery
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

Return only a single JSON object matching the schema below. Do not include reasoning, prose, or code fences.

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

    /// Convert AI response to structured recovery options
    pub fn convert_response_to_options(
        &self,
        response: RecoveryResponse,
    ) -> Result<RecoveryOptions> {
        use super::types::{CommandAlternative, InstallationInstruction};

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
        })
    }
}
