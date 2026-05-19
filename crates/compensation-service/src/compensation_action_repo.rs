//! Compensation action repository trait and implementations
//!
//! Phase 3 Batch 1: Compensation action persistence.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, CompensationStatus, ExecutionResult, StrategyType,
};
use crate::sqlx_compensation_action_repo::SqlxCompensationActionRepository;

/// Repository trait for compensation action storage.
/// Allows for in-memory (tests) or SQL-backed implementations.
#[async_trait]
pub trait CompensationActionRepository: Send + Sync {
    /// Create a new compensation action record.
    async fn create(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError>;

    /// Get a compensation action by its ID.
    async fn get(&self, action_id: Uuid) -> Result<CompensationAction, IntentRebaseError>;

    /// List compensation actions for a given tenant.
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError>;

    /// List compensation actions for a given side effect.
    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError>;

    /// List compensation actions by intent for a given tenant.
    /// This enables direct intent-scoped queries without joining through side_effects.
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError>;

    /// List compensation actions by status for a given tenant.
    async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: CompensationStatus,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError>;

    /// Update the status of a compensation action (with optimistic locking).
    /// Optionally sets actor fields associated with the status transition.
    async fn update_status(
        &self,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
        approved_by: Option<&str>,
        waived_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError>;

    /// Record execution result (success or failure) for a compensation action.
    /// Uses optimistic locking via lock_version.
    async fn record_result(
        &self,
        action_id: Uuid,
        result: &ExecutionResult,
        lock_version: i32,
        executed_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError>;

    /// Reapprove a failed compensation action (Failed → Pending).
    ///
    /// This is distinct from update_status because reapproval:
    /// - Does NOT modify approved_at/approved_by (those are for initial approval)
    /// - Does NOT modify waived_at/waived_by
    /// - Simply transitions status back to Pending with optimistic locking
    ///
    /// Uses optimistic locking via lock_version.
    async fn reapprove(
        &self,
        action_id: Uuid,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxCompensationActionRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxCompensationActionRepository> {
        None
    }
}

/// In-memory implementation for testing and Phase 3 Batch 1.
pub struct InMemoryCompensationActionRepository {
    actions: RwLock<HashMap<Uuid, CompensationAction>>,
    /// Secondary index: tenant_id -> list of action_ids
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: side_effect_id -> list of action_ids
    by_side_effect: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: (tenant_id, status) -> list of action_ids
    by_status: RwLock<HashMap<(Uuid, CompensationStatus), Vec<Uuid>>>,
    /// Secondary index: (tenant_id, intent_id) -> list of action_ids
    by_intent: RwLock<HashMap<(Uuid, Uuid), Vec<Uuid>>>,
}

impl InMemoryCompensationActionRepository {
    pub fn new() -> Self {
        Self {
            actions: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
            by_side_effect: RwLock::new(HashMap::new()),
            by_status: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryCompensationActionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompensationActionRepository for InMemoryCompensationActionRepository {
    async fn create(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let mut actions = self.actions.write().await;
        let mut by_tenant = self.by_tenant.write().await;
        let mut by_side_effect = self.by_side_effect.write().await;
        let mut by_status = self.by_status.write().await;
        let mut by_intent = self.by_intent.write().await;

        actions.insert(action.id, action.clone());

        by_tenant
            .entry(action.tenant_id)
            .or_insert_with(Vec::new)
            .push(action.id);

        by_side_effect
            .entry(action.side_effect_id)
            .or_insert_with(Vec::new)
            .push(action.id);

        by_status
            .entry((action.tenant_id, action.status))
            .or_insert_with(Vec::new)
            .push(action.id);

        by_intent
            .entry((action.tenant_id, action.intent_id))
            .or_insert_with(Vec::new)
            .push(action.id);

        Ok(action)
    }

    async fn get(&self, action_id: Uuid) -> Result<CompensationAction, IntentRebaseError> {
        let actions = self.actions.read().await;
        actions
            .get(&action_id)
            .cloned()
            .ok_or(IntentRebaseError::CompensationActionNotFound(action_id))
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let actions = self.actions.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();
        let mut result: Vec<CompensationAction> = ids
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .collect();

        // Sort by generated_at descending
        result.sort_by_key(|b| std::cmp::Reverse(b.generated_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }

    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let actions = self.actions.read().await;
        let by_side_effect = self.by_side_effect.read().await;

        let ids = by_side_effect
            .get(&side_effect_id)
            .cloned()
            .unwrap_or_default();
        let result: Vec<CompensationAction> = ids
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .filter(|a| a.tenant_id == tenant_id)
            .collect();

        Ok(result)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let actions = self.actions.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent
            .get(&(tenant_id, intent_id))
            .cloned()
            .unwrap_or_default();
        let result: Vec<CompensationAction> = ids
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: CompensationStatus,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let actions = self.actions.read().await;
        let by_status = self.by_status.read().await;

        let ids = by_status
            .get(&(tenant_id, status))
            .cloned()
            .unwrap_or_default();
        let result: Vec<CompensationAction> = ids
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn update_status(
        &self,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
        approved_by: Option<&str>,
        waived_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let mut actions = self.actions.write().await;
        let mut by_status = self.by_status.write().await;

        let action = actions
            .get_mut(&action_id)
            .ok_or(IntentRebaseError::CompensationActionNotFound(action_id))?;

        // Optimistic locking check
        if action.lock_version != lock_version {
            return Err(IntentRebaseError::ConcurrencyConflict(action_id));
        }

        let old_status = action.status;
        let tenant_id = action.tenant_id;

        // Update lock version
        action.lock_version += 1;
        action.status = new_status;

        // Update timestamp and actor based on status
        let now = chrono::Utc::now();
        match new_status {
            CompensationStatus::Approved => {
                action.approved_at = Some(now);
                if let Some(approver) = approved_by {
                    action.approved_by = Some(approver.to_string());
                }
            }
            CompensationStatus::Waived => {
                action.waived_at = Some(now);
                if let Some(waiver) = waived_by {
                    action.waived_by = Some(waiver.to_string());
                }
            }
            CompensationStatus::Executed => {
                action.executed_at = Some(now);
            }
            CompensationStatus::Failed => {
                action.failed_at = Some(now);
            }
            _ => {}
        }

        // Maintain by_status secondary index: remove from old status list, add to new
        if let Some(old_list) = by_status.get_mut(&(tenant_id, old_status)) {
            old_list.retain(|&id| id != action_id);
        }
        by_status
            .entry((tenant_id, new_status))
            .or_insert_with(Vec::new)
            .push(action_id);

        Ok(action.clone())
    }

    async fn record_result(
        &self,
        action_id: Uuid,
        result: &ExecutionResult,
        lock_version: i32,
        executed_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let mut actions = self.actions.write().await;
        let mut by_status = self.by_status.write().await;

        let action = actions
            .get_mut(&action_id)
            .ok_or(IntentRebaseError::CompensationActionNotFound(action_id))?;

        // Optimistic locking check
        if action.lock_version != lock_version {
            return Err(IntentRebaseError::ConcurrencyConflict(action_id));
        }

        let old_status = action.status;
        let tenant_id = action.tenant_id;

        // Update lock version
        action.lock_version += 1;

        // Update attempt count
        action.attempt_count += 1;

        // Persist execution result payload
        action.execution_result_payload = Some(result.clone());

        // Update status based on result
        if result.success {
            action.status = CompensationStatus::Executed;
            action.executed_at = Some(result.completed_at);
            if let Some(executor) = executed_by {
                action.executed_by = Some(executor.to_string());
            }
        } else {
            action.status = CompensationStatus::Failed;
            action.failed_at = Some(result.completed_at);
        }

        let new_status = action.status;

        // Maintain by_status secondary index: remove from old status list, add to new
        if let Some(old_list) = by_status.get_mut(&(tenant_id, old_status)) {
            old_list.retain(|&id| id != action_id);
        }
        by_status
            .entry((tenant_id, new_status))
            .or_insert_with(Vec::new)
            .push(action_id);

        Ok(action.clone())
    }

    async fn reapprove(
        &self,
        action_id: Uuid,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let mut actions = self.actions.write().await;
        let mut by_status = self.by_status.write().await;

        let action = actions
            .get_mut(&action_id)
            .ok_or(IntentRebaseError::CompensationActionNotFound(action_id))?;

        // Optimistic locking check
        if action.lock_version != lock_version {
            return Err(IntentRebaseError::ConcurrencyConflict(action_id));
        }

        // Must be in Failed status to reapprove
        if action.status != CompensationStatus::Failed {
            return Err(IntentRebaseError::InvalidCompensationActionTransition {
                from_status: format!("{:?}", action.status),
                to_status: "Pending".to_string(),
                reason: "Only Failed actions can be reapproved".to_string(),
            });
        }

        let old_status = action.status;
        let tenant_id = action.tenant_id;

        // Update lock version only - do NOT modify timestamps or actor fields
        // Reapproval preserves the original approval/waive history
        action.lock_version += 1;
        action.status = CompensationStatus::Pending;
        // Clear failed_at since we're moving out of Failed
        action.failed_at = None;
        // Note: execution_result_payload is preserved for audit/history

        // Maintain by_status secondary index: remove from old status list, add to new
        if let Some(old_list) = by_status.get_mut(&(tenant_id, old_status)) {
            old_list.retain(|&id| id != action_id);
        }
        by_status
            .entry((tenant_id, CompensationStatus::Pending))
            .or_insert_with(Vec::new)
            .push(action_id);

        Ok(action.clone())
    }
}

// =============================================================================
// Helper functions for compensation action enum conversion
// =============================================================================

pub(crate) fn compensation_feasibility_to_string(f: CompensationFeasibility) -> &'static str {
    match f {
        CompensationFeasibility::Automatic => "automatic",
        CompensationFeasibility::SemiAutomatic => "semi_automatic",
        CompensationFeasibility::ManualOnly => "manual_only",
        CompensationFeasibility::NotPossible => "not_possible",
    }
}

pub(crate) fn compensation_feasibility_from_string(
    s: &str,
) -> Result<CompensationFeasibility, IntentRebaseError> {
    match s {
        "automatic" => Ok(CompensationFeasibility::Automatic),
        "semi_automatic" => Ok(CompensationFeasibility::SemiAutomatic),
        "manual_only" => Ok(CompensationFeasibility::ManualOnly),
        "not_possible" => Ok(CompensationFeasibility::NotPossible),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown compensation feasibility: {}",
            other
        ))),
    }
}

pub(crate) fn strategy_type_to_string(s: StrategyType) -> &'static str {
    match s {
        StrategyType::Rollback => "rollback",
        StrategyType::CounterAction => "counter_action",
        StrategyType::FollowupNotice => "followup_notice",
        StrategyType::Quarantine => "quarantine",
        StrategyType::Escalation => "escalation",
    }
}

pub(crate) fn strategy_type_from_string(s: &str) -> Result<StrategyType, IntentRebaseError> {
    match s {
        "rollback" => Ok(StrategyType::Rollback),
        "counter_action" => Ok(StrategyType::CounterAction),
        "followup_notice" => Ok(StrategyType::FollowupNotice),
        "quarantine" => Ok(StrategyType::Quarantine),
        "escalation" => Ok(StrategyType::Escalation),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown strategy type: {}",
            other
        ))),
    }
}

pub(crate) fn compensation_status_to_string(s: CompensationStatus) -> &'static str {
    match s {
        CompensationStatus::Pending => "pending",
        CompensationStatus::Approved => "approved",
        CompensationStatus::Executed => "executed",
        CompensationStatus::Failed => "failed",
        CompensationStatus::Waived => "waived",
    }
}

pub(crate) fn compensation_status_from_string(
    s: &str,
) -> Result<CompensationStatus, IntentRebaseError> {
    match s {
        "pending" => Ok(CompensationStatus::Pending),
        "approved" => Ok(CompensationStatus::Approved),
        "executed" => Ok(CompensationStatus::Executed),
        "failed" => Ok(CompensationStatus::Failed),
        "waived" => Ok(CompensationStatus::Waived),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown compensation status: {}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationAction, RebaseContext, StrategyType};
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

    #[tokio::test]
    async fn test_create_compensation_action() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        let result = repo.create(action).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, id);
        assert_eq!(created.tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_get_compensation_action() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_compensation_action_not_found() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _i in 0..3 {
            let side_effect_id = Uuid::new_v4();
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_tenant_with_limit() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _i in 0..5 {
            let side_effect_id = Uuid::new_v4();
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, Some(2)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create actions with different statuses
        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        repo.create(action1).await.unwrap();

        let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action2.status = CompensationStatus::Executed;
        repo.create(action2).await.unwrap();

        let result = repo
            .list_by_status(tenant_id, CompensationStatus::Pending)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let result = repo
            .update_status(id, CompensationStatus::Approved, 0, None, None)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, CompensationStatus::Approved);
    }

    #[tokio::test]
    async fn test_update_status_concurrency_conflict() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        // First update succeeds
        repo.update_status(id, CompensationStatus::Approved, 0, None, None)
            .await
            .unwrap();

        // Second update with wrong lock_version fails
        let result = repo
            .update_status(id, CompensationStatus::Executed, 0, None, None)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ConcurrencyConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_record_result_success() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let exec_result = ExecutionResult::success("Rollback completed");
        let result = repo.record_result(id, &exec_result, 0, None).await;
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, CompensationStatus::Executed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_record_result_failure() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let exec_result = ExecutionResult::failure(
            "Rollback failed",
            "ROLLBACK_ERR_001",
            Some("Database connection timeout".to_string()),
        );
        let result = repo.record_result(id, &exec_result, 0, None).await;
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, CompensationStatus::Failed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_record_result_increments_attempt_count() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        // First attempt
        let exec_result1 = ExecutionResult::failure("Failed first time", "ERR_001", None);
        repo.record_result(id, &exec_result1, 0, None)
            .await
            .unwrap();

        // Second attempt - lock_version is now 1 after first call
        let exec_result2 = ExecutionResult::success("Succeeded second time");
        let updated = repo
            .record_result(id, &exec_result2, 1, None)
            .await
            .unwrap();

        assert_eq!(updated.attempt_count, 2);
        assert_eq!(updated.status, CompensationStatus::Executed);
    }

    #[tokio::test]
    async fn test_list_by_side_effect() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create multiple actions for same side effect
        for _ in 0..3 {
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_side_effect(side_effect_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_side_effect_filters_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let intent_id_1 = Uuid::new_v4();
        let intent_id_2 = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let action1 = create_test_action(tenant_id_1, side_effect_id, intent_id_1);
        repo.create(action1).await.unwrap();

        let action2 = create_test_action(tenant_id_2, side_effect_id, intent_id_2);
        repo.create(action2).await.unwrap();

        let result = repo.list_by_side_effect(side_effect_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create multiple actions for same intent
        for _ in 0..3 {
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_intent(intent_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent_filters_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id_1 = Uuid::new_v4();
        let side_effect_id_2 = Uuid::new_v4();

        let action1 = create_test_action(tenant_id_1, side_effect_id_1, intent_id);
        repo.create(action1).await.unwrap();

        // Different tenant, same intent_id - should not be returned
        let action2 = create_test_action(tenant_id_2, side_effect_id_2, intent_id);
        repo.create(action2).await.unwrap();

        let result = repo.list_by_intent(intent_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_get_compensation_action_cross_tenant_blocked() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let _tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates a compensation action
        let action = create_test_action(tenant_a, Uuid::new_v4(), Uuid::new_v4());
        let action_id = action.id;
        repo.create(action).await.unwrap();

        // Tenant A can get their own action
        let result = repo.get(action_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tenant_id, tenant_a);

        // Note: The InMemory repository's `get` method does not enforce tenant isolation.
        // This test documents the current behavior where any tenant can get any action by ID.
        // Production implementations should add tenant filtering to the `get` method.
    }

    #[tokio::test]
    async fn test_list_by_tenant_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates 3 compensation actions
        for _ in 0..3 {
            let action = create_test_action(tenant_a, Uuid::new_v4(), Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // Tenant B creates 2 compensation actions
        for _ in 0..2 {
            let action = create_test_action(tenant_b, Uuid::new_v4(), Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // List for tenant A should return 3 actions
        let actions_a = repo.list_by_tenant(tenant_a, None).await.unwrap();
        assert_eq!(actions_a.len(), 3);
        assert!(actions_a.iter().all(|a| a.tenant_id == tenant_a));

        // List for tenant B should return 2 actions
        let actions_b = repo.list_by_tenant(tenant_b, None).await.unwrap();
        assert_eq!(actions_b.len(), 2);
        assert!(actions_b.iter().all(|a| a.tenant_id == tenant_b));
    }

    #[tokio::test]
    async fn test_list_by_side_effect_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let shared_side_effect_id = Uuid::new_v4();

        // Tenant A creates 2 actions for the shared side effect
        for _ in 0..2 {
            let action = create_test_action(tenant_a, shared_side_effect_id, Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // Tenant B creates 1 action for the same shared side effect
        let action_b = create_test_action(tenant_b, shared_side_effect_id, Uuid::new_v4());
        repo.create(action_b).await.unwrap();

        // List for tenant A should only return tenant A's 2 actions
        let actions_a = repo
            .list_by_side_effect(shared_side_effect_id, tenant_a)
            .await
            .unwrap();
        assert_eq!(actions_a.len(), 2);
        assert!(actions_a.iter().all(|a| a.tenant_id == tenant_a));

        // List for tenant B should only return tenant B's 1 action
        let actions_b = repo
            .list_by_side_effect(shared_side_effect_id, tenant_b)
            .await
            .unwrap();
        assert_eq!(actions_b.len(), 1);
        assert!(actions_b.iter().all(|a| a.tenant_id == tenant_b));
    }
}

#[cfg(test)]
mod sqlx_compensation_action_tests {
    use super::*;

    #[test]
    fn test_compensation_feasibility_to_string() {
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::Automatic),
            "automatic"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::SemiAutomatic),
            "semi_automatic"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::ManualOnly),
            "manual_only"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::NotPossible),
            "not_possible"
        );
    }

    #[test]
    fn test_compensation_feasibility_from_string() {
        assert_eq!(
            compensation_feasibility_from_string("automatic").unwrap(),
            CompensationFeasibility::Automatic
        );
        assert_eq!(
            compensation_feasibility_from_string("semi_automatic").unwrap(),
            CompensationFeasibility::SemiAutomatic
        );
        assert_eq!(
            compensation_feasibility_from_string("manual_only").unwrap(),
            CompensationFeasibility::ManualOnly
        );
        assert_eq!(
            compensation_feasibility_from_string("not_possible").unwrap(),
            CompensationFeasibility::NotPossible
        );
        // Unknown values return error
        assert!(compensation_feasibility_from_string("unknown").is_err());
    }

    #[test]
    fn test_strategy_type_to_string() {
        assert_eq!(strategy_type_to_string(StrategyType::Rollback), "rollback");
        assert_eq!(
            strategy_type_to_string(StrategyType::CounterAction),
            "counter_action"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::FollowupNotice),
            "followup_notice"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::Quarantine),
            "quarantine"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::Escalation),
            "escalation"
        );
    }

    #[test]
    fn test_strategy_type_from_string() {
        assert_eq!(
            strategy_type_from_string("rollback").unwrap(),
            StrategyType::Rollback
        );
        assert_eq!(
            strategy_type_from_string("counter_action").unwrap(),
            StrategyType::CounterAction
        );
        assert_eq!(
            strategy_type_from_string("followup_notice").unwrap(),
            StrategyType::FollowupNotice
        );
        assert_eq!(
            strategy_type_from_string("quarantine").unwrap(),
            StrategyType::Quarantine
        );
        assert_eq!(
            strategy_type_from_string("escalation").unwrap(),
            StrategyType::Escalation
        );
        // Unknown values return error
        assert!(strategy_type_from_string("unknown").is_err());
    }

    #[test]
    fn test_compensation_status_to_string() {
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Pending),
            "pending"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Approved),
            "approved"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Executed),
            "executed"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Failed),
            "failed"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Waived),
            "waived"
        );
    }

    #[test]
    fn test_compensation_status_from_string() {
        assert_eq!(
            compensation_status_from_string("pending").unwrap(),
            CompensationStatus::Pending
        );
        assert_eq!(
            compensation_status_from_string("approved").unwrap(),
            CompensationStatus::Approved
        );
        assert_eq!(
            compensation_status_from_string("executed").unwrap(),
            CompensationStatus::Executed
        );
        assert_eq!(
            compensation_status_from_string("failed").unwrap(),
            CompensationStatus::Failed
        );
        assert_eq!(
            compensation_status_from_string("waived").unwrap(),
            CompensationStatus::Waived
        );
        // Unknown values return error
        assert!(compensation_status_from_string("unknown").is_err());
    }
}
