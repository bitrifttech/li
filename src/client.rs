use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, StatusCode, header::HeaderMap};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};

use crate::config::{LlmProvider, LlmSettings};

const MAX_RETRIES: usize = 3;

#[async_trait]
pub trait LlmClient {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse>;
}

pub type DynLlmClient = dyn LlmClient + Send + Sync;

#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    http: Client,
    base_url: String,
    api_key: String,
    user_agent: String,
    retry_base_delay: Duration,
}

impl OpenRouterClient {
    pub fn new(settings: &LlmSettings) -> Result<Self> {
        if settings.provider != LlmProvider::OpenRouter {
            return Err(anyhow!(
                "Unsupported LLM provider '{}'. Only OpenRouter is currently supported.",
                settings.provider
            ));
        }

        let timeout = Duration::from_secs(settings.timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            base_url: settings.base_url.clone(),
            api_key: settings.api_key.clone(),
            user_agent: settings.user_agent.clone(),
            retry_base_delay: Duration::from_secs(1),
        })
    }

    async fn execute_once(&self, request: &ChatCompletionRequest) -> Result<ResponseOutcome> {
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/bitrifttech/li")
            .header("X-Title", "li CLI")
            .json(request)
            .send()
            .await
            .context("Failed to send request to chat completions endpoint")?;

        match response.status() {
            StatusCode::OK => {
                let body = response
                    .json::<ChatCompletionResponse>()
                    .await
                    .context("Failed to parse chat completion response JSON")?;
                Ok(ResponseOutcome::Success(body))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let wait = parse_retry_after(response.headers()).unwrap_or(self.retry_base_delay);
                let message = response.text().await.unwrap_or_default();
                Ok(ResponseOutcome::Retry(wait, message))
            }
            StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::BAD_GATEWAY
            | StatusCode::GATEWAY_TIMEOUT => {
                let wait = self.retry_base_delay * 2;
                let message = response.text().await.unwrap_or_default();
                Ok(ResponseOutcome::Retry(wait, message))
            }
            StatusCode::UNAUTHORIZED => Err(anyhow!(
                "Invalid API key. Please check your API key configuration."
            )),
            StatusCode::BAD_REQUEST => {
                let error_text = response.text().await.unwrap_or_default();
                Err(anyhow!("Invalid request: {}", error_text))
            }
            status => {
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                Err(anyhow!("API error (status {}): {}", status, error_text))
            }
        }
    }
}

#[async_trait]
impl LlmClient for OpenRouterClient {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.execute_once(&request).await? {
                ResponseOutcome::Success(response) => return Ok(response),
                ResponseOutcome::Retry(delay, message) => {
                    if attempt > MAX_RETRIES {
                        return Err(anyhow!(
                            "OpenRouter request failed after retries: {}",
                            message
                        ));
                    }
                    sleep(delay).await;
                }
            }
        }
    }
}

enum ResponseOutcome {
    Success(ChatCompletionResponse),
    Retry(Duration, String),
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

pub type AIClient = OpenRouterClient;

pub trait LlmClientFactory: Send + Sync {
    fn build(&self, settings: &LlmSettings) -> Result<Arc<DynLlmClient>>;
}

#[derive(Debug, Default, Clone)]
pub struct OpenRouterClientFactory;

impl LlmClientFactory for OpenRouterClientFactory {
    fn build(&self, settings: &LlmSettings) -> Result<Arc<DynLlmClient>> {
        Ok(Arc::new(OpenRouterClient::new(settings)?))
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
