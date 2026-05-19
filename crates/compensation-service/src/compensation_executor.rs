//! Compensation executor — trait and implementations.
//!
//! Phase 3 Batch 1 (bounded execution slice): Provides a real RollbackExecutor
//! for the single supported path (Rollback + Automatic) and StubCompensationExecutor
//! for unsupported paths and backward compatibility.
//!
//! **Bounded scope (this slice):**
//! - Executor is `RollbackExecutor` for Rollback + Automatic strategy/feasibility combos
//! - All other strategy types (CounterAction, FollowupNotice, Quarantine, Escalation)
//!   fail closed with `UNSUPPORTED_STRATEGY_TYPE` error
//! - All other feasibility levels (SemiAutomatic, ManualOnly, NotPossible)
//!   fail closed with `UNSUPPORTED_FEASIBILITY` error
//! - Missing side effect context (side_effect_id not found in repository) fails closed
//! - No retry/DLQ/orchestration/background worker work
//! - Real rollback/counter-action logic for non-Rollback strategies is Batch 1+ scope
//!
//! **Executor result semantics (this slice):**
//! - Success means "rollback action acknowledged" — validated against side effect ledger
//! - Does NOT claim external reversal if current data model cannot support it
//! - Success summary is truthful: "Rollback of {effect_type} targeting {target} acknowledged"
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use crate::compensation_action::{CompensationAction, ExecutionResult};
use intent_rebase_types::IntentRebaseError;

/// Skeleton executor: executes a single compensation action.
///
/// The executor applies the compensation strategy (rollback, counter-action, etc.)
/// and returns a result payload. Failures are captured in the result, not thrown,
/// so callers can record them atomically.
///
/// **This slice:** Trait definition + bounded RollbackExecutor for supported path
/// + StubCompensationExecutor for unsupported paths.
///   Full executor logic for non-Rollback strategies is Batch 1+ scope.
pub trait CompensationExecutor: Send + Sync {
    /// Execute a compensation action and return the result.
    ///
    /// Returns a result indicating success or failure with detail.
    /// Does not throw — all outcomes are captured in ExecutionResult.
    #[allow(async_fn_in_trait)]
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError>;
}

#[cfg(test)]
#[path = "compensation_executor_tests.rs"]
mod tests;
