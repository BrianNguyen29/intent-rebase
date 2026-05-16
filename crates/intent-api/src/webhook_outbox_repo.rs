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
}
