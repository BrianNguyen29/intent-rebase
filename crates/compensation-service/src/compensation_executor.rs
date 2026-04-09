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

use std::sync::Arc;

use crate::compensation_action::{CompensationAction, ExecutionResult};
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::IntentRebaseError;

/// Skeleton executor: executes a single compensation action.
///
/// The executor applies the compensation strategy (rollback, counter-action, etc.)
/// and returns a result payload. Failures are captured in the result, not thrown,
/// so callers can record them atomically.
///
/// **This slice:** Trait definition + bounded RollbackExecutor for supported path
/// + StubCompensationExecutor for unsupported paths.
/// Full executor logic for non-Rollback strategies is Batch 1+ scope.
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

// =============================================================================
// RollbackExecutor — bounded real executor for Rollback + Automatic only
// =============================================================================

/// Bounded real executor for Rollback + Automatic compensation actions.
///
/// **Phase 3 Batch 1 bounded execution slice:**
/// - Validates action is Rollback strategy + Automatic feasibility
/// - Fetches side effect context from repository to validate target existence
/// - Returns truthful acknowledgment (not external reversal claim)
/// - All other strategy types or feasibility levels fail closed
#[derive(Clone)]
pub struct RollbackExecutor {
    side_effect_repo: Arc<dyn SideEffectRepository>,
}

impl RollbackExecutor {
    /// Create a new RollbackExecutor with the given side effect repository.
    pub fn new(side_effect_repo: Arc<dyn SideEffectRepository>) -> Self {
        Self { side_effect_repo }
    }

    /// Execute a compensation action with Rollback + Automatic semantics.
    ///
    /// **Bounded supported path:** Only Rollback strategy + Automatic feasibility succeeds.
    /// All other combinations fail closed.
    async fn execute_impl(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        use crate::compensation_action::{CompensationFeasibility, StrategyType};

        // Strategy gate: only Rollback is supported in this slice
        if action.strategy_type != StrategyType::Rollback {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported strategy type: {:?}. Only Rollback is supported in this slice.",
                    action.strategy_type
                ),
                "UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "Strategy {:?} requires Batch 1+ executor implementation",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only Automatic is supported in this slice
        if action.feasibility != CompensationFeasibility::Automatic {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported feasibility: {:?}. Only Automatic is supported in this slice.",
                    action.feasibility
                ),
                "UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "Feasibility {:?} requires human intervention or is not executable",
                    action.feasibility
                )),
            ));
        }

        // Side effect validation: fetch context to ensure target exists
        let side_effect = match self.side_effect_repo.get(action.side_effect_id).await {
            Ok(se) => se,
            Err(e) => {
                return Ok(ExecutionResult::failure(
                    &format!(
                        "Side effect {} not found: cannot validate rollback target",
                        action.side_effect_id
                    ),
                    "SIDE_EFFECT_NOT_FOUND",
                    Some(format!("{:?}", e)),
                ));
            }
        };

        // Validate side effect belongs to same tenant/intent as action
        if side_effect.tenant_id != action.tenant_id {
            return Ok(ExecutionResult::failure(
                "Side effect tenant_id mismatch",
                "TENANT_MISMATCH",
                Some(format!(
                    "Action tenant={}, side effect tenant={}",
                    action.tenant_id, side_effect.tenant_id
                )),
            ));
        }

        if side_effect.intent_id != action.intent_id {
            return Ok(ExecutionResult::failure(
                "Side effect intent_id mismatch",
                "INTENT_MISMATCH",
                Some(format!(
                    "Action intent={}, side effect intent={}",
                    action.intent_id, side_effect.intent_id
                )),
            ));
        }

        // Rollback acknowledgment: validated against side effect ledger.
        // Does NOT claim external reversal — current data model does not support
        // actual artifact reversal. Summary is truthful.
        let summary = format!(
            "Rollback of {} targeting {} acknowledged (side_effect_id={}, effect_class={:?})",
            side_effect.effect_type, side_effect.target, side_effect.id, side_effect.effect_class
        );

        Ok(ExecutionResult::success(&summary))
    }
}

impl CompensationExecutor for RollbackExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.execute_impl(action).await
    }
}

// =============================================================================
// StubCompensationExecutor — for testing and backward compatibility
// =============================================================================

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use crate::side_effect::{SideEffect, SideEffectClass};
    use std::sync::Arc;
    use uuid::Uuid;

    fn create_test_action(
        strategy_type: StrategyType,
        feasibility: CompensationFeasibility,
    ) -> CompensationAction {
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            feasibility,
            strategy_type,
            "Test compensation",
        )
    }

    fn create_test_side_effect(
        tenant_id: Uuid,
        intent_id: Uuid,
        side_effect_id: Uuid,
    ) -> SideEffect {
        SideEffect {
            id: side_effect_id, // Use the provided id so action.side_effect_id matches
            tenant_id,
            intent_id,
            intent_version: 1,
            effect_class: SideEffectClass::S1InternalReversible,
            effect_type: "metadata_write".to_string(),
            target: "db-record-123".to_string(),
            occurred_at: chrono::Utc::now(),
            idempotency_key: None,
        }
    }

    // === StubCompensationExecutor tests ===

    #[tokio::test]
    async fn test_stub_executor_always_succeeds() {
        let executor = StubCompensationExecutor::new();
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

        let result = executor.execute(&action).await.unwrap();

        assert!(result.success);
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_stub_executor_describes_action() {
        let executor = StubCompensationExecutor::new();
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

        let result = executor.execute(&action).await.unwrap();

        assert!(result.summary.contains("Rollback"));
    }

    // === RollbackExecutor tests ===

    #[tokio::test]
    async fn test_rollback_executor_success_rollback_automatic() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect.clone()).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Auto rollback internal metadata",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(
            result.success,
            "Expected success but got failure: {:?}",
            result
        );
        assert!(result.error_code.is_none());
        assert!(result.summary.contains("Rollback"));
        assert!(result.summary.contains("metadata_write"));
        assert!(result.summary.contains("db-record-123"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_non_rollback_strategy() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::CounterAction, // Not Rollback
            "Counter-action compensation",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
        assert!(result.summary.contains("CounterAction"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_semi_automatic_feasibility() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic, // Not Automatic
            StrategyType::Rollback,
            "Semi-auto rollback",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_FEASIBILITY".to_string())
        );
        assert!(result.summary.contains("SemiAutomatic"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_manual_only_feasibility() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly, // Not Automatic
            StrategyType::Rollback,
            "Manual rollback required",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_FEASIBILITY".to_string())
        );
        assert!(result.summary.contains("ManualOnly"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_not_possible_feasibility() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::NotPossible, // Not executable
            StrategyType::Rollback,
            "Cannot compensate",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_FEASIBILITY".to_string())
        );
        assert!(result.summary.contains("NotPossible"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_missing_side_effect() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // No side effects created in repo
        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Rollback missing side effect",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.error_code, Some("SIDE_EFFECT_NOT_FOUND".to_string()));
        assert!(result.summary.contains("not found"));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_tenant_mismatch() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let different_tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            different_tenant_id, // Different tenant than side effect
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Rollback with tenant mismatch",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.error_code, Some("TENANT_MISMATCH".to_string()));
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_intent_mismatch() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let different_intent_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(different_intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            different_intent_id, // Different intent than side effect
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Rollback with intent mismatch",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.error_code, Some("INTENT_MISMATCH".to_string()));
    }

    // === Strategy type failure tests (all non-Rollback strategies) ===

    #[tokio::test]
    async fn test_rollback_executor_fail_on_counter_action_strategy() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::CounterAction,
            "Counter-action compensation",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_followup_notice_strategy() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::FollowupNotice,
            "Followup notice compensation",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_quarantine_strategy() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Quarantine,
            "Quarantine compensation",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_rollback_executor_fail_on_escalation_strategy() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Escalation,
            "Escalation compensation",
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    // === Truthful summary tests ===

    #[tokio::test]
    async fn test_rollback_executor_summary_is_truthful() {
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
        repo.create(side_effect).await.unwrap();

        let executor = RollbackExecutor::new(repo);
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Auto rollback internal metadata",
        );

        let result = executor.execute(&action).await.unwrap();

        // Summary should contain "acknowledged" not "reversed" or "completed"
        assert!(result.summary.contains("acknowledged"));
        assert!(!result.summary.to_lowercase().contains("reversed"));
        // Should mention effect_type and target
        assert!(result.summary.contains("metadata_write"));
        assert!(result.summary.contains("db-record-123"));
    }
}
