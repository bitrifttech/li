use anyhow::{Result, anyhow};
use std::io::{self, Write};

use crate::client::DynLlmClient;

use super::transport::call_planner_with_context;
use super::types::{Plan, PlannerResponse, QuestionResolver};

pub(crate) fn default_question_resolver(question: &str, context: &str) -> Result<String> {
    println!("\n🤔 Planner asks: {}", question);
    if !context.trim().is_empty() {
        println!("Context: {}", context);
    }
    print!("Your answer (or 'skip' to cancel): ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_string())
}

pub(crate) async fn interactive_plan_with_resolver(
    client: &DynLlmClient,
    initial_request: &str,
    model: &str,
    max_tokens: u32,
    resolver: &QuestionResolver,
) -> Result<Plan> {
    let mut context = initial_request.to_string();
    let mut conversation: Vec<(String, String)> = vec![];

    loop {
        let response =
            call_planner_with_context(client, &context, &conversation, model, max_tokens).await?;

        match response {
            PlannerResponse::Plan {
                confidence,
                dry_run_commands,
                execute_commands,
                notes,
            } => {
                return Ok(Plan {
                    confidence,
                    dry_run_commands,
                    execute_commands,
                    notes,
                });
            }
            PlannerResponse::Question { text, context: ctx } => {
                let answer = resolver(&text, &ctx)?;
                if answer.trim().eq_ignore_ascii_case("skip") {
                    return Err(anyhow!("Planning cancelled by user"));
                }

                conversation.push(("question".to_string(), text));
                conversation.push(("answer".to_string(), answer.clone()));

                context = format!(
                    "{}\n\nPrevious Q&A:\n{}\nUser answered: {}",
                    ctx,
                    conversation
                        .iter()
                        .rev()
                        .take(2)
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    answer
                );
            }
        }
    }
}
