use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::config::Config;

use super::commands;

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

    /// Override the model (for OpenRouter, fetches free models list)
    #[arg(short = 'm', long = "model", num_args = 0..=1, default_missing_value = "")]
    pub model: Option<String>,

    /// Select the LLM provider (e.g., openrouter or cerebras)
    #[arg(long = "provider", num_args = 0..=1, default_missing_value = "")]
    pub provider: Option<String>,

    /// Enable verbose logging of LLM requests and responses
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Interactive setup for first-time configuration
    #[arg(long = "setup")]
    pub setup: bool,

    /// Direct chat with the AI model
    #[arg(long = "chat")]
    pub chat: bool,

    /// Explain command output using AI intelligence
    #[arg(short = 'i', long = "intelligence")]
    pub intelligence: bool,

    /// Ask a specific question about the command output (implies intelligence mode)
    #[arg(short = 'q', long = "question")]
    pub question: Option<String>,

    /// Configure li settings
    #[arg(long)]
    pub config: bool,

    /// Set the API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// Set timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Set max tokens
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Set planner model
    #[arg(long)]
    pub planner_model: Option<String>,

    /// Default task: words typed after `li`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub task: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Directly invoke the chat completion API.
    Chat(ChatArgs),
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    /// Optional override for the model (defaults to planner model from config).
    #[arg(long)]
    pub model: Option<String>,

    /// Optional override for max_tokens (defaults to config setting).
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Optional temperature to pass through to the API.
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Prompt to send to the OpenRouter model.
    #[arg(required = true)]
    pub prompt: Vec<String>,
}

impl Cli {
    pub async fn run_setup(self) -> Result<()> {
        commands::run_setup(self).await
    }

    pub async fn run(self, config: Config) -> Result<()> {
        commands::run(self, config).await
    }
}
