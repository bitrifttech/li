use anyhow::{Context, Result, anyhow};

use crate::client::{ChatCompletionRequest, ChatMessage, ChatMessageRole, DynLlmClient};
use crate::tokens::compute_completion_token_budget;

use super::parsing::extract_json_object;
use super::prompt::PLANNER_SYSTEM_PROMPT;
use super::types::PlannerResponse;

pub(crate) async fn call_planner_with_context(
    client: &DynLlmClient,
    request: &str,
    conversation: &[(String, String)],
    model: &str,
    max_tokens: u32,
) -> Result<PlannerResponse> {
    let mut messages = vec![ChatMessage {
        role: ChatMessageRole::System,
        content: PLANNER_SYSTEM_PROMPT.to_string(),
    }];

    for (role, content) in conversation {
        let message_role = if role == "question" {
            ChatMessageRole::Assistant
        } else {
            ChatMessageRole::User
        };
        messages.push(ChatMessage {
            role: message_role,
            content: content.clone(),
        });
    }

    messages.push(ChatMessage {
        role: ChatMessageRole::User,
        content: request.to_string(),
    });

    let completion_budget = compute_completion_token_budget(max_tokens, &messages);

    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        max_tokens: Some(completion_budget),
        temperature: Some(0.0),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("OpenRouter planner call failed")?;

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("OpenRouter planner returned no choices"))?;

    let content = choice.message.content.trim();
    if content.is_empty() {
        return Err(anyhow!("Planner response was empty"));
    }

    let json_fragment = extract_json_object(content)
        .ok_or_else(|| anyhow!("Planner response did not contain JSON object"))?;

    let response: PlannerResponse = serde_json::from_str(&json_fragment)
        .with_context(|| format!("Failed to parse planner JSON: {content}"))?;

    Ok(response)
}
