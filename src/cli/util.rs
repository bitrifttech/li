use anyhow::Result;
use std::io::{self, IsTerminal, Read, Write};

use crate::config::DEFAULT_MAX_TOKENS;

const CONTEXT_HEADROOM_TOKENS: usize = 1024;

pub(crate) fn derive_max_tokens(context_length: Option<usize>) -> u32 {
    context_length
        .map(|len| len.saturating_sub(CONTEXT_HEADROOM_TOKENS))
        .filter(|&len| len > 0)
        .map(|len| len.min(u32::MAX as usize) as u32)
        .filter(|&tokens| tokens > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS)
}

pub(crate) fn read_piped_stdin() -> Result<Option<String>> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }

    let mut buffer = String::new();
    stdin.lock().read_to_string(&mut buffer)?;

    if buffer.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(buffer))
    }
}

pub(crate) fn prompt_timeout(default: u64) -> Result<u64> {
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

pub(crate) fn prompt_string_with_default(prompt: &str, default: &str) -> Result<String> {
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

pub(crate) fn prompt_u32_with_default(prompt: &str, default: u32) -> Result<u32> {
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

pub(crate) fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return "(not set)".to_string();
    }

    let visible = key.len().min(8);
    format!("{}***", &key[..visible])
}
