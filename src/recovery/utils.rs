use anyhow::Result;
use colored::*;
use std::io::Write;

use crate::config::Config;

use super::types::{CommandAlternative, InstallationInstruction, RecoveryEngine, RecoveryResult};

/// Generate fallback alternatives for common missing commands
pub fn generate_fallback_alternatives(
    engine: &RecoveryEngine,
    missing_cmd: &str,
) -> Result<Vec<CommandAlternative>> {
    let mut alternatives = Vec::new();

    // Common fallbacks for missing commands
    match missing_cmd {
        "tar" => {
            if engine.available_tools.contains(&"zip".to_string()) {
                alternatives.push(CommandAlternative {
                    command: "zip -r archive.zip files".to_string(),
                    description: "Use zip for compression instead of tar".to_string(),
                    confidence: 0.8,
                });
            }
            if engine.available_tools.contains(&"gzip".to_string()) {
                alternatives.push(CommandAlternative {
                    command: "gzip files".to_string(),
                    description: "Use gzip for individual file compression".to_string(),
                    confidence: 0.6,
                });
            }
        }
        "curl" => {
            if engine.available_tools.contains(&"wget".to_string()) {
                alternatives.push(CommandAlternative {
                    command: "wget -O output.txt https://example.com".to_string(),
                    description: "Use wget instead of curl for downloading".to_string(),
                    confidence: 0.8,
                });
            }
        }
        "git" => {
            alternatives.push(CommandAlternative {
                command: "echo 'Git is required for version control. Please install git first.'"
                    .to_string(),
                description: "Git cannot be easily replaced - installation required".to_string(),
                confidence: 0.1,
            });
        }
        _ => {
            alternatives.push(CommandAlternative {
                command: format!(
                    "echo '{} command not found. Please install {} or find an alternative.'",
                    missing_cmd, missing_cmd
                ),
                description: "No suitable alternative found".to_string(),
                confidence: 0.1,
            });
        }
    }

    Ok(alternatives)
}

/// Generate fallback installation instructions based on platform
pub fn generate_fallback_instructions(
    _engine: &RecoveryEngine,
    missing_cmd: &str,
) -> Result<Vec<InstallationInstruction>> {
    let mut instructions = Vec::new();

    match std::env::consts::OS {
        "macos" => {
            instructions.push(InstallationInstruction {
                command: missing_cmd.to_string(),
                install_commands: vec![format!("brew install {}", missing_cmd)],
                package_managers: vec!["brew".to_string()],
                confidence: 0.8,
            });
        }
        "linux" => {
            instructions.push(InstallationInstruction {
                command: missing_cmd.to_string(),
                install_commands: vec![format!("sudo apt-get install {}", missing_cmd)],
                package_managers: vec!["apt".to_string()],
                confidence: 0.8,
            });
            instructions.push(InstallationInstruction {
                command: missing_cmd.to_string(),
                install_commands: vec![format!("sudo yum install {}", missing_cmd)],
                package_managers: vec!["yum".to_string()],
                confidence: 0.8,
            });
        }
        _ => {
            instructions.push(InstallationInstruction {
                command: missing_cmd.to_string(),
                install_commands: vec![format!(
                    "echo 'Please install {} using your system package manager'",
                    missing_cmd
                )],
                package_managers: vec!["generic".to_string()],
                confidence: 0.5,
            });
        }
    }

    Ok(instructions)
}

/// Execute an alternative command and return the result
pub async fn execute_alternative(
    _engine: &RecoveryEngine,
    alternative: CommandAlternative,
    _context: &super::types::RecoveryContext,
) -> Result<RecoveryResult> {
    println!("🔄 Using alternative: {}", alternative.command.green());

    // Execute the alternative command
    let result = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&alternative.command)
        .output()
        .await;

    match result {
        Ok(output) => {
            if output.status.success() {
                println!("✅ Alternative command succeeded!");
                if !output.stdout.is_empty() {
                    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                }
                Ok(RecoveryResult::AlternativeSucceeded(alternative))
            } else {
                println!("❌ Alternative command failed");
                if !output.stderr.is_empty() {
                    eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
                }
                Ok(RecoveryResult::AlternativeFailed(alternative))
            }
        }
        Err(e) => {
            println!("❌ Failed to execute alternative: {}", e);
            Ok(RecoveryResult::AlternativeFailed(alternative))
        }
    }
}

/// Execute installation instructions and return the result
pub async fn execute_installation(
    _engine: &RecoveryEngine,
    instruction: InstallationInstruction,
    context: &super::types::RecoveryContext,
    config: &Config,
) -> Result<RecoveryResult> {
    if !config.recovery.auto_install {
        println!(
            "📦 Installation instructions for {}:",
            context.missing_command.command.bold()
        );
        println!("Command: {}", instruction.command.cyan());
        println!("Description: {}", instruction.to_string());

        print!("Execute this installation command? [y/N]: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "y" {
            return Ok(RecoveryResult::InstallationCancelled);
        }
    }

    println!(
        "📦 Installing {}...",
        context.missing_command.command.bold()
    );

    let install_cmd = instruction
        .install_commands
        .first()
        .unwrap_or(&instruction.command);

    let result = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(install_cmd)
        .output()
        .await;

    match result {
        Ok(output) => {
            if output.status.success() {
                println!("✅ Installation completed successfully!");
                if !output.stdout.is_empty() {
                    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                }
                Ok(RecoveryResult::InstallationSucceeded(instruction))
            } else {
                println!("❌ Installation failed");
                if !output.stderr.is_empty() {
                    eprintln!("Error: {}", String::from_utf8_lossy(&output.stderr));
                }
                Ok(RecoveryResult::InstallationFailed(instruction))
            }
        }
        Err(e) => {
            println!("❌ Failed to execute installation: {}", e);
            Ok(RecoveryResult::InstallationFailed(instruction))
        }
    }
}
