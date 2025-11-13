mod parsing;
mod prompt;
mod session;
mod transport;
mod types;

pub use types::Plan;

use crate::client::DynLlmClient;
use anyhow::Result;
use session::{default_question_resolver, interactive_plan_with_resolver};

pub async fn plan(
    client: &DynLlmClient,
    request: &str,
    model: &str,
    max_tokens: u32,
) -> Result<Plan> {
    interactive_plan_with_resolver(
        client,
        request,
        model,
        max_tokens,
        &default_question_resolver,
    )
    .await
}

#[cfg(test)]
mod tests;
