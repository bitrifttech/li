pub mod adapters;
pub mod context;
pub mod orchestrator;
pub mod outcome;
pub mod stages;
pub mod types;

#[allow(unused_imports)]
pub use adapters::{
    CommandValidationAdapter, DirectPlanningAdapter, NoopExecutionAdapter, NoopRecoveryAdapter,
    PlanExecutionAdapter,
};
#[allow(unused_imports)]
pub use context::{AgentContext, AgentEvent, AgentRequest, AgentRun};
#[allow(unused_imports)]
pub use orchestrator::{AgentOrchestrator, AgentPipelineBuilder};
#[allow(unused_imports)]
pub use outcome::{AgentOutcome, ExecutionReport, RecoveryOutcome};
#[allow(unused_imports)]
pub use stages::{
    AgentStage, ExecutionStage, PlanningStage, RecoveryStage, StageOutcome, ValidationStage,
};
#[allow(unused_imports)]
pub use types::StageKind;

#[cfg(test)]
mod tests;
