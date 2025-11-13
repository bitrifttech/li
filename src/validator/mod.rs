//! Validator module providing command validation functionality
//! 
//! This module checks whether commands in execution plans are available on the system
//! before attempting to run them, preventing execution failures due to missing tools.

use anyhow::{Result, anyhow};

use crate::planner::Plan;

// Re-export all public types
pub use types::{
    CommandValidator, MissingCommand, ValidationResult,
};

// Module declarations
mod types;
mod checker;

impl CommandValidator {
    /// Create a new command validator
    pub fn new() -> Self {
        Default::default()
    }

    /// Extract the primary command from a complex command line
    pub fn extract_command(cmd: &str) -> Option<String> {
        checker::extract_command(cmd)
    }

    /// Check if a command exists in the system PATH
    pub async fn command_exists(&mut self, cmd: &str) -> bool {
        // Check cache first
        if let Some(&cached_result) = self.cache.get(cmd) {
            return cached_result;
        }

        let exists = checker::check_command_existence(self, cmd).await;

        // Cache the result for future use
        self.cache.insert(cmd.to_string(), exists);

        exists
    }

    /// Validate all commands in a plan
    pub async fn validate_plan(&mut self, plan: &Plan) -> Result<ValidationResult> {
        let mut missing_commands = Vec::new();

        // Check dry-run commands
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            if let Some(command_name) = Self::extract_command(cmd) {
                if !self.command_exists(&command_name).await {
                    missing_commands.push(MissingCommand {
                        command: command_name,
                        failed_command_line: cmd.clone(),
                        plan_step: idx,
                        is_dry_run: true,
                    });
                }
            }
        }

        // Check execute commands
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            if let Some(command_name) = Self::extract_command(cmd) {
                if !self.command_exists(&command_name).await {
                    missing_commands.push(MissingCommand {
                        command: command_name,
                        failed_command_line: cmd.clone(),
                        plan_step: idx,
                        is_dry_run: false,
                    });
                }
            }
        }

        let plan_can_continue =
            missing_commands.is_empty() || missing_commands.iter().all(|cmd| cmd.is_dry_run);

        Ok(ValidationResult {
            missing_commands,
            plan_can_continue,
        })
    }

    /// Check a single command for existence
    pub async fn check_single_command(&mut self, cmd_line: &str) -> Result<bool> {
        let command_name = Self::extract_command(cmd_line)
            .ok_or_else(|| anyhow!("Could not extract command from: {}", cmd_line))?;

        Ok(self.command_exists(&command_name).await)
    }

    /// Get available commands on the system (common utilities)
    pub async fn get_available_tools(&self) -> Vec<String> {
        let common_tools = checker::get_common_tools();

        let mut available = Vec::new();
        for tool in common_tools {
            // Create a new validator for this check to avoid cache pollution
            let mut validator = CommandValidator::new();
            if validator.command_exists(tool).await {
                available.push(tool.to_string());
            }
        }

        available
    }

    /// Clear the validation cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        let total = self.cache.len();
        let found = self.cache.values().filter(|&&exists| exists).count();
        (total, found)
    }
}

#[cfg(test)]
mod tests;
