use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct AIClient {
    http: Client,
    base_url: String,
    api_key: String,
    user_agent: String,
}

impl AIClient {
    pub fn new(config: &Config) -> Result<Self> {
        let timeout = Duration::from_secs(config.timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: config.api_key.clone(),
            user_agent: format!("li/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    pub async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let req_builder = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/bitrifttech/li")
            .header("X-Title", "li CLI")
            .json(&request);

        let response = req_builder
            .send()
            .await
            .context("Failed to send request to chat completions endpoint")?;

        match response.status() {
            reqwest::StatusCode::OK => {
                response.json::<ChatCompletionResponse>().await
                    .context("Failed to parse chat completion response JSON")
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => {
                let error_text = response.text().await.unwrap_or_default();
                let error_msg = if error_text.contains("per second") {
                    "Rate limit exceeded. Please wait a moment and try again."
                } else if error_text.contains("traffic") {
                    "Service is experiencing high traffic. Please try again in a few moments."
                } else {
                    "Too many requests. Please wait before trying again."
                };
                Err(anyhow!("{} (API response: {})", error_msg, error_text))
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(anyhow!("Invalid API key. Please check your API key configuration."))
            }
            reqwest::StatusCode::BAD_REQUEST => {
                let error_text = response.text().await.unwrap_or_default();
                Err(anyhow!("Invalid request: {}", error_text))
            }
            reqwest::StatusCode::INTERNAL_SERVER_ERROR | reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                Err(anyhow!("Service is temporarily unavailable. Please try again later."))
            }
            status => {
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                Err(anyhow!(
                    "API error (status {}): {}",
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

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

// Re-export for backward compatibility
