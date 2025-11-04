use anyhow::{Context, Result};
use dirs::home_dir;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub cerebras_api_key: String,
    pub timeout_secs: u64,
    pub max_tokens: u32,
    pub classifier_model: String,
    pub planner_model: String,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let mut path = home_dir().context("Could not determine home directory")?;
        path.push(".li/config");
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            anyhow::bail!("Config file not found at {}", path.display());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed reading config at {}", path.display()))?;
        let config = serde_json::from_str(&contents)
            .with_context(|| format!("Failed parsing JSON config at {}", path.display()))?;
        Ok(config)
    }
}
