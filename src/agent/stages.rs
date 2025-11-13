use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::adapters::{ExecutionAdapter, PlanningAdapter, RecoveryAdapter, ValidationAdapter};
use super::context::AgentContext;
use super::outcome::AgentOutcome;
use super::types::StageKind;

/// Control flow instruction returned by stage execution.
#[derive(Debug)]
pub enum StageOutcome {
    Continue,
    Finished(AgentOutcome),
}

#[async_trait]
pub trait AgentStage: Send + Sync {
    fn kind(&self) -> StageKind;

    async fn execute(&self, context: &mut AgentContext) -> Result<StageOutcome>;
}

pub struct PlanningStage<P> {
    adapter: Arc<P>,
}

impl<P> PlanningStage<P> {
    pub fn new(adapter: P) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }
}

#[async_trait]
impl<P> AgentStage for PlanningStage<P>
where
    P: PlanningAdapter + Send + Sync + 'static,
{
    fn kind(&self) -> StageKind {
        StageKind::Planning
    }

    async fn execute(&self, context: &mut AgentContext) -> Result<StageOutcome> {
        let plan = self.adapter.plan(context).await?;
        context.record_plan(plan);
        Ok(StageOutcome::Continue)
    }
}

pub struct ValidationStage<V> {
    adapter: Arc<V>,
}

impl<V> ValidationStage<V> {
    pub fn new(adapter: V) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }
}

#[async_trait]
impl<V> AgentStage for ValidationStage<V>
where
    V: ValidationAdapter + Send + Sync + 'static,
{
    fn kind(&self) -> StageKind {
        StageKind::Validation
    }

    async fn execute(&self, context: &mut AgentContext) -> Result<StageOutcome> {
        let Some(plan) = context.plan.clone() else {
            context.record_stage_skip(self.kind(), "no plan available for validation");
            return Ok(StageOutcome::Continue);
        };

        let validation = self.adapter.validate(context, &plan).await?;
        context.record_validation(validation);
        Ok(StageOutcome::Continue)
    }
}

pub struct ExecutionStage<E> {
    adapter: Arc<E>,
}

impl<E> ExecutionStage<E> {
    pub fn new(adapter: E) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }
}

#[async_trait]
impl<E> AgentStage for ExecutionStage<E>
where
    E: ExecutionAdapter + Send + Sync + 'static,
{
    fn kind(&self) -> StageKind {
        StageKind::Execution
    }

    async fn execute(&self, context: &mut AgentContext) -> Result<StageOutcome> {
        let Some(plan) = context.plan.clone() else {
            context.record_stage_skip(self.kind(), "no plan produced");
            return Ok(StageOutcome::Continue);
        };

        let report = self.adapter.execute(context, &plan).await?;
        context.record_execution(report);
        Ok(StageOutcome::Continue)
    }
}

pub struct RecoveryStage<R> {
    adapter: Arc<R>,
}

impl<R> RecoveryStage<R> {
    pub fn new(adapter: R) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }
}

#[async_trait]
impl<R> AgentStage for RecoveryStage<R>
where
    R: RecoveryAdapter + Send + Sync + 'static,
{
    fn kind(&self) -> StageKind {
        StageKind::Recovery
    }

    async fn execute(&self, context: &mut AgentContext) -> Result<StageOutcome> {
        let Some(execution) = &context.execution else {
            context.record_stage_skip(self.kind(), "execution was not attempted");
            return Ok(StageOutcome::Continue);
        };

        if execution.success {
            context.record_stage_skip(self.kind(), "execution succeeded");
            return Ok(StageOutcome::Continue);
        }

        let outcome = self.adapter.recover(context).await?;
        context.record_recovery(outcome);
        Ok(StageOutcome::Continue)
    }
}
