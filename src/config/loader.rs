use anyhow::{Context, Result};
use dirs::home_dir;
use std::{fs, path::Path};

use super::builder::ConfigBuilder;
use super::environment::apply_env_overrides;
use super::validation::validate;
use super::types::{
    RawConfig, FileConfigV1, FileConfigV2, PersistedConfig
};
use super::Config;

impl Config {
    pub fn config_path() -> Result<std::path::PathBuf> {
        let mut path = home_dir().context("Could not determine home directory")?;
        path.push(".li/config");
        Ok(path)
    }

    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut builder = ConfigBuilder::new();

        if path.exists() {
            builder = Self::apply_file(builder, &path)?;
        }

        builder = apply_env_overrides(builder)?;

        let config = builder.build()?;
        validate(&config)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Unable to create config directory {}", parent.display())
            })?;
        }

        let payload = PersistedConfig::from(self);
        let json = serde_json::to_string_pretty(&payload)
            .context("Failed to serialize configuration to JSON")?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        validate(self)
    }

    fn apply_file(mut builder: ConfigBuilder, path: &Path) -> Result<ConfigBuilder> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed reading config at {}", path.display()))?;

        if contents.trim().is_empty() {
            return Ok(builder);
        }

        let raw: RawConfig = serde_json::from_str(&contents)
            .with_context(|| format!("Failed parsing JSON config at {}", path.display()))?;

        builder = match raw {
            RawConfig::Nested(cfg) => cfg.apply(builder),
            RawConfig::Legacy(cfg) => cfg.apply(builder),
        };

        Ok(builder)
    }
}

impl FileConfigV1 {
    pub fn apply(self, builder: ConfigBuilder) -> ConfigBuilder {
        builder
            .with_llm(|llm| {
                if let Some(api_key) = self.openrouter_api_key.clone() {
                    llm.api_key = api_key;
                }
                if let Some(timeout) = self.timeout_secs {
                    llm.timeout_secs = timeout;
                }
            })
            .with_models(|models| {
                if let Some(max_tokens) = self.max_tokens {
                    models.max_tokens = max_tokens;
                }
                if let Some(planner) = self.planner_model.clone() {
                    models.planner = planner;
                }
            })
    }
}

impl FileConfigV2 {
    pub fn apply(self, builder: ConfigBuilder) -> ConfigBuilder {
        let builder = builder.with_llm(|llm| {
            if let Some(provider) = self.llm.provider.clone() {
                if let Ok(parsed) = provider.parse::<super::types::LlmProvider>() {
                    if llm.provider != parsed {
                        llm.provider = parsed;
                        llm.base_url = parsed.default_base_url().to_string();
                    }
                }
            }
            if let Some(api_key) = self.llm.api_key.clone() {
                llm.api_key = api_key;
            }
            if let Some(timeout) = self.llm.timeout_secs {
                llm.timeout_secs = timeout;
            }
            if let Some(base_url) = self.llm.base_url.clone() {
                llm.base_url = base_url;
            }
            if let Some(user_agent) = self.llm.user_agent.clone() {
                llm.user_agent = user_agent;
            }
        });

        let builder = builder.with_models(|models| {
            if let Some(planner) = self.models.planner.clone() {
                models.planner = planner;
            }
            if let Some(max_tokens) = self.models.max_tokens {
                models.max_tokens = max_tokens;
            }
        });

        if let Some(recovery) = self.recovery {
            builder.with_recovery(|settings| {
                if let Some(enabled) = recovery.enabled {
                    settings.enabled = enabled;
                }
            })
        } else {
            builder
        }
    }
}
