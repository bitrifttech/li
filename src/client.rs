use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, StatusCode, header::HeaderMap};
use serde::de::{self, Deserializer};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};

use crate::config::{LlmProvider, LlmSettings};

const MAX_RETRIES: usize = 3;

static VERBOSE_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_verbose_logging(enabled: bool) {
    VERBOSE_LOGGING.store(enabled, Ordering::Relaxed);
    if enabled {
        verbose_log("Verbose logging enabled");
    }
}

fn is_verbose() -> bool {
    VERBOSE_LOGGING.load(Ordering::Relaxed)
}

pub(crate) fn verbose_log(message: impl AsRef<str>) {
    if is_verbose() {
        println!("[verbose] {}", message.as_ref());
    }
}

#[async_trait]
pub trait LlmClient {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse>;
}

pub type DynLlmClient = dyn LlmClient + Send + Sync;

#[derive(Debug, Clone)]
pub struct ProviderClient {
    http: Client,
    provider: LlmProvider,
    base_url: String,
    api_key: String,
    user_agent: String,
    retry_base_delay: Duration,
}

impl ProviderClient {
    pub fn new(settings: &LlmSettings) -> Result<Self> {
        let timeout = Duration::from_secs(settings.timeout_secs);
        let http = Client::builder()
            .timeout(timeout)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            provider: settings.provider,
            base_url: settings.base_url.clone(),
            api_key: settings.api_key.clone(),
            user_agent: settings.user_agent.clone(),
            retry_base_delay: Duration::from_secs(1),
        })
    }

    async fn execute_once(&self, request: &ChatCompletionRequest) -> Result<ResponseOutcome> {
        let url = format!("{}/chat/completions", self.base_url);

        if is_verbose() {
            verbose_log(format!(
                "POST {} (provider: {}, model: {})",
                url,
                self.provider.display_name(),
                request.model
            ));
            match serde_json::to_string_pretty(request) {
                Ok(body) => verbose_log(format!("Request Body:\n{}", body)),
                Err(err) => verbose_log(format!("<failed to render request body: {}>", err)),
            }
        }

        let mut builder = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("User-Agent", &self.user_agent)
            .header("Content-Type", "application/json");

        if self.provider == LlmProvider::OpenRouter {
            builder = builder
                .header("HTTP-Referer", "https://github.com/bitrifttech/li")
                .header("X-Title", "li CLI");
        }

        let response = builder
            .json(request)
            .send()
            .await
            .context("Failed to send request to chat completions endpoint")?;

        let status = response.status();
        let headers = response.headers().clone();
        let response_url = response.url().to_string();
        let body_text = response
            .text()
            .await
            .context("Failed to read response body")?;

        if is_verbose() {
            verbose_log(format!("<- {} {}", status, response_url));
            verbose_log(format!("Response Headers: {:?}", headers));
            verbose_log(format!("Response Body:\n{}", body_text));
        }

        match status {
            StatusCode::OK => {
                let body = serde_json::from_str::<ChatCompletionResponse>(&body_text)
                    .context("Failed to parse chat completion response JSON")?;
                Ok(ResponseOutcome::Success(body))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let wait = parse_retry_after(&headers).unwrap_or(self.retry_base_delay);
                Ok(ResponseOutcome::Retry(wait, body_text))
            }
            StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::BAD_GATEWAY
            | StatusCode::GATEWAY_TIMEOUT => {
                let wait = self.retry_base_delay * 2;
                Ok(ResponseOutcome::Retry(wait, body_text))
            }
            StatusCode::UNAUTHORIZED => Err(anyhow!(
                "Invalid API key. Please check your API key configuration."
            )),
            StatusCode::BAD_REQUEST => Err(anyhow!("Invalid request: {}", body_text)),
            status => Err(anyhow!("API error (status {}): {}", status, body_text)),
        }
    }
}

#[async_trait]
impl LlmClient for ProviderClient {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse> {
        if is_verbose() {
            verbose_log(format!(
                "chat_completion invoked (provider: {}, model: {})",
                self.provider.display_name(),
                request.model
            ));
        }

        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.execute_once(&request).await? {
                ResponseOutcome::Success(response) => return Ok(response),
                ResponseOutcome::Retry(delay, message) => {
                    if attempt > MAX_RETRIES {
                        return Err(anyhow!(
                            "{} request failed after retries: {}",
                            self.provider.display_name(),
                            message
                        ));
                    }
                    if is_verbose() {
                        verbose_log(format!(
                            "Retrying in {:?} (attempt {} of {})",
                            delay, attempt, MAX_RETRIES
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

pub type AIClient = ProviderClient;

pub trait LlmClientFactory: Send + Sync {
    fn build(&self, settings: &LlmSettings) -> Result<Arc<DynLlmClient>>;
}

#[derive(Debug, Default, Clone)]
pub struct DefaultLlmClientFactory;

impl LlmClientFactory for DefaultLlmClientFactory {
    fn build(&self, settings: &LlmSettings) -> Result<Arc<DynLlmClient>> {
        Ok(Arc::new(ProviderClient::new(settings)?))
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

#[derive(Debug, Clone)]
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

impl Serialize for ChatMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ChatMessage", 2)?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("content", &self.content)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ChatMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut map = serde_json::Map::<String, Value>::deserialize(deserializer)?;

        let role_value = map
            .remove("role")
            .ok_or_else(|| de::Error::missing_field("role"))?;
        let role: ChatMessageRole =
            serde_json::from_value(role_value).map_err(de::Error::custom)?;

        let content_value = map
            .remove("content")
            .or_else(|| map.remove("contents"))
            .or_else(|| map.remove("message"))
            .or_else(|| map.remove("content_blocks"))
            .or_else(|| map.remove("blocks"))
            .or_else(|| map.remove("values"))
            .unwrap_or(Value::Null);
        let content = flatten_message_content(content_value);

        Ok(ChatMessage { role, content })
    }
}

fn flatten_message_content(value: Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text,
        Value::Array(items) => {
            let fallback = Value::Array(items.clone()).to_string();
            let mut parts = Vec::new();
            for item in items {
                let segment = flatten_message_content(item);
                if !segment.is_empty() {
                    parts.push(segment);
                }
            }
            if parts.is_empty() {
                fallback
            } else {
                parts.join("")
            }
        }
        Value::Object(mut obj) => {
            let original = obj.clone();
            let mut parts = Vec::new();

            if let Some(Value::String(text)) = obj.remove("text") {
                parts.push(text);
            }
            if let Some(Value::String(text)) = obj.remove("content") {
                parts.push(text);
            }
            if let Some(Value::String(text)) = obj.remove("reasoning") {
                parts.push(text);
            }

            for (_, nested) in obj.into_iter() {
                let segment = flatten_message_content(nested);
                if !segment.is_empty() {
                    parts.push(segment);
                }
            }

            if parts.is_empty() {
                Value::Object(original).to_string()
            } else {
                parts.join("")
            }
        }
        other => other.to_string(),
    }
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
