use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, LlmClient};
use crate::config::Config;
use crate::planner;
use crate::tokens::compute_completion_token_budget;

pub(crate) async fn handle_intelligence(
    question_flag: Option<String>,
    task: Vec<String>,
    piped_input: Option<String>,
    config: &Config,
) -> Result<()> {
    // Determine question and command inputs
    let mut question = question_flag
        .map(|q| q.trim().to_owned())
        .filter(|q| !q.is_empty());

    let command_candidate = if task.is_empty() {
        String::new()
    } else if question.is_some() || task.len() == 1 {
        task.join(" ").trim().to_owned()
    } else {
        let potential_command = task.last().unwrap().trim().to_owned();
        let potential_question = task[..task.len() - 1].join(" ").trim().to_owned();

        if potential_question.is_empty() {
            task.join(" ").trim().to_owned()
        } else {
            let looks_like_question =
                potential_question.ends_with('?') || potential_question.contains('?');
            let command_has_whitespace = potential_command.contains(char::is_whitespace);
            let command_starts_with_flag = potential_command.starts_with('-');

            if looks_like_question || command_has_whitespace {
                question = Some(potential_question);
                potential_command
            } else if command_starts_with_flag {
                task.join(" ").trim().to_owned()
            } else {
                task.join(" ").trim().to_owned()
            }
        }
    };

    let mut piped_input = piped_input;
    let piped_was_present = piped_input.is_some();
    if let Some(ref data) = piped_input {
        if data.trim().is_empty() {
            piped_input = None;
        }
    }

    if piped_input.is_none() && command_candidate.is_empty() {
        if piped_was_present {
            bail!(
                "Piped input was empty. Provide command output to analyze or specify a command to run."
            );
        } else {
            bail!("Intelligence mode requires a command to execute and explain");
        }
    }

    println!("🧠 AI Intelligence Mode");

    let command_display = if !command_candidate.is_empty() {
        command_candidate.clone()
    } else {
        "stdin (piped input)".to_string()
    };

    let (stdout, stderr) = if let Some(piped) = piped_input {
        if task.is_empty() {
            println!("🔧 Analyzing piped input from stdin");
        } else {
            println!("🔧 Analyzing piped input for '{}'", command_display);
        }
        println!();

        (piped, String::new())
    } else {
        println!("🔧 Executing: {}", command_display);
        println!();

        let output = Command::new("sh")
            .arg("-c")
            .arg(&command_candidate)
            .output()
            .context("Failed to execute command")?;

        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    };

    if !stdout.trim().is_empty() {
        println!("📤 Command Output:");
        println!("{}", stdout);
    }

    if !stderr.trim().is_empty() {
        println!("⚠️  Error Output:");
        println!("{}", stderr);
    }

    println!();

    let base_output = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };

    if base_output.trim().is_empty() {
        bail!("No output available for intelligence analysis");
    }

    let explanation_prompt = if let Some(question) = question {
        format!(
            "A user asked the following question about a command they ran:\n\
            Question: {}\n\
            Command: '{}'\n\
            Output:\n{}\n\
            Please answer the question directly, referencing the command output.\n\
            Include any helpful context, summaries, and actionable insights the user should know.",
            question, command_display, base_output
        )
    } else {
        format!(
            "Please explain the following command output in a clear, human-friendly way.\n\
            The command executed was: '{}'\n\n\
            Output:\n{}\n\
            Please provide:\n\
            1. What this output means in simple terms\n\
            2. Key insights or important information\n\
            3. Any warnings or things to pay attention to\n\
            4. What a user should understand from this result\n\
            Keep the explanation conversational and easy to understand for someone who might not be familiar with this command.",
            command_display, base_output
        )
    };

    println!("🤖 AI Explanation:");
    println!();

    let client = AIClient::new(&config.llm)?;

    let request = ChatCompletionRequest {
        model: config.models.planner.clone(),
        messages: vec![ChatMessage {
            role: ChatMessageRole::User,
            content: explanation_prompt,
        }],
        max_tokens: Some(config.models.max_tokens),
        temperature: Some(0.7),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Failed to get AI explanation")?;

    if let Some(choice) = response.choices.first() {
        println!("{}", choice.message.content);
    }

    println!();

    Ok(())
}

pub(crate) async fn explain_plan_output(
    client: &AIClient,
    config: &Config,
    plan: &planner::Plan,
    output: &str,
) -> Result<()> {
    println!("\n🤖 AI Intelligence Explanation:");
    println!();

    let commands_summary = {
        let mut summary = String::new();
        if !plan.dry_run_commands.is_empty() {
            summary.push_str("Dry-run Commands:\n");
            for cmd in &plan.dry_run_commands {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }
        if !plan.execute_commands.is_empty() {
            summary.push_str("Execute Commands:\n");
            for cmd in &plan.execute_commands {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }
        summary
    };

    let explanation_prompt = format!(
        "Please explain the following command execution results in a clear, human-friendly way.\n\n\
        The plan that was executed:\n{}\n\
        Plan Notes: {}\n\n\
        Command Output:\n{}\n\n\
        Please provide:\n\
        1. What this output means in simple terms\n\
        2. Key insights or important information from the results\n\
        3. Any warnings or things to pay attention to\n\
        4. Whether the plan achieved its intended goal\n\
        5. Any follow-up actions the user might need to take\n\n\
        Keep the explanation conversational and easy to understand.",
        commands_summary, plan.notes, output
    );

    let messages = vec![ChatMessage {
        role: ChatMessageRole::User,
        content: explanation_prompt,
    }];

    let completion_budget = compute_completion_token_budget(config.models.max_tokens, &messages);

    let request = ChatCompletionRequest {
        model: config.models.planner.clone(),
        messages,
        max_tokens: Some(completion_budget),
        temperature: Some(0.7),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Failed to get AI explanation")?;

    if let Some(choice) = response.choices.first() {
        println!("{}", choice.message.content);
    }

    println!();

    Ok(())
}
