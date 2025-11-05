use anyhow::{bail, Result};
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
    #[command(subcommand)]
    command: Option<Command>,

    /// Plain-English task to classify (default behaviour when no subcommand is used).
    #[arg()]
    task: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Directly invoke the Cerebras chat completion API.
    Chat(ChatArgs),
}

#[derive(Debug, Args)]
struct ChatArgs {
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
            None => handle_task(self.task, &config).await?,
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

async fn handle_task(words: Vec<String>, config: &Config) -> Result<()> {
    let prompt = words.join(" ").trim().to_owned();
    if prompt.is_empty() {
        println!(
            "li CLI is initialized. Provide a task or run `li chat \"your question\"` to call Cerebras."
        );
        return Ok(());
    }

    let client = CerebrasClient::new(config)?;
    let classification = classifier::classify(&client, &prompt, &config.classifier_model).await?;

    match classification {
        classifier::Classification::Terminal => {
            println!("Classification: Terminal");
            println!("Suggested action: execute input directly in the shell.");
            std::process::exit(100);
        }
        classifier::Classification::NaturalLanguage => {
            println!("Classification: NaturalLanguage");
            let plan =
                planner::plan(&client, &prompt, &config.planner_model, config.max_tokens).await?;

            render_plan(&plan);

            if prompt_for_approval()? {
                execute_plan(&plan).await?;
            } else {
                println!("\nPlan execution cancelled.");
            }
        }
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
    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

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

    let status = child.wait().await?;

    stdout_handle.await?;
    stderr_handle.await?;

    Ok(status.success())
}
