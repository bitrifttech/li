use std::fmt;
use std::sync::Arc;

use anyhow::Result;

use crate::client::{DynLlmClient, LlmClientFactory};
use crate::config::Config;
use crate::planner::Plan;
use crate::validator::ValidationResult;

use super::outcome::{AgentOutcome, ExecutionReport, RecoveryOutcome};
use super::types::StageKind;

/// Immutable request passed into the agent pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    pub task: String,
    pub intelligence: bool,
    pub intelligence_question: Option<String>,
    pub assume_yes: bool,
}

impl AgentRequest {
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            intelligence: false,
            intelligence_question: None,
            assume_yes: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.task.trim().is_empty()
    }
}

/// Mutable context threaded through the agent stages.
#[derive(Clone)]
pub struct AgentContext {
    pub config: Config,
    pub request: AgentRequest,
    pub plan: Option<Plan>,
    pub validation: Option<ValidationResult>,
    pub execution: Option<ExecutionReport>,
    pub recovery: Option<RecoveryOutcome>,
    events: Vec<AgentEvent>,
    llm_client: Option<Arc<DynLlmClient>>,
}

impl fmt::Debug for AgentContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentContext")
            .field("request", &self.request)
            .field("plan", &self.plan)
            .field("validation", &self.validation)
            .field("execution", &self.execution)
            .field("recovery", &self.recovery)
            .field("events", &self.events)
            .finish()
    }
}

impl AgentContext {
    pub fn new(config: Config, request: AgentRequest) -> Self {
        Self {
            config,
            request,
            plan: None,
            validation: None,
            execution: None,
            recovery: None,
            events: Vec::new(),
            llm_client: None,
        }
    }

    pub fn record_event(&mut self, event: AgentEvent) {
        self.events.push(event);
    }

    pub fn record_message(&mut self, message: impl Into<String>) {
        self.record_event(AgentEvent::Message(message.into()));
    }

    pub fn llm_client<F>(&mut self, factory: &F) -> Result<Arc<DynLlmClient>>
    where
        F: LlmClientFactory + ?Sized,
    {
        if let Some(client) = &self.llm_client {
            return Ok(client.clone());
        }

        let client = factory.build(&self.config.llm)?;
        self.llm_client = Some(client.clone());
        Ok(client)
    }

    pub fn record_stage_start(&mut self, stage: StageKind) {
        self.record_event(AgentEvent::StageStarted(stage));
    }

    pub fn record_stage_end(&mut self, stage: StageKind) {
        self.record_event(AgentEvent::StageCompleted(stage));
    }

    pub fn record_stage_skip(&mut self, stage: StageKind, reason: impl Into<String>) {
        self.record_event(AgentEvent::StageSkipped {
            stage,
            reason: reason.into(),
        });
    }

    pub fn record_stage_failure(&mut self, stage: StageKind, error: impl Into<String>) {
        self.record_event(AgentEvent::StageFailed {
            stage,
            error: error.into(),
        });
    }

    pub fn record_plan(&mut self, plan: Plan) {
        let confidence = plan.confidence;
        self.plan = Some(plan);
        self.record_event(AgentEvent::PlanReady { confidence });
    }

    pub fn record_validation(&mut self, validation: ValidationResult) {
        let missing = validation.missing_commands.len();
        self.validation = Some(validation.clone());
        self.record_event(AgentEvent::ValidationFinished {
            missing,
            can_continue: validation.plan_can_continue,
        });
    }

    pub fn record_execution(&mut self, report: ExecutionReport) {
        let success = report.success;
        self.execution = Some(report);
        self.record_event(AgentEvent::ExecutionFinished { success });
    }

    pub fn record_recovery(&mut self, outcome: RecoveryOutcome) {
        self.recovery = Some(outcome.clone());
        self.record_event(AgentEvent::RecoveryFinished { outcome });
    }

    pub fn into_run(self) -> AgentRun {
        let AgentContext {
            plan,
            validation,
            execution,
            recovery,
            events,
            ..
        } = self;

        let outcome = AgentOutcome::Planned {
            plan,
            validation,
            execution,
            recovery,
        };

        AgentRun { outcome, events }
    }

    pub fn into_run_with_outcome(self, outcome: AgentOutcome) -> AgentRun {
        let AgentContext { events, .. } = self;
        AgentRun { outcome, events }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRun {
    pub outcome: AgentOutcome,
    pub events: Vec<AgentEvent>,
}

/// Structured audit events emitted while progressing through the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    StageStarted(StageKind),
    StageCompleted(StageKind),
    StageSkipped { stage: StageKind, reason: String },
    StageFailed { stage: StageKind, error: String },
    PlanReady { confidence: f32 },
    ValidationFinished { missing: usize, can_continue: bool },
    ExecutionFinished { success: bool },
    RecoveryFinished { outcome: RecoveryOutcome },
    Message(String),
}
