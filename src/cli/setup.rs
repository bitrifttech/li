use anyhow::Result;

use crate::config::{Config, LlmProvider};

use super::models::{fetch_openrouter_free_models, prompt_model_index};
use super::providers::{prompt_api_key_for_provider, prompt_provider_interactive};
use super::util::{
    derive_max_tokens, mask_api_key, prompt_string_with_default, prompt_timeout,
    prompt_u32_with_default,
};

pub(crate) async fn run_setup() -> Result<()> {
    println!("🚀 Welcome to li CLI Setup!");
    println!("Let's configure your AI provider.\n");

    let provider = prompt_provider_interactive(None)?;
    let api_key = prompt_api_key_for_provider(provider, None)?;
    let timeout = prompt_timeout(30)?;

    let mut config = Config::builder().build()?;
    config.llm.provider = provider;
    config.llm.base_url = provider.default_base_url().to_string();
    config.llm.api_key = api_key.clone();
    config.llm.timeout_secs = timeout;

    match provider {
        LlmProvider::OpenRouter => configure_openrouter_setup(&mut config, &api_key).await?,
        LlmProvider::Cerebras => configure_cerebras_setup(&mut config)?,
    }

    config.validate()?;
    config.save()?;

    println!(
        "\n✅ Configuration saved to {}",
        Config::config_path()?.display()
    );
    println!("📋 Your configuration:");
    println!(
        "   Provider: {} ({})",
        config.llm.provider,
        config.llm.provider.display_name()
    );
    println!("   API Key: {}", mask_api_key(&config.llm.api_key));
    println!("   Base URL: {}", config.llm.base_url);
    println!("   Timeout: {}s", config.llm.timeout_secs);
    println!("   Max Tokens: {}", config.models.max_tokens);
    println!("   Planner Model: {}", config.models.planner);
    println!("\n🎉 Setup complete! You can now use 'li' with commands like:");
    println!("   li 'list all files in current directory'");
    println!("   li --chat 'what is the capital of France?'");
    println!("   li -m list  # show free models (OpenRouter only)");
    println!("   li --provider list  # see available providers\n");

    Ok(())
}

async fn configure_openrouter_setup(config: &mut Config, api_key: &str) -> Result<()> {
    println!("\n📡 Fetching available free models from OpenRouter...");
    let models = fetch_openrouter_free_models(api_key).await?;

    if models.is_empty() {
        println!("⚠️  No free OpenRouter models were returned. Keeping existing model settings.");
        return Ok(());
    }

    println!("\n🤖 Available Free Models:\n");
    for (idx, model) in models.iter().enumerate() {
        let context_len = model
            .context_length
            .map(|len| format!(" ({} context)", len))
            .unwrap_or_default();
        println!("  {}. {}{}", idx + 1, model.name, context_len);
    }

    let planner_index = prompt_model_index(
        &models,
        "\n📋 Select planner model (creates shell commands from natural language): ",
    )?;

    let planner_selection = &models[planner_index];
    let planner_model = planner_selection.id.clone();
    let derived_max_tokens = derive_max_tokens(planner_selection.context_length);

    config.models.planner = planner_model;
    config.models.max_tokens = derived_max_tokens;

    Ok(())
}

fn configure_cerebras_setup(config: &mut Config) -> Result<()> {
    println!("\nℹ️  Cerebras setup requires entering model identifiers manually.");
    println!("   Refer to your Cerebras deployment documentation for model IDs.\n");

    let default_planner = config.models.planner.clone();
    let default_max_tokens = config.models.max_tokens;

    config.models.planner =
        prompt_string_with_default("📋 Enter planner model ID", &default_planner)?;
    config.models.max_tokens = prompt_u32_with_default(
        "🔢 Enter max tokens for planner completions",
        default_max_tokens,
    )?;

    Ok(())
}
