use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// Number of DLQ replays performed
    pub replay_count: i32,
    /// Timestamp of the most recent DLQ replay
    pub replayed_at: Option<DateTime<Utc>>,
    /// Actor identity for the most recent DLQ replay
    pub replayed_by: Option<String>,
    /// Optimistic locking version
    pub lock_version: i32,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

/// Error summary for DLQ stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookOutboxDlqErrorSummary {
    pub error_pattern: String,
    pub count: i64,
}

/// DLQ stats for a tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookOutboxDlqStats {
    /// Number of failed records for this tenant.
    pub total_failed: i64,
    /// Age in seconds of the oldest failed record (based on updated_at).
    pub oldest_failed_age_seconds: Option<i64>,
    /// Number of records that have been replayed at least once.
    pub replayed_count: i64,
    /// Grouped error summary for failed records.
    pub by_error_summary: Vec<WebhookOutboxDlqErrorSummary>,
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
            replay_count: 0,
            replayed_at: None,
            replayed_by: None,
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
    ///
    /// Phase 1.2: increments replay_count, sets replayed_at/replayed_by on successful replay.
    async fn replay_failed(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        replayed_by: Option<String>,
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

    /// List replayed records for a tenant, ordered by `replayed_at` desc then id.
    ///
    /// Phase 1.3: returns records with `replay_count > 0` and `replayed_at` present,
    /// tenant-scoped, with optional `since` cutoff. This is a query-only local-dev
    /// helper — no production audit trail claim.
    async fn list_replayed(
        &self,
        tenant_id: Uuid,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError>;

    /// Compute DLQ stats for a tenant.
    ///
    /// Phase 2.3: bounded local-dev stats query — returns counts and age summary
    /// for failed/replayed records, plus a grouped error summary. No production
    /// dashboard or automated remediation claim.
    async fn dlq_stats(&self, tenant_id: Uuid) -> Result<WebhookOutboxDlqStats, IntentRebaseError>;
}
