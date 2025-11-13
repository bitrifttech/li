use anyhow::{Context, Result, anyhow};
use colored::*;
use std::io::{self, Write};

use crate::validator::MissingCommand;

use super::types::{RecoveryChoice, RecoveryEngine, RecoveryOptions};

/// Present recovery options to the user and get their choice
pub async fn present_recovery_menu(
    engine: &RecoveryEngine,
    options: &RecoveryOptions,
    missing: &MissingCommand,
) -> Result<RecoveryChoice> {
    println!();
    println!("{}", "🔍 Command Not Found Recovery".bold().yellow());
    println!(
        "The command '{}' is not available on your system.",
        missing.command.bold()
    );

    if !options.command_alternatives.is_empty() {
        println!();
        println!(
            "🤖 AI generated {} alternative solutions:",
            options.command_alternatives.len()
        );

        for (i, alt) in options.command_alternatives.iter().enumerate() {
            println!(
                "  [{}] {} ({:.0}% confidence)",
                (i + 1).to_string().cyan(),
                alt.command.green(),
                alt.confidence
            );
            println!("      {}", alt.to_string());
            println!();
        }
    }

    if !options.installation_instructions.is_empty() {
        println!("📦 Installation options:");

        let base_offset = options.command_alternatives.len();
        for (i, inst) in options.installation_instructions.iter().enumerate() {
            println!(
                "  [{}] Install {} ({})",
                (base_offset + i + 1).to_string().cyan(),
                missing.command.bold(),
                inst.package_managers
                    .first()
                    .unwrap_or(&"unknown".to_string())
                    .dimmed()
            );
            println!("      {}", inst.to_string());
            println!();
        }
    }

    // Add skip, retry, and cancel options
    let total_options =
        options.command_alternatives.len() + options.installation_instructions.len();

    if options.can_skip_step {
        println!(
            "  [{}] Skip this step",
            (total_options + 1).to_string().cyan()
        );
    }

    if options.retry_possible {
        println!("  [retry] Retry original command",);
    }

    println!("  {}", "[abort] Cancel entire plan".red());

    let prompt_options = if options.can_skip_step {
        format!("[1-{}] or [skip/retry/abort]", total_options + 2)
    } else {
        format!("[1-{}] or [retry/abort]", total_options + 1)
    };
    print!("Your choice {}: ", prompt_options);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim();

    parse_user_choice(engine, choice, options, missing)
}

/// Parse and validate user's recovery choice
fn parse_user_choice(
    _engine: &RecoveryEngine,
    choice: &str,
    options: &RecoveryOptions,
    _missing: &MissingCommand,
) -> Result<RecoveryChoice> {
    let choice_lower = choice.trim().to_lowercase();

    // Handle text-based choices
    if choice_lower == "skip" {
        if options.can_skip_step {
            return Ok(RecoveryChoice::SkipStep);
        } else {
            return Err(anyhow!("Skipping not allowed for this critical step"));
        }
    }
    if choice_lower == "retry" && options.retry_possible {
        return Ok(RecoveryChoice::RetryOriginal);
    }
    if choice_lower == "abort" {
        return Ok(RecoveryChoice::AbortPlan);
    }

    // Handle installation commands like "i1", "i2", etc.
    if choice_lower.starts_with('i') {
        let inst_num = choice_lower[1..]
            .parse::<usize>()
            .context("Please enter a valid installation number (e.g., i1, i2)")?;
        if inst_num > 0 && inst_num <= options.installation_instructions.len() {
            return Ok(RecoveryChoice::InstallCommand(inst_num - 1));
        } else {
            return Err(anyhow!(
                "Invalid installation number. Please enter i1-i{}",
                options.installation_instructions.len()
            ));
        }
    }

    // Handle numeric choices
    let choice_num = choice
        .parse::<usize>()
        .context("Please enter a valid number")?;

    let total_options =
        options.command_alternatives.len() + options.installation_instructions.len();

    // Check for command alternatives (1 to N where N = alternatives.len())
    if choice_num > 0 && choice_num <= options.command_alternatives.len() {
        return Ok(RecoveryChoice::UseAlternative(choice_num - 1));
    }

    // Check for installation instructions
    let install_start_idx = options.command_alternatives.len() + 1;
    let install_end_idx =
        options.command_alternatives.len() + options.installation_instructions.len();

    if (install_start_idx..=install_end_idx).contains(&choice_num) {
        let inst_idx = choice_num - install_start_idx;
        return Ok(RecoveryChoice::InstallCommand(inst_idx));
    }

    // Check for skip (only if can_skip_step is true)
    if options.can_skip_step && choice_num == total_options + 1 {
        return Ok(RecoveryChoice::SkipStep);
    }

    // Invalid numeric choice
    let max_choice = if options.can_skip_step {
        total_options + 1
    } else {
        total_options
    };
    Err(anyhow!(
        "Please enter a number between 1 and {}, or use text options (skip/retry/abort)",
        max_choice
    ))
}

impl RecoveryEngine {
    /// Display error message for recovery failures
    pub(super) fn display_recovery_error(&self, error: &str) {
        eprintln!();
        eprintln!("{}", "❌ Recovery Failed".bold().red());
        eprintln!("{}", error);
    }

    /// Display success message for recovery
    pub(super) fn display_recovery_success(&self, message: &str) {
        println!();
        println!("{}", "✅ Recovery Successful".bold().green());
        println!("{}", message);
    }

    /// Display confirmation prompt before executing recovery action
    pub(super) fn confirm_action(&self, action: &str) -> Result<bool> {
        print!("Execute this action? [y/N]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        Ok(input.trim().to_lowercase() == "y")
    }

    /// Display recovery progress indicator
    pub(super) fn show_progress(&self, message: &str) {
        println!("🔄 {}", message);
    }

    /// Display header for recovery session
    pub(super) fn display_recovery_header(&self, missing_command: &str) {
        println!();
        println!("{}", "🚨 Recovery Mode Activated".bold().yellow());
        println!(
            "Attempting to recover from missing command: {}",
            missing_command.bold().red()
        );
        println!();
    }
}
