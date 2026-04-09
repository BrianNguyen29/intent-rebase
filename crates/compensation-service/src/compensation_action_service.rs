//! Compensation action service facade for creating, querying, and executing compensation actions.
//!
//! Phase 3 Batch 1: Compensation action persistence and execution service.
//! Provides APIs for creating, querying, approving, waiving, and executing
//! compensation actions with proper status transition validation and tenant isolation.
//!
//! **Bounded executor slice scope:**
//! - Executor is RollbackExecutor for Rollback+Automatic path; StubCompensationExecutor for tests
//! - Only Approved actions can execute; illegal transitions fail closed
//! - No retry/DLQ systems; Failed actions remain Failed (retry/reapproval is Batch 1+ scope)
//! - No background workers; all operations are explicit API calls

use std::sync::Arc;
use uuid::Uuid;

use crate::compensation_action::{CompensationAction, CompensationStatus, ExecutionResult};
use crate::compensation_action_repo::CompensationActionRepository;
use crate::compensation_executor::CompensationExecutor;
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::IntentRebaseError;

/// Service facade for compensation action operations.
///
/// Provides a convenient API for creating, querying, approving, waiving, and executing
/// compensation actions with proper tenant isolation and status transition validation.
#[derive(Clone)]
pub struct CompensationActionService {
    repo: Arc<dyn CompensationActionRepository>,
    /// Side effect repository for RollbackExecutor validation.
    /// Phase 3 Batch 1: Used by execute_action to validate side effect context
    /// before running the bounded RollbackExecutor.
    side_effect_repo: Option<Arc<dyn SideEffectRepository>>,
}

impl CompensationActionService {
    /// Create a new CompensationActionService with the given repository.
    ///
    /// Uses a stub executor that always returns success (backward compatibility).
    /// **Note:** For production use with real execution, use `new_with_side_effect_repo`.
    pub fn new(repo: Arc<dyn CompensationActionRepository>) -> Self {
        Self {
            repo,
            side_effect_repo: None,
        }
    }

    /// Create a new CompensationActionService with side effect repository for
    /// real RollbackExecutor execution.
    ///
    /// This is the production constructor that enables real Rollback+Automatic execution.
    pub fn new_with_side_effect_repo(
        repo: Arc<dyn CompensationActionRepository>,
        side_effect_repo: Arc<dyn SideEffectRepository>,
    ) -> Self {
        Self {
            repo,
            side_effect_repo: Some(side_effect_repo),
        }
    }

    /// Create a new compensation action.
    ///
    /// Returns the created action with its generated ID.
    pub async fn create_action(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo.create(action).await
    }

    /// Get a compensation action by its ID.
    pub async fn get_action(
        &self,
        action_id: Uuid,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo.get(action_id).await
    }

    /// List compensation actions for a given tenant.
    ///
    /// Returns up to `limit` actions (default 100), ordered by generated_at descending.
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_tenant(tenant_id, limit).await
    }

    /// List compensation actions for a given side effect.
    pub async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_side_effect(side_effect_id, tenant_id)
            .await
    }

    /// List compensation actions for a given intent.
    ///
    /// Enables direct intent-scoped queries without joining through side_effects.
    pub async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_intent(intent_id, tenant_id).await
    }

    /// List compensation actions by status for a given tenant.
    pub async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: CompensationStatus,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_status(tenant_id, status).await
    }

    /// Update the status of a compensation action.
    ///
    /// Uses optimistic locking via lock_version to prevent concurrent updates.
    pub async fn update_status(
        &self,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo
            .update_status(action_id, new_status, lock_version, None, None)
            .await
    }

    /// Record the execution result of a compensation action.
    ///
    /// Updates status to Executed or Failed based on the result,
    /// and increments the attempt counter.
    ///
    /// **Note:** This method does NOT validate status transitions before calling
    /// the repository. The repository's `record_result` implementation handles
    /// the status update directly. Status transition validation is done in
    /// `execute_action` which calls this method after executor completion.
    pub async fn record_result(
        &self,
        action_id: Uuid,
        result: &ExecutionResult,
        lock_version: i32,
        executed_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo
            .record_result(action_id, result, lock_version, executed_by)
            .await
    }

    /// Approve a pending compensation action.
    ///
    /// Transitions the action from Pending → Approved.
    /// Uses optimistic locking via lock_version to prevent concurrent updates.
    ///
    /// **Fails closed on illegal transitions:**
    /// - If action is not Pending, returns InvalidCompensationActionTransition error
    /// - If lock_version doesn't match, returns ConcurrencyConflict error
    pub async fn approve_action(
        &self,
        action_id: Uuid,
        lock_version: i32,
        approved_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Fetch current action to validate transition
        let action = self.repo.get(action_id).await?;

        // Validate transition: must be Pending to approve
        let validation = action
            .status
            .can_transition_to(CompensationStatus::Approved);
        if !validation.allowed {
            return Err(IntentRebaseError::InvalidCompensationActionTransition {
                from_status: format!("{:?}", action.status),
                to_status: "Approved".into(),
                reason: validation.reason.unwrap_or_default(),
            });
        }

        // Update status with optimistic locking and persist actor info
        self.repo
            .update_status(
                action_id,
                CompensationStatus::Approved,
                lock_version,
                approved_by,
                None,
            )
            .await
    }

    /// Waive a pending compensation action.
    ///
    /// Transitions the action from Pending → Waived.
    /// Uses optimistic locking via lock_version to prevent concurrent updates.
    ///
    /// **Fails closed on illegal transitions:**
    /// - If action is not Pending, returns InvalidCompensationActionTransition error
    /// - If lock_version doesn't match, returns ConcurrencyConflict error
    ///
    /// **This slice:** Waived actions are terminal. No reactivation path exists.
    pub async fn waive_action(
        &self,
        action_id: Uuid,
        lock_version: i32,
        waived_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Fetch current action to validate transition
        let action = self.repo.get(action_id).await?;

        // Validate transition: must be Pending to waive
        let validation = action.status.can_transition_to(CompensationStatus::Waived);
        if !validation.allowed {
            return Err(IntentRebaseError::InvalidCompensationActionTransition {
                from_status: format!("{:?}", action.status),
                to_status: "Waived".into(),
                reason: validation.reason.unwrap_or_default(),
            });
        }

        // Update status with optimistic locking and persist actor info
        self.repo
            .update_status(
                action_id,
                CompensationStatus::Waived,
                lock_version,
                None,
                waived_by,
            )
            .await
    }

    /// Execute an approved compensation action.
    ///
    /// **Phase 3 Batch 1 bounded slice:** This method:
    /// 1. Validates the action is in Approved status (fails closed otherwise)
    /// 2. Validates execution policy: only `Automatic` feasibility can execute in this slice
    ///    (SemiAutomatic/ManualOnly require human intervention not in this slice;
    ///     NotPossible cannot be executed at all)
    /// 3. Runs the RollbackExecutor (for Rollback+Automatic path) or returns failure
    ///    (for all other strategy/feasibility combos)
    /// 4. Records the result via record_result, which transitions to Executed or Failed
    ///
    /// **Executor gate (status):** Only Approved actions can execute.
    /// **Execution policy gate (feasibility):** Only `Automatic` feasibility can execute.
    /// This prevents accidental execution of actions requiring manual intervention.
    ///
    /// **Bounded RollbackExecutor semantics:**
    /// - Rollback + Automatic: validates side effect context, returns acknowledgment
    /// - All other strategy types: fail closed with UNSUPPORTED_STRATEGY_TYPE
    /// - All other feasibility levels: fail closed with UNSUPPORTED_FEASIBILITY
    /// - Missing side effect: fail closed with SIDE_EFFECT_NOT_FOUND
    ///
    /// **This slice:** No retry/DLQ/orchestration. Real rollback/counter-action logic for
    /// non-Rollback strategies is Batch 1+ scope.
    ///
    /// **Fails closed on policy violations:**
    /// - If action is not Approved, returns CompensationActionNotExecutable error
    /// - If feasibility is not Automatic, returns CompensationActionNotExecutable error
    pub async fn execute_action(
        &self,
        action_id: Uuid,
        executed_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Fetch current action to validate transition
        let action = self.repo.get(action_id).await?;

        // Executor gate: only Approved actions can execute
        if action.status != CompensationStatus::Approved {
            return Err(IntentRebaseError::CompensationActionNotExecutable(
                action_id,
            ));
        }

        // Execution policy gate: only Automatic feasibility can execute in this slice.
        // SemiAutomatic/ManualOnly require human intervention workflows (not in this slice).
        // NotPossible cannot be executed at all.
        use crate::compensation_action::CompensationFeasibility;
        if action.feasibility != CompensationFeasibility::Automatic {
            return Err(IntentRebaseError::CompensationActionNotExecutable(
                action_id,
            ));
        }

        // Capture lock_version before executor runs for optimistic locking
        let lock_version = action.lock_version;

        // Run the bounded RollbackExecutor (inlined here to avoid dyn trait issues)
        // Fall back to StubCompensationExecutor behavior if no side_effect_repo is configured
        let executor_result = if let Some(ref side_effect_repo) = self.side_effect_repo {
            use crate::compensation_executor::RollbackExecutor;
            let executor = RollbackExecutor::new(side_effect_repo.clone());
            executor.execute(&action).await?
        } else {
            // Fallback to stub behavior for backward compatibility
            use crate::compensation_action::{CompensationFeasibility, StrategyType};
            // For non-Rollback strategy types, return failure (stub behavior)
            if action.strategy_type != StrategyType::Rollback {
                ExecutionResult::failure(
                    &format!("Unsupported strategy type: {:?}", action.strategy_type),
                    "UNSUPPORTED_STRATEGY_TYPE",
                    None,
                )
            } else if action.feasibility != CompensationFeasibility::Automatic {
                // This case is already caught above, but included for completeness
                ExecutionResult::failure(
                    &format!("Unsupported feasibility: {:?}", action.feasibility),
                    "UNSUPPORTED_FEASIBILITY",
                    None,
                )
            } else {
                // Stub success for backward compatibility (should not reach here with proper config)
                ExecutionResult::success(&format!(
                    "Stub: executed {:?} for action {}",
                    action.strategy_type, action.id
                ))
            }
        };

        // Record the result which will transition to Executed or Failed
        let updated = self
            .record_result(action_id, &executor_result, lock_version, executed_by)
            .await?;

        Ok(updated)
    }

    /// Get all pending compensation actions for a tenant.
    ///
    /// Useful for batch processing of pending compensations.
    pub async fn get_pending_actions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_status(tenant_id, CompensationStatus::Pending)
            .await
    }

    /// Get all failed compensation actions for a tenant (for retry review).
    pub async fn get_failed_actions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_status(tenant_id, CompensationStatus::Failed)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use crate::compensation_action_repo::InMemoryCompensationActionRepository;
    use std::sync::Arc;

    fn create_test_service() -> CompensationActionService {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        CompensationActionService::new(repo)
    }

    fn create_test_service_with_side_effect_repo() -> CompensationActionService {
        // Service configured with side effect repo for real RollbackExecutor path
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
    }

    fn create_test_action(
        tenant_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
    ) -> CompensationAction {
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test rollback",
        )
    }

    #[tokio::test]
    async fn test_create_action() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let result = service.create_action(action).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_get_action() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        let retrieved = service.get_action(created.id).await.unwrap();
        assert_eq!(retrieved.id, created.id);
    }

    #[tokio::test]
    async fn test_get_action_not_found() {
        let service = create_test_service();
        let result = service.get_action(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..3 {
            let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
            service.create_action(action).await.unwrap();
        }

        let result = service.list_by_tenant(tenant_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        service.create_action(action1).await.unwrap();

        let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action2.status = CompensationStatus::Executed;
        service.create_action(action2).await.unwrap();

        let pending = service
            .list_by_status(tenant_id, CompensationStatus::Pending)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);

        let executed = service
            .list_by_status(tenant_id, CompensationStatus::Executed)
            .await
            .unwrap();
        assert_eq!(executed.len(), 1);
    }

    #[tokio::test]
    async fn test_update_status() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        let updated = service
            .update_status(
                created.id,
                CompensationStatus::Approved,
                created.lock_version,
            )
            .await
            .unwrap();

        assert_eq!(updated.status, CompensationStatus::Approved);
        assert!(updated.approved_at.is_some());
    }

    #[tokio::test]
    async fn test_record_result_success() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        let result = ExecutionResult::success("Rollback completed");
        let updated = service
            .record_result(created.id, &result, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(updated.status, CompensationStatus::Executed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_record_result_failure() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        let result = ExecutionResult::failure("Rollback failed", "ERR_001", None);
        let updated = service
            .record_result(created.id, &result, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(updated.status, CompensationStatus::Failed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_get_pending_actions() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        service.create_action(action1).await.unwrap();

        let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action2.status = CompensationStatus::Executed;
        service.create_action(action2).await.unwrap();

        let pending = service.get_pending_actions(tenant_id).await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_get_failed_actions() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let mut action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action1.status = CompensationStatus::Failed;
        service.create_action(action1).await.unwrap();

        let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action2.status = CompensationStatus::Pending;
        service.create_action(action2).await.unwrap();

        let failed = service.get_failed_actions(tenant_id).await.unwrap();
        assert_eq!(failed.len(), 1);
    }

    // === Status Transition Tests ===

    #[tokio::test]
    async fn test_approve_pending_action_success() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        assert_eq!(created.status, CompensationStatus::Pending);

        let approved = service
            .approve_action(created.id, created.lock_version, Some("test-approver"))
            .await
            .unwrap();

        assert_eq!(approved.status, CompensationStatus::Approved);
        assert!(approved.approved_at.is_some());
        assert_eq!(approved.approved_by, Some("test-approver".to_string()));
    }

    #[tokio::test]
    async fn test_approve_action_fails_on_non_pending() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First approve it
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);

        // Try to approve again - should fail
        let result = service
            .approve_action(approved.id, approved.lock_version, None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidCompensationActionTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_approve_action_fails_on_executed() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Execute directly (bypass approval) by setting status to Approved first
        let approved = service
            .update_status(
                created.id,
                CompensationStatus::Approved,
                created.lock_version,
            )
            .await
            .unwrap();

        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();
        assert_eq!(executed.status, CompensationStatus::Executed);

        // Try to approve an executed action - should fail
        let result = service
            .approve_action(executed.id, executed.lock_version, None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidCompensationActionTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_approve_action_fails_on_concurrency_conflict() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Try to approve with wrong lock_version - should fail with ConcurrencyConflict
        let result = service
            .approve_action(created.id, created.lock_version + 1, None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ConcurrencyConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_waive_pending_action_success() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        assert_eq!(created.status, CompensationStatus::Pending);

        let waived = service
            .waive_action(created.id, created.lock_version, Some("test-waiver"))
            .await
            .unwrap();

        assert_eq!(waived.status, CompensationStatus::Waived);
        // waived_by is stored in dedicated waived_by field
        assert_eq!(waived.waived_by, Some("test-waiver".to_string()));
        assert!(waived.waived_at.is_some());
    }

    #[tokio::test]
    async fn test_waive_action_fails_on_non_pending() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First approve it
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);

        // Try to waive an approved action - should fail
        let result = service
            .waive_action(approved.id, approved.lock_version, None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidCompensationActionTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_execute_action_success_on_approved() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create service with side effect repo
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

        // Create the side effect first so executor can find it
        let side_effect = crate::side_effect::SideEffect {
            id: side_effect_id,
            tenant_id,
            intent_id,
            intent_version: 1,
            effect_class: crate::side_effect::SideEffectClass::S1InternalReversible,
            effect_type: "metadata_write".to_string(),
            target: "db-record-123".to_string(),
            occurred_at: chrono::Utc::now(),
            idempotency_key: None,
        };
        side_effect_repo.create(side_effect).await.unwrap();

        let service = CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo);

        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First approve it
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);

        // Execute - should succeed with real RollbackExecutor
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        assert_eq!(executed.status, CompensationStatus::Executed);
        assert!(executed.executed_at.is_some());
        assert_eq!(executed.executed_by, Some("test-executor".to_string()));
        assert!(executed.execution_result_payload.is_some());
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_pending() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        assert_eq!(created.status, CompensationStatus::Pending);

        // Try to execute without approval - should fail
        let result = service
            .execute_action(created.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_executed() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Approve and execute
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();
        assert_eq!(executed.status, CompensationStatus::Executed);

        // Try to execute again - should fail
        let result = service
            .execute_action(executed.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_waived() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Waive it
        let waived = service
            .waive_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(waived.status, CompensationStatus::Waived);

        // Try to execute a waived action - should fail
        let result = service
            .execute_action(waived.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_failed() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First make it Failed via record_result
        let failed_result = ExecutionResult::failure("Test failure", "TEST_ERR", None);
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(failed.status, CompensationStatus::Failed);

        // Try to execute a failed action - should fail
        let result = service
            .execute_action(failed.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    // === Execution Policy Gate Tests ===

    #[tokio::test]
    async fn test_execute_action_fails_on_non_automatic_feasibility() {
        // Phase 3 Batch 1 bounded slice: only Automatic feasibility can execute.
        // SemiAutomatic/ManualOnly require human intervention not in this slice.
        // NotPossible cannot be executed at all.
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with SemiAutomatic feasibility (requires human intervention)
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::FollowupNotice,
            "Send follow-up notice",
        );

        let created = service.create_action(action).await.unwrap();
        assert_eq!(created.feasibility, CompensationFeasibility::SemiAutomatic);

        // Approve it
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);

        // Try to execute - should fail because SemiAutomatic requires human intervention
        let result = service
            .execute_action(approved.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_manual_only_feasibility() {
        // Phase 3 Batch 1 bounded slice: ManualOnly feasibility requires human intervention
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly,
            StrategyType::Escalation,
            "Manual escalation required",
        );

        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Try to execute - should fail because ManualOnly requires human intervention
        let result = service
            .execute_action(approved.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    #[tokio::test]
    async fn test_execute_action_fails_on_not_possible_feasibility() {
        // Phase 3 Batch 1 bounded slice: NotPossible feasibility cannot be executed at all
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::NotPossible,
            StrategyType::Quarantine,
            "Cannot compensate",
        );

        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Try to execute - should fail because NotPossible cannot be executed
        let result = service
            .execute_action(approved.id, Some("test-executor"))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotExecutable(_)
        ));
    }

    // === Transition Matrix Tests ===

    #[test]
    fn test_status_transition_pending_to_approved() {
        let validation =
            CompensationStatus::Pending.can_transition_to(CompensationStatus::Approved);
        assert!(validation.allowed);
    }

    #[test]
    fn test_status_transition_pending_to_waived() {
        let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Waived);
        assert!(validation.allowed);
    }

    #[test]
    fn test_status_transition_approved_to_executed() {
        let validation =
            CompensationStatus::Approved.can_transition_to(CompensationStatus::Executed);
        assert!(validation.allowed);
    }

    #[test]
    fn test_status_transition_executed_is_terminal() {
        assert!(CompensationStatus::Executed.is_terminal());
        let validation =
            CompensationStatus::Executed.can_transition_to(CompensationStatus::Pending);
        assert!(!validation.allowed);
        assert!(validation.reason.is_some());
    }

    #[test]
    fn test_status_transition_failed_is_terminal() {
        assert!(CompensationStatus::Failed.is_terminal());
        let validation = CompensationStatus::Failed.can_transition_to(CompensationStatus::Pending);
        assert!(!validation.allowed);
        assert!(validation.reason.is_some());
    }

    #[test]
    fn test_status_transition_waived_is_terminal() {
        assert!(CompensationStatus::Waived.is_terminal());
        let validation = CompensationStatus::Waived.can_transition_to(CompensationStatus::Pending);
        assert!(!validation.allowed);
        assert!(validation.reason.is_some());
    }

    #[test]
    fn test_status_transition_pending_to_executed_not_allowed() {
        // Must be approved first
        let validation =
            CompensationStatus::Pending.can_transition_to(CompensationStatus::Executed);
        assert!(!validation.allowed);
    }

    #[test]
    fn test_status_transition_approved_to_pending_not_allowed() {
        // No undo of approval
        let validation =
            CompensationStatus::Approved.can_transition_to(CompensationStatus::Pending);
        assert!(!validation.allowed);
    }

    #[test]
    fn test_status_transition_to_same_status_not_allowed() {
        let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Pending);
        assert!(!validation.allowed);
        assert!(validation.reason.is_some());
    }
}
