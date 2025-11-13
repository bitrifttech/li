use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, DynLlmClient};
use crate::config::{Config, LlmProvider, LlmSettings, ModelSettings, RecoverySettings};

const CLASSIFIER_SYSTEM_PROMPT: &str = r#"You are a STRICT JSON classifier for a shell assistant.

TASK
- Classify the user’s input as either natural language (NL) or a terminal command (TERMINAL).

DECISION RULES
- Output NL unless the input is an actually executable shell command as-is.
- Examples of TERMINAL: starts with a known command or builtin (e.g., cd, ls, git, grep, cat, mkdir, rm, mv, cp, ssh, curl, wget, docker, kubectl, python, node, npm, yarn, pnpm, go, cargo, brew, tar, unzip, chmod, chown, echo, export, set, alias), or starts with ./, /, ~/, or a path plus flags; includes typical command syntax like flags (-, --), pipes (|), redirections (> >> 2>), subshells ($()), operators (&& || ;), or shebang lines.
- If the text is a fragment like “home directory”, “make a repo”, “how do I…”, “list files”, it is NL.

OUTPUT FORMAT (MUST FOLLOW EXACTLY)
- Return a single JSON object with exactly one key "type" and a value that is exactly "NL" or "TERMINAL".
- No other keys, no explanations, no commands, no trailing text.
- Output must be a single line, no leading/trailing spaces.

ALLOWED OUTPUTS (the only two):
{"type":"NL"}
{"type":"TERMINAL"}

NEGATIVE EXAMPLES (DO NOT DO)
- {"type":"NL","command":"cd ~"}
- {"result":"NL"}
- type: NL
- NL
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Terminal,
    NaturalLanguage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassificationPayload {
    #[serde(rename = "type")]
    classification_type: String,
}

pub async fn classify(client: &DynLlmClient, input: &str, model: &str) -> Result<Classification> {
    let trimmed_input = input.trim();
    if trimmed_input.is_empty() {
        return Err(anyhow!("Cannot classify empty input"));
    }

    let request = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: ChatMessageRole::System,
                content: CLASSIFIER_SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatMessageRole::User,
                content: trimmed_input.to_string(),
            },
        ],
        max_tokens: Some(16),
        temperature: Some(0.0),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Classifier LLM call failed")?;

    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Classifier response contained no choices"))?;

    let content = choice.message.content.trim();
    if content.is_empty() {
        return Err(anyhow!("Classifier response was empty"));
    }

    let payload: ClassificationPayload = serde_json::from_str(content)
        .with_context(|| format!("Failed to parse classifier JSON: {content}"))?;

    match payload.classification_type.trim().to_uppercase().as_str() {
        "NL" => Ok(Classification::NaturalLanguage),
        "TERMINAL" => Ok(Classification::Terminal),
        other => Err(anyhow!("Unexpected classifier result: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    use crate::config::DEFAULT_MAX_TOKENS;

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
                max_tokens: DEFAULT_MAX_TOKENS,
            },
            recovery: RecoverySettings::default(),
        }
    }

    fn expected_request_body(user_input: &str) -> serde_json::Value {
        json!({
            "model": "nvidia/nemotron-nano-12b-v2-vl:free",
            "messages": [
                {
                    "role": "system",
                    "content": CLASSIFIER_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": user_input
                }
            ],
            "max_tokens": 16,
            "temperature": 0.0
        })
    }

    #[tokio::test]
    async fn classify_returns_terminal() {
        let server = MockServer::start_async().await;
        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key")
                    .json_body(expected_request_body("git status"));

                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "index": 0,
                            "finish_reason": "stop",
                            "message": {
                                "role": "assistant",
                                "content": "{\"type\":\"TERMINAL\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        let classification = classify(&client, "git status", &config.models.classifier)
            .await
            .unwrap();

        assert_eq!(classification, Classification::Terminal);
        _mock.assert_async().await;
    }

    #[tokio::test]
    async fn classify_returns_natural_language() {
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
                                "content": "{\"type\":\"NL\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        let classification = classify(&client, "make a new git repo", &config.models.classifier)
            .await
            .unwrap();

        assert_eq!(classification, Classification::NaturalLanguage);
        _mock.assert_async().await;
    }

    #[tokio::test]
    async fn classify_errors_on_malformed_response() {
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
                                "content": "{\"unexpected\":\"value\"}"
                            }
                        }
                    ]
                }));
            })
            .await;

        let mut config = sample_config();
        config.llm.base_url = server.url("/v1");
        let client = AIClient::new(&config.llm).unwrap();

        let err = classify(&client, "git status", &config.models.classifier)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Failed to parse classifier JSON"));
        _mock.assert_async().await;
    }
}
