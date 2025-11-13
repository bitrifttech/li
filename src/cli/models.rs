use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use std::io::{self, Write};

use crate::config::LlmProvider;

#[derive(Debug, Deserialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pricing: Option<Pricing>,
    #[serde(default)]
    pub context_length: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
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

pub(crate) async fn fetch_openrouter_free_models(api_key: &str) -> Result<Vec<OpenRouterModel>> {
    if api_key.trim().is_empty() {
        return Err(anyhow!(
            "{} API key not configured. Set {} or run 'li --setup'.",
            LlmProvider::OpenRouter.display_name(),
            LlmProvider::OpenRouter.api_key_env_var()
        ));
    }

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

pub(crate) fn prompt_model_index(models: &[OpenRouterModel], label: &str) -> Result<usize> {
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
