use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::client::{DefaultLlmClientFactory, LlmClientFactory};
use crate::exec;
use crate::planner::{self, Plan};
use crate::validator::{self, ValidationResult};

use super::context::AgentContext;
use super::outcome::{ExecutionReport, RecoveryOutcome};

#[async_trait]
pub trait PlanningAdapter {
    async fn plan(&self, context: &mut AgentContext) -> Result<Plan>;
}

#[async_trait]
pub trait ValidationAdapter {
    async fn validate(&self, context: &mut AgentContext, plan: &Plan) -> Result<ValidationResult>;
}

#[async_trait]
pub trait ExecutionAdapter {
    async fn execute(&self, context: &mut AgentContext, plan: &Plan) -> Result<ExecutionReport>;
}

#[async_trait]
pub trait RecoveryAdapter {
    async fn recover(&self, context: &mut AgentContext) -> Result<RecoveryOutcome>;
}

/// Adapter that invokes the existing planner module.
pub struct DirectPlanningAdapter {
    factory: Arc<dyn LlmClientFactory>,
}

impl DirectPlanningAdapter {
    pub fn new(factory: Arc<dyn LlmClientFactory>) -> Self {
        Self { factory }
    }
}

impl Default for DirectPlanningAdapter {
    fn default() -> Self {
        Self::new(Arc::new(DefaultLlmClientFactory::default()))
    }
}

#[async_trait]
impl PlanningAdapter for DirectPlanningAdapter {
    async fn plan(&self, context: &mut AgentContext) -> Result<Plan> {
        let client = context.llm_client(self.factory.as_ref())?;
        planner::plan(
            client.as_ref(),
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
    async fn validate(&self, _context: &mut AgentContext, plan: &Plan) -> Result<ValidationResult> {
        let mut validator = validator::CommandValidator::new();
        validator.validate_plan(plan).await
    }
}

/// Placeholder execution adapter that captures the intent without running commands.
pub struct NoopExecutionAdapter;

#[async_trait]
impl ExecutionAdapter for NoopExecutionAdapter {
    async fn execute(&self, _context: &mut AgentContext, plan: &Plan) -> Result<ExecutionReport> {
        Ok(ExecutionReport {
            commands: plan.execute_commands.clone(),
            success: false,
            stdout: None,
            stderr: None,
            notes: vec!["Execution adapter not configured".to_string()],
        })
    }
}

/// Execution adapter that delegates to the shared plan executor when permitted.
pub struct PlanExecutionAdapter {
    assume_yes: bool,
}

impl PlanExecutionAdapter {
    pub fn new() -> Self {
        Self { assume_yes: false }
    }

    pub fn with_assume_yes(mut self, assume_yes: bool) -> Self {
        self.assume_yes = assume_yes;
        self
    }
}

impl Default for PlanExecutionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ExecutionAdapter for PlanExecutionAdapter {
    async fn execute(&self, context: &mut AgentContext, plan: &Plan) -> Result<ExecutionReport> {
        if let Some(validation) = &context.validation {
            if !validation.plan_can_continue {
                return Ok(ExecutionReport::skipped(
                    "Execution blocked: validator reported missing commands",
                ));
            }
        }

        if !self.assume_yes && !context.request.assume_yes {
            return Ok(ExecutionReport::skipped("Execution requires user approval"));
        }

        exec::execution_report(plan).await
    }
}

/// Placeholder recovery adapter that marks the stage as skipped.
pub struct NoopRecoveryAdapter;

#[async_trait]
impl RecoveryAdapter for NoopRecoveryAdapter {
    async fn recover(&self, _context: &mut AgentContext) -> Result<RecoveryOutcome> {
        Ok(RecoveryOutcome::Skipped)
    }
}
