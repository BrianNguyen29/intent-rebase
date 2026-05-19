//! SQLx-backed compensation action repository
//!
//! Provides `SqlxCompensationActionRepository` for PostgreSQL persistence.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

use crate::compensation_action::{
    CompensationAction, CompensationStatus, ExecutionResult, RebaseContext,
};
use crate::compensation_action_repo::{
    compensation_feasibility_from_string, compensation_feasibility_to_string,
    compensation_status_from_string, compensation_status_to_string, strategy_type_from_string,
    strategy_type_to_string, CompensationActionRepository,
};

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

    fn as_sqlx_repo(&self) -> Option<&SqlxCompensationActionRepository> {
        Some(self)
    }
}

// =============================================================================
// Transaction helper methods for RLS-aware operations
// =============================================================================

impl SqlxCompensationActionRepository {
    /// Update compensation action status with an external transaction.
    /// Used by approve/waive handlers that manage their own transaction context.
    pub async fn update_status_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
        approved_by: Option<&str>,
        waived_by: Option<&str>,
    ) -> Result<CompensationAction, IntentRebaseError> {
        let status_str = compensation_status_to_string(new_status);
        let now = chrono::Utc::now();

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
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update compensation action status: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => Err(IntentRebaseError::ConcurrencyConflict(action_id)),
        }
    }

    /// Record execution result with an external transaction.
    /// Used by execute handlers that manage their own transaction context.
    pub async fn record_result_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("record compensation result: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => Err(IntentRebaseError::ConcurrencyConflict(action_id)),
        }
    }

    /// Reapprove a failed compensation action with an external transaction.
    /// Used by reapprove handlers that manage their own transaction context.
    pub async fn reapprove_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        action_id: Uuid,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
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
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("reapprove compensation action: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_action(r),
            None => {
                // Could be either not found, wrong lock_version, or not in Failed status
                // Fetch to determine which error to return
                let action = self.get(action_id).await;
                match action {
                    Ok(a) if a.status != CompensationStatus::Failed => {
                        Err(IntentRebaseError::InvalidCompensationActionTransition {
                            from_status: format!("{:?}", a.status),
                            to_status: "Pending".to_string(),
                            reason: "Only Failed actions can be reapproved".to_string(),
                        })
                    }
                    Ok(_) => Err(IntentRebaseError::ConcurrencyConflict(action_id)),
                    Err(e) => Err(e),
                }
            }
        }
    }
}
