use anyhow::Result;
use std::io::{self, Write};

use crate::config::LlmProvider;

const PROVIDER_CHOICES: &[LlmProvider] = &[LlmProvider::OpenRouter, LlmProvider::Cerebras];

fn provider_description(provider: LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenRouter => "OpenRouter marketplace of hosted inference models",
        LlmProvider::Cerebras => "Cerebras Inference deployment",
    }
}

pub(crate) fn print_provider_list() {
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

pub(crate) fn prompt_provider_interactive(current: Option<LlmProvider>) -> Result<LlmProvider> {
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

pub(crate) fn prompt_api_key_for_provider(
    provider: LlmProvider,
    existing: Option<&str>,
) -> Result<String> {
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
