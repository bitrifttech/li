use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::cerebras::{CerebrasClient, ChatCompletionRequest, ChatMessage, ChatMessageRole};

const PLANNER_SYSTEM_PROMPT: &str = r#"You convert plain English into a safe, minimal shell plan.

RULES
1. Prefer dry-run commands and idempotent checks first.
2. Avoid destructive operations unless preceded by a safety probe.
3. Keep commands portable for macOS/Linux where possible.
4. Output only valid JSON matching the provided schema—no extra fields, no comments, no prose.
5. If the task requires coding or unsupported behaviour, set notes to explain and produce a minimal safe plan that stops.

SCHEMA (STRICT)
{
  "type": "plan",
  "confidence": number between 0 and 1,
  "dry_run_commands": string array,
  "execute_commands": string array,
  "notes": string
}"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub confidence: f32,
    pub dry_run_commands: Vec<String>,
    pub execute_commands: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanPayload {
    #[serde(rename = "type")]
    plan_type: String,
    confidence: f32,
    dry_run_commands: Vec<String>,
    execute_commands: Vec<String>,
    notes: String,
}

pub async fn plan(
    client: &CerebrasClient,
    request: &str,
    model: &str,
    max_tokens: u32,
) -> Result<Plan> {
    let trimmed_request = request.trim();
    if trimmed_request.is_empty() {
        return Err(anyhow!("Cannot plan for empty input"));
    }

    let chat_request = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: ChatMessageRole::System,
                content: PLANNER_SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatMessageRole::User,
                content: trimmed_request.to_string(),
            },
        ],
        max_tokens: Some(max_tokens),
        temperature: Some(0.2),
    };

    let response = client
        .chat_completion(chat_request)
        .await
        .context("Cerebras planner call failed")?;

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Cerebras planner returned no choices"))?;

    let content = choice.message.content.trim();
    if content.is_empty() {
        return Err(anyhow!("Planner response was empty"));
    }

    let payload: PlanPayload = serde_json::from_str(content)
        .with_context(|| format!("Failed to parse planner JSON: {content}"))?;

    if payload.plan_type != "plan" {
        return Err(anyhow!("Unexpected planner type: {}", payload.plan_type));
    }

    Ok(Plan {
        confidence: payload.confidence,
        dry_run_commands: payload.dry_run_commands,
        execute_commands: payload.execute_commands,
        notes: payload.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    use crate::config::Config;

    fn sample_config() -> Config {
        Config {
            cerebras_api_key: "test-key".to_string(),
            timeout_secs: 30,
            max_tokens: 512,
            classifier_model: "llama-3.3-70b".to_string(),
            planner_model: "qwen-3-235b".to_string(),
        }
    }

    fn expected_request_body(user_input: &str) -> serde_json::Value {
        json!({
            "model": "qwen-3-235b",
            "messages": [
                {
                    "role": "system",
                    "content": PLANNER_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": user_input
                }
            ],
            "max_tokens": 512,
            "temperature": 0.2
        })
    }

    #[tokio::test]
    async fn plan_parses_valid_response() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key")
                    .json_body(expected_request_body("make a new git repo"));

                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {
                                "role": "assistant",
                                "content": "{\"type\":\"plan\",\"confidence\":0.82,\"dry_run_commands\":[\"git status\"],\"execute_commands\":[\"git init\",\"git add .\",\"git commit -m \\\"Initial commit\\\"\"],\"notes\":\"Created minimal git repo with initial commit.\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let config = sample_config();
        let client = CerebrasClient::with_base_url(&config, server.base_url()).unwrap();

        let plan = plan(
            &client,
            "make a new git repo",
            &config.planner_model,
            config.max_tokens,
        )
        .await
        .unwrap();

        assert!((plan.confidence - 0.82).abs() < f32::EPSILON);
        assert_eq!(plan.dry_run_commands, vec!["git status".to_string()]);
        assert_eq!(
            plan.execute_commands,
            vec![
                "git init".to_string(),
                "git add .".to_string(),
                "git commit -m \"Initial commit\"".to_string()
            ]
        );
        assert_eq!(plan.notes, "Created minimal git repo with initial commit.");

        _mock.assert_async().await;
    }

    #[tokio::test]
    async fn plan_errors_on_invalid_json() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key");

                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {
                                "role": "assistant",
                                "content": "{\"type\":\"not_plan\",\"confidence\":0.5,\"dry_run_commands\":[],\"execute_commands\":[],\"notes\":\"Invalid type\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let config = sample_config();
        let client = CerebrasClient::with_base_url(&config, server.base_url()).unwrap();

        let err = plan(
            &client,
            "make a new git repo",
            &config.planner_model,
            config.max_tokens,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("Unexpected planner type"));
        _mock.assert_async().await;
    }

    #[tokio::test]
    async fn plan_errors_on_missing_fields() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key");

                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {
                                "role": "assistant",
                                "content": "{}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let config = sample_config();
        let client = CerebrasClient::with_base_url(&config, server.base_url()).unwrap();

        let err = plan(
            &client,
            "make a new git repo",
            &config.planner_model,
            config.max_tokens,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("Failed to parse planner JSON"));
        _mock.assert_async().await;
    }
}
