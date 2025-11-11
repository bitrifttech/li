use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::config::{Config, LlmProvider, LlmSettings, ModelSettings, RecoverySettings};
use crate::planner::Plan;
use crate::validator::{MissingCommand, ValidationResult};

use super::adapters::{ExecutionAdapter, PlanExecutionAdapter};
use super::context::{AgentEvent, AgentRequest};
use super::outcome::AgentOutcome;
use super::stages::{AgentStage, StageOutcome};
use super::types::StageKind;
use super::{AgentContext, AgentOrchestrator};

fn sample_config() -> Config {
    Config {
        llm: LlmSettings {
            provider: LlmProvider::OpenRouter,
            api_key: "test-key".to_string(),
            timeout_secs: 30,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            user_agent: "li/test".to_string(),
        },
        models: ModelSettings {
            classifier: "classifier/model".to_string(),
            planner: "planner/model".to_string(),
            max_tokens: 512,
        },
        recovery: RecoverySettings::default(),
    }
}

struct RecordingStage {
    kind: StageKind,
    events: Arc<Mutex<Vec<StageKind>>>,
}

impl RecordingStage {
    fn new(kind: StageKind, events: Arc<Mutex<Vec<StageKind>>>) -> Self {
        Self { kind, events }
    }
}

#[async_trait]
impl AgentStage for RecordingStage {
    fn kind(&self) -> StageKind {
        self.kind
    }

    async fn execute(&self, _context: &mut AgentContext) -> Result<StageOutcome> {
        self.events.lock().unwrap().push(self.kind);
        Ok(StageOutcome::Continue)
    }
}

struct FinishStage {
    kind: StageKind,
}

impl FinishStage {
    fn new(kind: StageKind) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl AgentStage for FinishStage {
    fn kind(&self) -> StageKind {
        self.kind
    }

    async fn execute(&self, _context: &mut AgentContext) -> Result<StageOutcome> {
        Ok(StageOutcome::Finished(AgentOutcome::DirectCommand {
            command: "echo hi".to_string(),
        }))
    }
}

struct ErrorStage {
    kind: StageKind,
}

impl ErrorStage {
    fn new(kind: StageKind) -> Self {
        Self { kind }
    }
}

#[async_trait]
impl AgentStage for ErrorStage {
    fn kind(&self) -> StageKind {
        self.kind
    }

    async fn execute(&self, _context: &mut AgentContext) -> Result<StageOutcome> {
        Err(anyhow!("stage failure"))
    }
}

#[tokio::test]
async fn orchestrator_runs_stages_in_order() {
    let events = Arc::new(Mutex::new(Vec::new()));

    let orchestrator = AgentOrchestrator::builder()
        .add_stage(RecordingStage::new(
            StageKind::Classification,
            events.clone(),
        ))
        .add_stage(RecordingStage::new(StageKind::Planning, events.clone()))
        .build();

    let run = orchestrator
        .run(sample_config(), AgentRequest::new("list files"))
        .await
        .expect("orchestrator should succeed");

    assert!(matches!(run.outcome, AgentOutcome::Planned { .. }));
    let recorded = events.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![StageKind::Classification, StageKind::Planning]
    );
    assert!(matches!(
        run.events[..],
        [AgentEvent::StageStarted(StageKind::Classification), ..]
    ));
}

#[tokio::test]
async fn orchestrator_stops_when_stage_finishes() {
    let events = Arc::new(Mutex::new(Vec::new()));

    let orchestrator = AgentOrchestrator::builder()
        .add_stage(FinishStage::new(StageKind::Classification))
        .add_stage(RecordingStage::new(StageKind::Planning, events.clone()))
        .build();

    let run = orchestrator
        .run(sample_config(), AgentRequest::new("echo hi"))
        .await
        .expect("orchestrator should succeed");

    assert!(matches!(run.outcome, AgentOutcome::DirectCommand { .. }));
    assert!(events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn orchestrator_reports_failures() {
    let orchestrator = AgentOrchestrator::builder()
        .add_stage(ErrorStage::new(StageKind::Planning))
        .build();

    let run = orchestrator
        .run(sample_config(), AgentRequest::new("list files"))
        .await
        .expect("orchestrator should succeed");

    match run.outcome {
        AgentOutcome::Failed { stage, error } => {
            assert_eq!(stage, StageKind::Planning);
            assert_eq!(error, "stage failure");
        }
        other => panic!("unexpected outcome: {:?}", other),
    }
}

#[test]
fn default_orchestrator_has_standard_stages() {
    let orchestrator = AgentOrchestrator::default();
    assert_eq!(orchestrator.stage_count(), 5);
}

#[tokio::test]
async fn plan_execution_adapter_skips_when_validation_blocks() {
    let mut context = AgentContext::new(sample_config(), AgentRequest::new("list files"));
    context.validation = Some(ValidationResult {
        missing_commands: vec![MissingCommand {
            command: "fakecmd".to_string(),
            failed_command_line: "fakecmd --version".to_string(),
            plan_step: 0,
            is_dry_run: true,
        }],
        plan_can_continue: false,
    });

    let adapter = PlanExecutionAdapter::default();
    let report = adapter
        .execute(&mut context, &empty_plan())
        .await
        .expect("execution should succeed");

    assert!(!report.success);
    assert!(report.notes.iter().any(|note| note.contains("blocked")));
}

#[tokio::test]
async fn plan_execution_adapter_runs_when_assumed_yes() {
    let mut context = AgentContext::new(sample_config(), AgentRequest::new("echo hi"));
    context.request.assume_yes = true;
    context.validation = Some(ValidationResult {
        missing_commands: Vec::new(),
        plan_can_continue: true,
    });

    let adapter = PlanExecutionAdapter::default();
    let report = adapter
        .execute(&mut context, &empty_plan())
        .await
        .expect("execution should succeed");

    assert!(report.success);
    assert!(report.notes.is_empty());
}

fn empty_plan() -> Plan {
    Plan {
        confidence: 0.5,
        dry_run_commands: Vec::new(),
        execute_commands: Vec::new(),
        notes: String::new(),
    }
}
