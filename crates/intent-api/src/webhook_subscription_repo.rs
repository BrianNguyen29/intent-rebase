//! Webhook subscription repository (Slice 4b — bounded local-dev subscription CRUD)
//!
//! Provides per-intent webhook subscription storage with create, list, get,
//! update, and soft-delete operations.
//!
//! Bounded scope: in-memory implementation is primary; SQLx skeleton is
//! provided for local-dev but is NOT production-ready. No retry/DLQ,
//! no secret manager, no tenant-scoped pattern matching.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 4b)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

// =============================================================================
// Domain Types
// =============================================================================

/// Webhook subscription record — per-intent subscription for webhook delivery.
///
/// Mirrors the `webhook_subscriptions` table (migration 020) with Slice 4a
/// fields: `status`, `max_attempts`, `event_types`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscriptionRecord {
    /// Unique record identifier (primary key)
    pub id: Uuid,
    /// Tenant this subscription belongs to
    pub tenant_id: Uuid,
    /// Intent this subscription is scoped to
    pub intent_id: Uuid,
    /// Public subscription identifier (stable external reference)
    pub subscription_id: Uuid,
    /// Target webhook URL
    pub webhook_url: String,
    /// Optional downstream system identifier for propagation matching
    pub downstream_system_id: Option<String>,
    /// Subscription lifecycle status: active, paused, disabled, deleted
    pub status: String,
    /// Max delivery attempts for this subscription
    pub max_attempts: i32,
    /// Event types this subscription receives
    pub event_types: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl WebhookSubscriptionRecord {
    /// Create a new active subscription record.
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        subscription_id: Uuid,
        webhook_url: String,
        downstream_system_id: Option<String>,
        event_types: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            subscription_id,
            webhook_url,
            downstream_system_id,
            status: "active".to_string(),
            max_attempts: 3,
            event_types,
            created_at: now,
            updated_at: now,
        }
    }

    /// Builder-style helper to set max_attempts.
    pub fn with_max_attempts(mut self, max_attempts: i32) -> Self {
        self.max_attempts = max_attempts;
        self
    }
}

// =============================================================================
// Repository Trait
// =============================================================================

#[async_trait]
pub trait WebhookSubscriptionRepository: Send + Sync {
    /// Create a new subscription record.
    async fn create(
        &self,
        record: WebhookSubscriptionRecord,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError>;

    /// List subscriptions by intent (tenant-scoped).
    async fn list_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscriptionRecord>, IntentRebaseError>;

    /// Get a subscription by ID (tenant-scoped).
    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError>;

    /// Update allowed fields on a subscription.
    ///
    /// Allowed fields: `webhook_url`, `downstream_system_id`, `status`,
    /// `max_attempts`, `event_types`. Other fields are ignored.
    #[allow(clippy::too_many_arguments)]
    async fn update(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        webhook_url: Option<String>,
        downstream_system_id: Option<Option<String>>,
        status: Option<String>,
        max_attempts: Option<i32>,
        event_types: Option<Vec<String>>,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError>;

    /// Soft-delete a subscription by setting status to `deleted`.
    async fn soft_delete(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError>;
}

// =============================================================================
// In-Memory Implementation (primary — DB-free tests)
// =============================================================================

pub struct InMemoryWebhookSubscriptionRepository {
    records: RwLock<HashMap<Uuid, WebhookSubscriptionRecord>>,
}

impl InMemoryWebhookSubscriptionRepository {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryWebhookSubscriptionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebhookSubscriptionRepository for InMemoryWebhookSubscriptionRepository {
    async fn create(
        &self,
        record: WebhookSubscriptionRecord,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        if records.contains_key(&record.id) {
            return Err(IntentRebaseError::StorageError(format!(
                "Webhook subscription {} already exists",
                record.id
            )));
        }
        records.insert(record.id, record.clone());
        Ok(record)
    }

    async fn list_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscriptionRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut list: Vec<_> = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.intent_id == intent_id)
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(list)
    }

    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let records = self.records.read().await;
        let record = records
            .get(&id)
            .ok_or(IntentRebaseError::WebhookSubscriptionNotFound(id))?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::WebhookSubscriptionNotFound(id));
        }
        Ok(record.clone())
    }

    async fn update(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        webhook_url: Option<String>,
        downstream_system_id: Option<Option<String>>,
        status: Option<String>,
        max_attempts: Option<i32>,
        event_types: Option<Vec<String>>,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records
            .get_mut(&id)
            .ok_or(IntentRebaseError::WebhookSubscriptionNotFound(id))?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::WebhookSubscriptionNotFound(id));
        }
        if let Some(url) = webhook_url {
            record.webhook_url = url;
        }
        if let Some(dsid) = downstream_system_id {
            record.downstream_system_id = dsid;
        }
        if let Some(s) = status {
            record.status = s;
        }
        if let Some(ma) = max_attempts {
            record.max_attempts = ma;
        }
        if let Some(et) = event_types {
            record.event_types = et;
        }
        record.updated_at = Utc::now();
        Ok(record.clone())
    }

    async fn soft_delete(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let record = records
            .get_mut(&id)
            .ok_or(IntentRebaseError::WebhookSubscriptionNotFound(id))?;
        if record.tenant_id != tenant_id {
            return Err(IntentRebaseError::WebhookSubscriptionNotFound(id));
        }
        record.status = "deleted".to_string();
        record.updated_at = Utc::now();
        Ok(record.clone())
    }
}

// =============================================================================
// SQLx Implementation (local-dev skeleton)
// =============================================================================

/// SQL-backed webhook subscription repository.
///
/// Bounded local-dev skeleton: uses `sqlx::query` (not `query!`) so no
/// compile-time DB is required.
///
/// **Non-production caveat:** This repository is local-dev only.
/// Production readiness requires subscription validation, secret manager,
/// and tenant-scoped pattern matching — all deferred.
pub struct SqlxWebhookSubscriptionRepository {
    pool: sqlx::PgPool,
}

impl SqlxWebhookSubscriptionRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookSubscriptionRepository for SqlxWebhookSubscriptionRepository {
    async fn create(
        &self,
        record: WebhookSubscriptionRecord,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        sqlx::query(
            r#"
            INSERT INTO webhook_subscriptions (
                id, tenant_id, intent_id, subscription_id, webhook_url,
                downstream_system_id, status, max_attempts, event_types,
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11
            )
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.intent_id)
        .bind(record.subscription_id)
        .bind(&record.webhook_url)
        .bind(record.downstream_system_id.as_ref())
        .bind(&record.status)
        .bind(record.max_attempts)
        .bind(&record.event_types)
        .bind(record.created_at)
        .bind(record.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("insert webhook subscription: {}", e))
        })?;

        Ok(record)
    }

    async fn list_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscriptionRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, subscription_id, webhook_url,
                   downstream_system_id, status, max_attempts, event_types,
                   created_at, updated_at
            FROM webhook_subscriptions
            WHERE tenant_id = $1 AND intent_id = $2
            ORDER BY created_at, id
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list webhook subscriptions: {}", e))
        })?;

        rows.iter().map(map_row).collect()
    }

    async fn get(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, subscription_id, webhook_url,
                   downstream_system_id, status, max_attempts, event_types,
                   created_at, updated_at
            FROM webhook_subscriptions
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::WebhookSubscriptionNotFound(id),
            _ => IntentRebaseError::StorageError(format!("get webhook subscription: {}", e)),
        })?;

        map_row(&row)
    }

    async fn update(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        webhook_url: Option<String>,
        downstream_system_id: Option<Option<String>>,
        status: Option<String>,
        max_attempts: Option<i32>,
        event_types: Option<Vec<String>>,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        // Build a dynamic update — always touch updated_at
        let mut set_clauses = vec!["updated_at = NOW()".to_string()];
        if webhook_url.is_some() {
            set_clauses.push("webhook_url = $3".to_string());
        }
        if downstream_system_id.is_some() {
            set_clauses.push("downstream_system_id = $4".to_string());
        }
        if status.is_some() {
            set_clauses.push("status = $5".to_string());
        }
        if max_attempts.is_some() {
            set_clauses.push("max_attempts = $6".to_string());
        }
        if event_types.is_some() {
            set_clauses.push("event_types = $7".to_string());
        }

        let sql = format!(
            r#"
            UPDATE webhook_subscriptions
            SET {}
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, tenant_id, intent_id, subscription_id, webhook_url,
                      downstream_system_id, status, max_attempts, event_types,
                      created_at, updated_at
            "#,
            set_clauses.join(", ")
        );

        let mut query = sqlx::query(&sql).bind(id).bind(tenant_id);
        if let Some(v) = webhook_url {
            query = query.bind(v);
        }
        if let Some(v) = downstream_system_id {
            query = query.bind(v);
        }
        if let Some(v) = status {
            query = query.bind(v);
        }
        if let Some(v) = max_attempts {
            query = query.bind(v);
        }
        if let Some(v) = event_types {
            query = query.bind(v);
        }

        let row = query.fetch_one(&self.pool).await.map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::WebhookSubscriptionNotFound(id),
            _ => IntentRebaseError::StorageError(format!("update webhook subscription: {}", e)),
        })?;

        map_row(&row)
    }

    async fn soft_delete(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            UPDATE webhook_subscriptions
            SET status = 'deleted', updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, tenant_id, intent_id, subscription_id, webhook_url,
                      downstream_system_id, status, max_attempts, event_types,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => IntentRebaseError::WebhookSubscriptionNotFound(id),
            _ => {
                IntentRebaseError::StorageError(format!("soft-delete webhook subscription: {}", e))
            }
        })?;

        map_row(&row)
    }
}

fn map_err_column(col: &str, e: sqlx::Error) -> IntentRebaseError {
    IntentRebaseError::StorageError(format!("Invalid {} column: {}", col, e))
}

fn map_row(row: &sqlx::postgres::PgRow) -> Result<WebhookSubscriptionRecord, IntentRebaseError> {
    Ok(WebhookSubscriptionRecord {
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
        webhook_url: row
            .try_get("webhook_url")
            .map_err(|e| map_err_column("webhook_url", e))?,
        downstream_system_id: row.try_get("downstream_system_id").ok(),
        status: row
            .try_get("status")
            .map_err(|e| map_err_column("status", e))?,
        max_attempts: row
            .try_get("max_attempts")
            .map_err(|e| map_err_column("max_attempts", e))?,
        event_types: row
            .try_get("event_types")
            .map_err(|e| map_err_column("event_types", e))?,
        created_at: row
            .try_get("created_at")
            .map_err(|e| map_err_column("created_at", e))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|e| map_err_column("updated_at", e))?,
    })
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
    ) -> WebhookSubscriptionRecord {
        WebhookSubscriptionRecord::new(
            tenant_id,
            intent_id,
            subscription_id,
            "https://example.com/webhook".to_string(),
            Some("system-a".to_string()),
            vec!["intent_changed".to_string()],
        )
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let sub = Uuid::new_v4();
        let record = sample_record(tenant, intent, sub);

        let created = repo.create(record.clone()).await.unwrap();
        assert_eq!(created.id, record.id);
        assert_eq!(created.status, "active");
        assert_eq!(created.max_attempts, 3);

        let fetched = repo.get(record.id, tenant).await.unwrap();
        assert_eq!(fetched.subscription_id, sub);
    }

    #[tokio::test]
    async fn test_get_wrong_tenant() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let record = sample_record(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let err = repo.get(record.id, Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(
            err,
            IntentRebaseError::WebhookSubscriptionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant = Uuid::new_v4();
        let intent_a = Uuid::new_v4();
        let intent_b = Uuid::new_v4();

        let r1 = sample_record(tenant, intent_a, Uuid::new_v4());
        let r2 = sample_record(tenant, intent_a, Uuid::new_v4());
        let r3 = sample_record(tenant, intent_b, Uuid::new_v4());

        repo.create(r1.clone()).await.unwrap();
        repo.create(r2.clone()).await.unwrap();
        repo.create(r3.clone()).await.unwrap();

        let list = repo.list_by_intent(tenant, intent_a).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_intent_wrong_tenant() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let intent = Uuid::new_v4();

        let r1 = sample_record(tenant_a, intent, Uuid::new_v4());
        repo.create(r1.clone()).await.unwrap();

        let list = repo.list_by_intent(tenant_b, intent).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_update_allowed_fields() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let updated = repo
            .update(
                record.id,
                tenant,
                Some("https://new.example.com".to_string()),
                Some(None),
                Some("paused".to_string()),
                Some(5),
                Some(vec![
                    "intent_changed".to_string(),
                    "intent_deleted".to_string(),
                ]),
            )
            .await
            .unwrap();

        assert_eq!(updated.webhook_url, "https://new.example.com");
        assert_eq!(updated.downstream_system_id, None);
        assert_eq!(updated.status, "paused");
        assert_eq!(updated.max_attempts, 5);
        assert_eq!(
            updated.event_types,
            vec!["intent_changed", "intent_deleted"]
        );
    }

    #[tokio::test]
    async fn test_update_not_found() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant = Uuid::new_v4();
        let err = repo
            .update(
                Uuid::new_v4(),
                tenant,
                Some("https://example.com".to_string()),
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            IntentRebaseError::WebhookSubscriptionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_soft_delete() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let tenant = Uuid::new_v4();
        let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let deleted = repo.soft_delete(record.id, tenant).await.unwrap();
        assert_eq!(deleted.status, "deleted");

        let fetched = repo.get(record.id, tenant).await.unwrap();
        assert_eq!(fetched.status, "deleted");
    }

    #[tokio::test]
    async fn test_soft_delete_wrong_tenant() {
        let repo = InMemoryWebhookSubscriptionRepository::new();
        let record = sample_record(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        repo.create(record.clone()).await.unwrap();

        let err = repo
            .soft_delete(record.id, Uuid::new_v4())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            IntentRebaseError::WebhookSubscriptionNotFound(_)
        ));
    }

    /// Smoke test for `SqlxWebhookSubscriptionRepository`.
    ///
    /// Ignored by default so `cargo test` does not require live Postgres.
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
        let repo = SqlxWebhookSubscriptionRepository::new(pool);
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let list = repo
            .list_by_intent(tenant_id, intent_id)
            .await
            .expect("list_by_intent failed");
        assert!(list.is_empty());
    }
}
