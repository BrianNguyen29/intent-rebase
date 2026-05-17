//! Webhook outbox repository (Phase 4a Slice 1)
//!
//! Provides storage for webhook outbox records that track pending,
//! claimed, delivered, and failed webhook deliveries.
//!
//! Bounded Slice 1: schema + types + repository + in-memory implementation.
//! Background worker (Slice 2), HMAC signing (Slice 3), subscription CRUD
//! (Slice 4), and retry/DLQ full lifecycle (Slice 5) remain deferred.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 1)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

// =============================================================================
// Domain Types
// =============================================================================

/// Status of a webhook outbox entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebhookOutboxStatus {
    /// Awaiting delivery
    Pending,
    /// Worker has taken ownership
    Claimed,
    /// Final success
    Delivered,
    /// Exhausted max_attempts or non-retryable error
    Failed,
}

/// A webhook outbox record — tracks a single webhook delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookOutboxRecord {
    /// Unique delivery identifier
    pub id: Uuid,
    /// Tenant this delivery belongs to
    pub tenant_id: Uuid,
    /// Intent this delivery is for
    pub intent_id: Uuid,
    /// Subscription this delivery targets
    pub subscription_id: Uuid,
    /// Event type (e.g., intent_changed)
    pub event_type: String,
    /// Payload envelope
    pub payload: Value,
    /// Target webhook URL (optional until subscription CRUD wires URLs)
    pub webhook_url: Option<String>,
    /// Current status
    pub status: WebhookOutboxStatus,
    /// Number of delivery attempts made
    pub attempt_count: i32,
    /// Maximum allowed attempts
    pub max_attempts: i32,
    /// Next scheduled delivery attempt time
    pub scheduled_at: DateTime<Utc>,
    /// Claim timestamp for worker concurrency
    pub locked_at: Option<DateTime<Utc>>,
    /// Worker identity token
    pub locked_by: Option<String>,
    /// Final success timestamp
    pub delivered_at: Option<DateTime<Utc>>,
    /// Last failure reason
    pub last_error: Option<String>,
    /// Optimistic locking version
    pub lock_version: i32,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl WebhookOutboxRecord {
    /// Create a new pending outbox record.
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        subscription_id: Uuid,
        event_type: String,
        payload: Value,
        webhook_url: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            subscription_id,
            event_type,
            payload,
            webhook_url,
            status: WebhookOutboxStatus::Pending,
            attempt_count: 0,
            max_attempts: 3,
            scheduled_at: now,
            locked_at: None,
            locked_by: None,
            delivered_at: None,
            last_error: None,
            lock_version: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Builder-style helper to set the webhook URL.
    ///
    /// Useful when constructing records before the subscription CRUD API
    /// provides URL resolution. As of migration 020, `webhook_url` is
    /// persisted by the SQLx repository when present.
    pub fn with_webhook_url(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }
}

// =============================================================================
// Repository Trait
// =============================================================================

#[async_trait]
pub trait WebhookOutboxRepository: Send + Sync {
    /// Create a new outbox record.
    async fn create(
        &self,
        record: WebhookOutboxRecord,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// Get an outbox record by ID (tenant-scoped).
    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// List pending records for a tenant, ordered by scheduled_at then id.
    async fn list_pending(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError>;

    /// Claim a pending record (status → Claimed, set locked_at/locked_by, increment lock_version).
    async fn claim(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        locked_by: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// Mark a record as delivered (status → Delivered, set delivered_at, increment lock_version).
    async fn mark_delivered(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// Mark a record as failed (status → Failed, set last_error, increment lock_version).
    async fn mark_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// Reschedule a record for retry (status → Pending, increment attempt_count,
    /// set scheduled_at, clear locked_at/locked_by, increment lock_version).
    ///
    /// Slice 5a: used by the worker to reschedule a retryable failure without
    /// blocking the worker loop on a real-time sleep.
    async fn reschedule_retry(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
        scheduled_at: DateTime<Utc>,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// List failed records for a tenant, ordered by updated_at desc then id.
    ///
    /// Slice 5b: local-dev DLQ view — no separate DLQ table, just failed-status
    /// records from the outbox.
    async fn list_failed(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError>;

    /// Replay a failed record (status → Pending, reset attempt_count,
    /// scheduled_at=now, clear last_error/locked_at/locked_by, increment lock_version).
    ///
    /// Slice 5b: idempotency-bounded replay. Only transitions from Failed.
    /// Returns an error if the record is not in Failed status.
    async fn replay_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError>;

    /// List failed records older than a cutoff for a tenant.
    ///
    /// Phase 1.1 retention query foundation: returns tenant-scoped failed records
    /// with `updated_at < before`, ordered by `updated_at` desc then id.
    /// This is a query-only local-dev helper — no purge, no enforcement.
    async fn list_failed_older_than(
        &self,
        tenant_id: Uuid,
        before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError>;

    /// List distinct tenant IDs that have at least one pending outbox record.
    ///
    /// Bounded local-dev helper for background worker tenant discovery.
    /// Production-grade tenant discovery remains future scope.
    async fn list_distinct_pending_tenants(&self) -> Result<Vec<Uuid>, IntentRebaseError>;
}

// =============================================================================
// In-Memory Implementation (for tests)
// =============================================================================

pub struct InMemoryWebhookOutboxRepository {
    records: RwLock<HashMap<Uuid, WebhookOutboxRecord>>,
}

impl InMemoryWebhookOutboxRepository {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryWebhookOutboxRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebhookOutboxRepository for InMemoryWebhookOutboxRepository {
    async fn create(
        &self,
        record: WebhookOutboxRecord,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        if records.contains_key(&record.id) {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} already exists",
                record.id
            )));
        }
        records.insert(record.id, record.clone());
        Ok(record)
    }

    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let records = self.records.read().await;
        let record = records.get(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        Ok(record.clone())
    }

    async fn list_pending(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut pending: Vec<_> = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.status == WebhookOutboxStatus::Pending)
            .cloned()
            .collect();
        pending.sort_by(|a, b| {
            a.scheduled_at
                .cmp(&b.scheduled_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        pending.truncate(limit as usize);
        Ok(pending)
    }

    async fn claim(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        locked_by: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        if record.status != WebhookOutboxStatus::Pending {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} is not pending (status: {:?})",
                id, record.status
            )));
        }
        record.status = WebhookOutboxStatus::Claimed;
        record.locked_at = Some(Utc::now());
        record.locked_by = Some(locked_by);
        record.lock_version += 1;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn mark_delivered(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        record.status = WebhookOutboxStatus::Delivered;
        record.delivered_at = Some(Utc::now());
        record.lock_version += 1;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        record.status = WebhookOutboxStatus::Failed;
        record.last_error = Some(last_error);
        record.lock_version += 1;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn reschedule_retry(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        last_error: String,
        scheduled_at: DateTime<Utc>,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        record.status = WebhookOutboxStatus::Pending;
        record.attempt_count += 1;
        record.scheduled_at = scheduled_at;
        record.last_error = Some(last_error);
        record.locked_at = None;
        record.locked_by = None;
        record.lock_version += 1;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn list_failed(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut failed: Vec<_> = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.status == WebhookOutboxStatus::Failed)
            .cloned()
            .collect();
        failed.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        failed.truncate(limit as usize);
        Ok(failed)
    }

    async fn replay_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookOutboxRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::StorageError(format!("Outbox record {} not found", id))
        })?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} not found for tenant {}",
                id, tenant_id
            )));
        }
        if record.status != WebhookOutboxStatus::Failed {
            return Err(IntentRebaseError::StorageError(format!(
                "Outbox record {} is not failed (status: {:?})",
                id, record.status
            )));
        }
        record.status = WebhookOutboxStatus::Pending;
        record.attempt_count = 0;
        record.scheduled_at = Utc::now();
        record.last_error = None;
        record.locked_at = None;
        record.locked_by = None;
        record.lock_version += 1;
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn list_failed_older_than(
        &self,
        tenant_id: Uuid,
        before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut failed: Vec<_> = records
            .values()
            .filter(|r| {
                r.tenant_id == tenant_id
                    && r.status == WebhookOutboxStatus::Failed
                    && r.updated_at < before
            })
            .cloned()
            .collect();
        failed.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        failed.truncate(limit as usize);
        Ok(failed)
    }

    async fn list_distinct_pending_tenants(&self) -> Result<Vec<Uuid>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut tenants: Vec<Uuid> = records
            .values()
            .filter(|r| r.status == WebhookOutboxStatus::Pending)
            .map(|r| r.tenant_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        tenants.sort();
        Ok(tenants)
    }
}

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
                lock_version, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14, $15,
                $16, $17, $18
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
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND status = 'failed'
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
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
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(
        tenant_id: Uuid,
        intent_id: Uuid,
        subscription_id: Uuid,
    ) -> WebhookOutboxRecord {
        WebhookOutboxRecord::new(
            tenant_id,
            intent_id,
            subscription_id,
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
            None,
        )
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();
        let record = sample_record(tenant, intent, sub);

        let created = repo.create(record.clone()).await.unwrap();
        assert_eq!(created.id, record.id);

        let fetched = repo.get(record.id, tenant).await.unwrap();
        assert_eq!(fetched.status, WebhookOutboxStatus::Pending);
        assert_eq!(fetched.lock_version, 0);
    }

    #[tokio::test]
    async fn test_get_wrong_tenant() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let record = sample_record(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let err = repo.get(record.id, Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(err, IntentRebaseError::StorageError(_)));
    }

    #[tokio::test]
    async fn test_list_pending() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();

        let r1 = sample_record(tenant, intent, sub);
        let r2 = sample_record(tenant, intent, sub);
        repo.create(r1.clone()).await.unwrap();
        repo.create(r2.clone()).await.unwrap();

        let pending = repo.list_pending(tenant, 10).await.unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_claim() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let claimed = repo
            .claim(record.id, tenant, "worker-1".to_string())
            .await
            .unwrap();
        assert_eq!(claimed.status, WebhookOutboxStatus::Claimed);
        assert_eq!(claimed.lock_version, 1);
        assert_eq!(claimed.locked_by, Some("worker-1".to_string()));
    }

    #[tokio::test]
    async fn test_claim_non_pending_fails() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant, "boom".to_string())
            .await
            .unwrap();

        let err = repo
            .claim(record.id, tenant, "worker-1".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, IntentRebaseError::StorageError(_)));
    }

    #[tokio::test]
    async fn test_mark_delivered() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let delivered = repo.mark_delivered(record.id, tenant).await.unwrap();
        assert_eq!(delivered.status, WebhookOutboxStatus::Delivered);
        assert!(delivered.delivered_at.is_some());
        assert_eq!(delivered.lock_version, 1);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let failed = repo
            .mark_failed(record.id, tenant, "timeout".to_string())
            .await
            .unwrap();
        assert_eq!(failed.status, WebhookOutboxStatus::Failed);
        assert_eq!(failed.last_error, Some("timeout".to_string()));
        assert_eq!(failed.lock_version, 1);
    }

    #[tokio::test]
    async fn test_reschedule_retry() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.claim(record.id, tenant, "worker-1".to_string())
            .await
            .unwrap();

        let future = Utc::now() + chrono::Duration::seconds(60);
        let rescheduled = repo
            .reschedule_retry(record.id, tenant, "network timeout".to_string(), future)
            .await
            .unwrap();
        assert_eq!(rescheduled.status, WebhookOutboxStatus::Pending);
        assert_eq!(rescheduled.attempt_count, 1);
        assert_eq!(rescheduled.last_error, Some("network timeout".to_string()));
        assert_eq!(rescheduled.scheduled_at, future);
        assert_eq!(rescheduled.locked_at, None);
        assert_eq!(rescheduled.locked_by, None);
        assert_eq!(rescheduled.lock_version, 2); // claim + reschedule
    }

    #[tokio::test]
    async fn test_list_failed() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();

        let r1 = sample_record(tenant, intent, sub);
        let r2 = sample_record(tenant, intent, sub);
        let r3 = sample_record(other_tenant, intent, sub);

        repo.create(r1.clone()).await.unwrap();
        repo.create(r2.clone()).await.unwrap();
        repo.create(r3.clone()).await.unwrap();

        repo.mark_failed(r1.id, tenant, "boom".to_string())
            .await
            .unwrap();
        repo.mark_failed(r3.id, other_tenant, "bang".to_string())
            .await
            .unwrap();

        let failed = repo.list_failed(tenant, 10).await.unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, r1.id);
        assert_eq!(failed[0].status, WebhookOutboxStatus::Failed);
    }

    #[tokio::test]
    async fn test_replay_failed() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant, "timeout".to_string())
            .await
            .unwrap();

        let replayed = repo.replay_failed(record.id, tenant).await.unwrap();
        assert_eq!(replayed.status, WebhookOutboxStatus::Pending);
        assert_eq!(replayed.attempt_count, 0);
        assert_eq!(replayed.last_error, None);
        assert_eq!(replayed.locked_at, None);
        assert_eq!(replayed.locked_by, None);
        assert_eq!(replayed.lock_version, 2); // create + mark_failed + replay
    }

    #[tokio::test]
    async fn test_replay_failed_idempotency() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant, "timeout".to_string())
            .await
            .unwrap();

        // First replay succeeds
        let _ = repo.replay_failed(record.id, tenant).await.unwrap();

        // Second replay fails because status is no longer Failed
        let err = repo.replay_failed(record.id, tenant).await.unwrap_err();
        assert!(matches!(err, IntentRebaseError::StorageError(_)));
    }

    #[tokio::test]
    async fn test_replay_non_failed_fails() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        // Record is still Pending, not Failed
        let err = repo.replay_failed(record.id, tenant).await.unwrap_err();
        assert!(matches!(err, IntentRebaseError::StorageError(_)));
    }

    #[tokio::test]
    async fn test_list_failed_older_than_includes_stale() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant, "timeout".to_string())
            .await
            .unwrap();

        let before = Utc::now() + chrono::Duration::seconds(1);
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let results = repo
            .list_failed_older_than(tenant, before, 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, record.id);
    }

    #[tokio::test]
    async fn test_list_failed_older_than_excludes_recent() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant, "boom".to_string())
            .await
            .unwrap();

        let before = Utc::now() - chrono::Duration::seconds(1);
        let results = repo
            .list_failed_older_than(tenant, before, 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_list_failed_older_than_excludes_non_failed() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let before = Utc::now() + chrono::Duration::seconds(1);
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let results = repo
            .list_failed_older_than(tenant, before, 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_list_failed_older_than_tenant_boundary() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let record = sample_record(tenant_a, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();
        repo.mark_failed(record.id, tenant_a, "err".to_string())
            .await
            .unwrap();

        let before = Utc::now() + chrono::Duration::seconds(1);
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let results = repo
            .list_failed_older_than(tenant_b, before, 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_list_failed_older_than_limit() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let r1 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        let r2 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(r1.clone()).await.unwrap();
        repo.create(r2.clone()).await.unwrap();
        repo.mark_failed(r1.id, tenant, "e1".to_string())
            .await
            .unwrap();
        repo.mark_failed(r2.id, tenant, "e2".to_string())
            .await
            .unwrap();

        let before = Utc::now() + chrono::Duration::seconds(1);
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        let results = repo
            .list_failed_older_than(tenant, before, 1)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_status_serialization() {
        let statuses = vec![
            WebhookOutboxStatus::Pending,
            WebhookOutboxStatus::Claimed,
            WebhookOutboxStatus::Delivered,
            WebhookOutboxStatus::Failed,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let roundtrip: WebhookOutboxStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, roundtrip);
        }
    }

    #[tokio::test]
    async fn test_list_distinct_pending_tenants() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();

        let r1 = sample_record(tenant_a, intent, sub);
        let r2 = sample_record(tenant_a, intent, sub);
        let r3 = sample_record(tenant_b, intent, sub);

        repo.create(r1.clone()).await.unwrap();
        repo.create(r2.clone()).await.unwrap();
        repo.create(r3.clone()).await.unwrap();

        let tenants = repo.list_distinct_pending_tenants().await.unwrap();
        assert_eq!(tenants.len(), 2);
        assert!(tenants.contains(&tenant_a));
        assert!(tenants.contains(&tenant_b));

        // Mark one tenant's records as delivered
        repo.mark_delivered(r1.id, tenant_a).await.unwrap();
        repo.mark_delivered(r2.id, tenant_a).await.unwrap();

        let tenants = repo.list_distinct_pending_tenants().await.unwrap();
        assert_eq!(tenants.len(), 1);
        assert!(tenants.contains(&tenant_b));
    }

    #[test]
    fn test_with_webhook_url() {
        let tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();
        let record = WebhookOutboxRecord::new(
            tenant,
            intent,
            sub,
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
            None,
        )
        .with_webhook_url("https://example.com/hook");
        assert_eq!(
            record.webhook_url,
            Some("https://example.com/hook".to_string())
        );
    }

    #[tokio::test]
    async fn test_in_memory_repo_preserves_webhook_url() {
        let repo = InMemoryWebhookOutboxRepository::new();
        let tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();
        let record = WebhookOutboxRecord::new(
            tenant,
            intent,
            sub,
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
            Some("https://example.com/hook".to_string()),
        );

        let created = repo.create(record.clone()).await.unwrap();
        assert_eq!(
            created.webhook_url,
            Some("https://example.com/hook".to_string())
        );

        let fetched = repo.get(record.id, tenant).await.unwrap();
        assert_eq!(
            fetched.webhook_url,
            Some("https://example.com/hook".to_string())
        );
    }

    /// Smoke test for `SqlxWebhookOutboxRepository`.
    ///
    /// Ignored by default so `cargo test` does not require live Postgres.
    /// Run manually with:
    ///   DATABASE_URL=postgres://... cargo test -p intent-api --lib webhook_outbox_repo -- --ignored
    #[tokio::test]
    #[ignore = "requires live Postgres (set DATABASE_URL to run)"]
    async fn test_sqlx_repo_smoke() {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("Skipping SQLx smoke test: DATABASE_URL not set");
                return;
            }
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect failed");
        let repo = SqlxWebhookOutboxRepository::new(pool);
        let tenant_id = Uuid::new_v4();
        let pending = repo
            .list_pending(tenant_id, 1)
            .await
            .expect("list_pending failed");
        assert!(pending.is_empty());
    }

    /// Smoke test for `SqlxWebhookOutboxRepository` DLQ list and replay.
    ///
    /// Ignored by default so `cargo test` does not require live Postgres.
    /// Run manually with:
    ///   DATABASE_URL=postgres://... cargo test -p intent-api --lib webhook_outbox_repo -- --ignored
    #[tokio::test]
    #[ignore = "requires live Postgres (set DATABASE_URL to run)"]
    async fn test_sqlx_repo_dlq_smoke() {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("Skipping SQLx DLQ smoke test: DATABASE_URL not set");
                return;
            }
        };
        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect failed");
        let repo = SqlxWebhookOutboxRepository::new(pool);
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();

        // Seed a record and mark it failed
        let record = WebhookOutboxRecord::new(
            tenant_id,
            intent_id,
            subscription_id,
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
            None,
        );
        let created = repo.create(record.clone()).await.expect("create failed");
        repo.mark_failed(created.id, tenant_id, "timeout".to_string())
            .await
            .expect("mark_failed failed");

        // list_failed should return the record
        let failed = repo
            .list_failed(tenant_id, 10)
            .await
            .expect("list_failed failed");
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, created.id);
        assert_eq!(failed[0].status, WebhookOutboxStatus::Failed);

        // replay_failed should transition to Pending
        let replayed = repo
            .replay_failed(created.id, tenant_id)
            .await
            .expect("replay_failed failed");
        assert_eq!(replayed.status, WebhookOutboxStatus::Pending);
        assert_eq!(replayed.attempt_count, 0);
        assert_eq!(replayed.last_error, None);

        // list_failed should now be empty
        let failed = repo
            .list_failed(tenant_id, 10)
            .await
            .expect("list_failed failed");
        assert!(failed.is_empty());
    }
}
