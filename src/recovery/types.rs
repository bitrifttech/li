use serde::Deserialize;
use std::fmt;

use crate::planner::Plan;
use crate::validator::MissingCommand;

/// Main recovery engine structure
pub struct RecoveryEngine {
    pub client: crate::client::AIClient,
    pub config: crate::config::Config,
    pub available_tools: Vec<String>,
}

/// Recovery options presented to the user
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    pub command_alternatives: Vec<CommandAlternative>,
    pub installation_instructions: Vec<InstallationInstruction>,
    pub can_skip_step: bool,
    pub retry_possible: bool,
}

/// Alternative command suggestion
#[derive(Debug, Clone)]
pub struct CommandAlternative {
    pub command: String,
    pub description: String,
    pub confidence: f32,
}

impl fmt::Display for CommandAlternative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.command, self.description)
    }
}

/// Installation instruction for missing commands
#[derive(Debug, Clone)]
pub struct InstallationInstruction {
    pub command: String,
    pub install_commands: Vec<String>,
    pub package_managers: Vec<String>,
    pub confidence: f32,
}

impl fmt::Display for InstallationInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Install using: {}", self.install_commands.join(", "))
    }
}

/// User's recovery choice
#[derive(Debug, Clone)]
pub enum RecoveryChoice {
    UseAlternative(usize),
    InstallCommand(usize),
    SkipStep,
    AbortPlan,
    RetryOriginal,
}

/// Recovery execution result
#[derive(Debug)]
pub enum RecoveryResult {
    AlternativeSucceeded(CommandAlternative),
    AlternativeFailed(CommandAlternative),
    InstallationSucceeded(InstallationInstruction),
    InstallationFailed(InstallationInstruction),
    InstallationCancelled,
    StepSkipped,
    PlanAborted(String),
    RetryRequested,
    RetryWithDifferentApproach,
}

/// Context for recovery operations
#[derive(Debug)]
pub struct RecoveryContext {
    pub missing_command: MissingCommand,
    pub original_plan: Plan,
    pub original_goal: String,
}

/// AI response structure for recovery suggestions
#[derive(Debug, Deserialize)]
pub struct RecoveryResponse {
    pub alternatives: Vec<AlternativeResponse>,
    pub installation_instructions: Vec<InstallResponse>,
    pub can_skip: bool,
    pub original_goal_achievable: bool,
}

/// Individual alternative command from AI
#[derive(Debug, Deserialize)]
pub struct AlternativeResponse {
    pub command: String,
    pub description: String,
    pub confidence: f32,
}

/// Installation instruction from AI
#[derive(Debug, Deserialize)]
pub struct InstallResponse {
    pub command: String,
    pub description: String,
    pub platform: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    InstallationFirst,
    SkipOnError,
    NeverRecover,
}

impl RecoveryOptions {
    /// Create recovery options that only allow skipping
    pub fn skip_only() -> Self {
        Self {
            command_alternatives: Vec::new(),
            installation_instructions: Vec::new(),
            can_skip_step: true,
            retry_possible: false,
        }
    }
}
