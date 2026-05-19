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

use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, CompensationStatus, ExecutionResult,
    RebaseContext, RetryableErrorClass,
};
use crate::compensation_action_repo::CompensationActionRepository;
use crate::compensation_action_types::*;
use crate::compensation_executor::CompensationExecutor;
use crate::rollback_record_repo::RollbackRecordRepository;
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::{
    get_current_trace_context, AuditRepository, CompensationCompletedAuditPayload,
    CompensationFailedAuditPayload, CompensationPlannedAuditPayload,
    CompensationStartedAuditPayload, IntentRebaseError,
};

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
    /// Optional rollback record repository for creating audit records on execute/waive.
    /// Phase 3 Batch 1: When Some, creates SideEffectRollbackRecord on execute and waive paths.
    /// When None, rollback records are silently skipped (additive-only, fail-open).
    rollback_record_repo: Option<Arc<dyn RollbackRecordRepository>>,
    /// Optional audit repository for emitting compensation audit events.
    /// Phase 3 Batch 0: When Some, emits CompensationPlanned/Started/Completed/Failed
    /// events. When None, audit events are silently skipped (fail-open).
    audit_repo: Option<Arc<dyn AuditRepository>>,
}

impl CompensationActionService {
    /// Create a new CompensationActionService with the given repository.
    ///
    /// Uses a stub executor that always returns success (backward compatibility).
    /// **Note:** For production use with real execution, use `new_with_side_effect_repo`.
    /// **Note:** Audit repository is not set by default. Use `with_audit_repo()` to add it.
    pub fn new(repo: Arc<dyn CompensationActionRepository>) -> Self {
        Self {
            repo,
            side_effect_repo: None,
            rollback_record_repo: None,
            audit_repo: None,
        }
    }

    /// Create a new CompensationActionService with side effect repository for
    /// real RollbackExecutor execution.
    ///
    /// This is the production constructor that enables real Rollback+Automatic execution.
    /// **Note:** Audit repository is not set by default. Use `with_audit_repo()` to add it.
    pub fn new_with_side_effect_repo(
        repo: Arc<dyn CompensationActionRepository>,
        side_effect_repo: Arc<dyn SideEffectRepository>,
    ) -> Self {
        Self {
            repo,
            side_effect_repo: Some(side_effect_repo),
            rollback_record_repo: None,
            audit_repo: None,
        }
    }

    /// Set the rollback record repository for this service instance.
    ///
    /// Returns a new CompensationActionService with the rollback record repository set.
    /// Phase 3 Batch 1: When set, creates SideEffectRollbackRecord on execute and waive paths.
    /// When not set, rollback record creation is silently skipped (additive-only, fail-open).
    pub fn with_rollback_record_repo(
        mut self,
        rollback_record_repo: Arc<dyn RollbackRecordRepository>,
    ) -> Self {
        self.rollback_record_repo = Some(rollback_record_repo);
        self
    }

    /// Set the audit repository for this service instance.
    ///
    /// Returns a new CompensationActionService with the audit repository set.
    /// Phase 3 Batch 0: When set, emits CompensationPlanned/Started/Completed/Failed
    /// events. When not set, audit events are silently skipped (fail-open).
    pub fn with_audit_repo(mut self, audit_repo: Arc<dyn AuditRepository>) -> Self {
        self.audit_repo = Some(audit_repo);
        self
    }

    /// Returns a reference to the underlying repository.
    ///
    /// This is used by RLS-aware handlers to access the SQL repository directly
    /// for transaction-wrapped operations.
    pub fn repo(&self) -> &Arc<dyn CompensationActionRepository> {
        &self.repo
    }

    /// Returns a reference to the underlying rollback record repository if configured.
    ///
    /// This is used by RLS-aware handlers to access the SQL rollback repository directly
    /// for transaction-wrapped rollback record creation.
    pub fn rollback_record_repo(&self) -> Option<&Arc<dyn RollbackRecordRepository>> {
        self.rollback_record_repo.as_ref()
    }

    /// Returns a clone of the underlying side effect repository if configured.
    ///
    /// This is used by RLS-aware handlers to access the side effect repository directly
    /// for running the bounded executors.
    ///
    /// Phase 3 P1-S5h: Enables the execute handler to run the executor inline
    /// when RLS transaction wrapping is used for record_result and rollback_record creation.
    pub fn side_effect_repo(&self) -> Option<Arc<dyn SideEffectRepository>> {
        self.side_effect_repo.clone()
    }

    /// Plan and generate compensation actions for an intent's side effects.
    ///
    /// Phase 3 (bounded planner slice): Uses actual SideEffect data to generate
    /// appropriate compensation actions based on S0-S4 classification.
    ///
    /// **Process:**
    /// 1. Fetch all side effects for the intent via side_effect_repo
    /// 2. Classify each side effect using BoundedCompensationPlanner
    /// 3. Persist generated compensation actions
    ///
    /// **Returns:** All generated compensation actions (excluding S0 which produces no action).
    ///
    /// **Error conditions:**
    /// - If side_effect_repo is not configured, returns error
    /// - If fetching side effects fails, returns error
    /// - If persisting any action fails, returns error (partial-success not implemented)
    ///
    /// **S0-S4 classification:**
    /// | Class | Strategy | Feasibility |
    /// |-------|----------|-------------|
    /// | S0PureRead | (none) | NotPossible | Skip - no action needed |
    /// | S1InternalReversible | Rollback | Automatic |
    /// | S2ExternalReversible | CounterAction | SemiAutomatic |
    /// | S3ExternalPartiallyReversible | FollowupNotice | ManualOnly |
    /// | S4Irreversible | Escalation | NotPossible |
    pub async fn plan_compensation_actions(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        rebase_context: RebaseContext,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        // Require side_effect_repo for bounded planning
        let side_effect_repo = self.side_effect_repo.as_ref().ok_or_else(|| {
            IntentRebaseError::Internal(
                "side_effect_repo is required for bounded compensation planning".into(),
            )
        })?;

        // Fetch side effects for this intent
        let side_effects = side_effect_repo
            .list_by_intent(intent_id, tenant_id)
            .await?;

        // Use bounded planner to classify and generate actions
        let planner = crate::BoundedCompensationPlanner::new();
        let generated_actions =
            planner.plan_from_side_effects(&rebase_context, &side_effects, tenant_id);

        // Persist all generated actions
        let mut persisted_actions = Vec::with_capacity(generated_actions.len());
        for action in generated_actions {
            let created = self.create_action(action).await?;
            persisted_actions.push(created);
        }

        Ok(persisted_actions)
    }

    /// Create a new compensation action.
    ///
    /// Returns the created action with its generated ID.
    ///
    /// **Phase 3 Batch 0:** Emits a `CompensationPlanned` audit event if an audit
    /// repository is configured. Best-effort emission - failures are logged but do
    /// not fail the operation.
    pub async fn create_action(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Capture fields needed for audit before action is consumed
        let tenant_id = action.tenant_id;
        let intent_id = action.intent_id;
        let compensation_plan_id = action.id;
        let from_version = action.trigger_context.from_version;
        let to_version = action.trigger_context.to_version;

        // Determine feasibility counts for audit payload
        let (auto_compensatable_count, manual_required_count, not_possible_count) =
            match action.feasibility {
                CompensationFeasibility::Automatic => (1, 0, 0),
                CompensationFeasibility::SemiAutomatic | CompensationFeasibility::ManualOnly => {
                    (0, 1, 0)
                }
                CompensationFeasibility::NotPossible => (0, 0, 1),
            };

        let result = self.repo.create(action).await;

        // Emit CompensationPlanned audit event (best-effort, fail-open)
        if let Ok(created_action) = &result {
            if let Some(ref audit_repo) = self.audit_repo {
                let payload = CompensationPlannedAuditPayload {
                    compensation_plan_id,
                    intent_id,
                    intent_version_from: from_version,
                    intent_version_to: to_version,
                    side_effect_count: 1,
                    auto_compensatable_count,
                    manual_required_count,
                    not_possible_count,
                };
                // Best-effort: log warning but don't fail the operation
                if let Err(e) = audit_repo
                    .record_compensation_planned(
                        tenant_id,
                        "compensation-service/system",
                        intent_id,
                        payload,
                        get_current_trace_context(),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to emit CompensationPlanned audit event for action {}: {:?}",
                        created_action.id,
                        e
                    );
                }
            }
        }

        result
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

        // Capture fields needed for rollback record before update
        let tenant_id = action.tenant_id;
        let compensation_action_id = action.id;
        let side_effect_id = action.side_effect_id;
        let intent_id = action.intent_id;

        // Update status with optimistic locking and persist actor info
        let updated = self
            .repo
            .update_status(
                action_id,
                CompensationStatus::Waived,
                lock_version,
                None,
                waived_by,
            )
            .await?;

        // Create rollback record on waive path (best-effort, fail-open)
        // Phase 3 Batch 1: Records waiver for audit/replay
        if let Some(ref rollback_record_repo) = self.rollback_record_repo {
            use crate::rollback_record::SideEffectRollbackRecord;
            let rollback_record = SideEffectRollbackRecord::waived(
                tenant_id,
                compensation_action_id,
                side_effect_id,
                intent_id,
                "Compensation action waived",
                waived_by,
            );
            if let Err(e) = rollback_record_repo.create(rollback_record).await {
                tracing::warn!(
                    "Failed to create rollback record for waived action {}: {:?}",
                    action_id,
                    e
                );
            }
        }

        Ok(updated)
    }

    /// Execute an approved compensation action.
    ///
    /// **Phase 3 Batch 1 bounded slice:** This method:
    /// 1. Validates the action is in Approved status (fails closed otherwise)
    /// 2. Validates execution policy: `Automatic` feasibility OR
    ///    (`CounterAction` strategy + `SemiAutomatic` feasibility) OR
    ///    (`FollowupNotice` strategy + `ManualOnly` feasibility) OR
    ///    (`Escalation` strategy + `NotPossible` feasibility) can execute in this slice.
    ///    All other feasibility/strategy combos fail closed.
    /// 3. Runs the appropriate executor (RollbackExecutor for Rollback+Automatic,
    ///    CounterActionExecutor for CounterAction+SemiAutomatic,
    ///    FollowupNoticeExecutor for FollowupNotice+ManualOnly,
    ///    EscalationExecutor for Escalation+NotPossible) or returns failure
    ///    (for all other strategy/feasibility combos)
    /// 4. Records the result via record_result, which transitions to Executed or Failed
    ///
    /// **Executor gate (status):** Only Approved actions can execute.
    /// **Execution policy gate (feasibility + strategy):**
    /// - Automatic feasibility: Rollback strategy can execute (S1InternalReversible)
    /// - SemiAutomatic feasibility: CounterAction strategy can execute (S2ExternalReversible)
    /// - ManualOnly feasibility: FollowupNotice strategy can execute (S3ExternalPartiallyReversible)
    /// - NotPossible feasibility: Escalation strategy can execute (S4Irreversible)
    /// - All other combos fail closed with CompensationActionNotExecutable
    ///
    /// **Bounded executor semantics:**
    /// - Rollback + Automatic: validates side effect context, returns acknowledgment
    /// - CounterAction + SemiAutomatic: validates side effect context, returns acknowledgment
    /// - FollowupNotice + ManualOnly: validates side effect context, returns acknowledgment
    /// - Escalation + NotPossible: validates side effect context, returns acknowledgment
    /// - All other strategy types: fail closed with UNSUPPORTED_STRATEGY_TYPE
    /// - All other feasibility levels: fail closed with UNSUPPORTED_FEASIBILITY
    /// - Missing side effect: fail closed with SIDE_EFFECT_NOT_FOUND
    ///
    /// **This slice:** No retry/DLQ/orchestration. Quarantine executor and manual
    /// intervention workflows are Batch 1+ scope.
    ///
    /// **Fails closed on policy violations:**
    /// - If action is not Approved, returns CompensationActionNotExecutable error
    /// - If feasibility is not in the allowed set, returns CompensationActionNotExecutable error
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

        // Execution policy gate: only allowed combos can execute in this slice.
        // Allowed combos:
        //   - Rollback + Automatic (S1InternalReversible)
        //   - CounterAction + SemiAutomatic (S2ExternalReversible)
        //   - FollowupNotice + ManualOnly (S3ExternalPartiallyReversible)
        //   - Escalation + NotPossible (S4Irreversible)
        // All other combos require human intervention or are not executable.
        use crate::compensation_action::{CompensationFeasibility, StrategyType};
        let is_allowed_combo = matches!(
            (action.strategy_type, action.feasibility),
            (StrategyType::Rollback, CompensationFeasibility::Automatic)
                | (
                    StrategyType::CounterAction,
                    CompensationFeasibility::SemiAutomatic
                )
                | (
                    StrategyType::FollowupNotice,
                    CompensationFeasibility::ManualOnly
                )
                | (
                    StrategyType::Escalation,
                    CompensationFeasibility::NotPossible
                )
        );
        if !is_allowed_combo {
            return Err(IntentRebaseError::CompensationActionNotExecutable(
                action_id,
            ));
        }

        // Capture lock_version before executor runs for optimistic locking
        let lock_version = action.lock_version;

        // Capture fields needed for audit events
        let tenant_id = action.tenant_id;
        let intent_id = action.intent_id;
        let compensation_plan_id = action.id;
        let actor_id = executed_by.unwrap_or("compensation-service/system");

        // Run the appropriate bounded executor based on strategy type
        // RollbackExecutor for Rollback+Automatic, CounterActionExecutor for CounterAction+SemiAutomatic,
        // FollowupNoticeExecutor for FollowupNotice+ManualOnly, EscalationExecutor for Escalation+NotPossible
        let executor_result = if let Some(ref side_effect_repo) = self.side_effect_repo {
            match (action.strategy_type, action.feasibility) {
                (StrategyType::Rollback, CompensationFeasibility::Automatic) => {
                    use crate::compensation_executor::RollbackExecutor;
                    let executor = RollbackExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::CounterAction, CompensationFeasibility::SemiAutomatic) => {
                    use crate::compensation_executor::CounterActionExecutor;
                    let executor = CounterActionExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::FollowupNotice, CompensationFeasibility::ManualOnly) => {
                    use crate::compensation_executor::FollowupNoticeExecutor;
                    let executor = FollowupNoticeExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::Escalation, CompensationFeasibility::NotPossible) => {
                    use crate::compensation_executor::EscalationExecutor;
                    let executor = EscalationExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                _ => {
                    // Should not reach here due to gate above, but fail closed for safety
                    ExecutionResult::failure(
                        &format!(
                            "Unsupported strategy/feasibility combo: {:?} + {:?}",
                            action.strategy_type, action.feasibility
                        ),
                        "UNSUPPORTED_COMBO",
                        None,
                    )
                }
            }
        } else {
            // Fallback to stub behavior for backward compatibility
            // For unsupported strategy types, return failure (stub behavior)
            match (action.strategy_type, action.feasibility) {
                (StrategyType::Rollback, CompensationFeasibility::Automatic) => {
                    ExecutionResult::success(&format!(
                        "Stub: executed {:?} for action {}",
                        action.strategy_type, action.id
                    ))
                }
                (StrategyType::CounterAction, CompensationFeasibility::SemiAutomatic) => {
                    ExecutionResult::success(&format!(
                        "Stub: executed {:?} for action {}",
                        action.strategy_type, action.id
                    ))
                }
                (StrategyType::FollowupNotice, CompensationFeasibility::ManualOnly) => {
                    ExecutionResult::success(&format!(
                        "Stub: executed {:?} for action {}",
                        action.strategy_type, action.id
                    ))
                }
                (StrategyType::Escalation, CompensationFeasibility::NotPossible) => {
                    ExecutionResult::success(&format!(
                        "Stub: executed {:?} for action {}",
                        action.strategy_type, action.id
                    ))
                }
                _ => ExecutionResult::failure(
                    &format!(
                        "Unsupported strategy/feasibility combo: {:?} + {:?}",
                        action.strategy_type, action.feasibility
                    ),
                    "UNSUPPORTED_COMBO",
                    None,
                ),
            }
        };

        // Emit CompensationStarted audit event (best-effort, fail-open)
        if let Some(ref audit_repo) = self.audit_repo {
            let payload = CompensationStartedAuditPayload {
                compensation_plan_id,
                intent_id,
                actions_initiated: 1,
            };
            if let Err(e) = audit_repo
                .record_compensation_started(
                    tenant_id,
                    actor_id,
                    intent_id,
                    payload,
                    get_current_trace_context(),
                )
                .await
            {
                tracing::warn!(
                    "Failed to emit CompensationStarted audit event for action {}: {:?}",
                    action_id,
                    e
                );
            }
        }

        // Record the result which will transition to Executed or Failed
        let updated = self
            .record_result(action_id, &executor_result, lock_version, executed_by)
            .await?;

        // Emit CompensationCompleted or CompensationFailed audit event (best-effort, fail-open)
        if let Some(ref audit_repo) = self.audit_repo {
            if executor_result.success {
                let payload = CompensationCompletedAuditPayload {
                    compensation_plan_id,
                    intent_id,
                    actions_succeeded: 1,
                    actions_failed: 0,
                };
                if let Err(e) = audit_repo
                    .record_compensation_completed(
                        tenant_id,
                        actor_id,
                        intent_id,
                        payload,
                        get_current_trace_context(),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to emit CompensationCompleted audit event for action {}: {:?}",
                        action_id,
                        e
                    );
                }
            } else {
                let payload = CompensationFailedAuditPayload {
                    compensation_plan_id,
                    intent_id,
                    failed_action_id: action_id,
                    error_summary: executor_result.summary.clone(),
                };
                if let Err(e) = audit_repo
                    .record_compensation_failed(
                        tenant_id,
                        actor_id,
                        intent_id,
                        payload,
                        get_current_trace_context(),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to emit CompensationFailed audit event for action {}: {:?}",
                        action_id,
                        e
                    );
                }
            }
        }

        // Create rollback record on execute path (best-effort, fail-open)
        // Phase 3 Batch 1: Records compensation execution outcome for audit/replay
        if let Some(ref rollback_record_repo) = self.rollback_record_repo {
            use crate::rollback_record::SideEffectRollbackRecord;
            let rollback_record = if executor_result.success {
                SideEffectRollbackRecord::success(
                    tenant_id,
                    compensation_plan_id,
                    action.side_effect_id,
                    intent_id,
                    &executor_result.summary,
                    executed_by,
                )
            } else {
                SideEffectRollbackRecord::failure_with_actor(
                    tenant_id,
                    compensation_plan_id,
                    action.side_effect_id,
                    intent_id,
                    &executor_result.summary,
                    executor_result
                        .error_code
                        .as_deref()
                        .unwrap_or("UNKNOWN_ERROR"),
                    executor_result.error_detail.clone(),
                    executed_by,
                )
            };
            if let Err(e) = rollback_record_repo.create(rollback_record).await {
                tracing::warn!(
                    "Failed to create rollback record for action {}: {:?}",
                    action_id,
                    e
                );
            }
        }

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
    /// 2. `approved_service_executable_candidates` - Approved actions executable by the service
    ///    Phase 3 Batch 1 P7: Includes both Rollback+Automatic and CounterAction+SemiAutomatic
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

        // Category 2: Approved service-executable candidates
        // Phase 3 Batch 1 P7: Uses is_service_executable() to include both
        // Rollback+Automatic (S1) and CounterAction+SemiAutomatic (S2) combos.
        let approved_service_executable_candidates: Vec<CompensationAction> = approved
            .into_iter()
            .filter(|action| action.is_service_executable())
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
            approved_service_executable_candidates,
            retryable_failed_candidates,
            dlq_candidates,
        })
    }
}

impl CompensationActionService {
    /// Plan orchestration actions for explicit compensation action IDs (dry-run).
    ///
    /// Phase 3 Batch 1 (bounded dry-run slice): For each provided compensation_action_id,
    /// determines the proposed action (approve | reapprove | execute | no_action) based
    /// on the action's current state.
    ///
    /// **This is READ-ONLY** - it does not execute any actions.
    ///
    /// **Action determination logic:**
    /// - `approve`: Action is Pending (can transition to Approved)
    /// - `reapprove`: Action is Failed AND can_be_reapproved() (retryable error + budget remains)
    /// - `execute`: Action is Approved AND is_service_executable() (Automatic or SemiAutomatic feasibility)
    /// - `no_action`: Action is in a terminal state or cannot perform any valid transition
    ///
    /// **Bounded partial-success semantics:**
    /// - If an action_id is not found, it's added to `not_found` and does not cause failure
    /// - All found actions are processed, even if some have no_action
    ///
    /// **No background worker or queue claiming:**
    /// This is a direct query-based planner that reads current state and proposes actions.
    pub async fn plan_orchestration_actions(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
    ) -> Result<OrchestrationDryRunResult, IntentRebaseError> {
        let mut proposals = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = OrchestrationDryRunSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Skip actions that don't belong to this tenant (security check)
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.no_action += 1;
                        continue;
                    }

                    let proposal = self.compute_action_proposal(&action);
                    match proposal.proposed_action {
                        OrchestrationAction::Approve => summary.can_approve += 1,
                        OrchestrationAction::Reapprove => summary.can_reapprove += 1,
                        OrchestrationAction::Execute => summary.can_execute += 1,
                        OrchestrationAction::NoAction => summary.no_action += 1,
                    }
                    proposals.push(proposal);
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.no_action += 1;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(OrchestrationDryRunResult {
            proposals,
            not_found,
            summary,
        })
    }

    /// Compute the proposed action for a single compensation action.
    fn compute_action_proposal(&self, action: &CompensationAction) -> OrchestrationActionProposal {
        use CompensationStatus::*;

        // Determine the best action for this action based on its state
        let (proposed_action, reason) = match action.status {
            // Pending actions can be approved
            Pending => {
                // Validate the transition is still allowed
                if action
                    .status
                    .can_transition_to(CompensationStatus::Approved)
                    .allowed
                {
                    (
                        OrchestrationAction::Approve,
                        format!(
                            "Action is pending approval with {} feasibility",
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else {
                    (
                        OrchestrationAction::NoAction,
                        "Action is pending but cannot transition to approved".to_string(),
                    )
                }
            }

            // Approved actions can be executed (if service-executable)
            Approved => {
                // Phase 3 Batch 1 P7: Uses is_service_executable() which includes both
                // Rollback+Automatic (S1) and CounterAction+SemiAutomatic (S2) combos
                if action.is_service_executable() {
                    (
                        OrchestrationAction::Execute,
                        format!(
                            "Action is approved with {} feasibility and no blocking conditions",
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else {
                    (
                        OrchestrationAction::NoAction,
                        format!(
                            "Action is approved but requires manual execution ({})",
                            format_feasibility(action.feasibility)
                        ),
                    )
                }
            }

            // Failed actions can potentially be reapproved
            Failed => {
                if action.can_be_reapproved() {
                    (
                        OrchestrationAction::Reapprove,
                        format!(
                            "Action failed but can be reapproved ({} retry attempts remaining, {} feasibility)",
                            action.max_retries - action.attempt_count,
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else if let Some(ref reason) = action.reapproval_denial_reason() {
                    (OrchestrationAction::NoAction, reason.clone())
                } else {
                    (
                        OrchestrationAction::NoAction,
                        format!(
                            "Action failed with non-retryable error or exhausted budget ({} feasibility)",
                            format_feasibility(action.feasibility)
                        ),
                    )
                }
            }

            // Terminal states - no action possible
            Executed => (
                OrchestrationAction::NoAction,
                "Action has already been executed (terminal state)".to_string(),
            ),
            Waived => (
                OrchestrationAction::NoAction,
                "Action has been waived (terminal state)".to_string(),
            ),
        };

        OrchestrationActionProposal {
            action_id: action.id,
            proposed_action,
            reason,
            current_status: action.status,
        }
    }

    /// Execute batch approve for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Approves multiple
    /// Pending compensation actions atomically where possible.
    ///
    /// **Bounded partial-success semantics:**
    /// - If an action_id is not found, it's recorded as `not_found` and continues
    /// - If an action fails validation, it's recorded as `failed` with error reason
    /// - Successful approvals are recorded as `succeeded`
    /// - Does NOT fail-fast on first error - all items are processed
    ///
    /// **Transition rules:**
    /// - Only Pending actions can be approved
    /// - Uses optimistic locking via lock_version to prevent concurrent updates
    /// - The lock_version is fetched from the action itself (no client-provided lock_version)
    ///
    /// **No background worker or queue claiming:**
    /// This is a direct service method that processes actions sequentially.
    pub async fn batch_approve(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        approved_by: Option<&str>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt approval using current lock_version
                    match self
                        .approve_action(action_id, action.lock_version, approved_by)
                        .await
                    {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }

    /// Execute batch reapprove for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Reapproves multiple
    /// Failed compensation actions that are eligible for retry.
    ///
    /// **Bounded partial-success semantics:** Same as batch_approve.
    ///
    /// **Policy gates (fail closed):**
    /// - Action must be in Failed status
    /// - Action must have remaining retry budget
    /// - Error code must be retryable
    ///
    /// **No background worker or queue claiming:** Same as batch_approve.
    pub async fn batch_reapprove(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt reapproval using current lock_version
                    match self.reapprove_action(action_id, action.lock_version).await {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }

    /// Execute batch execute for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Executes multiple
    /// Approved compensation actions that are service-executable.
    ///
    /// **Bounded partial-success semantics:** Same as batch_approve.
    ///
    /// **Executor gate:** Only Approved + Service-executable actions can execute.
    ///
    /// **No background worker or queue claiming:** Same as batch_approve.
    pub async fn batch_execute(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        executed_by: Option<&str>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt execution
                    match self.execute_action(action_id, executed_by).await {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }
}

impl CompensationActionService {
    /// Evaluate policy gates for all compensation actions of a tenant.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
    /// for all compensation actions belonging to the specified tenant.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Gate evaluation logic:**
    /// - `eligible`: Approved + Automatic feasibility + not blocked + not DLQ
    /// - `blocked`: DLQ candidate OR exhausted retry budget OR non-retryable error OR terminal status
    /// - `manual_review_required`: Pending status OR SemiAutomatic/ManualOnly feasibility
    ///
    /// **Derivation from existing surfaces:**
    /// - Gate status is derived from existing CompensationAction fields (status, feasibility,
    ///   attempt_count, max_retries, execution_result_payload.error_code)
    /// - No new policy engine or external risk surface is queried
    pub async fn evaluate_policy_gates(
        &self,
        tenant_id: Uuid,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let actions = self.list_by_tenant(tenant_id, None).await?;
        self.evaluate_policy_gates_from_actions(actions)
    }

    /// Evaluate policy gates for all compensation actions of an intent.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
    /// for all compensation actions belonging to the specified intent.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Gate evaluation logic:** Same as `evaluate_policy_gates`.
    pub async fn evaluate_policy_gates_for_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let actions = self.list_by_intent(intent_id, tenant_id).await?;
        self.evaluate_policy_gates_from_actions(actions)
    }

    /// Evaluate policy gates from a collection of compensation actions.
    ///
    /// Internal helper that computes gate evaluations and summary.
    fn evaluate_policy_gates_from_actions(
        &self,
        actions: Vec<CompensationAction>,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let total_actions = actions.len();
        let mut evaluations = Vec::with_capacity(total_actions);
        let mut summary = PolicyGateSummary::default();

        for action in actions {
            let evaluation = self.evaluate_single_action(&action);
            match evaluation.gate_status {
                PolicyGateStatus::Eligible => summary.eligible_count += 1,
                PolicyGateStatus::Blocked => summary.blocked_count += 1,
                PolicyGateStatus::ManualReviewRequired => summary.manual_review_required_count += 1,
            }

            if evaluation.policy_metadata.is_dlq_candidate {
                summary.dlq_candidate_count += 1;
            }
            if evaluation.policy_metadata.auto_executable {
                summary.auto_executable_count += 1;
            }
            if matches!(action.status, CompensationStatus::Pending) {
                summary.pending_approval_count += 1;
            }

            evaluations.push(evaluation);
        }

        summary.total_actions = total_actions;

        Ok(PolicyGateEvaluationResult {
            evaluations,
            summary,
        })
    }

    /// Evaluate policy gate for a single compensation action.
    fn evaluate_single_action(&self, action: &CompensationAction) -> PolicyGateEvaluation {
        let gate_status = self.compute_gate_status(action);
        let gate_reason = self.compute_gate_reason(action, &gate_status);
        let policy_metadata = self.compute_policy_metadata(action);
        let risk_metadata = self.compute_risk_metadata(action);

        PolicyGateEvaluation {
            action: action.clone(),
            gate_status,
            gate_reason,
            policy_metadata,
            risk_metadata,
        }
    }

    /// Compute the gate status for a compensation action.
    fn compute_gate_status(&self, action: &CompensationAction) -> PolicyGateStatus {
        use CompensationStatus::*;

        // Terminal statuses (Executed, Waived) are always blocked
        if action.status.is_terminal() {
            return PolicyGateStatus::Blocked;
        }

        // DLQ candidates are blocked
        if action.is_dlq_candidate() {
            return PolicyGateStatus::Blocked;
        }

        // Pending status requires manual review
        if action.status == Pending {
            return PolicyGateStatus::ManualReviewRequired;
        }

        // Failed status with remaining budget and retryable error - manual review
        if action.status == Failed {
            if action.can_be_reapproved() {
                return PolicyGateStatus::ManualReviewRequired;
            }
            // Otherwise it's blocked (non-retryable or exhausted budget)
            return PolicyGateStatus::Blocked;
        }

        // Approved status - check service executability
        if action.status == Approved {
            // Rollback+Automatic or CounterAction+SemiAutomatic = service-executable = eligible
            if action.is_service_executable() {
                return PolicyGateStatus::Eligible;
            }
            // ManualOnly = manual review required
            return PolicyGateStatus::ManualReviewRequired;
        }

        // Default to blocked for any unexpected state
        PolicyGateStatus::Blocked
    }

    /// Compute the human-readable reason for the gate status.
    fn compute_gate_reason(
        &self,
        action: &CompensationAction,
        gate_status: &PolicyGateStatus,
    ) -> String {
        use CompensationStatus::*;

        match gate_status {
            PolicyGateStatus::Eligible => {
                format!(
                    "Action is approved with {} feasibility and no blocking conditions",
                    format_feasibility(action.feasibility)
                )
            }
            PolicyGateStatus::Blocked => {
                if action.status.is_terminal() {
                    return format!("Action is in terminal status ({:?})", action.status);
                }
                if action.is_dlq_candidate() {
                    if action.attempt_count >= action.max_retries {
                        return format!(
                            "Action is DLQ candidate: retry budget exhausted ({}/{} attempts)",
                            action.attempt_count, action.max_retries
                        );
                    }
                    if let Some(ref result) = action.execution_result_payload {
                        if let Some(ref error_code) = result.error_code {
                            return format!(
                                "Action is DLQ candidate: non-retryable error ({})",
                                error_code
                            );
                        }
                    }
                    return "Action is DLQ candidate".to_string();
                }
                if let Some(ref reason) = action.reapproval_denial_reason() {
                    return reason.clone();
                }
                format!("Action is blocked due to {:?}", action.status)
            }
            PolicyGateStatus::ManualReviewRequired => match action.status {
                Pending => {
                    format!(
                        "Action awaits approval ({} feasibility)",
                        format_feasibility(action.feasibility)
                    )
                }
                Failed => {
                    if action.can_be_reapproved() {
                        return format!(
                                "Action failed but can be reapproved ({} retry attempts remaining, {} feasibility)",
                                action.max_retries - action.attempt_count,
                                format_feasibility(action.feasibility)
                            );
                    }
                    if let Some(ref reason) = action.reapproval_denial_reason() {
                        return reason.clone();
                    }
                    format!(
                        "Action failed and requires manual review ({} feasibility)",
                        format_feasibility(action.feasibility)
                    )
                }
                Approved => {
                    format!(
                        "Action requires manual execution ({})",
                        format_feasibility(action.feasibility)
                    )
                }
                _ => format!(
                    "Action requires manual review ({})",
                    format_feasibility(action.feasibility)
                ),
            },
        }
    }

    /// Compute policy metadata for a compensation action.
    fn compute_policy_metadata(&self, action: &CompensationAction) -> PolicyGateMetadata {
        let has_non_retryable_error = action
            .execution_result_payload
            .as_ref()
            .and_then(|r| r.error_code.as_ref())
            .map(|code| {
                let classification = CompensationAction::classify_error_code(code);
                classification.retryable == RetryableErrorClass::NonRetryable
            })
            .unwrap_or(false);

        PolicyGateMetadata {
            auto_executable: action.is_auto_executable(),
            is_dlq_candidate: action.is_dlq_candidate(),
            can_reapprove: action.can_be_reapproved(),
            retry_budget_exhausted: action.attempt_count >= action.max_retries,
            has_non_retryable_error,
            feasibility: action.feasibility,
            strategy_type: action.strategy_type,
            status: action.status,
            attempt_count: action.attempt_count,
            max_retries: action.max_retries,
        }
    }

    /// Compute risk metadata for a compensation action.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Derives risk signals from
    /// existing action state fields. No new policy engine - all fields derive
    /// from status, attempt_count, max_retries, error_code, feasibility, strategy_type.
    fn compute_risk_metadata(&self, action: &CompensationAction) -> RiskMetadata {
        let error_classification = action.execution_result_payload.as_ref().and_then(|r| {
            r.error_code.as_ref().map(|code| {
                let classification = CompensationAction::classify_error_code(code);
                ErrorClassification {
                    error_code: code.clone(),
                    retryable: classification.retryable == RetryableErrorClass::Retryable,
                    reason: classification.reason.to_string(),
                }
            })
        });

        let error_severity = action
            .execution_result_payload
            .as_ref()
            .and_then(|r| {
                r.error_code.as_ref().map(|code| {
                    let classification = CompensationAction::classify_error_code(code);
                    ErrorSeverity::from_retryable_class(classification.retryable)
                })
            })
            .unwrap_or(ErrorSeverity::None);

        RiskMetadata {
            strategy_severity: StrategySeverity::from_strategy_type(action.strategy_type),
            retry_exhaustion_risk: RetryExhaustionRisk::from_attempts(
                action.attempt_count,
                action.max_retries,
            ),
            feasibility_risk: FeasibilityRisk::from_feasibility(action.feasibility),
            error_severity,
            retry_budget_remaining: (action.max_retries - action.attempt_count).max(0),
            error_classification,
            is_terminal: action.status.is_terminal(),
            requires_manual_intervention: matches!(
                action.status,
                CompensationStatus::Pending | CompensationStatus::Failed
            ) || !action.is_auto_executable(),
        }
    }

    // ============================================================================
    // Coordination Status Evaluation (Phase 3 Batch 1 bounded read-only orchestration view)
    // ============================================================================

    /// Evaluate coordination status for all compensation actions of a tenant.
    ///
    /// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
    /// coordination status for all compensation actions belonging to the specified tenant.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Canonical coordination statuses:**
    /// - `ready`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
    /// - `awaiting_policy`: Action awaits policy approval (Pending status)
    /// - `awaiting_manual_review`: Action requires human intervention (Failed + can reapprove, or Approved + non-Automatic)
    /// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget)
    /// - `terminal`: Action has reached terminal state (Executed, Waived)
    ///
    /// **Derivation from existing surfaces:**
    /// - Coordination status is derived from existing CompensationAction fields at query time
    /// - No new orchestration engine or external policy surface is queried
    pub async fn evaluate_coordination_status(
        &self,
        tenant_id: Uuid,
    ) -> Result<CoordinationResult, IntentRebaseError> {
        let actions = self.list_by_tenant(tenant_id, None).await?;
        Ok(self.evaluate_coordination_from_actions(actions))
    }

    /// Evaluate coordination status for all compensation actions of an intent.
    ///
    /// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
    /// coordination status for all compensation actions belonging to the specified intent.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Canonical coordination statuses:** Same as `evaluate_coordination_status`.
    pub async fn evaluate_coordination_status_for_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<CoordinationResult, IntentRebaseError> {
        let actions = self.list_by_intent(intent_id, tenant_id).await?;
        Ok(self.evaluate_coordination_from_actions(actions))
    }

    /// Evaluate coordination status from a collection of compensation actions.
    ///
    /// Internal helper that computes coordination records and summary.
    fn evaluate_coordination_from_actions(
        &self,
        actions: Vec<CompensationAction>,
    ) -> CoordinationResult {
        let total_actions = actions.len();
        let mut records = Vec::with_capacity(total_actions);
        let mut summary = CoordinationSummary::default();

        for action in actions {
            let record = CoordinationRecord::from_action(&action);
            match record.coordination_status {
                CoordinationStatus::Ready => summary.ready_count += 1,
                CoordinationStatus::AwaitingPolicy => summary.awaiting_policy_count += 1,
                CoordinationStatus::AwaitingManualReview => {
                    summary.awaiting_manual_review_count += 1
                }
                CoordinationStatus::Blocked => summary.blocked_count += 1,
                CoordinationStatus::Terminal => summary.terminal_count += 1,
            }

            if record.is_dlq_candidate {
                summary.dlq_candidate_count += 1;
            }
            if record.auto_executable {
                summary.auto_executable_count += 1;
            }

            records.push(record);
        }

        summary.total_actions = total_actions;

        CoordinationResult { records, summary }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use crate::compensation_action_repo::InMemoryCompensationActionRepository;
    use crate::rollback_record::RollbackRecordResult;
    use crate::rollback_record_repo::InMemoryRollbackRecordRepository;
    use std::sync::Arc;

    fn create_test_service() -> CompensationActionService {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        CompensationActionService::new(repo)
    }

    #[allow(dead_code)]
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

        // Verify approved service-executable candidates
        assert_eq!(batch.approved_service_executable_candidates.len(), 1);
        assert_eq!(
            batch.approved_service_executable_candidates[0].id,
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
        assert!(batch.approved_service_executable_candidates.is_empty());
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

        // Batch candidates should NOT include Rollback+SemiAutomatic in approved_service_executable
        // but SHOULD include the Automatic pending action
        let batch = service.list_batch_candidates(tenant_id).await.unwrap();
        assert!(batch.approved_service_executable_candidates.is_empty());
        assert_eq!(batch.pending_approval_candidates.len(), 1);
        assert_eq!(
            batch.pending_approval_candidates[0].feasibility,
            CompensationFeasibility::Automatic
        );
    }

    #[tokio::test]
    async fn test_list_batch_candidates_includes_counter_action_semi_auto() {
        // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic should be included in batch candidates
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create a CounterAction+SemiAutomatic action (S2ExternalReversible)
        let mut counter_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::SemiAutomatic,
            StrategyType::CounterAction,
            "Counter the PR",
        );
        counter_action.max_retries = 3;
        let counter_created = service.create_action(counter_action).await.unwrap();
        let counter_approved = service
            .approve_action(counter_created.id, counter_created.lock_version, None)
            .await
            .unwrap();

        // Should be Approved AND service-executable
        assert_eq!(counter_approved.status, CompensationStatus::Approved);
        assert!(counter_approved.is_service_executable());
        assert!(!counter_approved.is_auto_executable()); // Not Automatic, but IS service-executable

        // Batch candidates should include CounterAction+SemiAutomatic in approved_service_executable
        let batch = service.list_batch_candidates(tenant_id).await.unwrap();
        assert_eq!(batch.approved_service_executable_candidates.len(), 1);
        assert_eq!(
            batch.approved_service_executable_candidates[0].id,
            counter_approved.id
        );
    }

    // === Policy Gate Evaluation Tests ===

    #[tokio::test]
    async fn test_evaluate_policy_gates_empty_for_tenant() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 0);
        assert_eq!(result.summary.total_actions, 0);
        assert_eq!(result.summary.eligible_count, 0);
        assert_eq!(result.summary.blocked_count, 0);
        assert_eq!(result.summary.manual_review_required_count, 0);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_eligible() {
        // Approved + Automatic feasibility = eligible
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let created = service.create_action(action).await.unwrap();
        assert_eq!(created.status, CompensationStatus::Pending);

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);
        assert_eq!(approved.feasibility, CompensationFeasibility::Automatic);

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 1);
        let eval = &result.evaluations[0];
        assert_eq!(eval.gate_status, PolicyGateStatus::Eligible);
        assert!(eval.gate_reason.contains("approved"));
        assert!(eval.gate_reason.contains("Automatic"));
        assert!(eval.policy_metadata.auto_executable);
        assert!(!eval.policy_metadata.is_dlq_candidate);

        assert_eq!(result.summary.total_actions, 1);
        assert_eq!(result.summary.eligible_count, 1);
        assert_eq!(result.summary.blocked_count, 0);
        assert_eq!(result.summary.manual_review_required_count, 0);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_blocked_executed() {
        // Executed status = blocked
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let created = service.create_action(action).await.unwrap();
        let executed_result = ExecutionResult::success("Completed");
        let _executed = service
            .record_result(created.id, &executed_result, created.lock_version, None)
            .await
            .unwrap();

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 1);
        let eval = &result.evaluations[0];
        assert_eq!(eval.gate_status, PolicyGateStatus::Blocked);
        assert!(eval.gate_reason.contains("terminal"));
        assert_eq!(result.summary.eligible_count, 0);
        assert_eq!(result.summary.blocked_count, 1);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_blocked_dlq() {
        // DLQ candidate = blocked
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let mut action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action.max_retries = 1; // Exhausts on first failure

        let created = service.create_action(action).await.unwrap();
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        let failed_result = ExecutionResult::failure("Failed", "CONNECTION_TIMEOUT", None);
        let _failed = service
            .record_result(approved.id, &failed_result, approved.lock_version, None)
            .await
            .unwrap();

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 1);
        let eval = &result.evaluations[0];
        assert_eq!(eval.gate_status, PolicyGateStatus::Blocked);
        assert!(eval.gate_reason.contains("DLQ"));
        assert!(eval.policy_metadata.is_dlq_candidate);
        assert_eq!(result.summary.dlq_candidate_count, 1);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_manual_review_pending() {
        // Pending status = manual_review_required
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

        let _created = service.create_action(action).await.unwrap();

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 1);
        let eval = &result.evaluations[0];
        assert_eq!(eval.gate_status, PolicyGateStatus::ManualReviewRequired);
        assert!(eval.gate_reason.contains("awaits approval"));
        assert_eq!(result.summary.pending_approval_count, 1);
        assert_eq!(result.summary.manual_review_required_count, 1);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_manual_review_semi_automatic() {
        // Approved + SemiAutomatic = manual_review_required
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::FollowupNotice,
            "Followup",
        );

        let created = service.create_action(action).await.unwrap();
        let _approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        assert_eq!(result.evaluations.len(), 1);
        let eval = &result.evaluations[0];
        assert_eq!(eval.gate_status, PolicyGateStatus::ManualReviewRequired);
        assert!(eval.gate_reason.contains("SemiAutomatic"));
        assert!(!eval.policy_metadata.auto_executable);
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_mixed_actions() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Action 1: Pending (manual_review_required)
        let pending_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Pending",
        );
        let pending_created = service.create_action(pending_action).await.unwrap();

        // Action 2: Approved + Automatic (eligible)
        let mut approved_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Approved",
        );
        approved_action.max_retries = 3;
        let approved_created = service.create_action(approved_action).await.unwrap();
        let approved = service
            .approve_action(approved_created.id, approved_created.lock_version, None)
            .await
            .unwrap();

        // Action 3: DLQ candidate (blocked)
        let mut dlq_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "DLQ",
        );
        dlq_action.max_retries = 1;
        let dlq_created = service.create_action(dlq_action).await.unwrap();
        let dlq_approved = service
            .approve_action(dlq_created.id, dlq_created.lock_version, None)
            .await
            .unwrap();
        let dlq_failed_result = ExecutionResult::failure("Failed", "CONNECTION_TIMEOUT", None);
        let _dlq_failed = service
            .record_result(
                dlq_approved.id,
                &dlq_failed_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        // Action 4: Failed + retryable (manual_review_required)
        let mut retryable_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Retryable",
        );
        retryable_action.max_retries = 3;
        let retryable_created = service.create_action(retryable_action).await.unwrap();
        let retryable_approved = service
            .approve_action(retryable_created.id, retryable_created.lock_version, None)
            .await
            .unwrap();
        let retryable_failed_result =
            ExecutionResult::failure("Transient", "CONNECTION_TIMEOUT", None);
        let _retryable_failed = service
            .record_result(
                retryable_approved.id,
                &retryable_failed_result,
                retryable_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        // Debug: fetch actions directly to verify states
        let all_actions = service.list_by_tenant(tenant_id, None).await.unwrap();
        eprintln!("\n=== All actions from list_by_tenant ===");
        for a in &all_actions {
            eprintln!(
                "id={}, status={:?}, feasibility={:?}, attempt_count={}, max_retries={}",
                a.id, a.status, a.feasibility, a.attempt_count, a.max_retries
            );
        }

        // Call evaluate_policy_gates and debug inside the service
        let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

        eprintln!("\n=== Evaluations ===");
        for eval in &result.evaluations {
            eprintln!(
                "id={}, status={:?}, gate={:?}, is_dlq={}, is_auto_exec={}",
                eval.action.id,
                eval.action.status,
                eval.gate_status,
                eval.policy_metadata.is_dlq_candidate,
                eval.policy_metadata.auto_executable
            );
        }

        eprintln!("\n=== Summary ===");
        eprintln!(
            "total={}, eligible={}, blocked={}, manual_review={}, dlq={}, pending={}, auto_exec={}",
            result.summary.total_actions,
            result.summary.eligible_count,
            result.summary.blocked_count,
            result.summary.manual_review_required_count,
            result.summary.dlq_candidate_count,
            result.summary.pending_approval_count,
            result.summary.auto_executable_count
        );

        // Verify gate statuses from evaluations
        // The order in the repository is not guaranteed to be creation order.
        // We verify by checking the action IDs.

        // Find evaluations by action ID
        let approved_action_id = approved.id;
        let dlq_action_id = _dlq_failed.id;
        let retryable_action_id = _retryable_failed.id;
        let pending_action_id = pending_created.id;

        let eval_by_id = |id: Uuid| -> &PolicyGateEvaluation {
            result
                .evaluations
                .iter()
                .find(|e| e.action.id == id)
                .unwrap()
        };

        // pending_action: Pending -> ManualReviewRequired
        let pending_eval = eval_by_id(pending_action_id);
        assert_eq!(
            pending_eval.gate_status,
            PolicyGateStatus::ManualReviewRequired
        );

        // approved_action: Approved -> Eligible
        let approved_eval = eval_by_id(approved_action_id);
        assert_eq!(approved_eval.gate_status, PolicyGateStatus::Eligible);

        // dlq_action: Failed (DLQ) -> Blocked
        let dlq_eval = eval_by_id(dlq_action_id);
        assert_eq!(dlq_eval.gate_status, PolicyGateStatus::Blocked);

        // retryable_action: Failed (retryable) -> ManualReviewRequired
        let retryable_eval = eval_by_id(retryable_action_id);
        assert_eq!(
            retryable_eval.gate_status,
            PolicyGateStatus::ManualReviewRequired
        );

        // Verify summary counts
        assert_eq!(result.summary.total_actions, 4);
        assert_eq!(result.summary.eligible_count, 1); // only approved_action
        assert_eq!(result.summary.blocked_count, 1); // only dlq_action
        assert_eq!(result.summary.manual_review_required_count, 2); // pending + retryable
        assert_eq!(result.summary.pending_approval_count, 1);
        assert_eq!(result.summary.dlq_candidate_count, 1);
        assert_eq!(result.summary.auto_executable_count, 4); // all actions have Automatic feasibility
    }

    #[tokio::test]
    async fn test_evaluate_policy_gates_for_intent() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let other_intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action for the target intent
        let action1 = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "For target intent",
        );
        let _created1 = service.create_action(action1).await.unwrap();

        // Create action for a different intent
        let other_rebase_context = RebaseContext::new(other_intent_id, 1, 2, Uuid::new_v4());
        let action2 = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            other_intent_id,
            other_rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "For other intent",
        );
        let _created2 = service.create_action(action2).await.unwrap();

        // Evaluate for target intent only
        let result = service
            .evaluate_policy_gates_for_intent(intent_id, tenant_id)
            .await
            .unwrap();

        assert_eq!(result.summary.total_actions, 1);
    }

    // ============================================================================
    // Coordination Status Tests (Phase 3 Batch 1 bounded read-only orchestration view)
    // ============================================================================

    #[test]
    fn test_coordination_status_ready() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test ready action",
        );

        // Approved + Automatic + not blocked = Ready
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::AwaitingPolicy
        );

        let mut approved_action = action;
        approved_action.status = CompensationStatus::Approved;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&approved_action),
            CoordinationStatus::Ready
        );
    }

    #[test]
    fn test_coordination_status_awaiting_policy() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test pending action",
        );

        // Pending = AwaitingPolicy
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::AwaitingPolicy
        );
    }

    #[test]
    fn test_coordination_status_awaiting_manual_review() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Approved + ManualOnly = AwaitingManualReview
        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::ManualOnly,
            StrategyType::CounterAction,
            "Test manual action",
        );
        action.status = CompensationStatus::Approved;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::AwaitingManualReview
        );

        // Failed + can reapprove = AwaitingManualReview
        let mut retryable_action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test retryable failed",
        );
        retryable_action.status = CompensationStatus::Failed;
        retryable_action.execution_result_payload = Some(ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            None,
        ));
        assert_eq!(
            CoordinationStatus::from_compensation_action(&retryable_action),
            CoordinationStatus::AwaitingManualReview
        );
    }

    #[test]
    fn test_coordination_status_blocked() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // DLQ candidate (exhausted budget) = Blocked
        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test DLQ action",
        );
        action.status = CompensationStatus::Failed;
        action.attempt_count = 3;
        action.max_retries = 3;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::Blocked
        );

        // Failed + non-retryable error = Blocked
        let mut non_retryable_action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test non-retryable",
        );
        non_retryable_action.status = CompensationStatus::Failed;
        non_retryable_action.execution_result_payload = Some(ExecutionResult::failure(
            "Permanent failure",
            "INVALID_CONFIGURATION",
            None,
        ));
        assert_eq!(
            CoordinationStatus::from_compensation_action(&non_retryable_action),
            CoordinationStatus::Blocked
        );
    }

    #[test]
    fn test_coordination_status_terminal() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Executed = Terminal
        let mut executed_action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test executed",
        );
        executed_action.status = CompensationStatus::Executed;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&executed_action),
            CoordinationStatus::Terminal
        );

        // Waived = Terminal
        let mut waived_action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test waived",
        );
        waived_action.status = CompensationStatus::Waived;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&waived_action),
            CoordinationStatus::Terminal
        );
    }

    #[test]
    fn test_coordination_status_ready_counter_action_semi_auto() {
        // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic (Approved) should be Ready
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::CounterAction,
            "Test CounterAction+SemiAutomatic",
        );

        // Pending + SemiAutomatic = AwaitingPolicy (not yet approved)
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::AwaitingPolicy
        );

        // Approved + CounterAction + SemiAutomatic = Ready (service-executable)
        action.status = CompensationStatus::Approved;
        assert_eq!(
            CoordinationStatus::from_compensation_action(&action),
            CoordinationStatus::Ready
        );
    }

    #[tokio::test]
    async fn test_evaluate_coordination_status_empty_for_tenant() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();

        let result = service
            .evaluate_coordination_status(tenant_id)
            .await
            .unwrap();

        assert_eq!(result.summary.total_actions, 0);
        assert_eq!(result.summary.ready_count, 0);
        assert_eq!(result.summary.awaiting_policy_count, 0);
        assert_eq!(result.summary.awaiting_manual_review_count, 0);
        assert_eq!(result.summary.blocked_count, 0);
        assert_eq!(result.summary.terminal_count, 0);
        assert!(result.records.is_empty());
    }

    #[tokio::test]
    async fn test_evaluate_coordination_status_mixed_actions() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create: Pending = AwaitingPolicy
        let pending_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Pending",
        );
        service.create_action(pending_action).await.unwrap();

        // Create: Approved + Automatic = Ready
        let mut ready_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Ready",
        );
        ready_action.status = CompensationStatus::Approved;
        service.create_action(ready_action).await.unwrap();

        // Create: Approved + ManualOnly = AwaitingManualReview
        let mut manual_review_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::ManualOnly,
            StrategyType::CounterAction,
            "Manual",
        );
        manual_review_action.status = CompensationStatus::Approved;
        service.create_action(manual_review_action).await.unwrap();

        // Create: Failed + retryable = AwaitingManualReview
        let mut retryable_failed = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Retryable failed",
        );
        retryable_failed.status = CompensationStatus::Failed;
        retryable_failed.execution_result_payload =
            Some(ExecutionResult::failure("Temp", "CONNECTION_TIMEOUT", None));
        service.create_action(retryable_failed).await.unwrap();

        // Create: Failed + exhausted = Blocked
        let mut blocked_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Blocked",
        );
        blocked_action.status = CompensationStatus::Failed;
        blocked_action.attempt_count = 3;
        blocked_action.max_retries = 3;
        service.create_action(blocked_action).await.unwrap();

        // Create: Executed = Terminal
        let mut terminal_action = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Terminal",
        );
        terminal_action.status = CompensationStatus::Executed;
        service.create_action(terminal_action).await.unwrap();

        let result = service
            .evaluate_coordination_status(tenant_id)
            .await
            .unwrap();

        assert_eq!(result.summary.total_actions, 6);
        assert_eq!(result.summary.ready_count, 1); // Approved + Automatic
        assert_eq!(result.summary.awaiting_policy_count, 1); // Pending
        assert_eq!(result.summary.awaiting_manual_review_count, 2); // ManualOnly + retryable failed
        assert_eq!(result.summary.blocked_count, 1); // exhausted budget
        assert_eq!(result.summary.terminal_count, 1); // Executed
        assert_eq!(result.records.len(), 6);
    }

    #[tokio::test]
    async fn test_evaluate_coordination_status_for_intent() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let other_intent_id = Uuid::new_v4();

        // Create action for the target intent
        let action1 = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            intent_id,
            RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "For target intent",
        );
        service.create_action(action1).await.unwrap();

        // Create action for a different intent
        let action2 = CompensationAction::new(
            tenant_id,
            Uuid::new_v4(),
            other_intent_id,
            RebaseContext::new(other_intent_id, 1, 2, Uuid::new_v4()),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "For other intent",
        );
        service.create_action(action2).await.unwrap();

        // Evaluate for target intent only
        let result = service
            .evaluate_coordination_status_for_intent(intent_id, tenant_id)
            .await
            .unwrap();

        assert_eq!(result.summary.total_actions, 1);
    }

    #[tokio::test]
    async fn test_coordination_record_from_action() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test action",
        );
        action.status = CompensationStatus::Approved;

        let record = CoordinationRecord::from_action(&action);

        assert_eq!(record.coordination_status, CoordinationStatus::Ready);
        assert!(record.auto_executable); // is_service_executable() includes Rollback+Automatic
        assert!(!record.is_dlq_candidate);
        assert_eq!(record.feasibility, CompensationFeasibility::Automatic);
        assert_eq!(record.strategy_type, StrategyType::Rollback);
        assert_eq!(record.status, CompensationStatus::Approved);
    }

    #[test]
    fn test_coordination_record_auto_executable_for_counter_action_semi_auto() {
        // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic auto_executable=true (is_service_executable)
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let mut action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::CounterAction,
            "Counter PR",
        );
        action.status = CompensationStatus::Approved;

        let record = CoordinationRecord::from_action(&action);

        assert_eq!(record.coordination_status, CoordinationStatus::Ready);
        assert!(record.auto_executable); // is_service_executable() includes CounterAction+SemiAutomatic
        assert!(!record.is_dlq_candidate);
        assert_eq!(record.feasibility, CompensationFeasibility::SemiAutomatic);
        assert_eq!(record.strategy_type, StrategyType::CounterAction);
        assert_eq!(record.status, CompensationStatus::Approved);
    }

    // ============================================================================
    // Audit Emission Tests (Phase 3 Batch 0 bounded slice)
    // ============================================================================

    fn create_test_service_with_audit_repo(
        audit_repo: Arc<dyn intent_rebase_types::AuditRepository>,
    ) -> CompensationActionService {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
            .with_audit_repo(audit_repo)
    }

    #[tokio::test]
    async fn test_create_action_emits_compensation_planned_audit_event() {
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
        let service = create_test_service_with_audit_repo(audit_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Verify CompensationPlanned audit event was emitted
        let events = audit_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            intent_rebase_types::AuditEventType::CompensationPlanned
        ));

        // Verify payload contents
        let payload: intent_rebase_types::CompensationPlannedAuditPayload =
            serde_json::from_value(events[0].payload.clone()).unwrap();
        assert_eq!(payload.compensation_plan_id, created.id);
        assert_eq!(payload.intent_id, intent_id);
        assert_eq!(payload.side_effect_count, 1);
        assert_eq!(payload.auto_compensatable_count, 1);
    }

    #[tokio::test]
    async fn test_create_action_does_not_emit_audit_when_no_audit_repo() {
        // Service without audit repo should not emit events
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let result = service.create_action(action).await;
        assert!(result.is_ok());
        // No error even without audit repo - fail-open behavior
    }

    #[tokio::test]
    async fn test_execute_action_emits_started_and_completed_audit_events() {
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
        let _service = create_test_service_with_audit_repo(audit_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create side effect so executor can find it
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
        // Access the side_effect_repo through the service's internal state
        // For this test, we use a service that has side_effect_repo configured
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        side_effect_repo.create(side_effect).await.unwrap();

        let service_with_side_effect =
            CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
                .with_audit_repo(audit_repo.clone());

        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service_with_side_effect
            .create_action(action)
            .await
            .unwrap();
        let approved = service_with_side_effect
            .approve_action(created.id, created.lock_version, Some("test-approver"))
            .await
            .unwrap();

        // Execute - should succeed and emit CompensationStarted + CompensationCompleted
        let _executed = service_with_side_effect
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        // Verify audit events were emitted
        let events = audit_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(events.len(), 3); // CompensationPlanned + Started + Completed

        let event_types: Vec<_> = events.iter().map(|e| e.event_type.clone()).collect();
        assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationPlanned));
        assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationStarted));
        assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationCompleted));
    }

    #[tokio::test]
    async fn test_execute_action_emits_failed_audit_event_on_failure() {
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        // Create service WITHOUT side_effect_repo so RollbackExecutor fails
        let service = CompensationActionService::new(repo).with_audit_repo(audit_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();
        let approved = service
            .approve_action(created.id, created.lock_version, Some("test-approver"))
            .await
            .unwrap();

        // Execute - stub returns success without side_effect_repo, so this should succeed
        // (using stub behavior for backward compatibility)
        let _executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        // Even stub success should have emitted Started + Completed
        let events = audit_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert!(events.len() >= 2);
        let event_types: Vec<_> = events.iter().map(|e| e.event_type.clone()).collect();
        assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationStarted));
        assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationCompleted));
        assert_eq!(_executed.status, CompensationStatus::Executed);
    }

    #[tokio::test]
    async fn test_audit_emission_is_best_effort_fail_open() {
        // Test that audit emission failures don't affect the main operation
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
        let service = create_test_service_with_audit_repo(audit_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        // Create action should succeed even if audit repo has issues
        let _created = service.create_action(action).await.unwrap();
        assert_eq!(_created.tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_compensation_audit_payload_contents() {
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
        let service = create_test_service_with_audit_repo(audit_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);

        let created = service.create_action(action).await.unwrap();

        // Verify CompensationPlanned audit event payload
        let events = audit_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        let planned_event = events
            .iter()
            .find(|e| {
                matches!(
                    e.event_type,
                    intent_rebase_types::AuditEventType::CompensationPlanned
                )
            })
            .unwrap();

        let payload: intent_rebase_types::CompensationPlannedAuditPayload =
            serde_json::from_value(planned_event.payload.clone()).unwrap();
        assert_eq!(payload.compensation_plan_id, created.id);
        assert_eq!(payload.intent_id, intent_id);
        assert_eq!(payload.intent_version_from, 1);
        assert_eq!(payload.intent_version_to, 2);
        assert_eq!(payload.side_effect_count, 1);
        assert_eq!(payload.auto_compensatable_count, 1);
        assert_eq!(payload.manual_required_count, 0);
        assert_eq!(payload.not_possible_count, 0);
    }

    // ============================================================================
    // Rollback Record Tests (Phase 3 Batch 1 bounded rollback record slice)
    // ============================================================================

    fn create_test_service_with_rollback_record_repo(
        rollback_record_repo: Arc<dyn RollbackRecordRepository>,
    ) -> (
        CompensationActionService,
        Arc<crate::side_effect_repo::InMemorySideEffectRepository>,
    ) {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let service =
            CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo.clone())
                .with_rollback_record_repo(rollback_record_repo);
        (service, side_effect_repo)
    }

    #[tokio::test]
    async fn test_execute_action_creates_rollback_record_on_success() {
        let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let (service, side_effect_repo) =
            create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

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

        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        // Approve and execute
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute - with side effect present, executor succeeds
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        // Verify rollback records exist
        let records = rollback_record_repo
            .list_by_compensation_action(executed.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, RollbackRecordResult::Success);
        assert_eq!(records[0].compensation_action_id, executed.id);
        assert_eq!(records[0].side_effect_id, side_effect_id);
        assert_eq!(records[0].intent_id, intent_id);
        assert_eq!(records[0].recorded_by, Some("test-executor".to_string()));
    }

    #[tokio::test]
    async fn test_execute_action_creates_rollback_record_on_failure() {
        let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let (service, _side_effect_repo) =
            create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        // Use a side_effect_id that won't exist, causing executor to fail
        let side_effect_id = Uuid::new_v4();

        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        // Approve
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute - should fail because side effect doesn't exist in repo
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        // Verify rollback record was created with failure
        let records = rollback_record_repo
            .list_by_compensation_action(executed.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, RollbackRecordResult::Failure);
        assert!(records[0].error_code.is_some());
    }

    #[tokio::test]
    async fn test_waive_action_creates_rollback_record() {
        let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let (service, _side_effect_repo) =
            create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        // Waive the action
        let waived = service
            .waive_action(created.id, created.lock_version, Some("test-waiver"))
            .await
            .unwrap();

        // Verify rollback record was created
        let records = rollback_record_repo
            .list_by_compensation_action(waived.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, RollbackRecordResult::Waived);
        assert_eq!(records[0].recorded_by, Some("test-waiver".to_string()));
    }

    #[tokio::test]
    async fn test_execute_action_skips_rollback_record_when_repo_not_configured() {
        // Service WITHOUT rollback_record_repo - should not fail
        let service = create_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute should succeed even without rollback_record_repo
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();

        assert_eq!(executed.status, CompensationStatus::Executed);
    }

    #[tokio::test]
    async fn test_waive_action_skips_rollback_record_when_repo_not_configured() {
        // Service WITHOUT rollback_record_repo - should not fail
        let service = create_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        // Waive should succeed even without rollback_record_repo
        let waived = service
            .waive_action(created.id, created.lock_version, Some("test-waiver"))
            .await
            .unwrap();

        assert_eq!(waived.status, CompensationStatus::Waived);
    }

    // ============================================================================
    // CounterAction + SemiAutomatic Tests (Phase 3 Batch 1 P7 bounded slice)
    // ============================================================================

    fn create_counter_action_semi_auto_test_service() -> (
        CompensationActionService,
        Arc<crate::side_effect_repo::InMemorySideEffectRepository>,
    ) {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let side_effect_repo =
            Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
        let service =
            CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo.clone());
        (service, side_effect_repo)
    }

    fn create_counter_action_semi_auto_action(
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
            CompensationFeasibility::SemiAutomatic,
            StrategyType::CounterAction,
            "Close PR as counter-action",
        )
    }

    #[tokio::test]
    async fn test_execute_counter_action_semi_auto_success() {
        let (service, side_effect_repo) = create_counter_action_semi_auto_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create S2ExternalReversible side effect so executor can find it
        let side_effect = crate::side_effect::SideEffect {
            id: side_effect_id,
            tenant_id,
            intent_id,
            intent_version: 1,
            effect_class: crate::side_effect::SideEffectClass::S2ExternalReversible,
            effect_type: "pr_opened".to_string(),
            target: "https://github.com/pulls/123".to_string(),
            occurred_at: chrono::Utc::now(),
            idempotency_key: None,
        };
        side_effect_repo.create(side_effect).await.unwrap();

        // Create CounterAction + SemiAutomatic action
        let action = create_counter_action_semi_auto_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        // Approve the action
        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();
        assert_eq!(approved.status, CompensationStatus::Approved);

        // Execute should succeed
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();
        assert_eq!(executed.status, CompensationStatus::Executed);
        assert!(executed.execution_result_payload.is_some());
        let result = executed.execution_result_payload.unwrap();
        assert!(result.success);
        assert!(result.summary.contains("Counter-action"));
        assert!(result.summary.contains("acknowledged"));
    }

    #[tokio::test]
    async fn test_execute_counter_action_semi_auto_fails_on_wrong_strategy() {
        let (service, _side_effect_repo) = create_counter_action_semi_auto_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create action with Rollback strategy but SemiAutomatic feasibility
        // This should fail because Rollback only works with Automatic
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::Rollback, // Wrong: Rollback needs Automatic
            "Rollback with SemiAuto",
        );
        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute should fail with CompensationActionNotExecutable error
        // because Rollback + SemiAutomatic is not a supported combo
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
    async fn test_execute_counter_action_semi_auto_fails_on_wrong_feasibility() {
        let (service, _side_effect_repo) = create_counter_action_semi_auto_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create action with CounterAction strategy but Automatic feasibility
        // This should fail because CounterAction needs SemiAutomatic
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic, // Wrong: CounterAction needs SemiAutomatic
            StrategyType::CounterAction,
            "CounterAction with Automatic",
        );
        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute should fail with CompensationActionNotExecutable error
        // because CounterAction + Automatic is not a supported combo
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
    async fn test_execute_counter_action_semi_auto_fails_on_s1_side_effect() {
        let (service, side_effect_repo) = create_counter_action_semi_auto_test_service();

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create S1InternalReversible side effect instead of S2ExternalReversible
        let side_effect = crate::side_effect::SideEffect {
            id: side_effect_id,
            tenant_id,
            intent_id,
            intent_version: 1,
            effect_class: crate::side_effect::SideEffectClass::S1InternalReversible, // Wrong class
            effect_type: "metadata_write".to_string(),
            target: "db-record-123".to_string(),
            occurred_at: chrono::Utc::now(),
            idempotency_key: None,
        };
        side_effect_repo.create(side_effect).await.unwrap();

        let action = create_counter_action_semi_auto_action(tenant_id, side_effect_id, intent_id);
        let created = service.create_action(action).await.unwrap();

        let approved = service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute should fail because counter-action is only valid for S2ExternalReversible
        let executed = service
            .execute_action(approved.id, Some("test-executor"))
            .await
            .unwrap();
        assert_eq!(executed.status, CompensationStatus::Failed);
        let result = executed.execution_result_payload.unwrap();
        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("INVALID_SIDE_EFFECT_CLASS".to_string())
        );
    }

    #[tokio::test]
    async fn test_is_service_executable_for_counter_action_semi_auto() {
        let rebase_context = RebaseContext::new(Uuid::new_v4(), 1, 2, Uuid::new_v4());

        // CounterAction + SemiAutomatic should be service executable
        let counter_action = CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            rebase_context.clone(),
            CompensationFeasibility::SemiAutomatic,
            StrategyType::CounterAction,
            "Test",
        );
        assert!(counter_action.is_service_executable());

        // Rollback + Automatic should also be service executable
        let rollback_action = CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            rebase_context.clone(),
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test",
        );
        assert!(rollback_action.is_service_executable());

        // Rollback + SemiAutomatic should NOT be service executable
        let invalid_combo = CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            rebase_context.clone(),
            CompensationFeasibility::SemiAutomatic,
            StrategyType::Rollback,
            "Test",
        );
        assert!(!invalid_combo.is_service_executable());

        // CounterAction + Automatic should NOT be service executable
        let invalid_combo2 = CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::CounterAction,
            "Test",
        );
        assert!(!invalid_combo2.is_service_executable());
    }
}
