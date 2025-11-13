use anyhow::{Context, Result, bail};
use std::io::{self, Write};

use crate::agent::{AgentOrchestrator, AgentOutcome, AgentRequest, StageKind};
use crate::client::AIClient;
use crate::config::Config;
use crate::planner;
use crate::recovery::{RecoveryContext, RecoveryEngine, RecoveryResult, RecoveryStrategy};
use crate::validator::ValidationResult;

use super::intelligence::explain_plan_output;

pub(crate) async fn handle_task(words: Vec<String>, config: &Config) -> Result<()> {
    let prompt = words.join(" ").trim().to_owned();
    if prompt.is_empty() {
        println!(
            "li CLI is initialized. Provide a task or run `li --chat \"your question\"` to call your configured provider."
        );
        return Ok(());
    }

    let orchestrator = AgentOrchestrator::default();
    let request = AgentRequest::new(prompt.clone());

    let run = orchestrator
        .run(config.clone(), request)
        .await
        .context("Agent pipeline failed")?;

    match run.outcome {
        AgentOutcome::Planned {
            plan: Some(plan),
            validation,
            ..
        } => {
            if let Some(validation) = validation.clone() {
                let proceed =
                    resolve_validation_issues(&validation, &plan, config, &prompt).await?;
                if !proceed {
                    return Ok(());
                }
            }

            render_plan(&plan, config);

            match prompt_for_approval()? {
                ApprovalResponse::Yes => {
                    crate::exec::execute_plan(&plan).await?;
                }
                ApprovalResponse::YesWithIntelligence => {
                    let output = crate::exec::execute_plan_with_capture(&plan).await?;
                    let client = AIClient::new(&config.llm)?;
                    explain_plan_output(&client, config, &plan, &output).await?;
                }
                ApprovalResponse::No => {
                    println!("\nPlan execution cancelled.");
                }
            }

            Ok(())
        }
        AgentOutcome::Planned { plan: None, .. } => {
            bail!("Agent pipeline returned no plan");
        }
        AgentOutcome::AwaitingClarification { question, context } => {
            println!("The agent needs more information before proceeding.");
            println!("Question: {}", question);
            if !context.trim().is_empty() {
                println!("Context: {}", context);
            }
            Ok(())
        }
        AgentOutcome::Cancelled { reason } => {
            println!("Agent cancelled the request: {}", reason);
            Ok(())
        }
        AgentOutcome::Failed { stage, error } => {
            let guidance = match stage {
                StageKind::Planning => format!(
                    "Verify your {} API key (set {} or run 'li --setup') and ensure you have internet connectivity. Retry if the service is rate limited.",
                    config.llm.provider.display_name(),
                    config.llm.provider.api_key_env_var()
                ),
                StageKind::Validation => "Inspect the validator warnings above for missing tools before rerunning the command.".to_string(),
                StageKind::Execution => {
                    "Review the command output above for failures before retrying.".to_string()
                }
                StageKind::Recovery => {
                    "Recovery cancelled. Resolve tool installation manually or re-run with recovery enabled.".to_string()
                }
            };
            bail!("Agent stage {} failed: {}. {}", stage, error, guidance);
        }
    }
}

async fn resolve_validation_issues(
    validation: &ValidationResult,
    plan: &planner::Plan,
    config: &Config,
    goal: &str,
) -> Result<bool> {
    if validation.missing_commands.is_empty() {
        return Ok(true);
    }

    let count = validation.missing_commands.len();
    println!(
        "⚠️  Validator identified {} missing command{}:",
        count,
        if count == 1 { "" } else { "s" }
    );
    for missing in &validation.missing_commands {
        let phase = if missing.is_dry_run {
            "dry-run"
        } else {
            "execute"
        };
        println!(
            "   • {} (step {}: {})",
            missing.command,
            missing.plan_step + 1,
            phase
        );
    }

    if validation.plan_can_continue {
        println!(
            "Plan can continue, but results may be degraded until the missing tool{} {} installed.",
            if count == 1 { "" } else { "s are" },
            if count == 1 { "is" } else { "are" }
        );
        return Ok(true);
    }

    println!("Plan cannot continue until the missing commands are addressed.");

    if !config.recovery.enabled {
        println!("Recovery is disabled in your configuration. Enable it to receive guided fixes.");
        return Ok(false);
    }

    let mut engine = RecoveryEngine::new(config)?;
    engine.set_available_tools().await?;
    let mut any_success = false;

    for missing in &validation.missing_commands {
        loop {
            let strategy = prompt_recovery_strategy()?;
            if matches!(strategy, RecoveryStrategy::NeverRecover) {
                println!(
                    "Recovery cancelled. Resolve missing tools manually and rerun the command."
                );
                return Ok(false);
            }

            let options = engine
                .generate_recovery_options(strategy, missing, plan, goal)
                .await?;

            if options.command_alternatives.is_empty()
                && options.installation_instructions.is_empty()
                && !options.can_skip_step
            {
                println!(
                    "No automated recovery options available for '{}'.",
                    missing.command
                );
                continue;
            }

            let choice = engine.present_recovery_menu(&options, missing).await?;
            let context = RecoveryContext {
                missing_command: missing.clone(),
                original_plan: plan.clone(),
                original_goal: goal.to_string(),
            };

            match engine.execute_recovery(choice, &context, &options).await? {
                RecoveryResult::AlternativeSucceeded(alt) => {
                    println!("✅ Alternative executed: {}", alt.command);
                    any_success = true;
                    break;
                }
                RecoveryResult::InstallationSucceeded(inst) => {
                    println!("✅ Installation succeeded: {}", inst.command);
                    any_success = true;
                    break;
                }
                RecoveryResult::InstallationCancelled => {
                    println!("Installation cancelled. Re-run the command when ready.");
                    return Ok(false);
                }
                RecoveryResult::PlanAborted(reason) => {
                    println!("Plan aborted: {}", reason);
                    return Ok(false);
                }
                RecoveryResult::AlternativeFailed(_) | RecoveryResult::InstallationFailed(_) => {
                    println!("Recovery attempt did not succeed. Try another option.");
                }
                RecoveryResult::StepSkipped => {
                    println!("Recovery step skipped.");
                    break;
                }
                RecoveryResult::RetryRequested | RecoveryResult::RetryWithDifferentApproach => {
                    println!("Retry requested. Re-run the command after addressing the prompt.");
                    return Ok(false);
                }
            }
        }
    }

    if any_success {
        println!("Re-run your original command to take advantage of the recovery steps.");
    }

    Ok(false)
}

fn render_plan(plan: &planner::Plan, config: &Config) {
    println!("\n=== Proposed Plan ===");
    println!("Provider: {}", config.llm.provider.display_name());
    println!("Planner Model: {}", config.models.planner);
    println!("Plan confidence: {:.2}", plan.confidence);

    if !plan.dry_run_commands.is_empty() {
        println!("\nDry-run Commands:");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.execute_commands.is_empty() {
        println!("\nExecute Commands:");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.notes.trim().is_empty() {
        println!("\nNotes: {}", plan.notes.trim());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalResponse {
    Yes,
    YesWithIntelligence,
    No,
}

fn prompt_for_approval() -> Result<ApprovalResponse> {
    print!("\nExecute this plan? [y/N/i]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();
    Ok(match answer.as_str() {
        "y" | "yes" => ApprovalResponse::Yes,
        "i" | "intelligence" => ApprovalResponse::YesWithIntelligence,
        _ => ApprovalResponse::No,
    })
}

fn prompt_recovery_strategy() -> Result<RecoveryStrategy> {
    println!("\nChoose a recovery approach:");
    println!("  1) Look at alternate commands / install missing command");
    println!("  2) Skip this missing step and continue");
    println!("  3) Cancel recovery and exit");

    loop {
        print!("Selection [1-3]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        match trimmed {
            "1" => return Ok(RecoveryStrategy::InstallationFirst),
            "2" => return Ok(RecoveryStrategy::SkipOnError),
            "3" => return Ok(RecoveryStrategy::NeverRecover),
            _ => println!("❌ Please enter a number between 1 and 3."),
        }
    }
}
