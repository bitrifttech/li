use crate::agent::{AgentOrchestrator, AgentOutcome, AgentRequest, StageKind};
use crate::client::{
    AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, LlmClient, set_verbose_logging,
};
use crate::config::{Config, DEFAULT_MAX_TOKENS, LlmProvider};
use crate::exec;
use crate::planner;
use crate::recovery::{RecoveryContext, RecoveryEngine, RecoveryResult};
use crate::validator::ValidationResult;
use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::io::{self, Write};
use std::str::FromStr;

const CONTEXT_HEADROOM_TOKENS: usize = 1024;
const PROVIDER_CHOICES: &[LlmProvider] = &[LlmProvider::OpenRouter, LlmProvider::Cerebras];

fn derive_max_tokens(context_length: Option<usize>) -> u32 {
    context_length
        .map(|len| len.saturating_sub(CONTEXT_HEADROOM_TOKENS))
        .filter(|&len| len > 0)
        .map(|len| len.min(u32::MAX as usize) as u32)
        .filter(|&tokens| tokens > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS)
}

fn provider_description(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenRouter => "OpenRouter marketplace of hosted inference models",
        LlmProvider::Cerebras => "Cerebras Inference deployment",
    }
}

fn print_provider_list() {
    println!("\n🌐 Available Providers:\n");
    for provider in PROVIDER_CHOICES {
        println!(
            "  {} ({}) - {}",
            provider,
            provider.display_name(),
            provider_description(*provider)
        );
    }
    println!();
}

fn prompt_provider_interactive(current: Option<LlmProvider>) -> Result<LlmProvider> {
    println!("\n🌐 Available Providers:\n");
    for (idx, provider) in PROVIDER_CHOICES.iter().enumerate() {
        let marker = if Some(*provider) == current {
            " (current)"
        } else {
            ""
        };
        println!(
            "  {}. {}{} - {}",
            idx + 1,
            provider.display_name(),
            marker,
            provider_description(*provider)
        );
    }

    loop {
        print!("\nSelect provider (1-{}): ", PROVIDER_CHOICES.len());
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        match trimmed.parse::<usize>() {
            Ok(num) if num >= 1 && num <= PROVIDER_CHOICES.len() => {
                return Ok(PROVIDER_CHOICES[num - 1]);
            }
            _ => println!(
                "❌ Please enter a number between 1 and {}.",
                PROVIDER_CHOICES.len()
            ),
        }
    }
}

fn prompt_api_key_for_provider(provider: LlmProvider, existing: Option<&str>) -> Result<String> {
    loop {
        match provider {
            LlmProvider::OpenRouter => {
                print!(
                    "🔑 Enter your OpenRouter API key{}: ",
                    existing
                        .map(|_| " (leave blank to keep current)")
                        .unwrap_or("")
                );
            }
            LlmProvider::Cerebras => {
                print!(
                    "🔑 Enter your Cerebras API key{}: ",
                    existing
                        .map(|_| " (leave blank to keep current)")
                        .unwrap_or("")
                );
            }
        }
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let key = input.trim();

        if key.is_empty() {
            if let Some(existing) = existing {
                return Ok(existing.to_string());
            }
            println!("❌ API key cannot be empty. Please try again.");
            continue;
        }

        if provider == LlmProvider::OpenRouter && !key.starts_with("sk-or-v1") {
            println!(
                "⚠️  OpenRouter API keys typically start with 'sk-or-v1'. Are you sure this is correct?"
            );
            print!("Continue anyway? [y/N]: ");
            io::stdout().flush()?;

            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;
            if confirm.trim().to_lowercase() != "y" {
                continue;
            }
        }

        return Ok(key.to_string());
    }
}

fn prompt_timeout(default: u64) -> Result<u64> {
    loop {
        print!("⏱️  Enter timeout in seconds (default: {default}): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let timeout_str = input.trim();

        if timeout_str.is_empty() {
            return Ok(default);
        }

        match timeout_str.parse::<u64>() {
            Ok(timeout) if timeout > 0 => return Ok(timeout),
            Ok(_) => println!("❌ Timeout must be a positive number."),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    }
}

fn prompt_string_with_default(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} (default: {default}): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();

    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_u32_with_default(prompt: &str, default: u32) -> Result<u32> {
    loop {
        print!("{prompt} (default: {default}): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Ok(default);
        }

        match trimmed.parse::<u32>() {
            Ok(value) if value > 0 => return Ok(value),
            Ok(_) => println!("❌ Value must be greater than zero."),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    }
}

fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return "(not set)".to_string();
    }

    let visible = key.len().min(8);
    format!("{}***", &key[..visible])
}

fn prompt_model_index(models: &[OpenRouterModel], label: &str) -> Result<usize> {
    loop {
        print!("{label}");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();

        if choice.is_empty() {
            println!("❌ Please select a model number.");
            continue;
        }

        match choice.parse::<usize>() {
            Ok(num) if num >= 1 && num <= models.len() => {
                return Ok(num - 1);
            }
            Ok(_) => println!("❌ Please enter a number between 1 and {}.", models.len()),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    }
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

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    name: String,
    pricing: Option<Pricing>,
    context_length: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct Pricing {
    prompt: String,
    completion: String,
    request: Option<String>,
    image: Option<String>,
    web_search: Option<String>,
    internal_reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

async fn fetch_openrouter_free_models(api_key: &str) -> Result<Vec<OpenRouterModel>> {
    use reqwest::Client;

    let client = Client::new();
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("Failed to fetch models from OpenRouter")?;

    if !response.status().is_success() {
        return Err(anyhow!("OpenRouter API error: {}", response.status()));
    }

    let models_response: OpenRouterModelsResponse = response
        .json()
        .await
        .context("Failed to parse OpenRouter models response")?;

    // Filter for free models
    let free_models = models_response
        .data
        .into_iter()
        .filter(|model| {
            if let Some(pricing) = &model.pricing {
                pricing.prompt == "0" && pricing.completion == "0"
            } else {
                false
            }
        })
        .collect();

    Ok(free_models)
}

/// Entry point for the `li` command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "li",
    about = "Plain-English to shell assistant",
    version,
    long_about = None
)]
pub struct Cli {
    /// Optional subcommand (e.g., `chat`)
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Override the model (for OpenRouter, fetches free models list)
    #[arg(short = 'm', long = "model", num_args = 0..=1, default_missing_value = "")]
    pub model: Option<String>,

    /// Select the LLM provider (e.g., openrouter or cerebras)
    #[arg(long = "provider", num_args = 0..=1, default_missing_value = "")]
    pub provider: Option<String>,

    /// Enable verbose logging of LLM requests and responses
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Interactive setup for first-time configuration
    #[arg(long = "setup")]
    pub setup: bool,

    /// Direct chat with the AI model
    #[arg(long = "chat")]
    pub chat: bool,

    /// Explain command output using AI intelligence
    #[arg(short = 'i', long = "intelligence")]
    pub intelligence: bool,

    /// Ask a specific question about the command output when using intelligence mode
    #[arg(short = 'q', long = "question", requires = "intelligence")]
    pub question: Option<String>,

    /// Configure li settings
    #[arg(long)]
    pub config: bool,

    /// Set the API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// Set timeout in seconds
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Set max tokens
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Set planner model
    #[arg(long)]
    pub planner_model: Option<String>,

    /// Default task: words typed after `li`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub task: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Directly invoke the chat completion API.
    Chat(ChatArgs),
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    /// Optional override for the model (defaults to planner model from config).
    #[arg(long)]
    model: Option<String>,

    /// Optional override for max_tokens (defaults to config setting).
    #[arg(long)]
    max_tokens: Option<u32>,

    /// Optional temperature to pass through to the API.
    #[arg(long)]
    temperature: Option<f32>,

    /// Prompt to send to the OpenRouter model.
    #[arg(required = true)]
    prompt: Vec<String>,
}

impl Cli {
    pub async fn run_setup(self) -> Result<()> {
        set_verbose_logging(self.verbose);
        handle_setup().await
    }

    pub async fn run(self, mut config: Config) -> Result<()> {
        set_verbose_logging(self.verbose);
        // Check for empty task (show welcome message)
        let prompt = self.task.join(" ").trim().to_owned();
        if prompt.is_empty()
            && !self.setup
            && !self.chat
            && !self.intelligence
            && !self.config
            && self.command.is_none()
            && self.model.is_none()
            && self.provider.is_none()
            && self.api_key.is_none()
            && self.timeout.is_none()
            && self.max_tokens.is_none()
            && self.planner_model.is_none()
        {
            // Check if config file exists
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
            println!(
                "   li 'list all files in current directory'           # Plan & execute commands"
            );
            println!(
                "   li --chat 'what is the capital of France?'         # Direct AI conversation"
            );
            println!(
                "   li -i 'df -h'                                      # Explain command output with AI"
            );
            println!(
                "   li -i -q 'Which disk has the most space?' 'df -h'  # Ask a question about output"
            );
            println!(
                "   li --model                                         # Interactive model selection"
            );
            println!(
                "   li --model list                                    # Show available models"
            );
            println!(
                "   li --provider                                      # Interactive provider selection"
            );
            println!(
                "   li --provider list                                 # Show supported providers"
            );
            println!(
                "   li --config --api-key YOUR_KEY                     # Set API key manually"
            );
            println!(
                "   li --config --timeout 60                           # Set timeout in seconds"
            );
            println!("   li --config --max-tokens 4096                      # Set max tokens");
            println!("   li --config --planner-model MODEL                  # Set planner model");
            println!();

            if config_exists {
                // Load config just to show current settings
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

            return Ok(());
        }

        // Handle setup flag (no config required)
        if self.setup {
            return handle_setup().await;
        }

        // Handle chat flag
        if self.chat {
            let prompt = self.task.join(" ").trim().to_owned();
            if prompt.is_empty() {
                bail!("Chat message cannot be empty. Usage: li --chat \"your message\"");
            }
            return handle_chat_direct(&prompt, &config).await;
        }

        // Handle provider override
        if let Some(ref provider_arg) = self.provider {
            let provider_arg = provider_arg.trim();
            if provider_arg.eq_ignore_ascii_case("list") {
                print_provider_list();
                return Ok(());
            } else if provider_arg.eq_ignore_ascii_case("interactive") || provider_arg.is_empty() {
                let selected = prompt_provider_interactive(Some(config.llm.provider))?;
                let existing_key =
                    if config.llm.provider == selected && !config.llm.api_key.is_empty() {
                        Some(config.llm.api_key.as_str())
                    } else {
                        None
                    };
                let api_key = prompt_api_key_for_provider(selected, existing_key)?;

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
            } else {
                match LlmProvider::from_str(provider_arg) {
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
                            provider_arg
                        );
                        return Ok(());
                    }
                }
            }
        }

        // Handle model override
        if let Some(ref model_arg) = self.model {
            if config.llm.provider != LlmProvider::OpenRouter {
                println!(
                    "Model selection via --model is currently supported only for the OpenRouter provider."
                );
                println!(
                    "Use 'li --provider openrouter' or 'li --provider interactive' to switch providers."
                );
                return Ok(());
            }
            let models = fetch_openrouter_free_models(&config.llm.api_key).await?;
            if model_arg == "list" {
                // Just list the models
                for model in models {
                    let context_len = model
                        .context_length
                        .map(|len| format!(" ({} context)", len))
                        .unwrap_or_default();
                    println!("{}: {}{}", model.id, model.name, context_len);
                }
                return Ok(());
            } else if model_arg == "interactive" || model_arg.is_empty() {
                // Interactive selection for planner model
                println!("\n🤖 Available Free Models:\n");
                for (idx, model) in models.iter().enumerate() {
                    let context_len = model
                        .context_length
                        .map(|len| format!(" ({} context)", len))
                        .unwrap_or_default();
                    println!("  {}. {}{}", idx + 1, model.name, context_len);
                }

                let planner_index = loop {
                    print!(
                        "\n📋 Select planner model (creates shell commands from natural language): "
                    );
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    let choice = input.trim();

                    if choice.is_empty() {
                        println!("❌ Please select a model number.");
                        continue;
                    }

                    match choice.parse::<usize>() {
                        Ok(num) if num >= 1 && num <= models.len() => {
                            break num - 1;
                        }
                        Ok(_) => {
                            println!("❌ Please enter a number between 1 and {}.", models.len())
                        }
                        Err(_) => println!("❌ Please enter a valid number."),
                    }
                };
                let planner_selection = &models[planner_index];
                let planner_model = planner_selection.id.clone();
                let derived_max_tokens = derive_max_tokens(planner_selection.context_length);

                // Update config
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
            } else {
                // Check if the model is in the free list
                if !models.iter().any(|m| m.id == *model_arg) {
                    println!("Model '{}' not found in free models list.", model_arg);
                    println!("Use 'li -m list' to see available free models.");
                    println!("Or use 'li -m' to select interactively.");
                    return Ok(());
                }
                if let Some(selected) = models.iter().find(|m| m.id == *model_arg) {
                    config.models.max_tokens = derive_max_tokens(selected.context_length);
                }
                config.models.planner = model_arg.clone();
            }
        }

        // Handle intelligence flag
        if self.intelligence {
            handle_intelligence(self.question.clone(), self.task, &config).await?;
            return Ok(());
        }

        // Handle config flags
        if self.config
            || self.api_key.is_some()
            || self.timeout.is_some()
            || self.max_tokens.is_some()
            || self.planner_model.is_some()
        {
            handle_config_direct(&self, &mut config).await?;
            return Ok(());
        }

        match self.command {
            Some(Command::Chat(args)) => handle_chat(args, &config).await?,
            None => handle_task(self.task, &config).await?,
        }
        Ok(())
    }
}

async fn handle_chat(args: ChatArgs, config: &Config) -> Result<()> {
    let prompt = args.prompt.join(" ").trim().to_owned();
    if prompt.is_empty() {
        bail!("Prompt cannot be empty");
    }

    let model = args.model.unwrap_or_else(|| config.models.planner.clone());
    let max_tokens = args.max_tokens.unwrap_or(config.models.max_tokens);
    let temperature = args.temperature;

    let client = AIClient::new(&config.llm)?;
    let response = client
        .chat_completion(ChatCompletionRequest {
            model: model.clone(),
            messages: vec![ChatMessage {
                role: ChatMessageRole::User,
                content: prompt,
            }],
            max_tokens: Some(max_tokens),
            temperature,
        })
        .await?;

    println!("Provider: {}", config.llm.provider.display_name());
    println!("Model: {}", model);

    for (idx, choice) in response.choices.iter().enumerate() {
        println!("\nChoice {}:", idx + 1);
        println!("{}", choice.message.content.trim());

        if let Some(reason) = &choice.finish_reason {
            println!("Finish reason: {}", reason);
        }
    }

    Ok(())
}

async fn handle_task(words: Vec<String>, config: &Config) -> Result<()> {
    let prompt = words.join(" ").trim().to_owned();
    if prompt.is_empty() {
        println!(
            "li CLI is initialized. Provide a task or run `li --chat \"your question\"` to call your configured provider."
        );
        return Ok(());
    }

    let orchestrator = AgentOrchestrator::default();
    let request = AgentRequest::new(prompt.clone());

    let run = orchestrator
        .run(config.clone(), request)
        .await
        .context("Agent pipeline failed")?;

    match run.outcome {
        AgentOutcome::Planned {
            plan: Some(plan),
            validation,
            ..
        } => {
            if let Some(validation) = validation.clone() {
                let proceed =
                    resolve_validation_issues(&validation, &plan, config, &prompt).await?;
                if !proceed {
                    return Ok(());
                }
            }

            render_plan(&plan, config);

            match prompt_for_approval()? {
                ApprovalResponse::Yes => {
                    exec::execute_plan(&plan).await?;
                }
                ApprovalResponse::YesWithIntelligence => {
                    let output = exec::execute_plan_with_capture(&plan).await?;
                    let client = AIClient::new(&config.llm)?;
                    explain_plan_output(&client, config, &plan, &output).await?;
                }
                ApprovalResponse::No => {
                    println!("\nPlan execution cancelled.");
                }
            }

            Ok(())
        }
        AgentOutcome::Planned { plan: None, .. } => {
            bail!("Agent pipeline returned no plan");
        }
        AgentOutcome::AwaitingClarification { question, context } => {
            println!("The agent needs more information before proceeding.");
            println!("Question: {}", question);
            if !context.trim().is_empty() {
                println!("Context: {}", context);
            }
            Ok(())
        }
        AgentOutcome::Cancelled { reason } => {
            println!("Agent cancelled the request: {}", reason);
            Ok(())
        }
        AgentOutcome::Failed { stage, error } => {
            let guidance = match stage {
                StageKind::Planning => format!(
                    "Verify your {} API key (set {} or run 'li --setup') and ensure you have internet connectivity. Retry if the service is rate limited.",
                    config.llm.provider.display_name(),
                    config.llm.provider.api_key_env_var()
                ),
                StageKind::Validation => "Inspect the validator warnings above for missing tools before rerunning the command.".to_string(),
                StageKind::Execution => {
                    "Review the command output above for failures before retrying.".to_string()
                }
                StageKind::Recovery => {
                    "Recovery cancelled. Resolve tool installation manually or re-run with recovery enabled.".to_string()
                }
            };
            bail!("Agent stage {} failed: {}. {}", stage, error, guidance);
        }
    }
}

fn render_plan(plan: &planner::Plan, config: &Config) {
    println!("\nProvider: {}", config.llm.provider.display_name());
    println!("Model: {}", config.models.planner);
    println!("Plan confidence: {:.2}", plan.confidence);

    if !plan.dry_run_commands.is_empty() {
        println!("\nDry-run Commands:");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.execute_commands.is_empty() {
        println!("\nExecute Commands:");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!("  {}. {}", idx + 1, cmd);
        }
    }

    if !plan.notes.trim().is_empty() {
        println!("\nNotes: {}", plan.notes.trim());
    }
}

async fn resolve_validation_issues(
    validation: &ValidationResult,
    plan: &planner::Plan,
    config: &Config,
    goal: &str,
) -> Result<bool> {
    if validation.missing_commands.is_empty() {
        return Ok(true);
    }

    let count = validation.missing_commands.len();
    println!(
        "⚠️  Validator identified {} missing command{}:",
        count,
        if count == 1 { "" } else { "s" }
    );
    for missing in &validation.missing_commands {
        let phase = if missing.is_dry_run {
            "dry-run"
        } else {
            "execute"
        };
        println!(
            "   • {} (step {}: {})",
            missing.command,
            missing.plan_step + 1,
            phase
        );
    }

    if validation.plan_can_continue {
        println!(
            "Plan can continue, but results may be degraded until the missing tool{} {} installed.",
            if count == 1 { "" } else { "s are" },
            if count == 1 { "is" } else { "are" }
        );
        return Ok(true);
    }

    println!("Plan cannot continue until the missing commands are addressed.");

    if !config.recovery.enabled {
        println!("Recovery is disabled in your configuration. Enable it to receive guided fixes.");
        return Ok(false);
    }

    let mut engine = RecoveryEngine::new(config)?;
    engine.set_available_tools().await?;
    let mut any_success = false;

    for missing in &validation.missing_commands {
        let options = engine
            .generate_recovery_options(missing, plan, goal)
            .await?;

        if options.command_alternatives.is_empty()
            && options.installation_instructions.is_empty()
            && !options.can_skip_step
        {
            println!(
                "No automated recovery options available for '{}'.",
                missing.command
            );
            continue;
        }

        let choice = engine.present_recovery_menu(&options, missing).await?;
        let context = RecoveryContext {
            missing_command: missing.clone(),
            original_plan: plan.clone(),
            original_goal: goal.to_string(),
        };

        match engine.execute_recovery(choice, &context).await? {
            RecoveryResult::AlternativeSucceeded(alt) => {
                println!("✅ Alternative executed: {}", alt.command);
                any_success = true;
            }
            RecoveryResult::InstallationSucceeded(inst) => {
                println!("✅ Installation succeeded: {}", inst.command);
                any_success = true;
            }
            RecoveryResult::InstallationCancelled => {
                println!("Installation cancelled. Re-run the command when ready.");
                return Ok(false);
            }
            RecoveryResult::PlanAborted(reason) => {
                println!("Plan aborted: {}", reason);
                return Ok(false);
            }
            RecoveryResult::AlternativeFailed(_) | RecoveryResult::InstallationFailed(_) => {
                println!("Recovery attempt did not succeed.");
            }
            RecoveryResult::StepSkipped => {
                println!("Recovery step skipped.");
            }
            RecoveryResult::RetryRequested | RecoveryResult::RetryWithDifferentApproach => {
                println!("Retry requested. Re-run the command after addressing the prompt.");
                return Ok(false);
            }
        }
    }

    if any_success {
        println!("Re-run your original command to take advantage of the recovery steps.");
    }

    Ok(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalResponse {
    Yes,
    YesWithIntelligence,
    No,
}

fn prompt_for_approval() -> Result<ApprovalResponse> {
    print!("\nExecute this plan? [y/N/i]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();
    match answer.as_str() {
        "y" | "yes" => Ok(ApprovalResponse::Yes),
        "i" | "intelligence" => Ok(ApprovalResponse::YesWithIntelligence),
        _ => Ok(ApprovalResponse::No),
    }
}

#[allow(dead_code)]
async fn execute_plan_with_capture(plan: &planner::Plan) -> Result<String> {
    exec::execute_plan_with_capture(plan).await
}

#[allow(dead_code)]
async fn run_command(cmd: &str) -> Result<bool> {
    exec::run_command(cmd).await
}

async fn handle_config_direct(args: &Cli, config: &mut Config) -> Result<()> {
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
            if let Ok(models) = fetch_openrouter_free_models(&existing_config.llm.api_key).await {
                if let Some(selected) = models.into_iter().find(|m| m.id == *planner_model) {
                    planner_model_context_tokens = Some(derive_max_tokens(selected.context_length));
                }
            }
        }
    }

    if let Some(adjusted) = planner_model_context_tokens {
        existing_config.models.max_tokens = adjusted;
    }

    existing_config.save()?;
    *config = existing_config.clone();

    let truncated_key = if existing_config.llm.api_key.len() > 8 {
        format!("{}***", &existing_config.llm.api_key[..8])
    } else {
        format!("{}***", existing_config.llm.api_key)
    };

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

async fn handle_setup() -> Result<()> {
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

async fn handle_chat_direct(prompt: &str, config: &Config) -> Result<()> {
    let client = AIClient::new(&config.llm)?;

    let request = ChatCompletionRequest {
        model: config.models.planner.clone(),
        messages: vec![ChatMessage {
            role: ChatMessageRole::User,
            content: prompt.to_string(),
        }],
        max_tokens: Some(config.models.max_tokens),
        temperature: Some(0.7),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Chat completion failed")?;

    println!("Provider: {}", config.llm.provider.display_name());
    println!("Model: {}", config.models.planner);
    println!();

    for (i, choice) in response.choices.iter().enumerate() {
        println!("Choice {}:", i + 1);
        println!("{}", choice.message.content);
        if let Some(reason) = &choice.finish_reason {
            println!("Finish reason: {}", reason);
        }
        println!();
    }

    Ok(())
}

async fn explain_plan_output(
    client: &AIClient,
    config: &Config,
    plan: &planner::Plan,
    output: &str,
) -> Result<()> {
    use crate::tokens::compute_completion_token_budget;

    println!("\n🤖 AI Intelligence Explanation:");
    println!();

    let commands_summary = {
        let mut summary = String::new();
        if !plan.dry_run_commands.is_empty() {
            summary.push_str("Dry-run Commands:\n");
            for cmd in &plan.dry_run_commands {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }
        if !plan.execute_commands.is_empty() {
            summary.push_str("Execute Commands:\n");
            for cmd in &plan.execute_commands {
                summary.push_str(&format!("  - {}\n", cmd));
            }
        }
        summary
    };

    let explanation_prompt = format!(
        "Please explain the following command execution results in a clear, human-friendly way.\n\n\
        The plan that was executed:\n{}\n\
        Plan Notes: {}\n\n\
        Command Output:\n{}\n\n\
        Please provide:\n\
        1. What this output means in simple terms\n\
        2. Key insights or important information from the results\n\
        3. Any warnings or things to pay attention to\n\
        4. Whether the plan achieved its intended goal\n\
        5. Any follow-up actions the user might need to take\n\n\
        Keep the explanation conversational and easy to understand.",
        commands_summary, plan.notes, output
    );

    let messages = vec![ChatMessage {
        role: ChatMessageRole::User,
        content: explanation_prompt,
    }];

    let completion_budget = compute_completion_token_budget(config.models.max_tokens, &messages);

    let request = ChatCompletionRequest {
        model: config.models.planner.clone(),
        messages,
        max_tokens: Some(completion_budget),
        temperature: Some(0.7),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Failed to get AI explanation")?;

    if let Some(choice) = response.choices.first() {
        println!("{}", choice.message.content);
    }

    println!();

    Ok(())
}

async fn handle_intelligence(
    question_flag: Option<String>,
    task: Vec<String>,
    config: &Config,
) -> Result<()> {
    use std::process::Command;

    if task.is_empty() {
        bail!("Intelligence mode requires a command to execute and explain");
    }

    // Determine question and command inputs
    let mut question = question_flag
        .map(|q| q.trim().to_owned())
        .filter(|q| !q.is_empty());

    let command_str = if question.is_some() || task.len() == 1 {
        task.join(" ").trim().to_owned()
    } else {
        let potential_command = task.last().unwrap().trim().to_owned();
        let potential_question = task[..task.len() - 1].join(" ").trim().to_owned();

        if potential_question.is_empty() {
            task.join(" ").trim().to_owned()
        } else {
            let looks_like_question =
                potential_question.ends_with('?') || potential_question.contains('?');
            let command_has_whitespace = potential_command.contains(char::is_whitespace);
            let command_starts_with_flag = potential_command.starts_with('-');

            if looks_like_question || command_has_whitespace {
                question = Some(potential_question);
                potential_command
            } else if command_starts_with_flag {
                task.join(" ").trim().to_owned()
            } else {
                task.join(" ").trim().to_owned()
            }
        }
    };

    if command_str.is_empty() {
        bail!("Intelligence mode requires a command to execute and explain");
    }

    println!("🧠 AI Intelligence Mode");
    println!("🔧 Executing: {}", command_str);
    println!();

    // Execute the command and capture output
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command_str)
        .output()
        .context("Failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Show the raw output
    if !stdout.trim().is_empty() {
        println!("📤 Command Output:");
        println!("{}", stdout);
    }

    if !stderr.trim().is_empty() {
        println!("⚠️  Error Output:");
        println!("{}", stderr);
    }

    println!();

    // Prepare the explanation prompt
    let base_output = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };
    let explanation_prompt = if let Some(question) = question {
        format!(
            "A user asked the following question about a command they ran:\n\
            Question: {}\n\
            Command: '{}'\n\
            Output:\n{}\n\
            Please answer the question directly, referencing the command output.\n\
            Include any helpful context, summaries, and actionable insights the user should know.",
            question, command_str, base_output
        )
    } else {
        format!(
            "Please explain the following command output in a clear, human-friendly way.\n\
            The command executed was: '{}'\n\n\
            Output:\n{}\n\
            Please provide:\n\
            1. What this output means in simple terms\n\
            2. Key insights or important information\n\
            3. Any warnings or things to pay attention to\n\
            4. What a user should understand from this result\n\
            Keep the explanation conversational and easy to understand for someone who might not be familiar with this command.",
            command_str, base_output
        )
    };

    println!("🤖 AI Explanation:");
    println!();

    let client = AIClient::new(&config.llm)?;

    let request = ChatCompletionRequest {
        model: config.models.planner.clone(),
        messages: vec![ChatMessage {
            role: ChatMessageRole::User,
            content: explanation_prompt,
        }],
        max_tokens: Some(config.models.max_tokens),
        temperature: Some(0.7),
    };

    let response = client
        .chat_completion(request)
        .await
        .context("Failed to get AI explanation")?;

    if let Some(choice) = response.choices.first() {
        println!("{}", choice.message.content);
    }

    println!();

    Ok(())
}
