use crate::compensation_action::{CompensationAction, ExecutionResult};
use crate::compensation_executor::CompensationExecutor;
use intent_rebase_types::IntentRebaseError;

/// Stub executor for testing and backward compatibility.
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
