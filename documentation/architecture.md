# Agent Architecture Overview

This document records the current architecture of the `li` agent after the modular refactor (November 2025).

## End-to-End Flow
1. **Entry Point** (`src/main.rs`)
   - Constructs the CLI and loads configuration.
   - `mod recovery;` is exported so downstream modules can opt-in to recovery features.
2. **CLI Runtime** (`src/cli/runtime.rs`)
   - Parses user intent and converts task requests into an `AgentRequest`.
   - Delegates to `AgentOrchestrator::default()` for the classify → plan → validate → execute → recover pipeline.
   - Surfaces classification verdicts, validation warnings, execution output, AI explanations, and contextual recovery guidance to the terminal.
   - Falls back to direct OpenRouter calls for chat/diagnostic flows.
3. **Agent Orchestrator** (`src/agent/`)
   - `AgentPipelineBuilder` wires Classification, Planning, Validation, Execution, and Recovery stages using adapter traits.
   - `AgentContext` tracks shared state (request, plan, validation results, execution reports, recovery notes, event log) and caches a shared `Arc<dyn LlmClient>` for downstream adapters.
   - `AgentOutcome` returns rich results (direct command, planned run with validation/execution/recovery summaries, clarification requests, cancellations, or stage failures).
4. **Execution Layer** (`src/exec/mod.rs`)
   - Provides reusable helpers (`run_command`, `execute_plan`, `execute_plan_with_capture`, `execution_report`) that stream output and return structured reports.
   - Used by both the agent execution adapter and the CLI intelligence flow to avoid code duplication.
5. **Recovery Engine** (`src/recovery/mod.rs`)
   - Generates alternatives and installation options for missing commands.
   - Presents interactive menus when validation blocks progress and executes recovery choices.
6. **Service Layer** (`src/client.rs`)
   - `LlmClient` trait with an `OpenRouterClient` implementation and an `OpenRouterClientFactory` for constructing shared clients.

## Key Modules
- **Validation** (`src/validator/mod.rs`)
  - `CommandValidator` checks plan commands for availability.
  - `ValidationResult` now derives `PartialEq` so outcomes can be asserted in tests and embedded in `AgentOutcome`.
- **Adapters** (`src/agent/adapters.rs`)
  - `DirectClassifierAdapter` and `DirectPlanningAdapter` reuse the shared LLM client from the context.
  - `CommandValidationAdapter` gate-keeps plan execution.
  - `PlanExecutionAdapter` delegates to `exec::execution_report`, optionally skipping execution when validation blocks or user approval is required.
  - `NoopExecutionAdapter`/`NoopRecoveryAdapter` remain for lightweight pipelines and tests.
- **Tests** (`src/agent/tests.rs`, `src/recovery/tests.rs`, etc.)
  - Added coverage for `PlanExecutionAdapter` behavior with/without approval and for recovery engine configuration handling.

## Current Capabilities
- Shared LLM client reuse (single `Arc` per request) reduces connection churn and simplifies mocking.
- CLI emits validation highlights, recovery prompts, and stage-specific failure guidance.
- Recovery engine is invoked automatically when validators report blocking issues, preserving the ability to opt out via configuration.
- Execution and recovery stages are now driven through the orchestrator, enabling future front-ends to reuse the same pipeline.

## Future Enhancements
- Add structured telemetry hooks/event streaming from `AgentContext` for analytics and auditing.
- Expand adapter tests with mocked LLM clients to cover planner/classifier error paths deterministically.
- Introduce optional non-interactive recovery flows for headless environments.
- Continue trimming warnings in the recovery module by separating display-only structs from operational data.

