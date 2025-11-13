use anyhow::Result;
use super::types::{Config, LlmSettings, ModelSettings, RecoverySettings};

#[derive(Debug)]
pub struct ConfigBuilder {
    pub(super) llm: LlmSettings,
    pub(super) models: ModelSettings,
    pub(super) recovery: RecoverySettings,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            llm: LlmSettings::default(),
            models: ModelSettings::default(),
            recovery: RecoverySettings::default(),
        }
    }

    pub fn with_llm<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut LlmSettings),
    {
        update(&mut self.llm);
        self
    }

    pub fn with_models<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut ModelSettings),
    {
        update(&mut self.models);
        self
    }

    pub fn with_recovery<F>(mut self, update: F) -> Self
    where
        F: FnOnce(&mut RecoverySettings),
    {
        update(&mut self.recovery);
        self
    }

    pub fn build(self) -> Result<Config> {
        Ok(Config {
            llm: self.llm,
            models: self.models,
            recovery: self.recovery,
        })
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
