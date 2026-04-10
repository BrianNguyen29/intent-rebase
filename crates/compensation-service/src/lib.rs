//! Compensation Service — Phase 3 Batch 1 compensation action persistence slice
//!
//! This crate is responsible for tracking side effects and planning
//! compensation actions when intents change after effects have occurred.
//!
//! **Batch 1 scope (this slice):** compensation_actions table migration + repo + service
//!   + planner/executor skeleton contracts.
//! **Batch 1+ scope:** Full planner logic, executor logic, runtime adapter, API (not yet implemented).

pub mod compensation_action;
pub mod compensation_action_repo;
pub mod compensation_action_service;
pub mod compensation_executor;
pub mod compensation_planner;
pub mod side_effect;
pub mod side_effect_repo;
pub mod side_effect_service;

pub use compensation_action::*;
pub use side_effect::*;
pub use side_effect_repo::*;
pub use side_effect_service::*;

// Explicitly re-export traits to avoid ambiguous glob re-exports
pub use compensation_action_repo::{
    CompensationActionRepository, InMemoryCompensationActionRepository,
    SqlxCompensationActionRepository,
};
pub use compensation_action_service::{BatchCandidates, CompensationActionService};
pub use compensation_executor::{CompensationExecutor, StubCompensationExecutor};
pub use compensation_planner::{CompensationPlanner, InMemoryCompensationPlanner};
