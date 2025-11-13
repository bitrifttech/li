use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub confidence: f32,
    pub dry_run_commands: Vec<String>,
    pub execute_commands: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum PlannerResponse {
    Plan {
        confidence: f32,
        dry_run_commands: Vec<String>,
        execute_commands: Vec<String>,
        notes: String,
    },
    Question {
        text: String,
        context: String,
    },
}

pub(crate) type QuestionResolver = dyn Fn(&str, &str) -> Result<String> + Send + Sync;
