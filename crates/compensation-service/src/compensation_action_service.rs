//! Compensation action service facade for creating, querying, and executing compensation actions.
//!
//! Phase 3 Batch 1: Compensation action persistence and execution service.
//! Provides APIs for creating, querying, approving, waiving, and executing
//! compensation actions with proper status transition validation and tenant isolation.
//!
//! **Bounded executor slice scope:**
//! - Executor is RollbackExecutor for Rollback+Automatic path; StubCompensationExecutor for tests
//! - Only Approved actions can execute; illegal transitions fail closed
//! - **Manual retry:** Failed actions can be reapproved when retryable error + budget remains
//! - **Derived DLQ:** Failed actions with exhausted budget or non-retryable error are DLQ candidates
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

    /// Manually reapprove a failed compensation action (Failed → Pending).
    ///
    /// **Phase 3 Batch 1 bounded manual retry slice:**
    /// This allows manual recovery of failed actions by transitioning them back to
    /// Pending status, where they can be approved and executed again.
    ///
    /// **Policy gates (fail closed):**
    /// - Action must be in Failed status
    /// - Action must have remaining retry budget (attempt_count < max_retries)
    /// - Error code must be retryable (not a permanent failure)
    ///
    /// **Fails closed when:**
    /// - Action is not in Failed status → InvalidCompensationActionTransition
    /// - Retry budget exhausted → CompensationActionNotReapprovable
    /// - Error is non-retryable → CompensationActionNotReapprovable
    /// - Optimistic lock conflict → ConcurrencyConflict
    ///
    /// **Note:** This does NOT reset the attempt_count. The action retains its
    /// failure history. Reapproval just allows another execution attempt within
    /// the retry budget.
    ///
    /// **Reapproval preserves:** approved_at/approved_by if previously approved
    /// (those fields are for initial approval, not reapproval).
    pub async fn reapprove_action(
        &self,
        action_id: Uuid,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Fetch current action to validate state
        let action = self.repo.get(action_id).await?;

        // Policy gate 1: Must be in Failed status
        if action.status != CompensationStatus::Failed {
            return Err(IntentRebaseError::InvalidCompensationActionTransition {
                from_status: format!("{:?}", action.status),
                to_status: "Pending".into(),
                reason: "Only Failed actions can be reapproved".to_string(),
            });
        }

        // Policy gate 2: Check retry budget
        if action.attempt_count >= action.max_retries {
            return Err(IntentRebaseError::CompensationActionNotReapprovable(
                action_id,
                format!(
                    "Retry budget exhausted: {} attempts made (max={})",
                    action.attempt_count, action.max_retries
                ),
            ));
        }

        // Policy gate 3: Error must be retryable (non-retryable error = denial)
        // reapproval_denial_reason() returns Some only if reapproval should be denied
        // (i.e., can_be_reapproved() would return false)
        if let Some(denial_reason) = action.reapproval_denial_reason() {
            return Err(IntentRebaseError::CompensationActionNotReapprovable(
                action_id,
                denial_reason,
            ));
        }

        // Perform the Failed → Pending transition using dedicated reapprove method
        // This preserves approval history without corrupting timestamps
        let updated = self.repo.reapprove(action_id, lock_version).await?;

        Ok(updated)
    }

    /// Get all DLQ (Dead Letter Queue) candidate compensation actions for a tenant.
    ///
    /// **Derived DLQ condition:** An action is a DLQ candidate when:
    /// 1. Status is Failed AND
    /// 2. Either:
    ///    a. attempt_count >= max_retries (exhausted retry budget), OR
    ///    b. The error code is non-retryable (permanent failure)
    ///
    /// **No DLQ table:** This is a read-only derived query from existing data.
    /// DLQ candidates cannot be reapproved - they represent failures that have
    /// exhausted automated retry possibilities and require manual investigation.
    ///
    /// **This slice:** No background worker processes DLQ. Manual intervention
    /// is the only path forward for DLQ candidates.
    pub async fn list_dlq_candidates(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let failed_actions = self
            .repo
            .list_by_status(tenant_id, CompensationStatus::Failed)
            .await?;

        // Filter to DLQ candidates only
        let dlq_candidates: Vec<CompensationAction> = failed_actions
            .into_iter()
            .filter(|action| action.is_dlq_candidate())
            .collect();

        Ok(dlq_candidates)
    }

    /// Get a summary of DLQ candidates for a tenant (count only).
    ///
    /// Useful for dashboards and alerting without fetching full action data.
    pub async fn get_dlq_candidate_count(
        &self,
        tenant_id: Uuid,
    ) -> Result<usize, IntentRebaseError> {
        let dlq_candidates = self.list_dlq_candidates(tenant_id).await?;
        Ok(dlq_candidates.len())
    }

    /// Get all batch candidate compensation actions for a tenant across all categories.
    ///
    /// Phase 3 Batch 1 (bounded read-only batch candidate queue slice): Returns a
    /// consolidated view of all actionable compensation categories for batch processing.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Four candidate categories:**
    /// 1. `pending_approval_candidates` - Actions in Pending status awaiting approval
    /// 2. `approved_auto_executable_candidates` - Approved actions with Automatic feasibility
    /// 3. `retryable_failed_candidates` - Failed actions that can be reapproved (retryable error + budget remains)
    /// 4. `dlq_candidates` - Failed actions that exhausted retry budget or have non-retryable errors
    ///
    /// **No execution, orchestration, or policy gate:**
    /// This is a read-only query endpoint. It does not trigger any mutations,
    /// execute any actions, or involve background workers.
    pub async fn list_batch_candidates(
        &self,
        tenant_id: Uuid,
    ) -> Result<BatchCandidates, IntentRebaseError> {
        // Fetch all relevant statuses in parallel for efficiency
        let (pending, approved, failed) = tokio::join!(
            self.list_by_status(tenant_id, CompensationStatus::Pending),
            self.list_by_status(tenant_id, CompensationStatus::Approved),
            self.list_by_status(tenant_id, CompensationStatus::Failed),
        );

        let pending = pending?;
        let approved = approved?;
        let failed = failed?;

        // Category 1: Pending approval candidates (all Pending status)
        let pending_approval_candidates = pending;

        // Category 2: Approved auto-executable candidates
        let approved_auto_executable_candidates: Vec<CompensationAction> = approved
            .into_iter()
            .filter(|action| action.is_auto_executable())
            .collect();

        // Category 3: Retryable failed candidates (can be reapproved)
        let retryable_failed_candidates: Vec<CompensationAction> = failed
            .iter()
            .filter(|action| action.can_be_reapproved())
            .cloned()
            .collect();

        // Category 4: DLQ candidates (exhausted budget or non-retryable error)
        let dlq_candidates: Vec<CompensationAction> = failed
            .iter()
            .filter(|action| action.is_dlq_candidate())
            .cloned()
            .collect();

        Ok(BatchCandidates {
            pending_approval_candidates,
            approved_auto_executable_candidates,
            retryable_failed_candidates,
            dlq_candidates,
        })
    }
}

/// Batch candidates response containing all four candidate categories.
#[derive(Debug, Clone)]
pub struct BatchCandidates {
    /// Actions in Pending status awaiting approval
    pub pending_approval_candidates: Vec<CompensationAction>,
    /// Approved actions with Automatic feasibility that can be auto-executed
    pub approved_auto_executable_candidates: Vec<CompensationAction>,
    /// Failed actions that can be reapproved (retryable error + budget remains)
    pub retryable_failed_candidates: Vec<CompensationAction>,
    /// Failed actions that exhausted retry budget or have non-retryable errors
    pub dlq_candidates: Vec<CompensationAction>,
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
    fn test_status_transition_failed_is_not_terminal() {
        // Phase 3 Batch 1: Failed is NOT terminal because manual retry allows Failed → Pending
        assert!(!CompensationStatus::Failed.is_terminal());
        let validation = CompensationStatus::Failed.can_transition_to(CompensationStatus::Pending);
        assert!(validation.allowed);
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

    // === Manual Retry Tests ===

    #[tokio::test]
    async fn test_reapprove_action_success() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First make it Failed with a retryable error via record_result
        let failed_result = ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            Some("Connection timed out".to_string()),
        );
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(failed.status, CompensationStatus::Failed);
        assert_eq!(failed.attempt_count, 1);

        // Now reapprove it
        let reapproved = service
            .reapprove_action(failed.id, failed.lock_version)
            .await
            .unwrap();

        assert_eq!(reapproved.status, CompensationStatus::Pending);
        assert_eq!(reapproved.attempt_count, 1); // attempt_count preserved
        assert!(reapproved.failed_at.is_none()); // failed_at cleared
    }

    #[tokio::test]
    async fn test_reapprove_action_fails_on_non_failed_status() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Try to reapprove a Pending action - should fail
        let result = service
            .reapprove_action(created.id, created.lock_version)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidCompensationActionTransition { .. }
        ));
    }

    #[tokio::test]
    async fn test_reapprove_action_fails_on_retry_budget_exhausted() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with max_retries = 1 for testing
        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test rollback",
        );
        action.max_retries = 1; // Set to 1 so first failure exhausts budget

        let created = service.create_action(action).await.unwrap();

        // First failure
        let failed_result1 = ExecutionResult::failure("First failure", "CONNECTION_TIMEOUT", None);
        let failed1 = service
            .record_result(created.id, &failed_result1, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(failed1.attempt_count, 1);

        // Try to reapprove - should fail because budget exhausted
        let result = service
            .reapprove_action(failed1.id, failed1.lock_version)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            IntentRebaseError::CompensationActionNotReapprovable(_, _)
        ));
    }

    #[tokio::test]
    async fn test_reapprove_action_fails_on_non_retryable_error() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Fail with a non-retryable error
        let failed_result = ExecutionResult::failure(
            "Permanent failure",
            "INVALID_CONFIGURATION", // Non-retryable error
            Some("Invalid configuration".to_string()),
        );
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(failed.status, CompensationStatus::Failed);

        // Try to reapprove - should fail because error is non-retryable
        let result = service
            .reapprove_action(failed.id, failed.lock_version)
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            IntentRebaseError::CompensationActionNotReapprovable(_, _)
        ));
    }

    #[tokio::test]
    async fn test_reapprove_action_fails_on_concurrency_conflict() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // First make it Failed
        let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        // Try to reapprove with wrong lock_version - should fail
        let result = service
            .reapprove_action(failed.id, failed.lock_version + 1)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ConcurrencyConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_list_dlq_candidates_empty() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();

        let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
        assert!(dlq.is_empty());
    }

    #[tokio::test]
    async fn test_list_dlq_candidates_returns_exhausted_budget() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with max_retries = 1
        let mut action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test",
        );
        action.max_retries = 1;

        let created = service.create_action(action).await.unwrap();

        // First failure exhausts budget
        let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        // Verify it's a DLQ candidate
        assert!(failed.is_dlq_candidate());

        // List DLQ candidates
        let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
        assert_eq!(dlq.len(), 1);
        assert_eq!(dlq[0].id, failed.id);
    }

    #[tokio::test]
    async fn test_list_dlq_candidates_returns_non_retryable_error() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let created = service.create_action(action).await.unwrap();

        // Fail with non-retryable error
        let failed_result =
            ExecutionResult::failure("Permanent failure", "INVALID_CONFIGURATION", None);
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        // Verify it's a DLQ candidate
        assert!(failed.is_dlq_candidate());

        // List DLQ candidates
        let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
        assert_eq!(dlq.len(), 1);
        assert_eq!(dlq[0].id, failed.id);
    }

    #[tokio::test]
    async fn test_list_dlq_candidates_excludes_retryable_failures() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let created = service.create_action(action).await.unwrap();

        // Fail with retryable error
        let failed_result = ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT", // Retryable
            None,
        );
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        // Verify it's NOT a DLQ candidate (can be reapproved)
        assert!(!failed.is_dlq_candidate());
        assert!(failed.can_be_reapproved());

        // List DLQ candidates
        let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
        assert!(dlq.is_empty());
    }

    #[tokio::test]
    async fn test_get_dlq_candidate_count() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with max_retries = 1
        let mut action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test",
        );
        action.max_retries = 1;

        let created = service.create_action(action).await.unwrap();

        // First failure exhausts budget
        let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
        service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        let count = service.get_dlq_candidate_count(tenant_id).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_reapprove_preserves_attempt_count() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let created = service.create_action(action).await.unwrap();

        // First failure
        let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
        let failed = service
            .record_result(created.id, &failed_result, created.lock_version, None)
            .await
            .unwrap();

        assert_eq!(failed.attempt_count, 1);

        // Reapprove
        let reapproved = service
            .reapprove_action(failed.id, failed.lock_version)
            .await
            .unwrap();

        // Attempt count should be preserved
        assert_eq!(reapproved.attempt_count, 1);

        // Execute and fail again
        let approved = service
            .approve_action(reapproved.id, reapproved.lock_version, None)
            .await
            .unwrap();

        let failed2_result = ExecutionResult::failure("Second failure", "READ_TIMEOUT", None);
        let failed2 = service
            .record_result(approved.id, &failed2_result, approved.lock_version, None)
            .await
            .unwrap();

        // Now attempt_count should be 2
        assert_eq!(failed2.attempt_count, 2);
    }

    #[tokio::test]
    async fn test_list_batch_candidates_all_categories() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create 4 actions, one for each category

        // Category 1: Pending approval (Pending status + Automatic feasibility)
        let mut pending_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Pending auto action",
        );
        pending_action.max_retries = 3;
        let pending_created = service.create_action(pending_action).await.unwrap();
        assert_eq!(pending_created.status, CompensationStatus::Pending);

        // Category 2: Approved auto-executable (Approved status + Automatic feasibility)
        let mut approved_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Approved auto action",
        );
        approved_action.max_retries = 3;
        let approved_created = service.create_action(approved_action).await.unwrap();
        let approved_updated = service
            .approve_action(approved_created.id, approved_created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved_updated.status, CompensationStatus::Approved);
        assert!(approved_updated.is_auto_executable());

        // Category 3: Retryable failed (Failed status + retryable error + budget remains)
        let mut retryable_failed_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Retryable failed action",
        );
        retryable_failed_action.max_retries = 3;
        let retryable_created = service
            .create_action(retryable_failed_action)
            .await
            .unwrap();
        // Approve then fail with retryable error
        let retryable_approved = service
            .approve_action(retryable_created.id, retryable_created.lock_version, None)
            .await
            .unwrap();
        let retryable_failed_result =
            ExecutionResult::failure("Transient", "CONNECTION_TIMEOUT", None);
        let retryable_failed = service
            .record_result(
                retryable_approved.id,
                &retryable_failed_result,
                retryable_approved.lock_version,
                None,
            )
            .await
            .unwrap();
        assert_eq!(retryable_failed.status, CompensationStatus::Failed);
        assert!(retryable_failed.can_be_reapproved());
        assert!(!retryable_failed.is_dlq_candidate());

        // Category 4: DLQ candidate (Failed status + exhausted budget)
        let mut dlq_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "DLQ candidate action",
        );
        dlq_action.max_retries = 1; // Exhausts on first failure
        let dlq_created = service.create_action(dlq_action).await.unwrap();
        // Approve then fail to exhaust budget
        let dlq_approved = service
            .approve_action(dlq_created.id, dlq_created.lock_version, None)
            .await
            .unwrap();
        let dlq_failed_result = ExecutionResult::failure("Exhausted", "CONNECTION_TIMEOUT", None);
        let dlq_failed = service
            .record_result(
                dlq_approved.id,
                &dlq_failed_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();
        assert_eq!(dlq_failed.status, CompensationStatus::Failed);
        assert!(dlq_failed.is_dlq_candidate());
        assert!(!dlq_failed.can_be_reapproved());

        // Also create a non-retryable DLQ candidate for additional coverage
        let mut non_retryable_dlq_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Non-retryable DLQ action",
        );
        non_retryable_dlq_action.max_retries = 3;
        let non_retryable_created = service
            .create_action(non_retryable_dlq_action)
            .await
            .unwrap();
        let non_retryable_approved = service
            .approve_action(
                non_retryable_created.id,
                non_retryable_created.lock_version,
                None,
            )
            .await
            .unwrap();
        let non_retryable_failed_result =
            ExecutionResult::failure("Permanent", "INVALID_CONFIG", None);
        let non_retryable_failed = service
            .record_result(
                non_retryable_approved.id,
                &non_retryable_failed_result,
                non_retryable_approved.lock_version,
                None,
            )
            .await
            .unwrap();
        assert_eq!(non_retryable_failed.status, CompensationStatus::Failed);
        assert!(non_retryable_failed.is_dlq_candidate());
        assert!(!non_retryable_failed.can_be_reapproved());

        // Now test the batch candidates endpoint
        let batch = service.list_batch_candidates(tenant_id).await.unwrap();

        // Verify pending approval candidates
        assert_eq!(batch.pending_approval_candidates.len(), 1);
        assert_eq!(batch.pending_approval_candidates[0].id, pending_created.id);

        // Verify approved auto-executable candidates
        assert_eq!(batch.approved_auto_executable_candidates.len(), 1);
        assert_eq!(
            batch.approved_auto_executable_candidates[0].id,
            approved_updated.id
        );

        // Verify retryable failed candidates
        assert_eq!(batch.retryable_failed_candidates.len(), 1);
        assert_eq!(batch.retryable_failed_candidates[0].id, retryable_failed.id);

        // Verify DLQ candidates (should be 2: exhausted budget + non-retryable error)
        assert_eq!(batch.dlq_candidates.len(), 2);
        let dlq_ids: Vec<_> = batch.dlq_candidates.iter().map(|a| a.id).collect();
        assert!(dlq_ids.contains(&dlq_failed.id));
        assert!(dlq_ids.contains(&non_retryable_failed.id));
    }

    #[tokio::test]
    async fn test_list_batch_candidates_empty_for_tenant() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();

        let batch = service.list_batch_candidates(tenant_id).await.unwrap();

        assert!(batch.pending_approval_candidates.is_empty());
        assert!(batch.approved_auto_executable_candidates.is_empty());
        assert!(batch.retryable_failed_candidates.is_empty());
        assert!(batch.dlq_candidates.is_empty());
    }

    #[tokio::test]
    async fn test_list_batch_candidates_approved_non_auto_not_included() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create an Approved action with SemiAutomatic feasibility (not auto-executable)
        let mut semi_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::SemiAutomatic,
            StrategyType::Rollback,
            "Semi-auto approved action",
        );
        semi_action.max_retries = 3;
        let semi_created = service.create_action(semi_action).await.unwrap();
        let semi_approved = service
            .approve_action(semi_created.id, semi_created.lock_version, None)
            .await
            .unwrap();

        // Should be Approved but NOT auto-executable
        assert_eq!(semi_approved.status, CompensationStatus::Approved);
        assert!(!semi_approved.is_auto_executable());

        // Also create a pending action (Automatic feasibility) so we have something in pending
        let mut pending_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Pending auto action",
        );
        pending_action.max_retries = 3;
        let _pending_created = service.create_action(pending_action).await.unwrap();

        // Batch candidates should NOT include SemiAutomatic in approved_auto_executable
        // but SHOULD include the Automatic pending action
        let batch = service.list_batch_candidates(tenant_id).await.unwrap();
        assert!(batch.approved_auto_executable_candidates.is_empty());
        assert_eq!(batch.pending_approval_candidates.len(), 1);
        assert_eq!(
            batch.pending_approval_candidates[0].feasibility,
            CompensationFeasibility::Automatic
        );
    }
}
