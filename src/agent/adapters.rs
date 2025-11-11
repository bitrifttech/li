#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;

use crate::classifier::{self, Classification};
use crate::client::AIClient;
use crate::planner::{self, Plan};
use crate::validator::{self, ValidationResult};

use super::context::AgentContext;
use super::outcome::{ExecutionReport, RecoveryOutcome};

#[async_trait]
pub trait ClassificationAdapter {
    async fn classify(&self, context: &AgentContext) -> Result<Classification>;
}

#[async_trait]
pub trait PlanningAdapter {
    async fn plan(&self, context: &AgentContext) -> Result<Plan>;
}

#[async_trait]
pub trait ValidationAdapter {
    async fn validate(&self, context: &AgentContext, plan: &Plan) -> Result<ValidationResult>;
}

#[async_trait]
pub trait ExecutionAdapter {
    async fn execute(&self, context: &AgentContext, plan: &Plan) -> Result<ExecutionReport>;
}

#[async_trait]
pub trait RecoveryAdapter {
    async fn recover(&self, context: &AgentContext) -> Result<RecoveryOutcome>;
}

/// Adapter that invokes the existing classifier module with a fresh AI client.
pub struct DirectClassifierAdapter;

#[async_trait]
impl ClassificationAdapter for DirectClassifierAdapter {
    async fn classify(&self, context: &AgentContext) -> Result<Classification> {
        let client = AIClient::new(&context.config.llm)?;
        classifier::classify(
            &client,
            &context.request.task,
            &context.config.models.classifier,
        )
        .await
    }
}

/// Adapter that invokes the existing planner module.
pub struct DirectPlanningAdapter;

#[async_trait]
impl PlanningAdapter for DirectPlanningAdapter {
    async fn plan(&self, context: &AgentContext) -> Result<Plan> {
        let client = AIClient::new(&context.config.llm)?;
        planner::plan(
            &client,
            &context.request.task,
            &context.config.models.planner,
            context.config.models.max_tokens,
        )
        .await
    }
}

/// Adapter that wraps `validator::CommandValidator`.
pub struct CommandValidationAdapter;

#[async_trait]
impl ValidationAdapter for CommandValidationAdapter {
    async fn validate(&self, _context: &AgentContext, plan: &Plan) -> Result<ValidationResult> {
        let mut validator = validator::CommandValidator::new();
        validator.validate_plan(plan).await
    }
}

/// Placeholder execution adapter that captures the intent without running commands.
pub struct NoopExecutionAdapter;

#[async_trait]
impl ExecutionAdapter for NoopExecutionAdapter {
    async fn execute(&self, _context: &AgentContext, plan: &Plan) -> Result<ExecutionReport> {
        Ok(ExecutionReport {
            commands: plan.execute_commands.clone(),
            success: false,
            stdout: None,
            stderr: None,
            notes: vec!["Execution adapter not configured".to_string()],
        })
    }
}

/// Placeholder recovery adapter that marks the stage as skipped.
pub struct NoopRecoveryAdapter;

#[async_trait]
impl RecoveryAdapter for NoopRecoveryAdapter {
    async fn recover(&self, _context: &AgentContext) -> Result<RecoveryOutcome> {
        Ok(RecoveryOutcome::Skipped)
    }
}
