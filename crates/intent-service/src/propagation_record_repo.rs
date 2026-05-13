//! Propagation record repository for propagation-status Slice 1
//!
//! Provides storage for propagation_records table entries that track
//! downstream system propagation status for intent changes.
//!
//! Bounded Slice 1: Only schema + types + repository + query helpers.
//! Webhook delivery, event streaming, and cross-workflow lineage are deferred.

use async_trait::async_trait;
use intent_rebase_types::{IntentRebaseError, PropagationRecord};
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
}
