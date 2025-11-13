use anyhow::{Result, bail};
use std::str::FromStr;

use crate::client::set_verbose_logging;
use crate::config::{Config, LlmProvider};

use super::args::{Cli, Command};
use super::chat;
use super::config_cmd;
use super::intelligence;
use super::models;
use super::providers;
use super::setup;
use super::task;
use super::util;

pub(crate) async fn run_setup(cli: Cli) -> Result<()> {
    set_verbose_logging(cli.verbose);
    setup::run_setup().await
}

pub(crate) async fn run(cli: Cli, mut config: Config) -> Result<()> {
    set_verbose_logging(cli.verbose);
    let piped_input = util::read_piped_stdin()?;
    let use_intelligence = cli.intelligence || cli.question.is_some();

    // Check for empty task (show welcome message)
    let prompt = cli.task.join(" ").trim().to_owned();
    if prompt.is_empty()
        && !cli.setup
        && !cli.chat
        && !use_intelligence
        && !cli.config
        && cli.command.is_none()
        && cli.model.is_none()
        && cli.provider.is_none()
        && cli.api_key.is_none()
        && cli.timeout.is_none()
        && cli.max_tokens.is_none()
        && cli.planner_model.is_none()
    {
        show_welcome_message()?;
        return Ok(());
    }

    // Handle setup flag (no config required)
    if cli.setup {
        return setup::run_setup().await;
    }

    // Handle chat flag
    if cli.chat {
        let prompt = cli.task.join(" ").trim().to_owned();
        if prompt.is_empty() {
            bail!("Chat message cannot be empty. Usage: li --chat \"your message\"");
        }
        return chat::handle_chat_direct(&prompt, &config).await;
    }

    if use_intelligence {
        intelligence::handle_intelligence(
            cli.question.clone(),
            cli.task.clone(),
            piped_input,
            &config,
        )
        .await?;
        return Ok(());
    }

    // Handle provider override
    if let Some(ref provider_arg) = cli.provider {
        handle_provider_override(provider_arg.trim(), &mut config).await?;
        return Ok(());
    }

    // Handle model override
    if let Some(ref model_arg) = cli.model {
        handle_model_override(model_arg.trim(), &mut config).await?;
        return Ok(());
    }

    // Handle config flags
    if cli.config
        || cli.api_key.is_some()
        || cli.timeout.is_some()
        || cli.max_tokens.is_some()
        || cli.planner_model.is_some()
    {
        config_cmd::handle_config_direct(&cli, &mut config).await?;
        return Ok(());
    }

    match cli.command {
        Some(Command::Chat(args)) => chat::handle_chat(args, &config).await?,
        None => task::handle_task(cli.task, &config).await?,
    }

    Ok(())
}

fn show_welcome_message() -> Result<()> {
    let config_path = Config::config_path()?;
    let config_exists = config_path.exists();

    println!("🚀 Welcome to li - Your AI-Powered CLI Assistant!");
    println!("   📱 Project: https://github.com/bitrifttech/li");
    println!();
    println!("📖 What li does:");
    println!("   • Converts natural language to shell commands");
    println!("   • Gives intellegent analysis of command output");
    println!("   • Executes safe, minimal command plans");
    println!("   • Powered by configurable LLM providers (OpenRouter, Cerebras)");
    println!();

    if !config_exists {
        println!("⚠️  Configuration not found. Let's get you set up!");
        println!("   Run: li --setup");
        println!();
    }

    println!("💡 How to use li:");
    println!(
        "   li --setup                                         # Interactive first-time setup"
    );
    println!("   li 'list all files in current directory'           # Plan & execute commands");
    println!("   li --chat 'what is the capital of France?'         # Direct AI conversation");
    println!(
        "   li -i 'df -h'                                      # Explain command output with AI"
    );
    println!("   li -i -q 'Which disk has the most space?' 'df -h'  # Ask a question about output");
    println!("   li --model                                         # Interactive model selection");
    println!("   li --model list                                    # Show available models");
    println!(
        "   li --provider                                      # Interactive provider selection"
    );
    println!("   li --provider list                                 # Show supported providers");
    println!("   li --config --api-key YOUR_KEY                     # Set API key manually");
    println!("   li --config --timeout 60                           # Set timeout in seconds");
    println!("   li --config --max-tokens 4096                      # Set max tokens");
    println!("   li --config --planner-model MODEL                  # Set planner model");
    println!();

    if config_exists {
        match Config::load() {
            Ok(loaded_config) => {
                println!("📋 Your current configuration:");
                println!(
                    "   Provider: {} ({})",
                    loaded_config.llm.provider,
                    loaded_config.llm.provider.display_name()
                );
                println!("   Planner: {}", loaded_config.models.planner);
                println!("   Timeout: {}s", loaded_config.llm.timeout_secs);
                println!();
            }
            Err(_) => {
                println!("⚠️  Configuration exists but couldn't be loaded.");
                println!("   Run: li --setup");
                println!();
            }
        }
    }

    println!("❓ For more help: li --help");
    Ok(())
}

async fn handle_provider_override(arg: &str, config: &mut Config) -> Result<()> {
    if arg.eq_ignore_ascii_case("list") {
        providers::print_provider_list();
        return Ok(());
    }

    if arg.eq_ignore_ascii_case("interactive") || arg.is_empty() {
        let selected = providers::prompt_provider_interactive(Some(config.llm.provider))?;
        let existing_key = if config.llm.provider == selected && !config.llm.api_key.is_empty() {
            Some(config.llm.api_key.as_str())
        } else {
            None
        };
        let api_key = providers::prompt_api_key_for_provider(selected, existing_key)?;

        if config.llm.provider != selected {
            config.llm.provider = selected;
            config.llm.base_url = selected.default_base_url().to_string();
        }
        config.llm.api_key = api_key;
        config.save()?;

        println!(
            "\n✅ Provider configuration saved to {}",
            Config::config_path()?.display()
        );
        println!("📋 Current provider:");
        println!(
            "   Provider: {} ({})",
            config.llm.provider,
            config.llm.provider.display_name()
        );
        println!("   Base URL: {}", config.llm.base_url);

        if config.llm.api_key.trim().is_empty() {
            println!(
                "⚠️  {} API key is empty. Set {} or run 'li --setup'.",
                config.llm.provider.display_name(),
                config.llm.provider.api_key_env_var()
            );
        }

        return Ok(());
    }

    match LlmProvider::from_str(arg) {
        Ok(provider) => {
            let mut changed = false;
            if config.llm.provider != provider {
                config.llm.provider = provider;
                config.llm.base_url = provider.default_base_url().to_string();
                changed = true;
            }
            if changed {
                config.save()?;
                println!(
                    "✅ Provider set to {} ({}).",
                    config.llm.provider,
                    config.llm.provider.display_name()
                );
                if config.llm.api_key.trim().is_empty() {
                    println!(
                        "⚠️  {} API key is not configured. Set {} or run 'li --setup'.",
                        config.llm.provider.display_name(),
                        config.llm.provider.api_key_env_var()
                    );
                }
            } else {
                println!(
                    "ℹ️  Provider already set to {}.",
                    config.llm.provider.display_name()
                );
            }
        }
        Err(_) => {
            println!(
                "Unknown provider '{}'. Use 'li --provider list' to see supported providers.",
                arg
            );
        }
    }

    Ok(())
}

async fn handle_model_override(arg: &str, config: &mut Config) -> Result<()> {
    if config.llm.provider != LlmProvider::OpenRouter {
        println!(
            "Model selection via --model is currently supported only for the OpenRouter provider."
        );
        println!(
            "Use 'li --provider openrouter' or 'li --provider interactive' to switch providers."
        );
        return Ok(());
    }

    let models = models::fetch_openrouter_free_models(&config.llm.api_key).await?;
    if models.is_empty() {
        println!("⚠️  No free OpenRouter models were returned.");
        return Ok(());
    }

    if arg.eq_ignore_ascii_case("list") {
        for model in models {
            let context_len = model
                .context_length
                .map(|len| format!(" ({} context)", len))
                .unwrap_or_default();
            println!("{}: {}{}", model.id, model.name, context_len);
        }
        return Ok(());
    }

    if arg.eq_ignore_ascii_case("interactive") || arg.is_empty() {
        println!("\n🤖 Available Free Models:\n");
        for (idx, model) in models.iter().enumerate() {
            let context_len = model
                .context_length
                .map(|len| format!(" ({} context)", len))
                .unwrap_or_default();
            println!("  {}. {}{}", idx + 1, model.name, context_len);
        }

        let planner_index = models::prompt_model_index(
            &models,
            "\n📋 Select planner model (creates shell commands from natural language): ",
        )?;
        let planner_selection = &models[planner_index];
        let planner_model = planner_selection.id.clone();
        let derived_max_tokens = util::derive_max_tokens(planner_selection.context_length);

        config.models.planner = planner_model.clone();
        config.models.max_tokens = derived_max_tokens;
        config.save()?;

        println!(
            "\n✅ Model configuration saved to {}",
            Config::config_path()?.display()
        );
        println!("📋 Updated configuration:");
        println!("   Planner Model: {}", config.models.planner);
        println!("   Max Tokens: {}", config.models.max_tokens);
        return Ok(());
    }

    if !models.iter().any(|m| m.id == arg) {
        println!("Model '{}' not found in free models list.", arg);
        println!("Use 'li -m list' to see available free models.");
        println!("Or use 'li -m' to select interactively.");
        return Ok(());
    }

    if let Some(selected) = models.iter().find(|m| m.id == arg) {
        config.models.max_tokens = util::derive_max_tokens(selected.context_length);
    }
    config.models.planner = arg.to_string();
    config.save()?;
    println!(
        "✅ Planner model set to {} with inferred max tokens {}.",
        config.models.planner, config.models.max_tokens
    );

    Ok(())
}
