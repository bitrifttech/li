use std::os::unix::fs::PermissionsExt;
use tokio::process::Command as TokioCommand;

use super::types::CommandValidator;

impl CommandValidator {
    /// Check if a command exists in the system PATH (internal implementation)
    pub(super) async fn check_command_existence(&self, cmd: &str) -> bool {
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

    /// Get list of common tools to check for availability
    pub fn get_common_tools() -> Vec<&'static str> {
        vec![
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
        ]
    }
}

/// Check if a command exists in the system PATH (internal implementation)
pub(super) async fn check_command_existence(validator: &CommandValidator, cmd: &str) -> bool {
    validator.check_command_existence(cmd).await
}

/// Get list of common tools to check for availability (public function)
pub(super) fn get_common_tools() -> Vec<&'static str> {
    CommandValidator::get_common_tools()
}

/// Extract the primary command from a complex command line
pub(super) fn extract_command(cmd: &str) -> Option<String> {
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