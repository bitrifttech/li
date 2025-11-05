mod cerebras;
mod classifier;
mod cli;
mod config;
mod exec;
mod hook;
mod planner;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let config = match config::Config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed to load configuration: {err}");
            std::process::exit(1);
        }
    };

    cli.run(config).await
}
