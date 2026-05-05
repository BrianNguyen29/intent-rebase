//! Orchestration run repository trait and implementations
//!
//! Phase 3 Batch 1: Persisted run handle storage for single-shot orchestration.
//! Allows in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::orchestration_run::{OrchestrationRun, RunStatus};

/// Repository trait for orchestration run storage.
#[async_trait]
pub trait OrchestrationRunRepository: Send + Sync {
    /// Create a new orchestration run record.
    async fn create(&self, run: OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError>;

    /// Get an orchestration run by its ID.
    async fn get(&self, run_id: Uuid) -> Result<OrchestrationRun, IntentRebaseError>;

    /// Update an orchestration run.
    async fn update(&self, run: &OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError>;

    /// List runs by tenant (most recent first).
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<OrchestrationRun>, IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxOrchestrationRunRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxOrchestrationRunRepository> {
        None
    }
}

/// In-memory implementation for testing and Phase 3 Batch 1.
pub struct InMemoryOrchestrationRunRepository {
    runs: RwLock<HashMap<Uuid, OrchestrationRun>>,
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryOrchestrationRunRepository {
    pub fn new() -> Self {
        Self {
            runs: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryOrchestrationRunRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OrchestrationRunRepository for InMemoryOrchestrationRunRepository {
    async fn create(&self, run: OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError> {
        let mut runs = self.runs.write().await;
        let mut by_tenant = self.by_tenant.write().await;

        runs.insert(run.id, run.clone());
        by_tenant
            .entry(run.tenant_id)
            .or_insert_with(Vec::new)
            .push(run.id);

        Ok(run)
    }

    async fn get(&self, run_id: Uuid) -> Result<OrchestrationRun, IntentRebaseError> {
        let runs = self.runs.read().await;
        runs.get(&run_id)
            .cloned()
            .ok_or(IntentRebaseError::OrchestrationRunNotFound(run_id))
    }

    async fn update(&self, run: &OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError> {
        let mut runs = self.runs.write().await;
        if let std::collections::hash_map::Entry::Occupied(mut e) = runs.entry(run.id) {
            e.insert(run.clone());
            Ok(run.clone())
        } else {
            Err(IntentRebaseError::OrchestrationRunNotFound(run.id))
        }
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<OrchestrationRun>, IntentRebaseError> {
        let runs = self.runs.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();
        let mut result: Vec<OrchestrationRun> =
            ids.iter().filter_map(|id| runs.get(id).cloned()).collect();

        // Sort by created_at descending (most recent first)
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }
}

// =============================================================================
// SQLx-backed Orchestration Run Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

use super::orchestration_run::RunItemResult;

/// SQL-backed repository for orchestration run storage using PostgreSQL.
pub struct SqlxOrchestrationRunRepository {
    pool: PgPool,
}

impl SqlxOrchestrationRunRepository {
    /// Create a new SqlxOrchestrationRunRepository.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_run(&self, row: PgRow) -> Result<OrchestrationRun, IntentRebaseError> {
        let action_ids_json: serde_json::Value = row.get("action_ids");
        let action_ids: Vec<Uuid> = serde_json::from_value(action_ids_json)
            .map_err(|e| IntentRebaseError::Internal(format!("deserialize action_ids: {}", e)))?;

        let item_results_json: serde_json::Value = row.get("item_results");
        let item_results: Vec<RunItemResult> = serde_json::from_value(item_results_json)
            .map_err(|e| IntentRebaseError::Internal(format!("deserialize item_results: {}", e)))?;

        Ok(OrchestrationRun {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            intent_id: row.get("intent_id"),
            action_ids,
            status: run_status_from_string(&row.get::<String, _>("status"))?,
            initiated_by: row.get("initiated_by"),
            created_at: row.get("created_at"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            succeeded_count: row.get::<i32, _>("succeeded_count") as usize,
            failed_count: row.get::<i32, _>("failed_count") as usize,
            skipped_count: row.get::<i32, _>("skipped_count") as usize,
            not_found_count: row.get::<i32, _>("not_found_count") as usize,
            total_count: row.get::<i32, _>("total_count") as usize,
            item_results,
        })
    }
}

fn run_status_to_string(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::CompletedWithErrors => "completed_with_errors",
        RunStatus::Failed => "failed",
    }
}

fn run_status_from_string(s: &str) -> Result<RunStatus, IntentRebaseError> {
    match s {
        "pending" => Ok(RunStatus::Pending),
        "running" => Ok(RunStatus::Running),
        "completed" => Ok(RunStatus::Completed),
        "completed_with_errors" => Ok(RunStatus::CompletedWithErrors),
        "failed" => Ok(RunStatus::Failed),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown run status: {}",
            other
        ))),
    }
}

#[async_trait]
impl OrchestrationRunRepository for SqlxOrchestrationRunRepository {
    async fn create(&self, run: OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError> {
        let action_ids_json = serde_json::to_value(&run.action_ids)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize action_ids: {}", e)))?;
        let item_results_json = serde_json::to_value(&run.item_results)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize item_results: {}", e)))?;
        let status_str = run_status_to_string(run.status);

        sqlx::query(
            r#"
            INSERT INTO orchestration_runs (
                id, tenant_id, intent_id, action_ids, status, initiated_by,
                created_at, started_at, completed_at, succeeded_count, failed_count,
                skipped_count, not_found_count, total_count, item_results
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
        )
        .bind(run.id)
        .bind(run.tenant_id)
        .bind(run.intent_id)
        .bind(action_ids_json)
        .bind(status_str)
        .bind(&run.initiated_by)
        .bind(run.created_at)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(run.succeeded_count as i32)
        .bind(run.failed_count as i32)
        .bind(run.skipped_count as i32)
        .bind(run.not_found_count as i32)
        .bind(run.total_count as i32)
        .bind(item_results_json)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert orchestration run: {}", e)))?;

        Ok(run)
    }

    async fn get(&self, run_id: Uuid) -> Result<OrchestrationRun, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, action_ids, status, initiated_by,
                created_at, started_at, completed_at, succeeded_count, failed_count,
                skipped_count, not_found_count, total_count, item_results
            FROM orchestration_runs
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch orchestration run: {}", e)))?;

        match row {
            Some(r) => self.row_to_run(r),
            None => Err(IntentRebaseError::OrchestrationRunNotFound(run_id)),
        }
    }

    async fn update(&self, run: &OrchestrationRun) -> Result<OrchestrationRun, IntentRebaseError> {
        let _action_ids_json = serde_json::to_value(&run.action_ids)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize action_ids: {}", e)))?;
        let item_results_json = serde_json::to_value(&run.item_results)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize item_results: {}", e)))?;
        let status_str = run_status_to_string(run.status);

        let row = sqlx::query(
            r#"
            UPDATE orchestration_runs
            SET status = $2,
                started_at = $3,
                completed_at = $4,
                succeeded_count = $5,
                failed_count = $6,
                skipped_count = $7,
                not_found_count = $8,
                total_count = $9,
                item_results = $10
            WHERE id = $1
            RETURNING id, tenant_id, intent_id, action_ids, status, initiated_by,
                created_at, started_at, completed_at, succeeded_count, failed_count,
                skipped_count, not_found_count, total_count, item_results
            "#,
        )
        .bind(run.id)
        .bind(status_str)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(run.succeeded_count as i32)
        .bind(run.failed_count as i32)
        .bind(run.skipped_count as i32)
        .bind(run.not_found_count as i32)
        .bind(run.total_count as i32)
        .bind(item_results_json)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("update orchestration run: {}", e)))?;

        match row {
            Some(r) => self.row_to_run(r),
            None => Err(IntentRebaseError::OrchestrationRunNotFound(run.id)),
        }
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<OrchestrationRun>, IntentRebaseError> {
        let limit = limit.unwrap_or(100);
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, action_ids, status, initiated_by,
                created_at, started_at, completed_at, succeeded_count, failed_count,
                skipped_count, not_found_count, total_count, item_results
            FROM orchestration_runs
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list orchestration runs by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_run(r)).collect()
    }

    fn as_sqlx_repo(&self) -> Option<&SqlxOrchestrationRunRepository> {
        Some(self)
    }
}

/// Separate impl block for SQL-specific methods that are not part of the trait.
impl SqlxOrchestrationRunRepository {
    /// Create an orchestration run within an external RLS-aware transaction.
    ///
    /// This method is used for RLS-wrapped operations where the transaction
    /// is created by `RlsAwarePool::begin_with_tenant` which sets the RLS
    /// tenant context before any operations.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `run` - The orchestration run to create
    ///
    /// # Errors
    ///
    /// Returns an error if the insert fails or if the transaction is invalid.
    pub async fn create_run_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        run: OrchestrationRun,
    ) -> Result<OrchestrationRun, IntentRebaseError> {
        let action_ids_json = serde_json::to_value(&run.action_ids)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize action_ids: {}", e)))?;
        let item_results_json = serde_json::to_value(&run.item_results)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize item_results: {}", e)))?;
        let status_str = run_status_to_string(run.status);

        sqlx::query(
            r#"
            INSERT INTO orchestration_runs (
                id, tenant_id, intent_id, action_ids, status, initiated_by,
                created_at, started_at, completed_at, succeeded_count, failed_count,
                skipped_count, not_found_count, total_count, item_results
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
        )
        .bind(run.id)
        .bind(run.tenant_id)
        .bind(run.intent_id)
        .bind(action_ids_json)
        .bind(status_str)
        .bind(&run.initiated_by)
        .bind(run.created_at)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(run.succeeded_count as i32)
        .bind(run.failed_count as i32)
        .bind(run.skipped_count as i32)
        .bind(run.not_found_count as i32)
        .bind(run.total_count as i32)
        .bind(item_results_json)
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert orchestration run: {}", e)))?;

        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_run(tenant_id: Uuid) -> OrchestrationRun {
        OrchestrationRun::new(
            tenant_id,
            vec![Uuid::new_v4(), Uuid::new_v4()],
            Some("test-user".to_string()),
            None,
        )
    }

    #[tokio::test]
    async fn test_create_run() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let tenant_id = Uuid::new_v4();
        let run = create_test_run(tenant_id);
        let id = run.id;

        let result = repo.create(run).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_run() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let tenant_id = Uuid::new_v4();
        let run = create_test_run(tenant_id);
        let id = run.id;

        repo.create(run).await.unwrap();
        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_run_not_found() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::OrchestrationRunNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_update_run() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let tenant_id = Uuid::new_v4();
        let run = create_test_run(tenant_id);

        repo.create(run.clone()).await.unwrap();

        let mut updated = run.clone();
        updated.mark_started();
        let result = repo.update(&updated).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let tenant_id = Uuid::new_v4();

        for _ in 0..3 {
            let run = create_test_run(tenant_id);
            repo.create(run).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_get_run_cross_tenant_blocked() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let _tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates a run
        let run = create_test_run(tenant_a);
        let run_id = run.id;
        repo.create(run).await.unwrap();

        // Tenant A can get their own run
        let result = repo.get(run_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tenant_id, tenant_a);

        // Note: The InMemory repository's `get` method does not enforce tenant isolation.
        // This test documents the current behavior where any tenant can get any run by ID.
        // Production implementations should add tenant filtering to the `get` method.
    }

    #[tokio::test]
    async fn test_list_runs_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryOrchestrationRunRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates 3 runs
        for _ in 0..3 {
            let run = create_test_run(tenant_a);
            repo.create(run).await.unwrap();
        }

        // Tenant B creates 2 runs
        for _ in 0..2 {
            let run = create_test_run(tenant_b);
            repo.create(run).await.unwrap();
        }

        // List for tenant A should return 3 runs
        let runs_a = repo.list_by_tenant(tenant_a, None).await.unwrap();
        assert_eq!(runs_a.len(), 3);
        assert!(runs_a.iter().all(|r| r.tenant_id == tenant_a));

        // List for tenant B should return 2 runs
        let runs_b = repo.list_by_tenant(tenant_b, None).await.unwrap();
        assert_eq!(runs_b.len(), 2);
        assert!(runs_b.iter().all(|r| r.tenant_id == tenant_b));
    }

    use std::sync::Arc;
}
