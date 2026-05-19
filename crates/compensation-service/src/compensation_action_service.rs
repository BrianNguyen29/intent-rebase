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
    CompensationAction, CompensationFeasibility, CompensationStatus, ExecutionResult, RebaseContext,
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
                    use crate::RollbackExecutor;
                    let executor = RollbackExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::CounterAction, CompensationFeasibility::SemiAutomatic) => {
                    use crate::CounterActionExecutor;
                    let executor = CounterActionExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::FollowupNotice, CompensationFeasibility::ManualOnly) => {
                    use crate::FollowupNoticeExecutor;
                    let executor = FollowupNoticeExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await?
                }
                (StrategyType::Escalation, CompensationFeasibility::NotPossible) => {
                    use crate::EscalationExecutor;
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
#[cfg(test)]
#[path = "compensation_action_service_tests.rs"]
mod tests;
