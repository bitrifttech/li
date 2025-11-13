use anyhow::{Result, anyhow};

use super::types::Config;

pub fn validate(config: &Config) -> Result<()> {
    if config.llm.api_key.trim().is_empty() {
        let provider = config.llm.provider;
        let env_var = provider.api_key_env_var();
        Err(anyhow!(
            "{} API key not found. Set {} or add it to {}",
            provider.display_name(),
            env_var,
            Config::config_path()?.display()
        ))
    } else {
        Ok(())
    }
}
