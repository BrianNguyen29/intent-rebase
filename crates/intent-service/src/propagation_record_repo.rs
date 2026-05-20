//! Propagation record repository for propagation-status Slice 1
//!
//! Provides storage for propagation_records table entries that track
//! downstream system propagation status for intent changes.
//!
//! Bounded Slice 1: Only schema + types + repository + query helpers.
//! Webhook delivery, event streaming, and cross-workflow lineage are deferred.

use async_trait::async_trait;
use intent_rebase_types::{IntentRebaseError, PropagationRecord};
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for propagation record storage
#[async_trait]
pub trait PropagationRecordRepository: Send + Sync {
    /// Create a new propagation record
    async fn create_record(
        &self,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError>;

    /// Create a new propagation record within an existing RLS-aware transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    /// In-memory implementations delegate to `create_record` and ignore the transaction.
    async fn create_record_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError>;

    /// Get a propagation record by ID (tenant-scoped)
    async fn get_record(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError>;

    /// List propagation records for an intent (tenant-scoped)
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PropagationRecord>, IntentRebaseError>;

    /// Update a propagation record's status
    async fn update_status(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        last_seen_version: i32,
    ) -> Result<PropagationRecord, IntentRebaseError>;

    /// Record a delivery attempt — increments delivery_attempt_count,
    /// sets last_delivery_attempt_at, and increments lock_version.
    async fn record_delivery_attempt(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError>;

    /// Record a delivery outcome — updates status, timestamps, failure_reason,
    /// and increments lock_version.
    async fn record_delivery_outcome(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        failure_reason: Option<String>,
    ) -> Result<PropagationRecord, IntentRebaseError>;
}

/// In-memory propagation record repository for Slice 1 bounded testing
pub struct InMemoryPropagationRecordRepository {
    records: RwLock<HashMap<Uuid, PropagationRecord>>,
    by_intent: RwLock<HashMap<(Uuid, Uuid), Vec<Uuid>>>, // (tenant_id, intent_id) -> record_ids
}

impl InMemoryPropagationRecordRepository {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryPropagationRecordRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PropagationRecordRepository for InMemoryPropagationRecordRepository {
    async fn create_record(
        &self,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let mut by_intent = self.by_intent.write().await;

        records.insert(record.id, record.clone());
        by_intent
            .entry((record.tenant_id, record.intent_id))
            .or_insert_with(Vec::new)
            .push(record.id);

        Ok(record)
    }

    async fn create_record_with_tx(
        &self,
        _tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        // In-memory implementation delegates to create_record and ignores the transaction.
        self.create_record(record).await
    }

    async fn get_record(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let records = self.records.read().await;
        records
            .get(&id)
            .cloned()
            .filter(|r| r.tenant_id == tenant_id)
            .ok_or(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            )))
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PropagationRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent
            .get(&(tenant_id, intent_id))
            .cloned()
            .unwrap_or_default();

        let mut result: Vec<PropagationRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect();

        // Sort by updated_at descending (newest first)
        result.sort_by_key(|r| std::cmp::Reverse(r.updated_at));

        Ok(result)
    }

    async fn update_status(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        last_seen_version: i32,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let mut records = self.records.write().await;

        let record = records
            .get_mut(&id)
            .filter(|r| r.tenant_id == tenant_id)
            .ok_or(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            )))?;

        record.status = status;
        record.last_seen_version = last_seen_version;
        record.updated_at = chrono::Utc::now();
        record.lock_version += 1;

        match record.status {
            intent_rebase_types::PropagationStatus::Acknowledged => {
                record.acknowledged_at = Some(chrono::Utc::now());
                record.failed_at = None;
            }
            intent_rebase_types::PropagationStatus::Failed => {
                record.failed_at = Some(chrono::Utc::now());
                record.acknowledged_at = None;
            }
            intent_rebase_types::PropagationStatus::Pending => {
                record.acknowledged_at = None;
                record.failed_at = None;
            }
        }

        Ok(record.clone())
    }

    async fn record_delivery_attempt(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let mut records = self.records.write().await;

        let record = records
            .get_mut(&id)
            .filter(|r| r.tenant_id == tenant_id)
            .ok_or(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            )))?;

        record.delivery_attempt_count += 1;
        record.last_delivery_attempt_at = Some(chrono::Utc::now());
        record.lock_version += 1;
        record.updated_at = chrono::Utc::now();

        Ok(record.clone())
    }

    async fn record_delivery_outcome(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        failure_reason: Option<String>,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let mut records = self.records.write().await;

        let record = records
            .get_mut(&id)
            .filter(|r| r.tenant_id == tenant_id)
            .ok_or(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            )))?;

        record.status = status;
        record.failure_reason = failure_reason;
        record.lock_version += 1;
        record.updated_at = chrono::Utc::now();

        match record.status {
            intent_rebase_types::PropagationStatus::Acknowledged => {
                record.acknowledged_at = Some(chrono::Utc::now());
                record.failed_at = None;
            }
            intent_rebase_types::PropagationStatus::Failed => {
                record.failed_at = Some(chrono::Utc::now());
                record.acknowledged_at = None;
            }
            intent_rebase_types::PropagationStatus::Pending => {
                record.acknowledged_at = None;
                record.failed_at = None;
            }
        }

        Ok(record.clone())
    }
}

/// SQL-backed propagation record repository for Slice 2 bounded implementation
pub struct SqlxPropagationRecordRepository {
    pool: sqlx::PgPool,
}

impl SqlxPropagationRecordRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PropagationRecordRepository for SqlxPropagationRecordRepository {
    async fn create_record(
        &self,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            INSERT INTO propagation_records (
                id, tenant_id, intent_id, downstream_system_id, status,
                last_seen_version, signaled_at, acknowledged_at, failed_at,
                failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                lock_version, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, intent_id, downstream_system_id, status,
                      last_seen_version, signaled_at, acknowledged_at, failed_at,
                      failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                      lock_version, created_at, updated_at
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.intent_id)
        .bind(&record.downstream_system_id)
        .bind(format!("{:?}", record.status).to_lowercase())
        .bind(record.last_seen_version)
        .bind(record.signaled_at)
        .bind(record.acknowledged_at)
        .bind(record.failed_at)
        .bind(record.failure_reason.as_deref().unwrap_or(""))
        .bind(record.delivery_attempt_count)
        .bind(record.last_delivery_attempt_at)
        .bind(record.lock_version)
        .bind(record.created_at)
        .bind(record.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to create propagation record: {}", e))
        })?;

        Ok(map_row_to_record(row))
    }

    async fn create_record_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        record: PropagationRecord,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            INSERT INTO propagation_records (
                id, tenant_id, intent_id, downstream_system_id, status,
                last_seen_version, signaled_at, acknowledged_at, failed_at,
                failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                lock_version, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, intent_id, downstream_system_id, status,
                      last_seen_version, signaled_at, acknowledged_at, failed_at,
                      failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                      lock_version, created_at, updated_at
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.intent_id)
        .bind(&record.downstream_system_id)
        .bind(format!("{:?}", record.status).to_lowercase())
        .bind(record.last_seen_version)
        .bind(record.signaled_at)
        .bind(record.acknowledged_at)
        .bind(record.failed_at)
        .bind(record.failure_reason.as_deref().unwrap_or(""))
        .bind(record.delivery_attempt_count)
        .bind(record.last_delivery_attempt_at)
        .bind(record.lock_version)
        .bind(record.created_at)
        .bind(record.updated_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "Failed to create propagation record in tx: {}",
                e
            ))
        })?;

        Ok(map_row_to_record(row))
    }

    async fn get_record(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, downstream_system_id, status,
                   last_seen_version, signaled_at, acknowledged_at, failed_at,
                   failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                   lock_version, created_at, updated_at
            FROM propagation_records
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to get propagation record: {}", e))
        })?;

        match row {
            Some(row) => Ok(map_row_to_record(row)),
            None => Err(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            ))),
        }
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PropagationRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, downstream_system_id, status,
                   last_seen_version, signaled_at, acknowledged_at, failed_at,
                   failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                   lock_version, created_at, updated_at
            FROM propagation_records
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY updated_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to list propagation records: {}", e))
        })?;

        Ok(rows.into_iter().map(map_row_to_record).collect())
    }

    async fn update_status(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        last_seen_version: i32,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let now = chrono::Utc::now();
        let (acknowledged_at, failed_at) = match status {
            intent_rebase_types::PropagationStatus::Acknowledged => (Some(now), None),
            intent_rebase_types::PropagationStatus::Failed => (None, Some(now)),
            intent_rebase_types::PropagationStatus::Pending => (None, None),
        };

        let row = sqlx::query(
            r#"
            UPDATE propagation_records
            SET status = $1,
                last_seen_version = $2,
                acknowledged_at = $3,
                failed_at = $4,
                updated_at = $5,
                lock_version = lock_version + 1
            WHERE id = $6 AND tenant_id = $7
            RETURNING id, tenant_id, intent_id, downstream_system_id, status,
                      last_seen_version, signaled_at, acknowledged_at, failed_at,
                      failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                      lock_version, created_at, updated_at
            "#,
        )
        .bind(format!("{:?}", status).to_lowercase())
        .bind(last_seen_version)
        .bind(acknowledged_at)
        .bind(failed_at)
        .bind(now)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to update propagation record: {}", e))
        })?;

        match row {
            Some(row) => Ok(map_row_to_record(row)),
            None => Err(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            ))),
        }
    }

    async fn record_delivery_attempt(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let now = chrono::Utc::now();

        let row = sqlx::query(
            r#"
            UPDATE propagation_records
            SET delivery_attempt_count = delivery_attempt_count + 1,
                last_delivery_attempt_at = $1,
                updated_at = $1,
                lock_version = lock_version + 1
            WHERE id = $2 AND tenant_id = $3
            RETURNING id, tenant_id, intent_id, downstream_system_id, status,
                      last_seen_version, signaled_at, acknowledged_at, failed_at,
                      failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                      lock_version, created_at, updated_at
            "#,
        )
        .bind(now)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to record delivery attempt: {}", e))
        })?;

        match row {
            Some(row) => Ok(map_row_to_record(row)),
            None => Err(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            ))),
        }
    }

    async fn record_delivery_outcome(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: intent_rebase_types::PropagationStatus,
        failure_reason: Option<String>,
    ) -> Result<PropagationRecord, IntentRebaseError> {
        let now = chrono::Utc::now();
        let (acknowledged_at, failed_at) = match status {
            intent_rebase_types::PropagationStatus::Acknowledged => (Some(now), None),
            intent_rebase_types::PropagationStatus::Failed => (None, Some(now)),
            intent_rebase_types::PropagationStatus::Pending => (None, None),
        };

        let row = sqlx::query(
            r#"
            UPDATE propagation_records
            SET status = $1,
                acknowledged_at = $2,
                failed_at = $3,
                failure_reason = $4,
                updated_at = $5,
                lock_version = lock_version + 1
            WHERE id = $6 AND tenant_id = $7
            RETURNING id, tenant_id, intent_id, downstream_system_id, status,
                      last_seen_version, signaled_at, acknowledged_at, failed_at,
                      failure_reason, delivery_attempt_count, last_delivery_attempt_at,
                      lock_version, created_at, updated_at
            "#,
        )
        .bind(format!("{:?}", status).to_lowercase())
        .bind(acknowledged_at)
        .bind(failed_at)
        .bind(failure_reason.as_deref().unwrap_or(""))
        .bind(now)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("Failed to record delivery outcome: {}", e))
        })?;

        match row {
            Some(row) => Ok(map_row_to_record(row)),
            None => Err(IntentRebaseError::StorageError(format!(
                "Propagation record not found: {}",
                id
            ))),
        }
    }
}

fn map_row_to_record(row: sqlx::postgres::PgRow) -> PropagationRecord {
    let status_str: String = row.get("status");
    let status = match status_str.as_str() {
        "acknowledged" => intent_rebase_types::PropagationStatus::Acknowledged,
        "failed" => intent_rebase_types::PropagationStatus::Failed,
        _ => intent_rebase_types::PropagationStatus::Pending,
    };

    PropagationRecord {
        id: row.get("id"),
        tenant_id: row.get("tenant_id"),
        intent_id: row.get("intent_id"),
        downstream_system_id: row.get("downstream_system_id"),
        status,
        last_seen_version: row.get("last_seen_version"),
        signaled_at: row.get("signaled_at"),
        acknowledged_at: row.get("acknowledged_at"),
        failed_at: row.get("failed_at"),
        failure_reason: row.get::<Option<String>, _>("failure_reason"),
        delivery_attempt_count: row.get("delivery_attempt_count"),
        last_delivery_attempt_at: row.get("last_delivery_attempt_at"),
        lock_version: row.get("lock_version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record() -> PropagationRecord {
        PropagationRecord::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "workflow-runner-a".to_string(),
        )
    }

    #[tokio::test]
    async fn test_create_and_get_record() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        let stored = repo.get_record(id, tenant_id).await.unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.downstream_system_id, "workflow-runner-a");
        assert_eq!(
            stored.status,
            intent_rebase_types::PropagationStatus::Pending
        );
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = InMemoryPropagationRecordRepository::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for i in 0..3 {
            let mut record = PropagationRecord::new(tenant_id, intent_id, format!("system-{}", i));
            // Stagger created_at to test sorting
            record.created_at = chrono::Utc::now() - chrono::Duration::seconds(i);
            record.updated_at = record.created_at;
            repo.create_record(record).await.unwrap();
        }

        let records = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(records.len(), 3);
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        let updated = repo
            .update_status(
                id,
                tenant_id,
                intent_rebase_types::PropagationStatus::Acknowledged,
                3,
            )
            .await
            .unwrap();

        assert_eq!(
            updated.status,
            intent_rebase_types::PropagationStatus::Acknowledged
        );
        assert_eq!(updated.last_seen_version, 3);
        assert!(updated.acknowledged_at.is_some());
        assert_eq!(updated.lock_version, 2);
    }

    #[tokio::test]
    async fn test_tenant_isolation() {
        let repo = InMemoryPropagationRecordRepository::new();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record1 = PropagationRecord::new(tenant_1, intent_id, "system-a".to_string());
        let record2 = PropagationRecord::new(tenant_2, intent_id, "system-b".to_string());

        repo.create_record(record1).await.unwrap();
        repo.create_record(record2).await.unwrap();

        let records_1 = repo.list_by_intent(intent_id, tenant_1).await.unwrap();
        assert_eq!(records_1.len(), 1);
        assert_eq!(records_1[0].downstream_system_id, "system-a");

        let records_2 = repo.list_by_intent(intent_id, tenant_2).await.unwrap();
        assert_eq!(records_2.len(), 1);
        assert_eq!(records_2[0].downstream_system_id, "system-b");
    }

    #[tokio::test]
    async fn test_record_delivery_attempt_increments_and_sets_timestamp() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        let before = chrono::Utc::now();
        let updated = repo.record_delivery_attempt(id, tenant_id).await.unwrap();
        let after = chrono::Utc::now();

        assert_eq!(updated.delivery_attempt_count, 1);
        assert!(updated.last_delivery_attempt_at.is_some());
        assert!(
            updated.last_delivery_attempt_at.unwrap() >= before
                && updated.last_delivery_attempt_at.unwrap() <= after
        );
        assert_eq!(updated.lock_version, 2);
    }

    #[tokio::test]
    async fn test_record_delivery_attempt_multiple_times() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        let first = repo.record_delivery_attempt(id, tenant_id).await.unwrap();
        let second = repo.record_delivery_attempt(id, tenant_id).await.unwrap();

        assert_eq!(first.delivery_attempt_count, 1);
        assert_eq!(second.delivery_attempt_count, 2);
        assert_eq!(second.lock_version, 3);
        assert!(
            second.last_delivery_attempt_at.unwrap() >= first.last_delivery_attempt_at.unwrap()
        );
    }

    #[tokio::test]
    async fn test_record_delivery_outcome_success() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();
        repo.record_delivery_attempt(id, tenant_id).await.unwrap();

        let updated = repo
            .record_delivery_outcome(
                id,
                tenant_id,
                intent_rebase_types::PropagationStatus::Acknowledged,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            updated.status,
            intent_rebase_types::PropagationStatus::Acknowledged
        );
        assert!(updated.acknowledged_at.is_some());
        assert!(updated.failed_at.is_none());
        assert_eq!(updated.failure_reason, None);
        assert_eq!(updated.lock_version, 3);
    }

    #[tokio::test]
    async fn test_record_delivery_outcome_failure() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        let updated = repo
            .record_delivery_outcome(
                id,
                tenant_id,
                intent_rebase_types::PropagationStatus::Failed,
                Some("timeout".to_string()),
            )
            .await
            .unwrap();

        assert_eq!(
            updated.status,
            intent_rebase_types::PropagationStatus::Failed
        );
        assert!(updated.failed_at.is_some());
        assert!(updated.acknowledged_at.is_none());
        assert_eq!(updated.failure_reason, Some("timeout".to_string()));
        assert_eq!(updated.lock_version, 2);
    }

    #[tokio::test]
    async fn test_record_delivery_attempt_wrong_tenant_not_found() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;

        repo.create_record(record).await.unwrap();

        let wrong_tenant = Uuid::new_v4();
        let result = repo.record_delivery_attempt(id, wrong_tenant).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_record_delivery_outcome_wrong_tenant_not_found() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;

        repo.create_record(record).await.unwrap();

        let wrong_tenant = Uuid::new_v4();
        let result = repo
            .record_delivery_outcome(
                id,
                wrong_tenant,
                intent_rebase_types::PropagationStatus::Acknowledged,
                None,
            )
            .await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_record_delivery_outcome_overwrites_failure_reason() {
        let repo = InMemoryPropagationRecordRepository::new();
        let record = create_test_record();
        let id = record.id;
        let tenant_id = record.tenant_id;

        repo.create_record(record).await.unwrap();

        // First fail
        repo.record_delivery_outcome(
            id,
            tenant_id,
            intent_rebase_types::PropagationStatus::Failed,
            Some("first failure".to_string()),
        )
        .await
        .unwrap();

        // Then succeed — failure_reason should be overwritten (kept as provided)
        let updated = repo
            .record_delivery_outcome(
                id,
                tenant_id,
                intent_rebase_types::PropagationStatus::Acknowledged,
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            updated.status,
            intent_rebase_types::PropagationStatus::Acknowledged
        );
        assert_eq!(updated.failure_reason, None);
    }
}
