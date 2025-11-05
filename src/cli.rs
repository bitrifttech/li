use anyhow::{anyhow, bail, Result};
use clap::{Args, Parser, Subcommand};
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::cerebras::{CerebrasClient, ChatCompletionRequest, ChatMessage, ChatMessageRole};
use crate::config::Config;
use crate::{classifier, planner};

/// Entry point for the `li` command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "li",
    about = "Plain-English to shell assistant",
    version,
    long_about = None
)]
pub struct Cli {
    /// Optional subcommand (e.g., `chat`)
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Enable classification before planning (use in shell hook mode)
    #[arg(short = 'c', long = "classify")]
    pub classify: bool,

    /// Default task: words typed after `li`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub task: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Directly invoke the Cerebras chat completion API.
    Chat(ChatArgs),
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    /// Optional override for the model (defaults to planner model from config).
    #[arg(long)]
    model: Option<String>,

    /// Optional override for max_tokens (defaults to config setting).
    #[arg(long)]
    max_tokens: Option<u32>,

    /// Optional temperature to pass through to the API.
    #[arg(long)]
    temperature: Option<f32>,

    /// Prompt to send to the Cerebras model.
    #[arg(required = true)]
    prompt: Vec<String>,
}

impl Cli {
    pub async fn run(self, config: Config) -> Result<()> {
        match self.command {
            Some(Command::Chat(args)) => handle_chat(args, &config).await?,
            None => handle_task(self.task, self.classify, &config).await?,
        }
        Ok(())
    }
}

async fn handle_chat(args: ChatArgs, config: &Config) -> Result<()> {
    let prompt = args.prompt.join(" ").trim().to_owned();
    if prompt.is_empty() {
        bail!("Prompt cannot be empty");
    }

    let model = args.model.unwrap_or_else(|| config.planner_model.clone());
    let max_tokens = args.max_tokens.unwrap_or(config.max_tokens);
    let temperature = args.temperature;

    let client = CerebrasClient::new(config)?;
    let response = client
        .chat_completion(ChatCompletionRequest {
            model: model.clone(),
            messages: vec![ChatMessage {
                role: ChatMessageRole::User,
                content: prompt,
            }],
            max_tokens: Some(max_tokens),
            temperature,
        })
        .await?;

    println!("Model: {}", model);

    for (idx, choice) in response.choices.iter().enumerate() {
        println!("\nChoice {}:", idx + 1);
        println!("{}", choice.message.content.trim());

        if let Some(reasoning) = &choice.message.reasoning {
            let trimmed = reasoning.trim();
            if !trimmed.is_empty() {
                println!("Reasoning: {}", trimmed);
            }
        }

        if let Some(reason) = &choice.finish_reason {
            println!("Finish reason: {}", reason);
        }
    }

    if let Some(usage) = response.usage {
        println!(
            "\nUsage - prompt: {} tokens, completion: {} tokens, total: {} tokens",
            format_option_u32(usage.prompt_tokens),
            format_option_u32(usage.completion_tokens),
            format_option_u32(usage.total_tokens)
        );
    }

    Ok(())
}

fn format_option_u32(value: Option<u32>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

async fn handle_task(words: Vec<String>, classify: bool, config: &Config) -> Result<()> {
    let prompt = words.join(" ").trim().to_owned();
    if prompt.is_empty() {
        println!(
            "li CLI is initialized. Provide a task or run `li chat \"your question\"` to call Cerebras."
        );
        return Ok(());
    }

    let client = CerebrasClient::new(config)?;

    // Only classify if --classify flag is set (used by shell hook)
    let plan = if classify {
        let classification = classifier::classify(&client, &prompt, &config.classifier_model).await
            .map_err(|e| {
                if e.to_string().contains("Rate limit") {
                    anyhow!("Classification service is rate limited. Please try again in a moment.")
                } else if e.to_string().contains("Invalid API key") {
                    anyhow!("Invalid API key for classification. Please check your configuration.")
                } else {
                    e
                }
            })?;
        
        match classification {
            classifier::Classification::Terminal => {
                // Direct execution for terminal commands
                println!("Executing direct terminal command: {}", prompt);
                let success = run_command(&prompt).await?;
                if !success {
                    bail!("Command failed: {}", prompt);
                }
                return Ok(());
            }
            classifier::Classification::NaturalLanguage => {
                // Planning for natural language
                planner::plan(&client, &prompt, &config.planner_model, config.max_tokens).await
                    .map_err(|e| {
                        if e.to_string().contains("Rate limit") {
                            anyhow!("Planner service is rate limited. Please wait a moment and try again.")
                        } else if e.to_string().contains("high traffic") {
                            anyhow!("Planner service is experiencing high traffic. Please try again soon.")
                        } else if e.to_string().contains("Planning cancelled") {
                            e // Keep the cancellation message as-is
                        } else {
                            e
                        }
                    })?
            }
        }
    } else {
        // Direct planning without classification
        planner::plan(&client, &prompt, &config.planner_model, config.max_tokens).await
            .map_err(|e| {
                if e.to_string().contains("Rate limit") {
                    anyhow!("Planner service is rate limited. Please wait a moment and try again.")
                } else if e.to_string().contains("high traffic") {
                    anyhow!("Planner service is experiencing high traffic. Please try again soon.")
                } else if e.to_string().contains("Planning cancelled") {
                    e // Keep the cancellation message as-is
                } else {
                    e
                }
            })?
    }; 
    render_plan(&plan);

    if prompt_for_approval()? {
        execute_plan(&plan).await?;
    } else {
        println!("\nPlan execution cancelled.");
    }

    Ok(())
}

fn render_plan(plan: &planner::Plan) {
    println!("\nPlan confidence: {:.2}", plan.confidence);

    if !plan.dry_run_commands.is_empty() {
        println!("\nDry-run Commands:");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.execute_commands.is_empty() {
        println!("\nExecute Commands:");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.notes.trim().is_empty() {
        println!("\nNotes: {}", plan.notes.trim());
    }

}

fn prompt_for_approval() -> Result<bool> {
    print!("\nExecute this plan? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

async fn execute_plan(plan: &planner::Plan) -> Result<()> {
    println!("\n=== Executing Plan ===");

    if !plan.dry_run_commands.is_empty() {
        println!("\n[Dry-run Phase]");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!("\n> Running check {}/{}: {}", idx + 1, plan.dry_run_commands.len(), cmd);
            let success = run_command(cmd).await?;
            if !success {
                bail!("Dry-run check failed: {}", cmd);
            }
        }
        println!("\n✓ All dry-run checks passed.");
    }

    if !plan.execute_commands.is_empty() {
        println!("\n[Execute Phase]");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!("\n> Executing {}/{}: {}", idx + 1, plan.execute_commands.len(), cmd);
            let success = run_command(cmd).await?;
            if !success {
                bail!("Command failed: {}", cmd);
            }
        }
        println!("\n✓ Plan execution completed.");
    }

    Ok(())
}

async fn run_command(cmd: &str) -> Result<bool> {
    // Force ls to use colors if it's an ls command
    let modified_cmd = if cmd.starts_with("ls ") || cmd == "ls" {
        cmd.replace("ls", "ls --color=always")
    } else {
        cmd.to_string()
    };
    
    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(&modified_cmd)
        .env("FORCE_COLOR", "1")           // Generic color forcing (npm, yarn, etc.)
        .env("CLICOLOR_FORCE", "1")        // BSD/macOS color forcing
        .env("COLORTERM", "truecolor")     // Advertise true color support
        .env("TERM", "xterm-256color")     // 256 color support
        .env("GIT_CONFIG_PARAMETERS", "'color.ui=always'")  // Force git colors
        .env("LS_COLORS", "di=1;34:fi=0:ln=1;36:pi=40;33:so=1;35:do=1;35:bd=40;33;01:cd=40;33;01:or=40;31;01:ex=1;32:*.tar=1;31:*.tgz=1;31:*.zip=1;31:*.gz=1;31:*.bz2=1;31:*.deb=1;31:*.rpm=1;31:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:*.mpg=1;37:*.avi=1;37:*.gl=1;37:*.dl=1;37:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:")  // Standard ls colors
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!("Command not found: {}. Please ensure the command exists in your PATH.", cmd)
            } else {
                anyhow!("Failed to execute command '{}': {}", cmd, e)
            }
        })?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let stdout_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            println!("{}", line);
        }
    });

    let stderr_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            eprintln!("{}", line);
        }
    });

    let status = child.wait().await
        .map_err(|e| anyhow!("Failed to wait for command completion: {}", e))?;

    stdout_handle.await
        .map_err(|e| anyhow!("Failed to read command output: {}", e))?;
    stderr_handle.await
        .map_err(|e| anyhow!("Failed to read command errors: {}", e))?;

    if !status.success() {
        if let Some(code) = status.code() {
            Err(anyhow!("Command failed with exit code {}: {}", code, cmd))
        } else {
            Err(anyhow!("Command was terminated by signal: {}", cmd))
        }
    } else {
        Ok(true)
    }
}
