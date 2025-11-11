#![allow(dead_code)]

use std::fmt;

/// Logical stages in the agent pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKind {
    Classification,
    Planning,
    Validation,
    Execution,
    Recovery,
}

impl fmt::Display for StageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            StageKind::Classification => "classification",
            StageKind::Planning => "planning",
            StageKind::Validation => "validation",
            StageKind::Execution => "execution",
            StageKind::Recovery => "recovery",
        };
        write!(f, "{label}")
    }
}
