mod agent;
mod classifier;
mod cli;
mod client;
mod config;
mod exec;
mod hook;
mod planner;
mod recovery;
mod tokens;
mod validator;

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
    let is_empty_task =
        cli.task.is_empty() && !cli.chat && cli.command.is_none() && cli.model.is_none();

    if is_empty_task {
        // Run with a dummy config that will trigger the welcome message
        let dummy_config = config::Config::builder()
            .with_models(|models| {
                models.classifier.clear();
                models.planner.clear();
            })
            .build()
            .unwrap();
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
