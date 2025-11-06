mod client;
mod classifier;
mod cli;
mod config;
mod tokens;
mod exec;
mod hook;
mod planner;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Handle setup command before loading config (for fresh installs)
    if cli.setup {
        return cli.run_setup().await;
    }

    // Check if this is just "li" with no arguments (show welcome message)
    let is_empty_task = cli.task.is_empty() && !cli.chat && cli.command.is_none() && cli.model.is_none();
    
    if is_empty_task {
        // Run with a dummy config that will trigger the welcome message
        let dummy_config = config::Config {
            api_key: "".to_string(),
            timeout_secs: 30,
            max_tokens: 2048,
            classifier_model: "".to_string(),
            planner_model: "".to_string(),
        };
        return cli.run(dummy_config).await;
    }

    let config = match config::Config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed to load configuration: {err}");
            eprintln!("Run 'li --setup' to configure li for first-time use.");
            std::process::exit(1);
        }
    };

    cli.run(config).await
}
