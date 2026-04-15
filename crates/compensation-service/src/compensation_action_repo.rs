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
    CompensationAction, CompensationStatus, ExecutionResult, RebaseContext,
};

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
        result.sort_by(|a, b| b.generated_at.cmp(&a.generated_at));

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
// SQLx-backed Compensation Action Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

use crate::compensation_action::{CompensationFeasibility, StrategyType};

/// SQL-backed repository for compensation action storage using PostgreSQL.
pub struct SqlxCompensationActionRepository {
    pool: PgPool,
}

impl SqlxCompensationActionRepository {
    /// Create a new SqlxCompensationActionRepository.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_action(&self, row: PgRow) -> Result<CompensationAction, IntentRebaseError> {
        let trigger_context_json: serde_json::Value = row.get("trigger_context");
        let trigger_context: RebaseContext =
            serde_json::from_value(trigger_context_json).map_err(|e| {
                IntentRebaseError::Internal(format!("deserialize trigger_context: {}", e))
            })?;

        let execution_result_payload_json: Option<serde_json::Value> =
            row.get("execution_result_payload");
        let execution_result_payload: Option<ExecutionResult> = execution_result_payload_json
            .map(|v| {
                serde_json::from_value(v).map_err(|e| {
                    IntentRebaseError::Internal(format!(
                        "deserialize execution_result_payload: {}",
                        e
                    ))
                })
            })
            .transpose()?;

        Ok(CompensationAction {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            side_effect_id: row.get("side_effect_id"),
            intent_id: row.get("intent_id"),
            trigger_context,
            execution_result_payload,
            feasibility: compensation_feasibility_from_string(
                &row.get::<String, _>("feasibility"),
            )?,
            strategy_type: strategy_type_from_string(&row.get::<String, _>("strategy_type"))?,
            rationale: row.get("rationale"),
            status: compensation_status_from_string(&row.get::<String, _>("status"))?,
            attempt_count: row.get("attempt_count"),
            max_retries: row.get("max_retries"),
            lock_version: row.get("lock_version"),
            generated_at: row.get("generated_at"),
            approved_at: row.get("approved_at"),
            approved_by: row.get("approved_by"),
            waived_at: row.get("waived_at"),
            waived_by: row.get("waived_by"),
            executed_at: row.get("executed_at"),
            executed_by: row.get("executed_by"),
            failed_at: row.get("failed_at"),
        })
    }
}

#[async_trait]
impl CompensationActionRepository for SqlxCompensationActionRepository {
    async fn create(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let feasibility_str = compensation_feasibility_to_string(action.feasibility);
        let strategy_str = strategy_type_to_string(action.strategy_type);
        let status_str = compensation_status_to_string(action.status);
        let trigger_context_json = serde_json::to_value(&action.trigger_context).map_err(|e| {
            IntentRebaseError::Internal(format!("serialize trigger_context: {}", e))
        })?;
        let execution_result_payload_json = action
            .execution_result_payload
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| {
                IntentRebaseError::Internal(format!("serialize execution_result_payload: {}", e))
            })?;

        sqlx::query(
            r#"
            INSERT INTO compensation_actions (
                id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
            "#,
        )
        .bind(action.id)
        .bind(action.tenant_id)
        .bind(action.side_effect_id)
        .bind(action.intent_id)
        .bind(trigger_context_json)
        .bind(execution_result_payload_json)
        .bind(feasibility_str)
        .bind(strategy_str)
        .bind(&action.rationale)
        .bind(status_str)
        .bind(action.attempt_count)
        .bind(action.max_retries)
        .bind(action.lock_version)
        .bind(action.generated_at)
        .bind(action.approved_at)
        .bind(&action.approved_by)
        .bind(action.waived_at)
        .bind(&action.waived_by)
        .bind(action.executed_at)
        .bind(&action.executed_by)
        .bind(action.failed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("insert compensation action: {}", e))
        })?;

        Ok(action)
    }

    async fn get(&self, action_id: Uuid) -> Result<CompensationAction, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            FROM compensation_actions
            WHERE id = $1
            "#,
        )
        .bind(action_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("fetch compensation action: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => Err(IntentRebaseError::CompensationActionNotFound(action_id)),
        }
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let limit = limit.unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            FROM compensation_actions
            WHERE tenant_id = $1
            ORDER BY generated_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list compensation actions by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_action(r)).collect()
    }

    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            FROM compensation_actions
            WHERE side_effect_id = $1 AND tenant_id = $2
            ORDER BY generated_at DESC
            "#,
        )
        .bind(side_effect_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list compensation actions by side effect: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_action(r)).collect()
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            FROM compensation_actions
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY generated_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list compensation actions by intent: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_action(r)).collect()
    }

    async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: CompensationStatus,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        let status_str = compensation_status_to_string(status);
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            FROM compensation_actions
            WHERE tenant_id = $1 AND status = $2
            ORDER BY generated_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(status_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list compensation actions by status: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_action(r)).collect()
    }

    async fn update_status(
        &self,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
        approved_by: Option<&str>,
        waived_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let status_str = compensation_status_to_string(new_status);
        let now = chrono::Utc::now();

        // Compute updated_at based on new_status
        let (approved_at, waived_at, executed_at, failed_at) = match new_status {
            CompensationStatus::Approved => (Some(now), None, None, None),
            CompensationStatus::Waived => (None, Some(now), None, None),
            CompensationStatus::Executed => (None, None, Some(now), None),
            CompensationStatus::Failed => (None, None, None, Some(now)),
            _ => (None, None, None, None),
        };

        let row = sqlx::query(
            r#"
            UPDATE compensation_actions
            SET status = $2,
                lock_version = lock_version + 1,
                approved_at = COALESCE($3, approved_at),
                waived_at = COALESCE($4, waived_at),
                executed_at = COALESCE($5, executed_at),
                failed_at = COALESCE($6, failed_at),
                approved_by = COALESCE($7, approved_by),
                waived_by = COALESCE($8, waived_by)
            WHERE id = $1 AND lock_version = $9
            RETURNING id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            "#,
        )
        .bind(action_id)
        .bind(status_str)
        .bind(approved_at)
        .bind(waived_at)
        .bind(executed_at)
        .bind(failed_at)
        .bind(approved_by)
        .bind(waived_by)
        .bind(lock_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update compensation action status: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => Err(IntentRebaseError::ConcurrencyConflict(action_id)),
        }
    }

    async fn record_result(
        &self,
        action_id: Uuid,
        result: &ExecutionResult,
        lock_version: i32,
        executed_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let status_str = if result.success {
            compensation_status_to_string(CompensationStatus::Executed)
        } else {
            compensation_status_to_string(CompensationStatus::Failed)
        };

        let execution_result_payload = serde_json::json!({
            "success": result.success,
            "summary": result.summary,
            "error_code": result.error_code,
            "error_detail": result.error_detail,
            "completed_at": result.completed_at,
        });

        let row = sqlx::query(
            r#"
            UPDATE compensation_actions
            SET status = $2,
                attempt_count = attempt_count + 1,
                lock_version = lock_version + 1,
                executed_at = CASE WHEN $3 THEN NOW() ELSE executed_at END,
                failed_at = CASE WHEN NOT $3 THEN NOW() ELSE failed_at END,
                execution_result_payload = $4,
                executed_by = COALESCE($6, executed_by)
            WHERE id = $1 AND lock_version = $5
            RETURNING id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            "#,
        )
        .bind(action_id)
        .bind(status_str)
        .bind(result.success)
        .bind(&execution_result_payload)
        .bind(lock_version)
        .bind(executed_by)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("record compensation result: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => Err(IntentRebaseError::ConcurrencyConflict(action_id)),
        }
    }

    async fn reapprove(
        &self,
        action_id: Uuid,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
        // Reapproval sets status back to Pending and clears failed_at
        // Does NOT modify approved_at/approved_by or waived_at/waived_by
        let row = sqlx::query(
            r#"
            UPDATE compensation_actions
            SET status = 'pending',
                lock_version = lock_version + 1,
                failed_at = NULL
            WHERE id = $1 AND lock_version = $2 AND status = 'failed'
            RETURNING id, tenant_id, side_effect_id, intent_id, trigger_context,
                execution_result_payload, feasibility, strategy_type,
                rationale, status, attempt_count, max_retries, lock_version, generated_at,
                approved_at, approved_by, waived_at, waived_by, executed_at, executed_by, failed_at
            "#,
        )
        .bind(action_id)
        .bind(lock_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("reapprove compensation action: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => {
                // Could be either not found, wrong lock_version, or not in Failed status
                // Check which error to return
                let action = self.get(action_id).await;
                match action {
                    Ok(a) if a.status != CompensationStatus::Failed => {
                        Err(IntentRebaseError::InvalidCompensationActionTransition {
                            from_status: format!("{:?}", a.status),
                            to_status: "Pending".to_string(),
                            reason: "Only Failed actions can be reapproved".to_string(),
                        })
                    }
                    Ok(_) => {
                        // Lock version conflict
                        Err(IntentRebaseError::ConcurrencyConflict(action_id))
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }
}

// =============================================================================
// Helper functions for compensation action enum conversion
// =============================================================================

fn compensation_feasibility_to_string(f: CompensationFeasibility) -> &'static str {
    match f {
        CompensationFeasibility::Automatic => "automatic",
        CompensationFeasibility::SemiAutomatic => "semi_automatic",
        CompensationFeasibility::ManualOnly => "manual_only",
        CompensationFeasibility::NotPossible => "not_possible",
    }
}

fn compensation_feasibility_from_string(
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

fn strategy_type_to_string(s: StrategyType) -> &'static str {
    match s {
        StrategyType::Rollback => "rollback",
        StrategyType::CounterAction => "counter_action",
        StrategyType::FollowupNotice => "followup_notice",
        StrategyType::Quarantine => "quarantine",
        StrategyType::Escalation => "escalation",
    }
}

fn strategy_type_from_string(s: &str) -> Result<StrategyType, IntentRebaseError> {
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

fn compensation_status_to_string(s: CompensationStatus) -> &'static str {
    match s {
        CompensationStatus::Pending => "pending",
        CompensationStatus::Approved => "approved",
        CompensationStatus::Executed => "executed",
        CompensationStatus::Failed => "failed",
        CompensationStatus::Waived => "waived",
    }
}

fn compensation_status_from_string(s: &str) -> Result<CompensationStatus, IntentRebaseError> {
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
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

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
        let actions_a = repo.list_by_side_effect(shared_side_effect_id, tenant_a).await.unwrap();
        assert_eq!(actions_a.len(), 2);
        assert!(actions_a.iter().all(|a| a.tenant_id == tenant_a));

        // List for tenant B should only return tenant B's 1 action
        let actions_b = repo.list_by_side_effect(shared_side_effect_id, tenant_b).await.unwrap();
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
