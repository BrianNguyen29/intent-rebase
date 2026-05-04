//! Side effect rollback record repository trait and implementations
//!
//! Phase 3 Batch 1: Side effect rollback record persistence.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::rollback_record::{RollbackRecordResult, SideEffectRollbackRecord};

/// Repository trait for side effect rollback record storage.
/// Allows for in-memory (tests) or SQL-backed implementations.
#[async_trait]
pub trait RollbackRecordRepository: Send + Sync {
    /// Create a new rollback record.
    async fn create(
        &self,
        record: SideEffectRollbackRecord,
    ) -> Result<SideEffectRollbackRecord, IntentRebaseError>;

    /// Get a rollback record by its ID.
    async fn get(&self, record_id: Uuid) -> Result<SideEffectRollbackRecord, IntentRebaseError>;

    /// List rollback records for a given tenant.
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError>;

    /// List rollback records for a given compensation action.
    async fn list_by_compensation_action(
        &self,
        compensation_action_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError>;

    /// List rollback records for a given side effect.
    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError>;

    /// List rollback records for a given intent.
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError>;

    /// List rollback records by result type for a given tenant.
    async fn list_by_result(
        &self,
        tenant_id: Uuid,
        result: RollbackRecordResult,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxRollbackRecordRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxRollbackRecordRepository> {
        None
    }
}

// =============================================================================
// In-memory implementation
// =============================================================================

/// In-memory implementation for testing and Phase 3 Batch 1.
pub struct InMemoryRollbackRecordRepository {
    records: RwLock<HashMap<Uuid, SideEffectRollbackRecord>>,
    /// Secondary index: tenant_id -> list of record_ids
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: compensation_action_id -> list of record_ids
    by_compensation_action: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: side_effect_id -> list of record_ids
    by_side_effect: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: (tenant_id, intent_id) -> list of record_ids
    by_intent: RwLock<HashMap<(Uuid, Uuid), Vec<Uuid>>>,
    /// Secondary index: (tenant_id, result) -> list of record_ids
    by_result: RwLock<HashMap<(Uuid, RollbackRecordResult), Vec<Uuid>>>,
}

impl InMemoryRollbackRecordRepository {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
            by_compensation_action: RwLock::new(HashMap::new()),
            by_side_effect: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
            by_result: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryRollbackRecordRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RollbackRecordRepository for InMemoryRollbackRecordRepository {
    async fn create(
        &self,
        record: SideEffectRollbackRecord,
    ) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        let mut records = self.records.write().await;
        let mut by_tenant = self.by_tenant.write().await;
        let mut by_compensation_action = self.by_compensation_action.write().await;
        let mut by_side_effect = self.by_side_effect.write().await;
        let mut by_intent = self.by_intent.write().await;
        let mut by_result = self.by_result.write().await;

        // Store record
        records.insert(record.id, record.clone());

        // Index by tenant
        by_tenant
            .entry(record.tenant_id)
            .or_insert_with(Vec::new)
            .push(record.id);

        // Index by compensation action
        by_compensation_action
            .entry(record.compensation_action_id)
            .or_insert_with(Vec::new)
            .push(record.id);

        // Index by side effect
        by_side_effect
            .entry(record.side_effect_id)
            .or_insert_with(Vec::new)
            .push(record.id);

        // Index by intent
        by_intent
            .entry((record.tenant_id, record.intent_id))
            .or_insert_with(Vec::new)
            .push(record.id);

        // Index by result
        by_result
            .entry((record.tenant_id, record.result))
            .or_insert_with(Vec::new)
            .push(record.id);

        Ok(record)
    }

    async fn get(&self, record_id: Uuid) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        let records = self.records.read().await;
        records.get(&record_id).cloned().ok_or_else(|| {
            IntentRebaseError::Internal(format!("rollback record not found: {}", record_id))
        })
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();
        let mut result: Vec<SideEffectRollbackRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect();

        // Sort by recorded_at descending (newest first)
        result.sort_by(|a, b| b.recorded_at.cmp(&a.recorded_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }

    async fn list_by_compensation_action(
        &self,
        compensation_action_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_compensation_action = self.by_compensation_action.read().await;

        let ids = by_compensation_action
            .get(&compensation_action_id)
            .cloned()
            .unwrap_or_default();

        let result: Vec<SideEffectRollbackRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .filter(|r| r.tenant_id == tenant_id)
            .collect();

        Ok(result)
    }

    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_side_effect = self.by_side_effect.read().await;

        let ids = by_side_effect
            .get(&side_effect_id)
            .cloned()
            .unwrap_or_default();
        let result: Vec<SideEffectRollbackRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .filter(|r| r.tenant_id == tenant_id)
            .collect();

        Ok(result)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent
            .get(&(tenant_id, intent_id))
            .cloned()
            .unwrap_or_default();
        let result: Vec<SideEffectRollbackRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list_by_result(
        &self,
        tenant_id: Uuid,
        result: RollbackRecordResult,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let records = self.records.read().await;
        let by_result = self.by_result.read().await;

        let ids = by_result
            .get(&(tenant_id, result))
            .cloned()
            .unwrap_or_default();
        let result: Vec<SideEffectRollbackRecord> = ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect();

        Ok(result)
    }
}

// =============================================================================
// SQLx-backed Rollback Record Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

/// SQL-backed repository for rollback record storage using PostgreSQL.
pub struct SqlxRollbackRecordRepository {
    pool: PgPool,
}

impl SqlxRollbackRecordRepository {
    /// Create a new SqlxRollbackRecordRepository.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to a SideEffectRollbackRecord domain object.
    fn row_to_record(&self, row: PgRow) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        let result_str: String = row.get("result");
        let result = RollbackRecordResult::from_db_str(&result_str).ok_or_else(|| {
            IntentRebaseError::Internal(format!("unknown rollback record result: {}", result_str))
        })?;

        Ok(SideEffectRollbackRecord {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            compensation_action_id: row.get("compensation_action_id"),
            side_effect_id: row.get("side_effect_id"),
            intent_id: row.get("intent_id"),
            result,
            summary: row.get("summary"),
            error_code: row.get("error_code"),
            error_detail: row.get("error_detail"),
            recorded_by: row.get("recorded_by"),
            recorded_at: row.get("recorded_at"),
            lock_version: row.get("lock_version"),
        })
    }

    /// Create a rollback record within an external transaction.
    /// Used by RLS-aware handlers that manage their own transaction context.
    pub async fn create_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        record: SideEffectRollbackRecord,
    ) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        sqlx::query(
            r#"
            INSERT INTO side_effect_rollback_records (
                id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.compensation_action_id)
        .bind(record.side_effect_id)
        .bind(record.intent_id)
        .bind(record.result.as_str())
        .bind(&record.summary)
        .bind(&record.error_code)
        .bind(&record.error_detail)
        .bind(&record.recorded_by)
        .bind(record.recorded_at)
        .bind(record.lock_version)
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert rollback record: {}", e)))?;

        Ok(record)
    }
}

#[async_trait]
impl RollbackRecordRepository for SqlxRollbackRecordRepository {
    async fn create(
        &self,
        record: SideEffectRollbackRecord,
    ) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        sqlx::query(
            r#"
            INSERT INTO side_effect_rollback_records (
                id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(record.id)
        .bind(record.tenant_id)
        .bind(record.compensation_action_id)
        .bind(record.side_effect_id)
        .bind(record.intent_id)
        .bind(record.result.as_str())
        .bind(&record.summary)
        .bind(&record.error_code)
        .bind(&record.error_detail)
        .bind(&record.recorded_by)
        .bind(record.recorded_at)
        .bind(record.lock_version)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert rollback record: {}", e)))?;

        Ok(record)
    }

    async fn get(&self, record_id: Uuid) -> Result<SideEffectRollbackRecord, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE id = $1
            "#,
        )
        .bind(record_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch rollback record: {}", e)))?;

        match row {
            Some(r) => self.row_to_record(r),
            None => Err(IntentRebaseError::RollbackRecordNotFound(record_id)),
        }
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let limit = limit.unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE tenant_id = $1
            ORDER BY recorded_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list rollback records by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_record(r)).collect()
    }

    async fn list_by_compensation_action(
        &self,
        compensation_action_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE compensation_action_id = $1 AND tenant_id = $2
            ORDER BY recorded_at DESC
            "#,
        )
        .bind(compensation_action_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list rollback records by compensation action: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_record(r)).collect()
    }

    async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE side_effect_id = $1 AND tenant_id = $2
            ORDER BY recorded_at DESC
            "#,
        )
        .bind(side_effect_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list rollback records by side effect: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_record(r)).collect()
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY recorded_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list rollback records by intent: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_record(r)).collect()
    }

    async fn list_by_result(
        &self,
        tenant_id: Uuid,
        result: RollbackRecordResult,
    ) -> Result<Vec<SideEffectRollbackRecord>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, compensation_action_id, side_effect_id, intent_id,
                result, summary, error_code, error_detail, recorded_by, recorded_at, lock_version
            FROM side_effect_rollback_records
            WHERE tenant_id = $1 AND result = $2
            ORDER BY recorded_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(result.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list rollback records by result: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_record(r)).collect()
    }

    fn as_sqlx_repo(&self) -> Option<&SqlxRollbackRecordRepository> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_record(
        tenant_id: Uuid,
        compensation_action_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        result: RollbackRecordResult,
    ) -> SideEffectRollbackRecord {
        match result {
            RollbackRecordResult::Success => SideEffectRollbackRecord::success(
                tenant_id,
                compensation_action_id,
                side_effect_id,
                intent_id,
                "Test success",
                None,
            ),
            RollbackRecordResult::Failure => SideEffectRollbackRecord::failure(
                tenant_id,
                compensation_action_id,
                side_effect_id,
                intent_id,
                "Test failure",
                "TEST_ERR",
                None,
            ),
            RollbackRecordResult::Waived => SideEffectRollbackRecord::waived(
                tenant_id,
                compensation_action_id,
                side_effect_id,
                intent_id,
                "Test waived",
                None,
            ),
        }
    }

    #[tokio::test]
    async fn test_create_rollback_record() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = create_test_record(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            RollbackRecordResult::Success,
        );
        let id = record.id;

        let result = repo.create(record).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, id);
        assert_eq!(created.tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_get_rollback_record() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = create_test_record(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            RollbackRecordResult::Success,
        );
        let id = record.id;

        repo.create(record).await.unwrap();

        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_rollback_record_not_found() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..3 {
            let record = create_test_record(
                tenant_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_compensation_action() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..3 {
            let record = create_test_record(
                tenant_id,
                compensation_action_id,
                Uuid::new_v4(),
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        let result = repo
            .list_by_compensation_action(compensation_action_id, tenant_id)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_side_effect() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..3 {
            let record = create_test_record(
                tenant_id,
                Uuid::new_v4(),
                side_effect_id,
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        let result = repo.list_by_side_effect(side_effect_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..3 {
            let record = create_test_record(
                tenant_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        let result = repo.list_by_intent(intent_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_result() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create 2 success records
        for _ in 0..2 {
            let record = create_test_record(
                tenant_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        // Create 1 failure record
        let failure_record = create_test_record(
            tenant_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            RollbackRecordResult::Failure,
        );
        repo.create(failure_record).await.unwrap();

        let success_result = repo
            .list_by_result(tenant_id, RollbackRecordResult::Success)
            .await;
        assert!(success_result.is_ok());
        assert_eq!(success_result.unwrap().len(), 2);

        let failure_result = repo
            .list_by_result(tenant_id, RollbackRecordResult::Failure)
            .await;
        assert!(failure_result.is_ok());
        assert_eq!(failure_result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_tenant_filters_tenant() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create record for tenant 1
        let record1 = create_test_record(
            tenant_id_1,
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            RollbackRecordResult::Success,
        );
        repo.create(record1).await.unwrap();

        // Create record for tenant 2
        let record2 = create_test_record(
            tenant_id_2,
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            RollbackRecordResult::Success,
        );
        repo.create(record2).await.unwrap();

        let result1 = repo.list_by_tenant(tenant_id_1, None).await;
        assert!(result1.is_ok());
        assert_eq!(result1.unwrap().len(), 1);

        let result2 = repo.list_by_tenant(tenant_id_2, None).await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_tenant_with_limit() {
        let repo = Arc::new(InMemoryRollbackRecordRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _ in 0..5 {
            let record = create_test_record(
                tenant_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                intent_id,
                RollbackRecordResult::Success,
            );
            repo.create(record).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, Some(2)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
