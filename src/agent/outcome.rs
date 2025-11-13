use crate::planner::Plan;
use crate::validator::ValidationResult;

use super::types::StageKind;

/// Summary of command execution produced by the executor stage.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionReport {
    pub commands: Vec<String>,
    pub success: bool,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub notes: Vec<String>,
}

impl ExecutionReport {
    pub fn skipped(note: impl Into<String>) -> Self {
        Self {
            commands: Vec::new(),
            success: false,
            stdout: None,
            stderr: None,
            notes: vec![note.into()],
        }
    }
}

/// High-level recovery outcome used to log recovery attempts.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOutcome {
    Skipped,
    AlternativeApplied { command: String },
    Installed { command: String },
    Cancelled,
}

/// Terminal result returned by the agent orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentOutcome {
    Planned {
        plan: Option<Plan>,
        validation: Option<ValidationResult>,
        execution: Option<ExecutionReport>,
        recovery: Option<RecoveryOutcome>,
    },
    AwaitingClarification {
        question: String,
        context: String,
    },
    Cancelled {
        reason: String,
    },
    Failed {
        stage: StageKind,
        error: String,
    },
}

impl AgentOutcome {
    pub fn failed(stage: StageKind, error: impl Into<String>) -> Self {
        Self::Failed {
            stage,
            error: error.into(),
        }
    }
}
