use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use serde_json;
use std::fs;
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole};
use crate::config::Config;
use crate::{classifier, planner};

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

async fn select_model_interactively(models: Vec<OpenRouterModel>) -> Result<String> {
    println!("\n🤖 Available OpenRouter Free Models:\n");
    
    for (idx, model) in models.iter().enumerate() {
        let context_len = model.context_length
            .map(|len| format!(" ({} context)", len))
            .unwrap_or_default();
        println!("  {}. {}{}", idx + 1, model.name, context_len);
    }
    
    print!("\nSelect a model (1-{}): ", models.len());
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let selection: usize = input.trim()
        .parse()
        .context("Please enter a valid number")?;
    
    if selection == 0 || selection > models.len() {
        return Err(anyhow!("Please select a number between 1 and {}", models.len()));
    }
    
    Ok(models[selection - 1].id.clone())
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

    /// Enable classification before planning (use in shell hook mode)
    #[arg(short = 'c', long = "classify")]
    pub classify: bool,

    /// Override the model (for OpenRouter, fetches free models list)
    #[arg(short = 'm', long = "model", num_args = 0..=1, default_missing_value = "")]
    pub model: Option<String>,

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

    /// Set classifier model
    #[arg(long)]
    pub classifier_model: Option<String>,

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
        handle_setup().await
    }

    pub async fn run(self, mut config: Config) -> Result<()> {
        // Check for empty task (show welcome message)
        let prompt = self.task.join(" ").trim().to_owned();
        if prompt.is_empty() && !self.setup && !self.chat && !self.intelligence && !self.config && self.command.is_none() && self.model.is_none() && 
           self.api_key.is_none() && self.timeout.is_none() && self.max_tokens.is_none() && 
           self.classifier_model.is_none() && self.planner_model.is_none() {
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
            println!("   • Powered by OpenRouter's free AI models");
            println!();
            
            if !config_exists {
                println!("⚠️  Configuration not found. Let's get you set up!");
                println!("   Run: li --setup");
                println!();
            }
            
            println!("💡 How to use li:");
            println!("   li --setup                                        # Interactive first-time setup");
            println!("   li 'list all files in current directory'          # Plan & execute commands");
            println!("   li --chat 'what is the capital of France?'        # Direct AI conversation");
            println!("   li --classify 'git status'                        # Classify input only");
            println!("   li -i 'df -h'                                     # Explain command output with AI");
            println!("   li -i -q 'Which disk has the most space?' 'df -h' # Ask a question about output");
            println!("   li --model                                        # Interactive model selection");
            println!("   li --model list                                   # Show available models");
            println!("   li --config --api-key YOUR_KEY                    # Set API key manually");
            println!("   li --config --timeout 60                          # Set timeout in seconds");
            println!("   li --config --max-tokens 4096                     # Set max tokens");
            println!("   li --config --classifier-model MODEL              # Set classifier model");
            println!("   li --config --planner-model MODEL                 # Set planner model");
            println!();
            
            if config_exists {
                // Load config just to show current settings
                match Config::load() {
                    Ok(loaded_config) => {
                        println!("📋 Your current configuration:");
                        println!("   Provider: OpenRouter");
                        println!("   Classifier: {}", loaded_config.classifier_model);
                        println!("   Planner: {}", loaded_config.planner_model);
                        println!("   Timeout: {}s", loaded_config.timeout_secs);
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
        
        // Handle model override
        if let Some(ref model_arg) = self.model {
            let models = fetch_openrouter_free_models(&config.api_key).await?;
            if model_arg == "list" {
                // Just list the models
                for model in models {
                    let context_len = model.context_length
                        .map(|len| format!(" ({} context)", len))
                        .unwrap_or_default();
                    println!("{}: {}{}", model.id, model.name, context_len);
                }
                return Ok(());
            } else if model_arg == "interactive" || model_arg.is_empty() {
                // Interactive selection for both classifier and planner models
                println!("\n🤖 Available Free Models:\n");
                for (idx, model) in models.iter().enumerate() {
                    let context_len = model.context_length
                        .map(|len| format!(" ({} context)", len))
                        .unwrap_or_default();
                    println!("  {}. {}{}", idx + 1, model.name, context_len);
                }
                
                // Get classifier model
                let classifier_model = loop {
                    print!("\n🧠 Select classifier model (determines if input is a command or needs planning): ");
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
                            break models[num - 1].id.clone();
                        }
                        Ok(_) => println!("❌ Please enter a number between 1 and {}.", models.len()),
                        Err(_) => println!("❌ Please enter a valid number."),
                    }
                };
                
                // Get planner model
                let planner_model = loop {
                    print!("\n📋 Select planner model (creates shell commands from natural language): ");
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
                            break models[num - 1].id.clone();
                        }
                        Ok(_) => println!("❌ Please enter a number between 1 and {}.", models.len()),
                        Err(_) => println!("❌ Please enter a valid number."),
                    }
                };
                
                // Update config
                config.classifier_model = classifier_model.clone();
                config.planner_model = planner_model.clone();
                
                // Save updated config
                let config_path = Config::config_path()?;
                if let Some(parent) = config_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                let config_json = serde_json::json!({
                    "openrouter_api_key": config.api_key,
                    "timeout_secs": config.timeout_secs,
                    "max_tokens": config.max_tokens,
                    "classifier_model": config.classifier_model,
                    "planner_model": config.planner_model,
                });
                
                fs::write(&config_path, serde_json::to_string_pretty(&config_json)?)?;
                
                println!("\n✅ Model configuration saved to {}", config_path.display());
                println!("📋 Updated configuration:");
                println!("   Classifier Model: {}", config.classifier_model);
                println!("   Planner Model: {}", config.planner_model);
                
                return Ok(());
            } else {
                // Check if the model is in the free list
                if !models.iter().any(|m| m.id == *model_arg) {
                    println!("Model '{}' not found in free models list.", model_arg);
                    println!("Use 'li -m list' to see available free models.");
                    println!("Or use 'li -m' to select interactively.");
                    return Ok(());
                }
                config.planner_model = model_arg.clone();
                config.classifier_model = model_arg.clone();
            }
        }
        
        // Handle intelligence flag
        if self.intelligence {
            handle_intelligence(self.question.clone(), self.task, &config).await?;
            return Ok(());
        }
        
        // Handle config flags
        if self.config || self.api_key.is_some() || self.timeout.is_some() || self.max_tokens.is_some() || 
           self.classifier_model.is_some() || self.planner_model.is_some() {
            handle_config_direct(&self, &mut config).await?;
            return Ok(());
        }
        
        match self.command {
            Some(Command::Chat(args)) => handle_chat(args, &config).await?,
            None => handle_task(self.task, self.classify, &config).await?,
        }
        Ok(())
    }
}

async fn handle_chat(args: ChatArgs, config: &Config) -> Result<()> {
    let prompt = args.prompt.join(" ").trim().to_owned();
    if prompt.is_empty() {
        bail!("Prompt cannot be empty");
    }

    let model = args.model.unwrap_or_else(|| config.planner_model.clone());
    let max_tokens = args.max_tokens.unwrap_or(config.max_tokens);
    let temperature = args.temperature;

    let client = AIClient::new(config)?;
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

    println!("Provider: OpenRouter");
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


async fn handle_task(words: Vec<String>, classify: bool, config: &Config) -> Result<()> {
    let prompt = words.join(" ").trim().to_owned();
    if prompt.is_empty() {
        println!(
            "li CLI is initialized. Provide a task or run `li --chat \"your question\"` to call OpenRouter."
        );
        return Ok(());
    }

    let client = AIClient::new(config)?;

    // Only classify if --classify flag is set (used by shell hook)
    let plan = if classify {
        let classification = classifier::classify(&client, &prompt, &config.classifier_model).await
            .map_err(|e| {
                if e.to_string().contains("Rate limit") {
                    anyhow!("Classification service is rate limited. Please try again in a moment.")
                } else if e.to_string().contains("Invalid API key") {
                    anyhow!("Invalid API key for classification. Please check your configuration.")
                } else {
                    e
                }
            })?;
        
        println!("Provider: OpenRouter");
        println!("Classifier Model: {}", config.classifier_model);
        match classification {
            classifier::Classification::Terminal => {
                // Direct execution for terminal commands
                println!("Executing direct terminal command: {}", prompt);
                let success = run_command(&prompt).await?;
                if !success {
                    bail!("Command failed: {}", prompt);
                }
                return Ok(());
            }
            classifier::Classification::NaturalLanguage => {
                // Planning for natural language
                planner::plan(&client, &prompt, &config.planner_model, config.max_tokens).await
                    .map_err(|e| {
                        if e.to_string().contains("Rate limit") {
                            anyhow!("Planner service is rate limited. Please wait a moment and try again.")
                        } else if e.to_string().contains("high traffic") {
                            anyhow!("Planner service is experiencing high traffic. Please try again soon.")
                        } else if e.to_string().contains("Planning cancelled") {
                            e // Keep the cancellation message as-is
                        } else {
                            e
                        }
                    })?
            }
        }
    } else {
        // Direct planning without classification
        planner::plan(&client, &prompt, &config.planner_model, config.max_tokens).await
            .map_err(|e| {
                if e.to_string().contains("Rate limit") {
                    anyhow!("Planner service is rate limited. Please wait a moment and try again.")
                } else if e.to_string().contains("high traffic") {
                    anyhow!("Planner service is experiencing high traffic. Please try again soon.")
                } else if e.to_string().contains("Planning cancelled") {
                    e // Keep the cancellation message as-is
                } else {
                    e
                }
            })?
    }; 
    render_plan(&plan, config);

    if prompt_for_approval()? {
        execute_plan(&plan).await?;
    } else {
        println!("\nPlan execution cancelled.");
    }

    Ok(())
}

fn render_plan(plan: &planner::Plan, config: &Config) {
    println!("\nProvider: OpenRouter");
    println!("Model: {}", config.planner_model);
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

fn prompt_for_approval() -> Result<bool> {
    print!("\nExecute this plan? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

async fn execute_plan(plan: &planner::Plan) -> Result<()> {
    println!("\n=== Executing Plan ===");

    if !plan.dry_run_commands.is_empty() {
        println!("\n[Dry-run Phase]");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!("\n> Running check {}/{}: {}", idx + 1, plan.dry_run_commands.len(), cmd);
            let success = run_command(cmd).await?;
            if !success {
                bail!("Dry-run check failed: {}", cmd);
            }
        }
        println!("\n✓ All dry-run checks passed.");
    }

    if !plan.execute_commands.is_empty() {
        println!("\n[Execute Phase]");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!("\n> Executing {}/{}: {}", idx + 1, plan.execute_commands.len(), cmd);
            let success = run_command(cmd).await?;
            if !success {
                bail!("Command failed: {}", cmd);
            }
        }
        println!("\n✓ Plan execution completed.");
    }

    Ok(())
}

async fn run_command(cmd: &str) -> Result<bool> {
    // Force ls to use colors if it's an ls command
    let modified_cmd = if cmd.starts_with("ls ") || cmd == "ls" {
        cmd.replace("ls", "ls --color=always")
    } else {
        cmd.to_string()
    };
    
    // Print command output separator
    println!("\n┌─ COMMAND OUTPUT: {}", cmd);
    println!("│");
    
    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(&modified_cmd)
        .env("FORCE_COLOR", "1")           // Generic color forcing (npm, yarn, etc.)
        .env("CLICOLOR_FORCE", "1")        // BSD/macOS color forcing
        .env("COLORTERM", "truecolor")     // Advertise true color support
        .env("TERM", "xterm-256color")     // 256 color support
        .env("GIT_CONFIG_PARAMETERS", "'color.ui=always'")  // Force git colors
        .env("LS_COLORS", "di=1;34:fi=0:ln=1;36:pi=40;33:so=1;35:do=1;35:bd=40;33;01:cd=40;33;01:or=40;31;01:ex=1;32:*.tar=1;31:*.tgz=1;31:*.zip=1;31:*.gz=1;31:*.bz2=1;31:*.deb=1;31:*.rpm=1;31:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:*.mpg=1;37:*.avi=1;37:*.gl=1;37:*.dl=1;37:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:")  // Standard ls colors
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!("Command not found: {}. Please ensure the command exists in your PATH.", cmd)
            } else {
                anyhow!("Failed to execute command '{}': {}", cmd, e)
            }
        })?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let stdout_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            println!("│ {}", line);
        }
    });

    let stderr_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            eprintln!("│ {}", line);
        }
    });

    let status = child.wait().await
        .map_err(|e| anyhow!("Failed to wait for command completion: {}", e))?;

    stdout_handle.await
        .map_err(|e| anyhow!("Failed to read command output: {}", e))?;
    stderr_handle.await
        .map_err(|e| anyhow!("Failed to read command errors: {}", e))?;

    // Print closing separator
    if status.success() {
        println!("│");
        println!("└─ Command completed successfully");
        Ok(true)
    } else {
        println!("│");
        if let Some(code) = status.code() {
            println!("└─ Command failed with exit code {}", code);
            Err(anyhow!("Command failed with exit code {}: {}", code, cmd))
        } else {
            println!("└─ Command was terminated by signal");
            Err(anyhow!("Command was terminated by signal: {}", cmd))
        }
    }
}

async fn handle_config_direct(args: &Cli, config: &mut Config) -> Result<()> {
    use std::fs;
    use serde_json;
    
    let config_path = Config::config_path()?;
    
    // Load existing config to preserve values not being updated
    let mut existing_config = if config_path.exists() {
        Config::load()?
    } else {
        config.clone()
    };
    
    if let Some(ref api_key) = args.api_key {
        existing_config.api_key = api_key.clone();
    }
    
    if let Some(timeout) = args.timeout {
        existing_config.timeout_secs = timeout;
    }
    
    if let Some(max_tokens) = args.max_tokens {
        existing_config.max_tokens = max_tokens;
    }
    
    if let Some(ref classifier_model) = args.classifier_model {
        existing_config.classifier_model = classifier_model.clone();
    }
    
    if let Some(ref planner_model) = args.planner_model {
        existing_config.planner_model = planner_model.clone();
    }
    
    // Create config directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Save config
    let config_json = serde_json::json!({
        "openrouter_api_key": existing_config.api_key,
        "timeout_secs": existing_config.timeout_secs,
        "max_tokens": existing_config.max_tokens,
        "classifier_model": existing_config.classifier_model,
        "planner_model": existing_config.planner_model,
    });
    
    fs::write(&config_path, serde_json::to_string_pretty(&config_json)?)?;
    
    println!("✅ Configuration saved to {}", config_path.display());
    println!("📋 Current configuration:");
    println!("   Provider: OpenRouter");
    println!("   API Key: {}***", &existing_config.api_key[..existing_config.api_key.len().min(8)]);
    println!("   Timeout: {}s", existing_config.timeout_secs);
    println!("   Max Tokens: {}", existing_config.max_tokens);
    println!("   Classifier Model: {}", existing_config.classifier_model);
    println!("   Planner Model: {}", existing_config.planner_model);
    
    Ok(())
}

async fn handle_setup() -> Result<()> {
    println!("🚀 Welcome to li CLI Setup!");
    println!("Let's configure your OpenRouter integration.\n");
    
    // Get API key
    let api_key = loop {
        print!("🔑 Enter your OpenRouter API key: ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let key = input.trim();
        
        if key.is_empty() {
            println!("❌ API key cannot be empty. Please try again.");
            continue;
        }
        
        if key.starts_with("sk-or-v1") {
            break key.to_string();
        } else {
            println!("⚠️  OpenRouter API keys typically start with 'sk-or-v1'. Are you sure this is correct?");
            print!("Continue anyway? [y/N]: ");
            io::stdout().flush()?;
            
            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;
            if confirm.trim().to_lowercase() == "y" {
                break key.to_string();
            }
        }
    };
    
    // Get timeout
    let timeout = loop {
        print!("⏱️  Enter timeout in seconds (default: 30): ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let timeout_str = input.trim();
        
        if timeout_str.is_empty() {
            break 30u64;
        }
        
        match timeout_str.parse::<u64>() {
            Ok(timeout) if timeout > 0 => break timeout,
            Ok(_) => println!("❌ Timeout must be a positive number."),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    };
    
    println!("\n📡 Fetching available free models from OpenRouter...");
    let models = fetch_openrouter_free_models(&api_key).await?;
    
    println!("\n🤖 Available Free Models:\n");
    for (idx, model) in models.iter().enumerate() {
        let context_len = model.context_length
            .map(|len| format!(" ({} context)", len))
            .unwrap_or_default();
        println!("  {}. {}{}", idx + 1, model.name, context_len);
    }
    
    // Get classifier model
    let classifier_model = loop {
        print!("\n🧠 Select classifier model (determines if input is a command or needs planning): ");
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
                break models[num - 1].id.clone();
            }
            Ok(_) => println!("❌ Please enter a number between 1 and {}.", models.len()),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    };
    
    // Get planner model
    let planner_model = loop {
        print!("\n📋 Select planner model (creates shell commands from natural language): ");
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
                break models[num - 1].id.clone();
            }
            Ok(_) => println!("❌ Please enter a number between 1 and {}.", models.len()),
            Err(_) => println!("❌ Please enter a valid number."),
        }
    };
    
    // Create config
    let config = Config {
        api_key,
        timeout_secs: timeout,
        max_tokens: 100000,
        classifier_model: classifier_model.clone(),
        planner_model: planner_model.clone(),
    };
    
    // Save config
    let config_path = Config::config_path()?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let config_json = serde_json::json!({
        "openrouter_api_key": config.api_key,
        "timeout_secs": config.timeout_secs,
        "max_tokens": config.max_tokens,
        "classifier_model": config.classifier_model,
        "planner_model": config.planner_model,
    });
    
    fs::write(&config_path, serde_json::to_string_pretty(&config_json)?)?;
    
    println!("\n✅ Configuration saved to {}", config_path.display());
    println!("📋 Your configuration:");
    println!("   Provider: OpenRouter");
    println!("   API Key: {}***", &config.api_key[..config.api_key.len().min(8)]);
    println!("   Timeout: {}s", config.timeout_secs);
    println!("   Max Tokens: {}", config.max_tokens);
    println!("   Classifier Model: {}", config.classifier_model);
    println!("   Planner Model: {}", config.planner_model);
    println!("\n🎉 Setup complete! You can now use 'li' with commands like:");
    println!("   li 'list all files in current directory'");
    println!("   li --chat 'what is the capital of France?'");
    println!("   li -m list  # to see available models\n");
    
    Ok(())
}

async fn handle_chat_direct(prompt: &str, config: &Config) -> Result<()> {
    let client = AIClient::new(config)?;
    
    let request = ChatCompletionRequest {
        model: config.planner_model.clone(),
        messages: vec![
            ChatMessage {
                role: ChatMessageRole::User,
                content: prompt.to_string(),
            },
        ],
        max_tokens: Some(config.max_tokens),
        temperature: Some(0.7),
    };
    
    let response = client
        .chat_completion(request)
        .await
        .context("Chat completion failed")?;
    
    println!("Provider: OpenRouter");
    println!("Model: {}", config.planner_model);
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
            let looks_like_question = potential_question.ends_with('?')
                || potential_question.contains('?');
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
    let base_output = if stdout.trim().is_empty() { &stderr } else { &stdout };
    let explanation_prompt = if let Some(question) = question {
        format!(
            "A user asked the following question about a command they ran:\n\
            Question: {}\n\
            Command: '{}'\n\
            Output:\n{}\n\
            Please answer the question directly, referencing the command output.\n\
            Include any helpful context, summaries, and actionable insights the user should know.",
            question,
            command_str,
            base_output
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
            command_str,
            base_output
        )
    };

    println!("🤖 AI Explanation:");
    println!();

    let client = AIClient::new(config)?;

    let request = ChatCompletionRequest {
        model: config.planner_model.clone(),
        messages: vec![ChatMessage {
            role: ChatMessageRole::User,
            content: explanation_prompt,
        }],
        max_tokens: Some(config.max_tokens),
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
