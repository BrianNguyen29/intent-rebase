use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use sqlx::Row;
use uuid::Uuid;

use super::types::{
    WebhookOutboxDlqErrorSummary, WebhookOutboxDlqStats, WebhookOutboxRecord,
    WebhookOutboxRepository, WebhookOutboxStatus,
};

// =============================================================================
// SQLx Implementation (local-dev foundation)
// =============================================================================

/// SQL-backed webhook outbox repository.
///
/// Bounded local-dev foundation: uses `sqlx::query` (not `query!`) so no
/// compile-time DB or offline macros are required.
///
/// **Non-production caveat:** `webhook_url` is persisted as of migration 020,
/// but this repository remains local-dev only and requires subscription CRUD +
/// retry design before being wired into the production propagation path.
pub struct SqlxWebhookOutboxRepository {
    pool: sqlx::PgPool,
}

impl SqlxWebhookOutboxRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

fn status_to_str(status: &WebhookOutboxStatus) -> &'static str {
    match status {
        WebhookOutboxStatus::Pending => "pending",
        WebhookOutboxStatus::Claimed => "claimed",
        WebhookOutboxStatus::Delivered => "delivered",
        WebhookOutboxStatus::Failed => "failed",
    }
}

fn status_from_str(s: &str) -> Result<WebhookOutboxStatus, IntentRebaseError> {
    match s {
        "pending" => Ok(WebhookOutboxStatus::Pending),
        "claimed" => Ok(WebhookOutboxStatus::Claimed),
        "delivered" => Ok(WebhookOutboxStatus::Delivered),
        "failed" => Ok(WebhookOutboxStatus::Failed),
        other => Err(IntentRebaseError::StorageError(format!(
            "unknown webhook_outbox status: {}",
            other
        ))),
    }
}

fn map_err_column(col: &str, e: sqlx::Error) -> IntentRebaseError {
    IntentRebaseError::StorageError(format!("Invalid {} column: {}", col, e))
}

fn map_row(row: &sqlx::postgres::PgRow) -> Result<WebhookOutboxRecord, IntentRebaseError> {
    Ok(WebhookOutboxRecord {
        id: row.try_get("id").map_err(|e| map_err_column("id", e))?,
        tenant_id: row
            .try_get("tenant_id")
            .map_err(|e| map_err_column("tenant_id", e))?,
        intent_id: row
            .try_get("intent_id")
            .map_err(|e| map_err_column("intent_id", e))?,
        subscription_id: row
            .try_get("subscription_id")
            .map_err(|e| map_err_column("subscription_id", e))?,
        event_type: row
            .try_get("event_type")
            .map_err(|e| map_err_column("event_type", e))?,
        payload: row
            .try_get::<serde_json::Value, _>("payload")
            .map_err(|e| map_err_column("payload", e))?,
        webhook_url: row.try_get("webhook_url").ok(),
        status: status_from_str(
            row.try_get::<String, _>("status")
                .map_err(|e| map_err_column("status", e))?
                .as_str(),
        )?,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|e| map_err_column("attempt_count", e))?,
        max_attempts: row
            .try_get("max_attempts")
            .map_err(|e| map_err_column("max_attempts", e))?,
        scheduled_at: row
            .try_get("scheduled_at")
            .map_err(|e| map_err_column("scheduled_at", e))?,
        locked_at: row.try_get("locked_at").ok(),
        locked_by: row.try_get("locked_by").ok(),
        delivered_at: row.try_get("delivered_at").ok(),
        last_error: row.try_get("last_error").ok(),
        replay_count: row
            .try_get("replay_count")
            .map_err(|e| map_err_column("replay_count", e))?,
        replayed_at: row.try_get("replayed_at").ok(),
        replayed_by: row.try_get("replayed_by").ok(),
        lock_version: row
            .try_get("lock_version")
            .map_err(|e| map_err_column("lock_version", e))?,
        created_at: row
            .try_get("created_at")
            .map_err(|e| map_err_column("created_at", e))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|e| map_err_column("updated_at", e))?,
    })
}

#[async_trait]
impl WebhookOutboxRepository for SqlxWebhookOutboxRepository {
    async fn create(
        &self,
        record: WebhookOutboxRecord,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        sqlx::query(
            r#"
            INSERT INTO webhook_outbox (
                id, tenant_id, intent_id, subscription_id, event_type, payload,
                webhook_url, status, attempt_count, max_attempts, scheduled_at,
                locked_at, locked_by, delivered_at, last_error,
                replay_count, replayed_at, replayed_by,
                lock_version, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14, $15,
                $16, $17, $18,
                $19, $20, $21
            )
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.intent_id)
        .bind(record.subscription_id)
        .bind(&record.event_type)
        .bind(&record.payload)
        .bind(record.webhook_url.as_ref())
        .bind(status_to_str(&record.status))
        .bind(record.attempt_count)
        .bind(record.max_attempts)
        .bind(record.scheduled_at)
        .bind(record.locked_at)
        .bind(record.locked_by.as_ref())
        .bind(record.delivered_at)
        .bind(record.last_error.as_ref())
        .bind(record.replay_count)
        .bind(record.replayed_at)
        .bind(record.replayed_by.as_ref())
        .bind(record.lock_version)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert outbox record: {}", e)))?;

        Ok(record)
    }

    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT * FROM webhook_outbox
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )),
            _ => IntentRebaseError::StorageError(format!("select outbox record: {}", e)),
        })?;

        map_row(&row)
    }

    async fn list_pending(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM webhook_outbox
            WHERE tenant_id = $1 AND status = 'pending'
            ORDER BY scheduled_at, id
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list pending outbox records: {}", e))
        })?;

        rows.iter().map(map_row).collect()
    }

    async fn claim(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        locked_by: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let result = sqlx::query(
            r#"
            UPDATE webhook_outbox
            SET status = 'claimed',
                locked_at = NOW(),
                locked_by = $3,
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND status = 'pending'
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&locked_by)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(row) => map_row(&row),
            Err(sqlx::Error::RowNotFound) => Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} is not pending or not found for tenant {}",
                id, tenant_id
            ))),
            Err(e) => Err(IntentRebaseError::StorageError(format!(
                "claim outbox record: {}",
                e
            ))),
        }
    }

    async fn mark_delivered(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            UPDATE webhook_outbox
            SET status = 'delivered',
                delivered_at = NOW(),
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )),
            _ => IntentRebaseError::StorageError(format!("mark delivered: {}", e)),
        })?;

        map_row(&row)
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            UPDATE webhook_outbox
            SET status = 'failed',
                last_error = $3,
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&last_error)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )),
            _ => IntentRebaseError::StorageError(format!("mark failed: {}", e)),
        })?;

        map_row(&row)
    }

    async fn reschedule_retry(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
        scheduled_at: DateTime<Utc>,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            UPDATE webhook_outbox
            SET status = 'pending',
                attempt_count = attempt_count + 1,
                scheduled_at = $3,
                last_error = $4,
                locked_at = NULL,
                locked_by = NULL,
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(scheduled_at)
        .bind(&last_error)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )),
            _ => IntentRebaseError::StorageError(format!("reschedule retry: {}", e)),
        })?;

        map_row(&row)
    }

    async fn list_failed(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM webhook_outbox
            WHERE tenant_id = $1 AND status = 'failed'
            ORDER BY updated_at DESC, id
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list failed outbox records: {}", e))
        })?;

        rows.iter().map(map_row).collect()
    }

    async fn replay_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        replayed_by: Option<String>,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let result = sqlx::query(
            r#"
            UPDATE webhook_outbox
            SET status = 'pending',
                attempt_count = 0,
                scheduled_at = NOW(),
                last_error = NULL,
                locked_at = NULL,
                locked_by = NULL,
                replay_count = replay_count + 1,
                replayed_at = NOW(),
                replayed_by = $3,
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND status = 'failed'
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(replayed_by.as_ref())
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(row) => map_row(&row),
            Err(sqlx::Error::RowNotFound) => Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} is not failed or not found for tenant {}",
                id, tenant_id
            ))),
            Err(e) => Err(IntentRebaseError::StorageError(format!(
                "replay failed outbox record: {}",
                e
            ))),
        }
    }

    async fn list_failed_older_than(
        &self,
        tenant_id: Uuid,
        before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM webhook_outbox
            WHERE tenant_id = $1 AND status = 'failed' AND updated_at < $2
            ORDER BY updated_at DESC, id
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(before)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list failed outbox records older than: {}", e))
        })?;

        rows.iter().map(map_row).collect()
    }

    async fn list_distinct_pending_tenants(&self) -> Result<Vec<Uuid>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT tenant_id FROM webhook_outbox
            WHERE status = 'pending'
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list distinct pending tenants: {}", e))
        })?;

        rows.iter()
            .map(|r| {
                r.try_get("tenant_id").map_err(|e| {
                    IntentRebaseError::StorageError(format!("Invalid tenant_id column: {}", e))
                })
            })
            .collect()
    }

    async fn list_replayed(
        &self,
        tenant_id: Uuid,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let rows = if let Some(since) = since {
            sqlx::query(
                r#"
                SELECT * FROM webhook_outbox
                WHERE tenant_id = $1 AND replay_count > 0 AND replayed_at IS NOT NULL AND replayed_at >= $2
                ORDER BY replayed_at DESC, id
                LIMIT $3
                "#,
            )
            .bind(tenant_id)
            .bind(since)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT * FROM webhook_outbox
                WHERE tenant_id = $1 AND replay_count > 0 AND replayed_at IS NOT NULL
                ORDER BY replayed_at DESC, id
                LIMIT $2
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list replayed outbox records: {}", e))
        })?;

        rows.iter().map(map_row).collect()
    }

    async fn dlq_stats(&self, tenant_id: Uuid) -> Result<WebhookOutboxDlqStats, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'failed') as total_failed,
                MIN(updated_at) FILTER (WHERE status = 'failed') as oldest_failed_at,
                COUNT(*) FILTER (WHERE replay_count > 0) as replayed_count
            FROM webhook_outbox
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("dlq stats aggregate: {}", e)))?;

        let total_failed: i64 = row
            .try_get("total_failed")
            .map_err(|e| map_err_column("total_failed", e))?;
        let oldest_failed_at: Option<DateTime<Utc>> = row
            .try_get("oldest_failed_at")
            .map_err(|e| map_err_column("oldest_failed_at", e))?;
        let replayed_count: i64 = row
            .try_get("replayed_count")
            .map_err(|e| map_err_column("replayed_count", e))?;

        let oldest_failed_age_seconds = oldest_failed_at.map(|dt| {
            let age = (Utc::now() - dt).num_seconds();
            age.max(0)
        });

        let error_rows = sqlx::query(
            r#"
            SELECT COALESCE(last_error, 'unknown') as error_pattern, COUNT(*) as count
            FROM webhook_outbox
            WHERE tenant_id = $1 AND status = 'failed'
            GROUP BY last_error
            ORDER BY count DESC, error_pattern ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("dlq stats error summary: {}", e)))?;

        let by_error_summary = error_rows
            .iter()
            .map(|r| {
                Ok(WebhookOutboxDlqErrorSummary {
                    error_pattern: r
                        .try_get("error_pattern")
                        .map_err(|e| map_err_column("error_pattern", e))?,
                    count: r.try_get("count").map_err(|e| map_err_column("count", e))?,
                })
            })
            .collect::<Result<Vec<_>, IntentRebaseError>>()?;

        Ok(WebhookOutboxDlqStats {
            total_failed,
            oldest_failed_age_seconds,
            replayed_count,
            by_error_summary,
        })
    }
}
