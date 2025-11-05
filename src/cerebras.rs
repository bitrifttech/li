use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;


#[derive(Debug, Clone)]
pub struct CerebrasClient {
    http: Client,
    base_url: String,
    api_key: String,
    user_agent: String,
}

impl CerebrasClient {
    #[allow(dead_code)] // Keep for potential future use
    pub fn new(config: &Config) -> Result<Self> {
        Self::new_with_url(
            config.cerebras_api_key.clone(),
            config.timeout_secs,
            "https://api.cerebras.ai",
        )
    }

    pub fn new_with_url(api_key: String, timeout_secs: u64, base_url: impl Into<String>) -> Result<Self> {
        let sanitized_base = base_url.into().trim_end_matches('/').to_string();
        if sanitized_base.is_empty() {
            return Err(anyhow!("Base URL cannot be empty"));
        }

        let timeout = Duration::from_secs(timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build Cerebras HTTP client")?;

        Ok(Self {
            http,
            base_url: sanitized_base,
            api_key,
            user_agent: format!("li/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    pub async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .header("User-Agent", &self.user_agent)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Cerebras chat completions endpoint")?;

        match response.status() {
            reqwest::StatusCode::OK => {
                response.json::<ChatCompletionResponse>().await
                    .context("Failed to parse Cerebras chat completion response JSON")
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let error_text = response.text().await.unwrap_or_default();
                let error_msg = if error_text.contains("per second") {
                    "Rate limit exceeded. Please wait a moment and try again."
                } else if error_text.contains("traffic") {
                    "Cerebras is experiencing high traffic. Please try again in a few moments."
                } else {
                    "Too many requests. Please wait before trying again."
                };
                Err(anyhow!("{} (API response: {})", error_msg, error_text))
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(anyhow!("Invalid API key. Please check your Cerebras API key configuration."))
            }
            reqwest::StatusCode::BAD_REQUEST => {
                let error_text = response.text().await.unwrap_or_default();
                Err(anyhow!("Invalid request: {}", error_text))
            }
            reqwest::StatusCode::INTERNAL_SERVER_ERROR | reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                Err(anyhow!("Cerebras service is temporarily unavailable. Please try again later."))
            }
            status => {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(anyhow!(
                    "Cerebras API error (status {}): {}",
                    status,
                    error_text
                ))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChoice {
    pub message: ChatCompletionMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionMessage {
    pub content: String,
    #[serde(default)]
    pub reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;

    fn sample_config() -> Config {
        Config {
            cerebras_api_key: "test-key".to_string(),
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
