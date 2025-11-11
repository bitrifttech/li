# Agent Architecture Baseline

This document captures the current (November 2025) state of the li agent and the gaps it exposes ahead of the refactor.

## Current Runtime Flow
- `src/main.rs` parses CLI arguments and forwards execution to `cli::Cli::run`.
- `cli::run` decides which user flow to trigger (task, chat, setup, intelligence, config) and orchestrates everything in-place:
  - Classification and planning go straight to `classifier::classify` and `planner::plan` with an `OpenRouterClient` built on demand.
  - Plan rendering, approval prompts, execution, and “intelligence” output explanations are implemented as local helper functions inside `cli.rs`.
  - The chat command bypasses any shared pipeline and calls the LLM client directly.
- `validator::CommandValidator` and `recovery::RecoveryEngine` exist but are never invoked from the CLI runtime, so validation and recovery are effectively dormant.
- `exec::execute_plan` and the shell `hook` module are empty placeholders; all command execution lives inside `cli.rs`.
- A new `agent/` module contains an orchestrator prototype and stage traits, but nothing in the binary references it yet.

## Configuration & Services Snapshot
- `config::Config` has already been refactored into nested settings (`llm`, `models`, `recovery`). The CLI currently uses only the `llm` and `models` portions.
- Recovery settings (enabled/preference/auto_install) are saved and loaded, but no runtime code reads them, leaving the feature effectively disabled.
- `client::OpenRouterClient` is the only LLM implementation. It provides retry logic but is constructed ad‑hoc in every caller, so swapping implementations or injecting fakes for tests requires additional indirection.

## Dead Code & Gaps Discovered
- The `agent` orchestrator, `exec`, and `hook` modules are unused; they represent the intended modular surface but currently contribute no behavior.
- Validation and recovery stages are defined but unreachable, so any failures during execution fall back to manual user handling.
- `recovery/tests.rs` only verifies string formatting helpers; there are no behavioral tests tying the recovery flow to configuration knobs.
- There is no shared `AgentContext` or request/response model outside the unreferenced orchestrator prototype, so state is passed implicitly through function arguments in `cli.rs`.

## Target Pipeline (Phases 2+)
1. **Input Parsing** – Front-ends (CLI, hooks) build an `AgentRequest`.
2. **Classification Stage** – Decide whether the request is an executable terminal command.
3. **Planning Stage** – Produce a structured `Plan` or a clarifying question.
4. **Validation Stage** – Inspect the plan for tool availability and safety concerns.
5. **Execution Stage** – Run dry-run checks, execute commands, and stream output.
6. **Recovery Stage** – Offer alternatives or installation guidance when failures occur.
7. **Reporting Stage** – Summarise results and optionally call the LLM for explanations.

Each stage will implement `AgentStage` (or a specialized trait) so the orchestrator can compose, test, or swap implementations independently.

## Immediate Follow-ups
- Strengthen recovery unit tests to reflect the real configuration model and enable future integration tests.
- Move orchestration logic out of `cli.rs` and into the agent pipeline (`agent::orchestrator`), allowing command handlers to stay thin.
- Replace direct `OpenRouterClient` construction with injected trait objects so tests can supply deterministic doubles.

