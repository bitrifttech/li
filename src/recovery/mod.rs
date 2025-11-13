//! Recovery module providing intelligent error recovery for missing commands
//!
//! This module offers AI-powered recovery suggestions when commands fail during execution,
//! including alternative commands, installation instructions, and user interaction.

use anyhow::Result;

use crate::client::AIClient;
use crate::config::Config;
use crate::planner::Plan;
use crate::validator::MissingCommand;

// Re-export all public types
pub use types::{
    CommandAlternative, InstallationInstruction, RecoveryChoice, RecoveryContext, RecoveryEngine,
    RecoveryOptions, RecoveryResult, RecoveryStrategy,
};

// Module declarations
mod ai;
mod types;
mod ui;
mod utils;

impl RecoveryEngine {
    /// Create a new recovery engine with the given configuration
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
        strategy: RecoveryStrategy,
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
            });
        }

        // Ensure we have available tools context
        if self.available_tools.is_empty() {
            self.set_available_tools().await?;
        }

        match strategy {
            RecoveryStrategy::InstallationFirst => {
                self.generate_installation_first(missing, original_plan, original_goal)
                    .await
            }
            RecoveryStrategy::SkipOnError => Ok(RecoveryOptions::skip_only()),
            RecoveryStrategy::NeverRecover => {
                Err(anyhow::anyhow!("Recovery disabled for this command"))
            }
        }
    }

    /// Check if recovery should be attempted for the given missing command
    pub fn should_attempt_recovery(&self, _missing: &MissingCommand) -> bool {
        self.config.recovery.enabled
    }

    /// Present recovery options to the user and get their choice
    pub async fn present_recovery_menu(
        &self,
        options: &RecoveryOptions,
        missing: &MissingCommand,
    ) -> Result<RecoveryChoice> {
        ui::present_recovery_menu(self, options, missing).await
    }

    /// Execute the user's recovery choice
    pub async fn execute_recovery(
        &mut self,
        choice: RecoveryChoice,
        context: &RecoveryContext,
        options: &RecoveryOptions,
    ) -> Result<RecoveryResult> {
        match choice {
            RecoveryChoice::UseAlternative(index) => {
                if let Some(alternative) = options.command_alternatives.get(index) {
                    utils::execute_alternative(self, alternative.clone(), context).await
                } else {
                    Ok(RecoveryResult::PlanAborted(
                        "Invalid alternative index".to_string(),
                    ))
                }
            }
            RecoveryChoice::InstallCommand(index) => {
                if let Some(instruction) = options.installation_instructions.get(index) {
                    utils::execute_installation(self, instruction.clone(), context).await
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
}

#[cfg(test)]
mod tests;
