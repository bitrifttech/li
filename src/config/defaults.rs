use super::constants::*;
use super::types::{LlmSettings, ModelSettings, RecoverySettings, LlmProvider};

pub fn default_user_agent() -> String {
    format!("li/{}", env!("CARGO_PKG_VERSION"))
}

impl Default for LlmSettings {
    fn default() -> Self {
        let provider = LlmProvider::OpenRouter;
        Self {
            provider,
            api_key: String::new(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            base_url: provider.default_base_url().to_string(),
            user_agent: default_user_agent(),
        }
    }
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            planner: DEFAULT_PLANNER_MODEL.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

impl Default for RecoverySettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}
