use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, DynLlmClient};
use crate::tokens::compute_completion_token_budget;

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
- Use tagged union with "type" field to distinguish responses:

For a complete plan:
{
  "type": "plan",
  "confidence": <number between 0 and 1 inclusive>,
  "dry_run_commands": [<string>, ...],
  "execute_commands": [<string>, ...],
  "notes": "<string>"
}

For a clarifying question:
{
  "type": "question",
  "text": "<specific question to user>",
  "context": "<brief description of what we're trying to accomplish>"
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
- If the user's goal lacks essential information, ask a specific question instead of generating a partial plan.
- Examples: "create a remote repo" → ask for server/path; "deploy my app" → ask for target platform.
- Only ask questions when the missing information is essential for safety or correctness.
- If you can make reasonable assumptions, proceed with the plan and note assumptions in `notes`.
- If a command is platform-specific, keep it but mention portability in `notes`.
- Prefer separate steps over complex pipelines unless a pipeline is clearly safer/clearer.

ALLOWED OUTPUT SHAPES (the only two shapes):
{"type":"plan","confidence":0.0,"dry_run_commands":[],"execute_commands":[],"notes":""}
{"type":"question","text":"What server should I use?","context":"Creating a remote git repository"}
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub confidence: f32,
    pub dry_run_commands: Vec<String>,
    pub execute_commands: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum PlannerResponse {
    Plan {
        confidence: f32,
        dry_run_commands: Vec<String>,
        execute_commands: Vec<String>,
        notes: String,
    },
    Question {
        text: String,
        context: String,
    },
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

pub async fn interactive_plan(
    client: &DynLlmClient,
    initial_request: &str,
    model: &str,
    max_tokens: u32,
) -> Result<Plan> {
    use std::io::{self, Write};

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
                println!("\n🤔 Planner asks: {}", text);
                print!("Your answer (or 'skip' to cancel): ");
                io::stdout().flush()?;

                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                answer = answer.trim().to_string();

                if answer.to_lowercase() == "skip" {
                    return Err(anyhow!("Planning cancelled by user"));
                }

                // Add Q&A to conversation context
                conversation.push(("question".to_string(), text));
                conversation.push(("answer".to_string(), answer.clone()));

                // Update context with new information
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

async fn call_planner_with_context(
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

    // Add conversation history if any
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

    // Add current request
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

pub async fn plan(
    client: &DynLlmClient,
    request: &str,
    model: &str,
    max_tokens: u32,
) -> Result<Plan> {
    interactive_plan(client, request, model, max_tokens).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    use crate::{
        config::{Config, LlmProvider, LlmSettings, ModelSettings, RecoverySettings},
        tokens::compute_completion_token_budget,
    };

    fn sample_config() -> Config {
        Config {
            llm: LlmSettings {
                provider: LlmProvider::OpenRouter,
                api_key: "test-key".to_string(),
                timeout_secs: 30,
                base_url: "https://openrouter.ai/api/v1".to_string(),
                user_agent: "li/test".to_string(),
            },
            models: ModelSettings {
                classifier: "nvidia/nemotron-nano-12b-v2-vl:free".to_string(),
                planner: "minimax/minimax-m2:free".to_string(),
                max_tokens: 512,
            },
            recovery: RecoverySettings::default(),
        }
    }

    fn expected_request_body(user_input: &str) -> serde_json::Value {
        let messages = vec![
            ChatMessage {
                role: ChatMessageRole::System,
                content: PLANNER_SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatMessageRole::User,
                content: user_input.to_string(),
            },
        ];
        let max_tokens = compute_completion_token_budget(512, &messages);

        json!({
            "model": "minimax/minimax-m2:free",
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
            "max_tokens": max_tokens,
            "temperature": 0.0
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

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        let plan = plan(
            &client,
            "make a new git repo",
            &config.models.planner,
            config.models.max_tokens,
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
    async fn plan_handles_question_response() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key")
                    .json_body(expected_request_body("create a remote git repo"));

                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {
                                "role": "assistant",
                                "content": "{\"type\":\"question\",\"text\":\"What server should I use for the remote repository?\",\"context\":\"Creating a remote git repository\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        // This should fail because interactive planning needs user input
        let result = plan(
            &client,
            "create a remote git repo",
            &config.models.planner,
            config.models.max_tokens,
        )
        .await;

        // Should fail due to stdin not being available in test
        assert!(result.is_err());
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

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        let err = plan(
            &client,
            "make a new git repo",
            &config.models.planner,
            config.models.max_tokens,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("Failed to parse planner JSON"));
        _mock.assert_async().await;
    }
}
