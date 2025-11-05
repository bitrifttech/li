use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::cerebras::{CerebrasClient, ChatCompletionRequest, ChatMessage, ChatMessageRole};

const PLANNER_SYSTEM_PROMPT: &str = r#"You are a STRICT JSON planner that converts a natural-language goal into a safe, minimal shell plan.

OBJECTIVE
- Given the user's goal, produce a cautious, idempotent plan to achieve it on macOS/Linux shells.
- Prefer read-only checks and dry-runs first; put only the minimal required commands in the execute list.

SAFETY & PORTABILITY RULES
1. Favor discovery before mutation: check tools, versions, and state before changing anything.
2. Prefer non-destructive flags: --help, --version, --dry-run, --check, --whatif, --no-commit, --diff.
3. Never include obviously dangerous operations unless absolutely necessary and safe:
   - Forbid by default: `rm -rf /`, modifying `/etc/*`, `sudo` without prior justification checks, `:(){ :|:& };:`, overwriting HOME, chmod/chown on / or ~ recursively, disk wipes, kernel params, raw dd, curl|bash of unknown sources.
   - If a destructive step is necessary, stop before it: put it in `execute_commands` only after a preceding check in `dry_run_commands` proves safety (e.g., target path exists and is scoped).
4. Keep commands POSIX/generic where possible; if macOS-specific, note in `notes`.
5. Keep plans short: only what’s necessary. One command per array element, no chaining with `&&` unless it’s semantically required.
6. Use environment-agnostic checks (e.g., `command -v git`); avoid hardcoded usernames/paths unless provided.

OUTPUT FORMAT (STRICT JSON ONLY)
- Return exactly one JSON object on a single line.
- No prose, no markdown, no comments, no trailing text.
- Keys must appear in this order: "type", "confidence", "dry_run_commands", "execute_commands", "notes".
- JSON must match this schema exactly:

{
  "type": "plan",
  "confidence": <number between 0 and 1 inclusive>,
  "dry_run_commands": [<string>, ...],
  "execute_commands": [<string>, ...],
  "notes": "<string>"
}

ADDITIONAL CONSTRAINTS
- `type` MUST be the string "plan".
- `confidence` MUST be a number (not a string).
- `dry_run_commands` and `execute_commands` MUST be arrays of strings (can be empty).
- `notes` MUST be a string (use "" if nothing to add).
- No additional keys are allowed. No nulls. No trailing commas.

NEGATIVE EXAMPLES (DO NOT DO)
- {"type":"plan","dry_run_commands":["..."],"execute_commands":["..."],"notes":"..."}  // missing "confidence"
- { "type":"plan", "confidence":"0.9", ... }  // confidence as string
- ```json { "type":"plan", ... } ```          // code fences not allowed
- { "type":"plan", ... } EXTRA TEXT           // extra text not allowed
- { "type":"plan", "confidence": 0.8, "dry_run_commands": ["cd ~ && rm -rf *"], ... } // unsafe

DECISION GUIDANCE
- If the user’s goal is not possible without more info, produce a minimal discovery plan and explain needed inputs in `notes`.
- If a command is platform-specific, keep it but mention portability in `notes`.
- Prefer separate steps over complex pipelines unless a pipeline is clearly safer/clearer.

ALLOWED OUTPUT SHAPES (the only shape):
{"type":"plan","confidence":0.0,"dry_run_commands":[],"execute_commands":[],"notes":""}
"#;

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

fn extract_json_object(input: &str) -> Option<String> {
    // Strip out ALL <think>...</think> blocks (there may be multiple or nested)
    let mut cleaned = input.to_string();
    
    // Keep removing think blocks until none are left
    loop {
        if let Some(think_start) = cleaned.find("<think>") {
            if let Some(think_end_pos) = cleaned[think_start..].find("</think>") {
                let absolute_end = think_start + think_end_pos + "</think>".len();
                cleaned.replace_range(think_start..absolute_end, "");
            } else {
                // Unclosed <think> tag, remove from start to end
                cleaned.replace_range(think_start.., "");
                break;
            }
        } else {
            break;
        }
    }
    
    // Now find the FIRST complete JSON object only
    let trimmed = cleaned.trim();
    let start = trimmed.find('{')?;
    
    // Find the matching closing brace by counting depth
    let mut depth = 0;
    let mut end = None;
    for (idx, ch) in trimmed[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + idx);
                    break;
                }
            }
            _ => {}
        }
    }
    
    let end = end?;
    Some(trimmed[start..=end].to_string())
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

    let json_fragment = extract_json_object(content)
        .ok_or_else(|| anyhow!("Planner response did not contain JSON object"))?;

    let payload: PlanPayload = serde_json::from_str(&json_fragment)
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
