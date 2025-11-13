use anyhow::anyhow;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::constants::{DEFAULT_CEREBRAS_BASE_URL, DEFAULT_OPENROUTER_BASE_URL};

#[derive(Debug, Clone)]
pub struct Config {
    pub llm: LlmSettings,
    pub models: ModelSettings,
    pub recovery: RecoverySettings,
}

#[derive(Debug, Clone)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub api_key: String,
    pub timeout_secs: u64,
    pub base_url: String,
    pub user_agent: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LlmProvider {
    OpenRouter,
    Cerebras,
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmProvider::OpenRouter => write!(f, "openrouter"),
            LlmProvider::Cerebras => write!(f, "cerebras"),
        }
    }
}

impl std::str::FromStr for LlmProvider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openrouter" => Ok(LlmProvider::OpenRouter),
            "cerebras" => Ok(LlmProvider::Cerebras),
            other => Err(anyhow!("Unknown LLM provider '{other}'")),
        }
    }
}

impl LlmProvider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => DEFAULT_OPENROUTER_BASE_URL,
            LlmProvider::Cerebras => DEFAULT_CEREBRAS_BASE_URL,
        }
    }

    pub fn api_key_env_var(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => "OPENROUTER_API_KEY",
            LlmProvider::Cerebras => "CEREBRAS_API_KEY",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            LlmProvider::OpenRouter => "OpenRouter",
            LlmProvider::Cerebras => "Cerebras",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelSettings {
    pub planner: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct RecoverySettings {
    pub enabled: bool,
}

// File configuration types
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum RawConfig {
    Nested(FileConfigV2),
    Legacy(FileConfigV1),
}

#[derive(Debug, Deserialize)]
pub(super) struct FileConfigV1 {
    pub openrouter_api_key: Option<String>,
    pub timeout_secs: Option<u64>,
    pub max_tokens: Option<u32>,
    pub planner_model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FileConfigV2 {
    pub llm: FileLlmSettings,
    pub models: FileModelSettings,
    #[serde(default)]
    pub recovery: Option<FileRecoverySettings>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FileLlmSettings {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub timeout_secs: Option<u64>,
    pub base_url: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FileModelSettings {
    pub planner: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(super) struct FileRecoverySettings {
    pub enabled: Option<bool>,
}

// Serialization helpers
#[derive(Serialize)]
pub(super) struct PersistedConfig<'a> {
    pub llm: PersistedLlm<'a>,
    pub models: PersistedModels<'a>,
    pub recovery: PersistedRecovery,
}

#[derive(Serialize)]
pub(super) struct PersistedLlm<'a> {
    pub provider: LlmProvider,
    pub api_key: &'a str,
    pub timeout_secs: u64,
    pub base_url: &'a str,
    pub user_agent: &'a str,
}

#[derive(Serialize)]
pub(super) struct PersistedModels<'a> {
    pub planner: &'a str,
    pub max_tokens: u32,
}

#[derive(Serialize)]
pub(super) struct PersistedRecovery {
    pub enabled: bool,
}

impl<'a> From<&'a Config> for PersistedConfig<'a> {
    fn from(config: &'a Config) -> Self {
        PersistedConfig {
            llm: PersistedLlm {
                provider: config.llm.provider,
                api_key: &config.llm.api_key,
                timeout_secs: config.llm.timeout_secs,
                base_url: &config.llm.base_url,
                user_agent: &config.llm.user_agent,
            },
            models: PersistedModels {
                planner: &config.models.planner,
                max_tokens: config.models.max_tokens,
            },
            recovery: PersistedRecovery {
                enabled: config.recovery.enabled,
            },
        }
    }
}
