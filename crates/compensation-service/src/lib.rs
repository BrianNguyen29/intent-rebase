//! Compensation Service — Phase 3 Batch 1 compensation action persistence slice
//!
//! This crate is responsible for tracking side effects and planning
//! compensation actions when intents change after effects have occurred.
//!
//! **Batch 1 scope (this slice):** compensation_actions table migration + repo + service
//!   + planner/executor skeleton contracts.
//!     **Batch 1+ scope:** Full planner logic, executor logic, runtime adapter, API (not yet implemented).

pub mod compensation_action;
pub mod compensation_action_repo;
pub mod compensation_action_service;
pub mod compensation_executor;
pub mod compensation_planner;
pub mod orchestration_run;
pub mod orchestration_run_repo;
pub mod orchestration_runtime;
pub mod rollback_record;
pub mod rollback_record_repo;
pub mod side_effect;
pub mod side_effect_repo;
pub mod side_effect_service;

pub use compensation_action::*;
pub use rollback_record::*;
pub use side_effect::*;
pub use side_effect_repo::*;
pub use side_effect_service::*;

// Explicitly re-export traits to avoid ambiguous glob re-exports
pub use compensation_action_repo::{
    CompensationActionRepository, InMemoryCompensationActionRepository,
    SqlxCompensationActionRepository,
};
pub use compensation_action_service::{
    BatchCandidates, BatchItemOutcome, BatchOrchestrationResult, BatchOrchestrationSummary,
    CompensationActionService, CoordinationRecord, CoordinationResult, CoordinationStatus,
    CoordinationSummary, ErrorClassification, ErrorSeverity, FeasibilityRisk, OrchestrationAction,
    OrchestrationActionProposal, OrchestrationDryRunResult, OrchestrationDryRunSummary,
    PolicyGateEvaluation, PolicyGateEvaluationResult, PolicyGateMetadata, PolicyGateStatus,
    PolicyGateSummary, RetryExhaustionRisk, RiskMetadata, StrategySeverity,
};
pub use compensation_executor::{CompensationExecutor, CounterActionExecutor, RollbackExecutor, StubCompensationExecutor};
pub use compensation_planner::{BoundedCompensationPlanner, CompensationPlanner, InMemoryCompensationPlanner};
pub use orchestration_run::{
    OrchestrationActionDecision, OrchestrationRun, RunItemResult, RunStatus,
};
pub use orchestration_run_repo::{
    InMemoryOrchestrationRunRepository, OrchestrationRunRepository, SqlxOrchestrationRunRepository,
};
pub use orchestration_runtime::OrchestrationRuntime;
// RollbackRecordRepository is used via explicit re-exports below
