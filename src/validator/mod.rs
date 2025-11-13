use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use tokio::process::Command as TokioCommand;

use crate::planner::Plan;

pub struct CommandValidator {
    cache: HashMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub missing_commands: Vec<MissingCommand>,
    pub plan_can_continue: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MissingCommand {
    pub command: String,
    pub failed_command_line: String,
    pub plan_step: usize,
    pub is_dry_run: bool,
}

impl CommandValidator {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Extract the primary command from a complex command line
    pub fn extract_command(cmd: &str) -> Option<String> {
        let trimmed = cmd.trim();

        // Handle empty command
        if trimmed.is_empty() {
            return None;
        }

        // Handle common shell constructs
        let primary_cmd = if trimmed.contains("&&") {
            trimmed.split("&&").next()?.trim()
        } else if trimmed.contains("||") {
            trimmed.split("||").next()?.trim()
        } else if trimmed.contains("|") {
            trimmed.split("|").next()?.trim()
        } else if trimmed.contains(";") {
            trimmed.split(";").next()?.trim()
        } else {
            trimmed
        };

        // Extract first word (handles pipes, redirects, etc.)
        let first_token = primary_cmd.split_whitespace().next()?;

        // Strip common prefixes
        let cleaned_cmd = if let Some(stripped) = first_token.strip_prefix("./") {
            stripped
        } else if let Some(_stripped) = first_token.strip_prefix("/") {
            first_token // Keep full path
        } else if let Some(_stripped) = first_token.strip_prefix("~/") {
            first_token // Keep full path
        } else {
            first_token
        };

        Some(cleaned_cmd.to_string())
    }

    /// Check if a command exists in the system PATH
    pub async fn command_exists(&mut self, cmd: &str) -> bool {
        // Check cache first
        if let Some(&cached_result) = self.cache.get(cmd) {
            return cached_result;
        }

        let exists = self.check_command_existence(cmd).await;

        // Cache the result for future use
        self.cache.insert(cmd.to_string(), exists);

        exists
    }

    async fn check_command_existence(&self, cmd: &str) -> bool {
        // Handle absolute paths and relative paths
        if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("~/") {
            // For paths, we need to check if the file exists and is executable
            let expanded_cmd = if cmd.starts_with("~/") {
                if let Some(home) = dirs::home_dir() {
                    cmd.replacen("~", &home.to_string_lossy(), 1)
                } else {
                    cmd.to_string()
                }
            } else {
                cmd.to_string()
            };

            return tokio::fs::metadata(&expanded_cmd)
                .await
                .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
        }

        // For regular commands, use 'which' or 'command -v'
        let result = TokioCommand::new("sh")
            .arg("-c")
            .arg(&format!("command -v {}", cmd))
            .output()
            .await;

        match result {
            Ok(output) => output.status.success(),
            Err(_) => {
                // Fallback: try to get command help/version
                let fallback_result = TokioCommand::new("sh")
                    .arg("-c")
                    .arg(&format!("{} --version >/dev/null 2>&1", cmd))
                    .output()
                    .await;

                fallback_result
                    .map(|output| output.status.success())
                    .unwrap_or(false)
            }
        }
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
        let common_tools = vec![
            "git",
            "docker",
            "npm",
            "node",
            "python",
            "python3",
            "pip",
            "pip3",
            "cargo",
            "rustc",
            "brew",
            "apt",
            "apt-get",
            "yum",
            "dnf",
            "tar",
            "zip",
            "unzip",
            "gzip",
            "curl",
            "wget",
            "ssh",
            "scp",
            "rsync",
            "find",
            "grep",
            "sed",
            "awk",
            "sort",
            "uniq",
            "wc",
            "head",
            "tail",
            "ls",
            "cd",
            "pwd",
            "mkdir",
            "rm",
            "cp",
            "mv",
            "chmod",
            "chown",
            "cat",
            "less",
            "more",
            "echo",
            "printf",
            "date",
            "whoami",
            "ps",
            "top",
            "htop",
            "kill",
            "killall",
            "jobs",
            "bg",
            "fg",
            "nohup",
            "mount",
            "umount",
            "df",
            "du",
            "free",
            "uname",
            "which",
            "whereis",
            "man",
            "info",
            "help",
            "history",
            "alias",
            "export",
            "source",
            "vim",
            "nano",
            "emacs",
            "code",
            "make",
            "cmake",
            "gcc",
            "g++",
            "java",
            "javac",
            "scala",
            "kotlin",
            "go",
            "ruby",
            "perl",
            "php",
            "mysql",
            "postgresql",
            "psql",
            "redis-cli",
            "mongo",
            "kubectl",
            "helm",
            "terraform",
            "ansible",
            "vault",
            "consul",
            "nomad",
        ];

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

impl Default for CommandValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

