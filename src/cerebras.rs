use anyhow::Result;
use crate::config::Config;
use crate::client::AIClient;

// Backward compatibility re-exports
pub use crate::client::{
    AIClient as CerebrasClient,
    ChatCompletionRequest,
    ChatMessage,
    ChatMessageRole,
    ChatCompletionResponse,
    ChatChoice,
};

#[deprecated(note = "Use AIClient::new instead")]
pub fn with_base_url(config: &Config, _base_url: &str) -> Result<AIClient> {
    // For backward compatibility, ignore the base_url and use the provider's default
    AIClient::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn sample_config() -> Config {
        Config {
            provider: crate::config::Provider::Cerebras,
            api_key: "test-key".to_string(),
            timeout_secs: 30,
            max_tokens: 2048,
            classifier_model: "llama-3.3-70b".to_string(),
            planner_model: "qwen-3-235b".to_string(),
        }
    }

    #[tokio::test]
    async fn chat_completion_successfully_parses_response() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/v1/chat/completions")
                    .header("Authorization", "Bearer test-key")
                    .json_body(json!({
                        "model": "llama-3.3-70b",
                        "messages": [
                            {"role": "user", "content": "Hello"}
                        ],
                        "max_tokens": 128,
                        "temperature": 0.2
                    }));

                then.status(200)
                    .header("Content-Type", "application/json")
                    .json_body(json!({
                        "choices": [
                            {
                                "index": 0,
                                "finish_reason": "stop",
                                "message": {
                                    "role": "assistant",
                                    "content": "Hi there!",
                                    "reasoning": null
                                }
                            }
                        ],
                        "usage": {
                            "prompt_tokens": 12,
                            "completion_tokens": 10,
                            "total_tokens": 22
                        }
                    }));
            })
            .await;

        let config = sample_config();
        let client = CerebrasClient::with_base_url(&config, server.base_url()).unwrap();

        let response = client
            .chat_completion(ChatCompletionRequest {
                model: "llama-3.3-70b".into(),
                messages: vec![ChatMessage {
                    role: ChatMessageRole::User,
                    content: "Hello".into(),
                }],
                max_tokens: Some(128),
                temperature: Some(0.2),
            })
            .await
            .unwrap();

        assert_eq!(response.choices.len(), 1);
        let choice = &response.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("stop"));
        assert_eq!(choice.message.content, "Hi there!");
        assert_eq!(choice.message.reasoning, None);
        assert!(response.usage.is_some());

        _mock.assert_async().await;
    }

    #[tokio::test]
    async fn chat_completion_returns_error_for_http_failure() {
        let server = MockServer::start_async().await;

        let _mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(401)
                    .header("Content-Type", "application/json")
                    .body(r#"{"error":"invalid_api_key"}"#);
            })
            .await;

        let config = sample_config();
        let client = CerebrasClient::with_base_url(&config, server.base_url()).unwrap();

        let err = client
            .chat_completion(ChatCompletionRequest {
                model: "llama-3.3-70b".into(),
                messages: vec![ChatMessage {
                    role: ChatMessageRole::User,
                    content: "Hello".into(),
                }],
                max_tokens: None,
                temperature: None,
            })
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Cerebras API error"));

        _mock.assert_async().await;
    }
}
