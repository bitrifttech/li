use anyhow::Result;

use crate::config::Config;

use super::adapters::{
    CommandValidationAdapter, DirectPlanningAdapter, NoopExecutionAdapter, NoopRecoveryAdapter,
};
use super::context::{AgentContext, AgentRequest, AgentRun};
use super::outcome::AgentOutcome;
use super::stages::{
    AgentStage, ExecutionStage, PlanningStage, RecoveryStage, StageOutcome, ValidationStage,
};

pub struct AgentOrchestrator {
    stages: Vec<Box<dyn AgentStage>>,
}

impl AgentOrchestrator {
    pub fn new(stages: Vec<Box<dyn AgentStage>>) -> Self {
        Self { stages }
    }

    pub fn builder() -> AgentPipelineBuilder {
        AgentPipelineBuilder::new()
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub async fn run(&self, config: Config, request: AgentRequest) -> Result<AgentRun> {
        let mut context = AgentContext::new(config, request);

        for stage in &self.stages {
            let kind = stage.kind();
            context.record_stage_start(kind);
            match stage.execute(&mut context).await {
                Ok(StageOutcome::Continue) => {
                    context.record_stage_end(kind);
                }
                Ok(StageOutcome::Finished(outcome)) => {
                    context.record_stage_end(kind);
                    return Ok(context.into_run_with_outcome(outcome));
                }
                Err(error) => {
                    let message = error.to_string();
                    context.record_stage_failure(kind, &message);
                    let outcome = AgentOutcome::failed(kind, message);
                    return Ok(context.into_run_with_outcome(outcome));
                }
            }
        }

        Ok(context.into_run())
    }
}

pub struct AgentPipelineBuilder {
    stages: Vec<Box<dyn AgentStage>>,
}

impl AgentPipelineBuilder {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn add_stage<S>(mut self, stage: S) -> Self
    where
        S: AgentStage + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    pub fn with_planning_adapter<P>(mut self, adapter: P) -> Self
    where
        P: super::adapters::PlanningAdapter + Send + Sync + 'static,
    {
        self.stages.push(Box::new(PlanningStage::new(adapter)));
        self
    }

    pub fn with_validation_adapter<V>(mut self, adapter: V) -> Self
    where
        V: super::adapters::ValidationAdapter + Send + Sync + 'static,
    {
        self.stages.push(Box::new(ValidationStage::new(adapter)));
        self
    }

    pub fn with_execution_adapter<E>(mut self, adapter: E) -> Self
    where
        E: super::adapters::ExecutionAdapter + Send + Sync + 'static,
    {
        self.stages.push(Box::new(ExecutionStage::new(adapter)));
        self
    }

    pub fn with_recovery_adapter<R>(mut self, adapter: R) -> Self
    where
        R: super::adapters::RecoveryAdapter + Send + Sync + 'static,
    {
        self.stages.push(Box::new(RecoveryStage::new(adapter)));
        self
    }

    pub fn with_default_adapters(self) -> Self {
        self.with_planning_adapter(DirectPlanningAdapter::default())
            .with_validation_adapter(CommandValidationAdapter)
            .with_execution_adapter(NoopExecutionAdapter)
            .with_recovery_adapter(NoopRecoveryAdapter)
    }

    pub fn build(self) -> AgentOrchestrator {
        AgentOrchestrator::new(self.stages)
    }
}

impl Default for AgentOrchestrator {
    fn default() -> Self {
        AgentPipelineBuilder::new().with_default_adapters().build()
    }
}
