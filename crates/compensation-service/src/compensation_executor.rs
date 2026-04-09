//! Compensation executor — skeleton contract and stub implementation.
//!
//! Phase 3 Batch 1 (bounded persistence slice): skeleton executor contract only.
//!
//! **This slice scope:** Trait definition + async stub implementation + basic test.
//! **Batch 1+ scope:** Full executor logic (actual rollback, counter-action, etc.)
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use crate::compensation_action::CompensationAction;
use crate::compensation_action::ExecutionResult;
use intent_rebase_types::IntentRebaseError;

/// Skeleton executor: executes a single compensation action.
///
/// The executor applies the compensation strategy (rollback, counter-action, etc.)
/// and returns a result payload. Failures are captured in the result, not thrown,
/// so callers can record them atomically.
///
/// **This slice:** Stub implementation that always returns success.
/// Full executor logic (actual rollback, counter-action, notification, etc.)
/// is Batch 1+ scope.
pub trait CompensationExecutor: Send + Sync {
    /// Execute a compensation action and return the result.
    ///
    /// Returns a result indicating success or failure with detail.
    /// Does not throw — all outcomes are captured in ExecutionResult.
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError>;
}

/// Stub executor for testing and Phase 3 Batch 1.
///
/// Always returns success — actual execution logic is Batch 1+ scope.
#[derive(Clone)]
pub struct StubCompensationExecutor;

impl StubCompensationExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubCompensationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CompensationExecutor for StubCompensationExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        // Stub: always succeed with a summary describing the action.
        // Real executor would perform actual compensation (rollback, counter-action, etc.)
        Ok(ExecutionResult::success(&format!(
            "Stub executor: executed {:?} for action {}",
            action.strategy_type, action.id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use uuid::Uuid;

    fn create_test_action() -> CompensationAction {
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test rollback",
        )
    }

    #[tokio::test]
    async fn test_stub_executor_always_succeeds() {
        let executor = StubCompensationExecutor::new();
        let action = create_test_action();

        let result = executor.execute(&action).await.unwrap();

        assert!(result.success);
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_stub_executor_describes_action() {
        let executor = StubCompensationExecutor::new();
        let action = create_test_action();

        let result = executor.execute(&action).await.unwrap();

        assert!(result.summary.contains("Rollback"));
    }
}
