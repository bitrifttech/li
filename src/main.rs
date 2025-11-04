mod cerebras;
mod cli;
mod config;
mod exec;
mod hook;
mod planner;
mod classifier;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli.run()
}
