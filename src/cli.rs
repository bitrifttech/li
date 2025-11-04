use anyhow::Result;
use clap::Parser;

/// Entry point for the `li` command-line interface (placeholder implementation).
#[derive(Debug, Parser)]
#[command(
    name = "li",
    about = "Plain-English to shell assistant",
    version,
    long_about = None
)]
pub struct Cli {
    /// Optional natural language task to route through the planner.
    #[arg()]
    pub task: Vec<String>,
}

impl Cli {
    pub fn run(self) -> Result<()> {
        if self.task.is_empty() {
            println!("li CLI is initialized. Provide a task to continue.");
        } else {
            let request = self.task.join(" ");
            println!("Received task: {request}");
        }
        Ok(())
    }
}
