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
