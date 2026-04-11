//! Single-shot orchestration runtime for compensation action execution
//!
//! Phase 3 Batch 1 (bounded single-shot orchestration slice):
//! A synchronous runtime that processes explicit compensation action IDs,
//! auto-deciding approve | reapprove | execute | skip per action using
//! existing planner/write paths.
//!
//! **Bounded scope:**
//! - Single-shot: one run = one explicit action list, one auto-decide pass
//! - No queue polling, no distributed claiming/locking, no scheduler
//! - Runtime auto-decides using existing CompensationActionService methods
//! - Reuses existing write paths; does not replace enforcement in underlying methods

use std::sync::Arc;
use uuid::Uuid;

use intent_rebase_types::IntentRebaseError;

use crate::compensation_action::{CompensationAction, CompensationStatus};
use crate::compensation_action_repo::CompensationActionRepository;
use crate::compensation_action_service::CompensationActionService;
use crate::orchestration_run::{
    OrchestrationActionDecision, OrchestrationRun, RunItemResult, RunStatus,
};
use crate::orchestration_run_repo::OrchestrationRunRepository;

/// Single-shot orchestration runtime.
///
/// Phase 3 Batch 1: Takes explicit action IDs, auto-decides per action
/// using existing CompensationActionService methods, and persists results.
///
/// **Bounded semantics:**
/// - Single-shot: run once, report results
/// - No queue polling, no distributed claiming/locking
/// - Uses existing service methods (approve_action, reapprove_action, execute_action)
/// - Partial-success: continues on per-item failures, records all outcomes
#[derive(Clone)]
pub struct OrchestrationRuntime {
    action_service: Arc<CompensationActionService>,
    run_repo: Arc<dyn OrchestrationRunRepository>,
}

impl OrchestrationRuntime {
    /// Create a new OrchestrationRuntime.
    pub fn new(
        action_service: Arc<CompensationActionService>,
        run_repo: Arc<dyn OrchestrationRunRepository>,
    ) -> Self {
        Self {
            action_service,
            run_repo,
        }
    }

    /// Create and persist a new orchestration run in Pending state.
    ///
    /// This is used by the HTTP accepted flow so the API can return a durable
    /// run handle immediately, then complete execution in the background.
    pub async fn create_run(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        initiated_by: Option<String>,
        intent_id: Option<Uuid>,
    ) -> Result<OrchestrationRun, IntentRebaseError> {
        let run = OrchestrationRun::new(tenant_id, action_ids, initiated_by, intent_id);
        self.run_repo.create(run).await
    }

    /// Execute a single-shot orchestration run over explicit action IDs.
    ///
    /// **Bounded single-shot semantics:**
    /// 1. Persists a new OrchestrationRun with Pending status
    /// 2. Marks run as Running
    /// 3. For each action_id, queries current state and auto-decides:
    ///    - Pending → approve via approve_action
    ///    - Failed (can_be_reapproved) → reapprove via reapprove_action
    ///    - Approved (is_auto_executable) → execute via execute_action
    ///    - Terminal / policy-blocked → skip
    ///    - Not found → record not_found
    /// 4. Records per-item results in the run
    /// 5. Marks run as Completed/CompletedWithErrors/Failed
    /// 6. Returns the completed run
    ///
    /// **Partial-success semantics:**
    /// - Continues on per-item failures, records all outcomes
    /// - Run status is Completed if all succeeded, CompletedWithErrors if some failed, Failed if all failed
    pub async fn execute_run(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        initiated_by: Option<String>,
        intent_id: Option<Uuid>,
    ) -> Result<OrchestrationRun, IntentRebaseError> {
        let run = self
            .create_run(tenant_id, action_ids, initiated_by, intent_id)
            .await?;
        self.execute_existing_run(run.id).await
    }

    /// Execute an already-persisted run by ID.
    ///
    /// This is the runtime entrypoint used by the HTTP accepted flow after the
    /// run handle has been returned to the caller.
    pub async fn execute_existing_run(
        &self,
        run_id: Uuid,
    ) -> Result<OrchestrationRun, IntentRebaseError> {
        let mut run = self.run_repo.get(run_id).await?;
        let tenant_id = run.tenant_id;

        // Step 2: Mark as running
        run.mark_started();
        run = self.run_repo.update(&run).await?;

        // Step 3: Execute single-shot pass over all actions
        for action_id in run.action_ids.clone() {
            let result = self.process_single_action(tenant_id, action_id).await;

            match result {
                Ok(item_result) => {
                    if item_result.success {
                        run.record_success(
                            action_id,
                            item_result.action_taken,
                            item_result.reason,
                            item_result.resulting_status,
                        );
                    } else {
                        run.record_failure(
                            action_id,
                            item_result.action_taken,
                            item_result.reason,
                            item_result.resulting_status,
                        );
                    }
                }
                Err(e) => {
                    // On error, record as not_found or failure
                    if matches!(e, IntentRebaseError::CompensationActionNotFound(_)) {
                        run.record_not_found(action_id);
                    } else {
                        run.record_failure(
                            action_id,
                            OrchestrationActionDecision::Skip,
                            e.to_string(),
                            "error".to_string(),
                        );
                    }
                }
            }

            // Update after each item
            run = self.run_repo.update(&run).await?;
        }

        // Step 4: Mark as completed and finalize
        run.mark_completed();
        run = self.run_repo.update(&run).await?;

        Ok(run)
    }

    /// Process a single action and return the result.
    ///
    /// Auto-decides using existing CompensationActionService methods.
    async fn process_single_action(
        &self,
        tenant_id: Uuid,
        action_id: Uuid,
    ) -> Result<RunItemResult, IntentRebaseError> {
        // Fetch the action
        let action = match self.action_service.get_action(action_id).await {
            Ok(a) => a,
            Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                return Err(IntentRebaseError::CompensationActionNotFound(action_id));
            }
            Err(e) => return Err(e),
        };

        // Security check: verify tenant ownership
        if action.tenant_id != tenant_id {
            return Err(IntentRebaseError::CompensationActionNotFound(action_id));
        }

        // Auto-decide based on current status
        match action.status {
            CompensationStatus::Pending => self.handle_pending_action(action).await,
            CompensationStatus::Approved => self.handle_approved_action(action).await,
            CompensationStatus::Failed => self.handle_failed_action(action).await,
            // Terminal states: Executed, Waived → skip
            CompensationStatus::Executed | CompensationStatus::Waived => Ok(RunItemResult {
                action_id,
                action_taken: OrchestrationActionDecision::Skip,
                success: true,
                reason: format!("Action is in {:?} status (terminal)", action.status),
                resulting_status: format!("{:?}", action.status),
            }),
        }
    }

    /// Handle a Pending action: approve it.
    async fn handle_pending_action(
        &self,
        action: CompensationAction,
    ) -> Result<RunItemResult, IntentRebaseError> {
        let lock_version = action.lock_version;

        match self
            .action_service
            .approve_action(action.id, lock_version, None)
            .await
        {
            Ok(updated) => Ok(RunItemResult {
                action_id: action.id,
                action_taken: OrchestrationActionDecision::Approve,
                success: true,
                reason: "Action approved successfully".to_string(),
                resulting_status: format!("{:?}", updated.status),
            }),
            Err(e) => Ok(RunItemResult {
                action_id: action.id,
                action_taken: OrchestrationActionDecision::Approve,
                success: false,
                reason: e.to_string(),
                resulting_status: format!("{:?}", action.status),
            }),
        }
    }

    /// Handle an Approved action: execute if auto-executable.
    async fn handle_approved_action(
        &self,
        action: CompensationAction,
    ) -> Result<RunItemResult, IntentRebaseError> {
        // Check if auto-executable
        if action.is_auto_executable() {
            match self.action_service.execute_action(action.id, None).await {
                Ok(updated) => Ok(RunItemResult {
                    action_id: action.id,
                    action_taken: OrchestrationActionDecision::Execute,
                    success: true,
                    reason: "Action executed successfully".to_string(),
                    resulting_status: format!("{:?}", updated.status),
                }),
                Err(e) => Ok(RunItemResult {
                    action_id: action.id,
                    action_taken: OrchestrationActionDecision::Execute,
                    success: false,
                    reason: e.to_string(),
                    resulting_status: format!("{:?}", action.status),
                }),
            }
        } else {
            // Not auto-executable: skip (requires manual execution)
            Ok(RunItemResult {
                action_id: action.id,
                action_taken: OrchestrationActionDecision::Skip,
                success: true,
                reason: format!(
                    "Action requires manual execution ({})",
                    format_feasibility(action.feasibility)
                ),
                resulting_status: format!("{:?}", action.status),
            })
        }
    }

    /// Handle a Failed action: reapprove if eligible.
    async fn handle_failed_action(
        &self,
        action: CompensationAction,
    ) -> Result<RunItemResult, IntentRebaseError> {
        if action.can_be_reapproved() {
            let lock_version = action.lock_version;
            match self
                .action_service
                .reapprove_action(action.id, lock_version)
                .await
            {
                Ok(updated) => Ok(RunItemResult {
                    action_id: action.id,
                    action_taken: OrchestrationActionDecision::Reapprove,
                    success: true,
                    reason: "Action reapproved successfully".to_string(),
                    resulting_status: format!("{:?}", updated.status),
                }),
                Err(e) => Ok(RunItemResult {
                    action_id: action.id,
                    action_taken: OrchestrationActionDecision::Reapprove,
                    success: false,
                    reason: e.to_string(),
                    resulting_status: format!("{:?}", action.status),
                }),
            }
        } else {
            // Cannot be reapproved: DLQ candidate or non-retryable error
            let reason = action
                .reapproval_denial_reason()
                .unwrap_or_else(|| "DLQ candidate or non-retryable error".to_string());
            Ok(RunItemResult {
                action_id: action.id,
                action_taken: OrchestrationActionDecision::Skip,
                success: true, // Skipped is not a failure
                reason,
                resulting_status: format!("{:?}", action.status),
            })
        }
    }

    /// Get a run by ID.
    pub async fn get_run(&self, run_id: Uuid) -> Result<OrchestrationRun, IntentRebaseError> {
        self.run_repo.get(run_id).await
    }

    /// List runs by tenant.
    pub async fn list_runs_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<OrchestrationRun>, IntentRebaseError> {
        self.run_repo.list_by_tenant(tenant_id, limit).await
    }
}

/// Format feasibility for display.
fn format_feasibility(f: crate::compensation_action::CompensationFeasibility) -> &'static str {
    match f {
        crate::compensation_action::CompensationFeasibility::Automatic => "Automatic",
        crate::compensation_action::CompensationFeasibility::SemiAutomatic => "SemiAutomatic",
        crate::compensation_action::CompensationFeasibility::ManualOnly => "ManualOnly",
        crate::compensation_action::CompensationFeasibility::NotPossible => "NotPossible",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{
        CompensationAction, CompensationFeasibility, CompensationStatus, RebaseContext,
        StrategyType,
    };
    use crate::compensation_action_repo::InMemoryCompensationActionRepository;
    use crate::orchestration_run_repo::InMemoryOrchestrationRunRepository;
    use std::sync::Arc;

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

    fn create_test_runtime() -> OrchestrationRuntime {
        let action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let action_service = Arc::new(CompensationActionService::new(action_repo));
        OrchestrationRuntime::new(action_service, run_repo)
    }

    #[tokio::test]
    async fn test_execute_run_all_pending_approve() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let action_service = Arc::new(CompensationActionService::new(action_repo.clone()));
        let runtime = OrchestrationRuntime::new(action_service, run_repo);

        // Create two Pending actions
        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        let action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        let id1 = action1.id;
        let id2 = action2.id;

        action_repo.create(action1.clone()).await.unwrap();
        action_repo.create(action2.clone()).await.unwrap();

        // Execute run
        let run = runtime
            .execute_run(
                tenant_id,
                vec![id1, id2],
                Some("test-user".to_string()),
                None,
            )
            .await
            .unwrap();

        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.succeeded_count, 2);
        assert_eq!(run.failed_count, 0);
        assert_eq!(run.skipped_count, 0);
        assert_eq!(run.not_found_count, 0);
    }

    #[tokio::test]
    async fn test_execute_run_partial_failure() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let action_service = Arc::new(CompensationActionService::new(action_repo.clone()));
        let runtime = OrchestrationRuntime::new(action_service, run_repo);

        // Create a Pending action and a non-existent ID
        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        let id1 = action1.id;
        let id2 = Uuid::new_v4(); // Non-existent

        action_repo.create(action1.clone()).await.unwrap();

        // Execute run with one valid and one invalid ID
        let run = runtime
            .execute_run(tenant_id, vec![id1, id2], None, None)
            .await
            .unwrap();

        assert_eq!(run.status, RunStatus::CompletedWithErrors);
        assert_eq!(run.succeeded_count, 1);
        assert_eq!(run.not_found_count, 1);
    }

    #[tokio::test]
    async fn test_execute_run_not_found() {
        let tenant_id = Uuid::new_v4();
        let runtime = create_test_runtime();

        // Execute run with only non-existent IDs
        let run = runtime
            .execute_run(tenant_id, vec![Uuid::new_v4()], None, None)
            .await
            .unwrap();

        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.not_found_count, 1);
    }

    #[tokio::test]
    async fn test_get_run() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let action_service = Arc::new(CompensationActionService::new(action_repo.clone()));
        let runtime = OrchestrationRuntime::new(action_service, run_repo);

        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        let id = action.id;
        action_repo.create(action.clone()).await.unwrap();

        let run = runtime
            .execute_run(tenant_id, vec![id], None, None)
            .await
            .unwrap();

        let fetched = runtime.get_run(run.id).await.unwrap();
        assert_eq!(fetched.id, run.id);
    }

    #[tokio::test]
    async fn test_list_runs_by_tenant() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let action_service = Arc::new(CompensationActionService::new(action_repo.clone()));
        let runtime = OrchestrationRuntime::new(action_service, run_repo);

        // Create actions for 3 runs
        for _ in 0..3 {
            let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
            action_repo.create(action.clone()).await.unwrap();
            runtime
                .execute_run(tenant_id, vec![action.id], None, None)
                .await
                .unwrap();
        }

        let runs = runtime.list_runs_by_tenant(tenant_id, None).await.unwrap();
        assert_eq!(runs.len(), 3);
    }
}
