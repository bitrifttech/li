use anyhow::{Result, bail};

use crate::config::{Config, LlmProvider};

use super::args::Cli;
use super::models::fetch_openrouter_free_models;
use super::util::{derive_max_tokens, mask_api_key};

pub(crate) async fn handle_config_direct(args: &Cli, config: &mut Config) -> Result<()> {
    let mut existing_config = if Config::config_path()?.exists() {
        Config::load()?
    } else {
        config.clone()
    };

    if let Some(ref api_key) = args.api_key {
        existing_config.llm.api_key = api_key.clone();
    }

    if let Some(timeout) = args.timeout {
        existing_config.llm.timeout_secs = timeout;
    }

    if let Some(max_tokens) = args.max_tokens {
        existing_config.models.max_tokens = max_tokens;
    }

    let mut planner_model_context_tokens: Option<u32> = None;

    if let Some(ref planner_model) = args.planner_model {
        existing_config.models.planner = planner_model.clone();
        if args.max_tokens.is_none() {
            if existing_config.llm.provider == LlmProvider::OpenRouter {
                if let Ok(models) = fetch_openrouter_free_models(&existing_config.llm.api_key).await
                {
                    if let Some(selected) = models.into_iter().find(|m| m.id == *planner_model) {
                        planner_model_context_tokens =
                            Some(derive_max_tokens(selected.context_length));
                    }
                }
            } else {
                bail!("Model selection currently supported only for OpenRouter provider.");
            }
        }
    }

    if let Some(adjusted) = planner_model_context_tokens {
        existing_config.models.max_tokens = adjusted;
    }

    existing_config.save()?;
    *config = existing_config.clone();

    let truncated_key = mask_api_key(&existing_config.llm.api_key);

    println!(
        "✅ Configuration saved to {}",
        Config::config_path()?.display()
    );
    println!("📋 Current configuration:");
    println!(
        "   Provider: {}",
        existing_config.llm.provider.display_name()
    );
    println!("   API Key: {}", truncated_key);
    println!("   Timeout: {}s", existing_config.llm.timeout_secs);
    println!("   Max Tokens: {}", existing_config.models.max_tokens);
    println!("   Planner Model: {}", existing_config.models.planner);

    Ok(())
}
