use anyhow::{Context, Result, bail};

use crate::client::{AIClient, ChatCompletionRequest, ChatMessage, ChatMessageRole, LlmClient};
use crate::config::Config;

use super::args::ChatArgs;

pub(crate) async fn handle_chat(args: ChatArgs, config: &Config) -> Result<()> {
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

pub(crate) async fn handle_chat_direct(prompt: &str, config: &Config) -> Result<()> {
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
