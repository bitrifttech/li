use crate::client::ChatMessage;

/// Conservative estimate of token usage for a single message content.
fn estimate_token_count(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }

    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();

    // Use both approximations and pick the larger one to stay conservative.
    let approx_from_chars = (char_count + 2) / 3; // ~3 chars per token, rounded up.
    let approx_from_words = word_count;

    approx_from_chars.max(approx_from_words) as u32
}

/// Estimate the total tokens contributed by a sequence of chat messages.
pub fn estimate_prompt_tokens(messages: &[ChatMessage]) -> u32 {
    messages
        .iter()
        .map(|message| estimate_token_count(&message.content) + 4) // small buffer per message metadata
        .sum()
}

/// Safety margin to reserve for planner responses so we stay under model limits.
pub const REQUEST_COMPLETION_SAFETY_MARGIN_TOKENS: u32 = 256;

/// Minimum completion tokens to request to avoid overly truncated answers when possible.
pub const MIN_COMPLETION_TOKENS: u32 = 32;

/// Derive a completion token budget given a context limit and prepared prompt messages.
pub fn compute_completion_token_budget(max_context_tokens: u32, messages: &[ChatMessage]) -> u32 {
    let prompt_tokens = estimate_prompt_tokens(messages);
    let max_possible_completion = max_context_tokens.saturating_sub(prompt_tokens);

    if max_possible_completion == 0 {
        return 1;
    }

    let available = max_context_tokens
        .saturating_sub(prompt_tokens)
        .saturating_sub(REQUEST_COMPLETION_SAFETY_MARGIN_TOKENS);

    let desired = available.max(MIN_COMPLETION_TOKENS);

    desired.min(max_possible_completion).max(1)
}
