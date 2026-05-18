use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::types::{
    WebhookOutboxDlqErrorSummary, WebhookOutboxDlqStats, WebhookOutboxRecord,
    WebhookOutboxRepository, WebhookOutboxStatus,
};

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
        replayed_by: Option<String>,
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
        record.replay_count += 1;
        record.replayed_at = Some(Utc::now());
        record.replayed_by = replayed_by;
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

    async fn list_replayed(
        &self,
        tenant_id: Uuid,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<WebhookOutboxRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let mut replayed: Vec<_> = records
            .values()
            .filter(|r| {
                r.tenant_id == tenant_id
                    && r.replay_count > 0
                    && r.replayed_at.is_some()
                    && since.is_none_or(|s| r.replayed_at.unwrap() >= s)
            })
            .cloned()
            .collect();
        replayed.sort_by(|a, b| {
            b.replayed_at
                .unwrap()
                .cmp(&a.replayed_at.unwrap())
                .then_with(|| a.id.cmp(&b.id))
        });
        replayed.truncate(limit as usize);
        Ok(replayed)
    }

    async fn dlq_stats(&self, tenant_id: Uuid) -> Result<WebhookOutboxDlqStats, IntentRebaseError> {
        let records = self.records.read().await;
        let now = Utc::now();

        let failed_records: Vec<_> = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.status == WebhookOutboxStatus::Failed)
            .collect();

        let total_failed = failed_records.len() as i64;

        let oldest_failed_age_seconds = failed_records
            .iter()
            .map(|r| r.updated_at)
            .min()
            .map(|oldest| (now - oldest).num_seconds().max(0));

        let replayed_count = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.replay_count > 0)
            .count() as i64;

        let mut error_counts: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for r in &failed_records {
            let key = r
                .last_error
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *error_counts.entry(key).or_insert(0) += 1;
        }
        let mut by_error_summary: Vec<WebhookOutboxDlqErrorSummary> = error_counts
            .into_iter()
            .map(|(error_pattern, count)| WebhookOutboxDlqErrorSummary {
                error_pattern,
                count,
            })
            .collect();
        by_error_summary.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.error_pattern.cmp(&b.error_pattern))
        });

        Ok(WebhookOutboxDlqStats {
            total_failed,
            oldest_failed_age_seconds,
            replayed_count,
            by_error_summary,
        })
    }
}
