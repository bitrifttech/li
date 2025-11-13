//! Configuration management for the li CLI assistant.
//!
//! This module provides a flexible configuration system that supports:
//! - File-based configuration with backward compatibility
//! - Environment variable overrides
//! - Builder pattern for programmatic configuration
//! - Validation of required settings

mod builder;
mod constants;
mod defaults;
mod environment;
mod loader;
mod types;
mod validation;

// Re-export the main types for convenience
pub use types::{
    Config, LlmSettings, LlmProvider
};

pub use constants::DEFAULT_MAX_TOKENS;

#[cfg(test)]
mod tests;
