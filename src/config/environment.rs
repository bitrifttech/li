use anyhow::{anyhow, Result, Context};
use std::env;

use super::builder::ConfigBuilder;
use super::types::LlmProvider;

pub fn apply_env_overrides(mut builder: ConfigBuilder) -> Result<ConfigBuilder> {
    if let Some(provider_raw) = env_string("LI_PROVIDER")? {
        let provider = provider_raw
            .parse::<LlmProvider>()
            .with_context(|| format!("Failed to parse LI_PROVIDER value '{provider_raw}'"))?;
        builder = builder.with_llm(|llm| {
            if llm.provider != provider {
                llm.provider = provider;
                llm.base_url = provider.default_base_url().to_string();
            }
        });
    }

    if let Some(base_url) = env_string("LI_LLM_BASE_URL")? {
        builder = builder.with_llm(|llm| llm.base_url = base_url.clone());
    }

    if let Some(api_key) = env_string("OPENROUTER_API_KEY")? {
        builder = builder.with_llm(|llm| {
            if llm.provider == LlmProvider::OpenRouter {
                llm.api_key = api_key.clone();
            }
        });
    }

    if let Some(api_key) = env_string("CEREBRAS_API_KEY")? {
        builder = builder.with_llm(|llm| {
            if llm.provider == LlmProvider::Cerebras {
                llm.api_key = api_key.clone();
            }
        });
    }

    if let Some(timeout) = env_u64("LI_TIMEOUT_SECS")? {
        builder = builder.with_llm(|llm| llm.timeout_secs = timeout);
    }

    if let Some(max_tokens) = env_u32("LI_MAX_TOKENS")? {
        builder = builder.with_models(|models| models.max_tokens = max_tokens);
    }

    if let Some(planner) = env_string("LI_PLANNER_MODEL")? {
        builder = builder.with_models(|models| models.planner = planner);
    }

    Ok(builder)
}

pub fn env_string(key: &str) -> Result<Option<String>> {
    match env::var(key) {
        Ok(val) => Ok(Some(val)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(anyhow!("{key} contains invalid UTF-8")),
    }
}

pub fn env_u64(key: &str) -> Result<Option<u64>> {
    if let Some(value) = env_string(key)? {
        let parsed = value
            .parse::<u64>()
            .with_context(|| format!("Failed to parse {key} as u64"))?;
        Ok(Some(parsed))
    } else {
        Ok(None)
    }
}

pub fn env_u32(key: &str) -> Result<Option<u32>> {
    if let Some(value) = env_string(key)? {
        let parsed = value
            .parse::<u32>()
            .with_context(|| format!("Failed to parse {key} as u32"))?;
        Ok(Some(parsed))
    } else {
        Ok(None)
    }
}
