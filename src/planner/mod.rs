use anyhow::Result;

#[derive(Debug)]
pub struct Plan;

pub fn plan(_request: &str) -> Result<Plan> {
    // TODO: integrate with Cerebras Qwen planner
    Ok(Plan)
}
