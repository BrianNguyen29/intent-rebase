//! Checkpoint repository trait and implementations
//!
//! Phase 2: Checkpoint storage for Temporal workflow checkpoint mapping.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::{Checkpoint, CheckpointStatus, IntentRebaseError};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for checkpoint storage
/// Allows for in-memory (tests) or SQL-backed implementations
#[async_trait]
pub trait CheckpointRepository: Send + Sync {
    /// Create a new checkpoint
    async fn create_checkpoint(
        &self,
        checkpoint: Checkpoint,
    ) -> Result<Checkpoint, IntentRebaseError>;

    /// Get a checkpoint by its ID
    async fn get_checkpoint(&self, checkpoint_id: Uuid) -> Result<Checkpoint, IntentRebaseError>;

    /// List checkpoints for a given workflow, ordered by created_at descending
    async fn list_by_workflow(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError>;

    /// List checkpoints for a given intent, ordered by created_at descending
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError>;

    /// Update the status of a checkpoint
    async fn update_status(
        &self,
        checkpoint_id: Uuid,
        status: CheckpointStatus,
    ) -> Result<Checkpoint, IntentRebaseError>;

    /// Mark all expired checkpoints as expired (batch operation)
    /// Returns the number of checkpoints expired
    async fn expire_checkpoints(&self) -> Result<usize, IntentRebaseError>;
}

/// In-memory implementation for testing and Phase 2
pub struct InMemoryCheckpointRepository {
    checkpoints: RwLock<HashMap<Uuid, Checkpoint>>,
    /// Secondary index: workflow_id -> list of checkpoint_ids
    by_workflow: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: intent_id -> list of checkpoint_ids
    by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryCheckpointRepository {
    pub fn new() -> Self {
        Self {
            checkpoints: RwLock::new(HashMap::new()),
            by_workflow: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryCheckpointRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CheckpointRepository for InMemoryCheckpointRepository {
    async fn create_checkpoint(
        &self,
        checkpoint: Checkpoint,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let mut checkpoints = self.checkpoints.write().await;
        let mut by_workflow = self.by_workflow.write().await;
        let mut by_intent = self.by_intent.write().await;

        // Store checkpoint
        checkpoints.insert(checkpoint.checkpoint_id, checkpoint.clone());

        // Index by workflow
        by_workflow
            .entry(checkpoint.workflow_id)
            .or_insert_with(Vec::new)
            .push(checkpoint.checkpoint_id);

        // Index by intent
        by_intent
            .entry(checkpoint.intent_id)
            .or_insert_with(Vec::new)
            .push(checkpoint.checkpoint_id);

        Ok(checkpoint)
    }

    async fn get_checkpoint(&self, checkpoint_id: Uuid) -> Result<Checkpoint, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.get(&checkpoint_id).cloned().ok_or_else(|| {
            IntentRebaseError::Internal(format!("checkpoint not found: {}", checkpoint_id))
        })
    }

    async fn list_by_workflow(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        let by_workflow = self.by_workflow.read().await;

        let checkpoint_ids = by_workflow.get(&workflow_id).cloned().unwrap_or_default();

        let mut result: Vec<Checkpoint> = checkpoint_ids
            .iter()
            .filter_map(|id| checkpoints.get(id).cloned())
            .filter(|c| c.tenant_id == tenant_id)
            .collect();

        // Sort by created_at descending (newest first)
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        Ok(result)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        let by_intent = self.by_intent.read().await;

        let checkpoint_ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<Checkpoint> = checkpoint_ids
            .iter()
            .filter_map(|id| checkpoints.get(id).cloned())
            .filter(|c| c.tenant_id == tenant_id)
            .collect();

        // Sort by created_at descending (newest first)
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        Ok(result)
    }

    async fn update_status(
        &self,
        checkpoint_id: Uuid,
        status: CheckpointStatus,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let mut checkpoints = self.checkpoints.write().await;

        let checkpoint = checkpoints.get_mut(&checkpoint_id).ok_or_else(|| {
            IntentRebaseError::Internal(format!("checkpoint not found: {}", checkpoint_id))
        })?;

        checkpoint.status = status;
        Ok(checkpoint.clone())
    }

    async fn expire_checkpoints(&self) -> Result<usize, IntentRebaseError> {
        let mut checkpoints = self.checkpoints.write().await;
        let now = chrono::Utc::now();
        let mut count = 0;

        for checkpoint in checkpoints.values_mut() {
            if let Some(expires_at) = checkpoint.expires_at {
                if expires_at < now && checkpoint.status != CheckpointStatus::Expired {
                    checkpoint.status = CheckpointStatus::Expired;
                    count += 1;
                }
            }
        }

        Ok(count)
    }
}

// =============================================================================
// SQLx-backed Checkpoint Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

use intent_rebase_types::CheckpointType;

/// SQL-backed repository for checkpoint storage using PostgreSQL.
/// Follows the same patterns as SqlxIntentRepository.
pub struct SqlxCheckpointRepository {
    pool: PgPool,
}

impl SqlxCheckpointRepository {
    /// Create a new SqlxCheckpointRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to a Checkpoint domain object
    fn row_to_checkpoint(&self, row: PgRow) -> Result<Checkpoint, IntentRebaseError> {
        let status_str: String = row.get("status");
        let checkpoint_type_str: String = row.get("checkpoint_type");
        let workflow_state_json: serde_json::Value = row.get("workflow_state");
        let metadata_json: serde_json::Value = row.get("metadata");

        Ok(Checkpoint {
            checkpoint_id: row.get("checkpoint_id"),
            intent_id: row.get("intent_id"),
            intent_version: row.get("intent_version"),
            workflow_id: row.get("workflow_id"),
            tenant_id: row.get("tenant_id"),
            workflow_state: workflow_state_json,
            checkpoint_type: checkpoint_type_from_string(&checkpoint_type_str),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            status: checkpoint_status_from_string(&status_str),
            metadata: metadata_json,
        })
    }

    /// Insert a new checkpoint into the database
    async fn insert_checkpoint(&self, checkpoint: &Checkpoint) -> Result<(), IntentRebaseError> {
        let workflow_state_json = serde_json::to_value(&checkpoint.workflow_state)
            .map_err(|e| IntentRebaseError::SerializationError(format!("workflow_state: {}", e)))?;
        let metadata_json = serde_json::to_value(&checkpoint.metadata)
            .map_err(|e| IntentRebaseError::SerializationError(format!("metadata: {}", e)))?;
        let checkpoint_type_str = checkpoint_type_to_string(checkpoint.checkpoint_type);
        let status_str = checkpoint_status_to_string(checkpoint.status);

        sqlx::query(
            r#"
            INSERT INTO checkpoints (
                checkpoint_id, intent_id, intent_version, workflow_id, tenant_id,
                workflow_state, checkpoint_type, created_at, expires_at, status, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(checkpoint.checkpoint_id)
        .bind(checkpoint.intent_id)
        .bind(checkpoint.intent_version)
        .bind(checkpoint.workflow_id)
        .bind(checkpoint.tenant_id)
        .bind(workflow_state_json)
        .bind(checkpoint_type_str)
        .bind(checkpoint.created_at)
        .bind(checkpoint.expires_at)
        .bind(status_str)
        .bind(metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert checkpoint: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl CheckpointRepository for SqlxCheckpointRepository {
    async fn create_checkpoint(
        &self,
        checkpoint: Checkpoint,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.insert_checkpoint(&checkpoint).await?;
        Ok(checkpoint)
    }

    async fn get_checkpoint(&self, checkpoint_id: Uuid) -> Result<Checkpoint, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT checkpoint_id, intent_id, intent_version, workflow_id, tenant_id,
                workflow_state, checkpoint_type, created_at, expires_at, status, metadata
            FROM checkpoints
            WHERE checkpoint_id = $1
            "#,
        )
        .bind(checkpoint_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch checkpoint: {}", e)))?;

        match row {
            Some(r) => self.row_to_checkpoint(r),
            None => Err(IntentRebaseError::Internal(format!(
                "checkpoint not found: {}",
                checkpoint_id
            ))),
        }
    }

    async fn list_by_workflow(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT checkpoint_id, intent_id, intent_version, workflow_id, tenant_id,
                workflow_state, checkpoint_type, created_at, expires_at, status, metadata
            FROM checkpoints
            WHERE workflow_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list checkpoints by workflow: {}", e))
        })?;

        rows.into_iter()
            .map(|r| self.row_to_checkpoint(r))
            .collect()
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT checkpoint_id, intent_id, intent_version, workflow_id, tenant_id,
                workflow_state, checkpoint_type, created_at, expires_at, status, metadata
            FROM checkpoints
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list checkpoints by intent: {}", e))
        })?;

        rows.into_iter()
            .map(|r| self.row_to_checkpoint(r))
            .collect()
    }

    async fn update_status(
        &self,
        checkpoint_id: Uuid,
        status: CheckpointStatus,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let status_str = checkpoint_status_to_string(status);

        sqlx::query(
            r#"
            UPDATE checkpoints
            SET status = $1
            WHERE checkpoint_id = $2
            "#,
        )
        .bind(status_str)
        .bind(checkpoint_id)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("update checkpoint status: {}", e)))?;

        self.get_checkpoint(checkpoint_id).await
    }

    async fn expire_checkpoints(&self) -> Result<usize, IntentRebaseError> {
        let now = chrono::Utc::now();
        let expired_status = checkpoint_status_to_string(CheckpointStatus::Expired);

        let result = sqlx::query(
            r#"
            UPDATE checkpoints
            SET status = $1
            WHERE expires_at IS NOT NULL
              AND expires_at < $2
              AND status NOT IN ($1, $3)
            "#,
        )
        .bind(expired_status)
        .bind(now)
        .bind(expired_status)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("expire checkpoints: {}", e)))?;

        Ok(result.rows_affected() as usize)
    }
}

// =============================================================================
// Helper functions for checkpoint enum conversion
// =============================================================================

fn checkpoint_type_to_string(ct: CheckpointType) -> &'static str {
    match ct {
        CheckpointType::Initial => "initial",
        CheckpointType::PreFlight => "pre_flight",
        CheckpointType::IntentReceived => "intent_received",
        CheckpointType::IntentValidated => "intent_validated",
        CheckpointType::RebaseStarted => "rebase_started",
        CheckpointType::RebaseCompleted => "rebase_completed",
        CheckpointType::Final => "final",
        CheckpointType::Custom => "custom",
    }
}

fn checkpoint_type_from_string(s: &str) -> CheckpointType {
    match s {
        "pre_flight" => CheckpointType::PreFlight,
        "intent_received" => CheckpointType::IntentReceived,
        "intent_validated" => CheckpointType::IntentValidated,
        "rebase_started" => CheckpointType::RebaseStarted,
        "rebase_completed" => CheckpointType::RebaseCompleted,
        "final" => CheckpointType::Final,
        "custom" => CheckpointType::Custom,
        _ => CheckpointType::Initial,
    }
}

fn checkpoint_status_to_string(status: CheckpointStatus) -> &'static str {
    match status {
        CheckpointStatus::Pending => "pending",
        CheckpointStatus::Created => "created",
        CheckpointStatus::Active => "active",
        CheckpointStatus::Superseded => "superseded",
        CheckpointStatus::Expired => "expired",
        CheckpointStatus::Invalidated => "invalidated",
    }
}

fn checkpoint_status_from_string(s: &str) -> CheckpointStatus {
    match s {
        "created" => CheckpointStatus::Created,
        "active" => CheckpointStatus::Active,
        "superseded" => CheckpointStatus::Superseded,
        "expired" => CheckpointStatus::Expired,
        "invalidated" => CheckpointStatus::Invalidated,
        _ => CheckpointStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::CheckpointType;
    use std::sync::Arc;

    fn create_test_checkpoint(
        intent_id: Uuid,
        workflow_id: Uuid,
        tenant_id: Uuid,
        checkpoint_type: CheckpointType,
    ) -> Checkpoint {
        Checkpoint::with_required(intent_id, 1, workflow_id, tenant_id, checkpoint_type)
    }

    #[tokio::test]
    async fn test_create_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Initial);

        let result = repo.create_checkpoint(checkpoint.clone()).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.checkpoint_id, checkpoint.checkpoint_id);
        assert_eq!(created.intent_id, intent_id);
        assert_eq!(created.workflow_id, workflow_id);
        assert_eq!(created.status, CheckpointStatus::Pending);
    }

    #[tokio::test]
    async fn test_get_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::PreFlight);
        let id = checkpoint.checkpoint_id;

        repo.create_checkpoint(checkpoint).await.unwrap();

        let result = repo.get_checkpoint(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().checkpoint_id, id);
    }

    #[tokio::test]
    async fn test_get_checkpoint_not_found() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());

        let result = repo.get_checkpoint(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_workflow() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create multiple checkpoints for the same workflow
        for _ in 0..3 {
            let checkpoint =
                create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Initial);
            repo.create_checkpoint(checkpoint).await.unwrap();
        }

        let result = repo.list_by_workflow(workflow_id, tenant_id).await;
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.len(), 3);
        // Should be sorted by created_at descending
        assert!(list.windows(2).all(|w| w[0].created_at >= w[1].created_at));
    }

    #[tokio::test]
    async fn test_list_by_workflow_empty() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());

        let result = repo.list_by_workflow(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_by_workflow_filters_tenant() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Create checkpoint for tenant 1
        let checkpoint1 =
            create_test_checkpoint(intent_id, workflow_id, tenant_id_1, CheckpointType::Initial);
        repo.create_checkpoint(checkpoint1).await.unwrap();

        // Create checkpoint for tenant 2
        let checkpoint2 = create_test_checkpoint(
            intent_id,
            workflow_id,
            tenant_id_2,
            CheckpointType::PreFlight,
        );
        repo.create_checkpoint(checkpoint2).await.unwrap();

        // Query for tenant 1 should only return tenant 1's checkpoint
        let result = repo.list_by_workflow(workflow_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);

        // Query for tenant 2 should only return tenant 2's checkpoint
        let result = repo.list_by_workflow(workflow_id, tenant_id_2).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create multiple checkpoints for the same intent
        for _ in 0..3 {
            let checkpoint =
                create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Initial);
            repo.create_checkpoint(checkpoint).await.unwrap();
        }

        let result = repo.list_by_intent(intent_id, tenant_id).await;
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent_empty() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());

        let result = repo.list_by_intent(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = create_test_checkpoint(
            intent_id,
            workflow_id,
            tenant_id,
            CheckpointType::IntentReceived,
        );
        let id = checkpoint.checkpoint_id;

        repo.create_checkpoint(checkpoint).await.unwrap();

        // Update status to Active
        let result = repo.update_status(id, CheckpointStatus::Active).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, CheckpointStatus::Active);

        // Update status to Superseded
        let result = repo.update_status(id, CheckpointStatus::Superseded).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, CheckpointStatus::Superseded);
    }

    #[tokio::test]
    async fn test_update_status_not_found() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());

        let result = repo
            .update_status(Uuid::new_v4(), CheckpointStatus::Active)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_expire_checkpoints() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create a checkpoint with past expiry
        let mut checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Final);
        checkpoint.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        checkpoint.status = CheckpointStatus::Active;

        repo.create_checkpoint(checkpoint).await.unwrap();

        // Expire checkpoints
        let result = repo.expire_checkpoints().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        // Verify status was updated
        let checkpoints: Vec<_> = {
            let checkpoints = repo.checkpoints.read().await;
            checkpoints.values().cloned().collect()
        };
        assert_eq!(checkpoints[0].status, CheckpointStatus::Expired);
    }

    #[tokio::test]
    async fn test_expire_checkpoints_none_expired() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create a checkpoint that doesn't expire
        let checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Initial);
        repo.create_checkpoint(checkpoint).await.unwrap();

        let result = repo.expire_checkpoints().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_expire_checkpoints_already_expired() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create a checkpoint already marked as Expired
        let mut checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Final);
        checkpoint.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        checkpoint.status = CheckpointStatus::Expired;

        repo.create_checkpoint(checkpoint).await.unwrap();

        // Expire should not double-count
        let result = repo.expire_checkpoints().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_checkpoints_persist_across_instances() {
        // Two services sharing the same repo should see the same data
        let repo = Arc::new(InMemoryCheckpointRepository::new());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Initial);
        let id = checkpoint.checkpoint_id;

        repo.create_checkpoint(checkpoint).await.unwrap();

        // Second "service" reading from same repo
        let result = repo.get_checkpoint(id).await;
        assert!(result.is_ok());
    }
}

// =============================================================================
// SqlxCheckpointRepository unit tests (helper function tests)
// These test the enum conversion logic without requiring a database connection.
// =============================================================================

#[cfg(test)]
mod sqlx_checkpoint_tests {
    use super::*;

    #[test]
    fn test_checkpoint_type_to_string() {
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::Initial),
            "initial"
        );
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::PreFlight),
            "pre_flight"
        );
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::IntentReceived),
            "intent_received"
        );
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::IntentValidated),
            "intent_validated"
        );
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::RebaseStarted),
            "rebase_started"
        );
        assert_eq!(
            checkpoint_type_to_string(CheckpointType::RebaseCompleted),
            "rebase_completed"
        );
        assert_eq!(checkpoint_type_to_string(CheckpointType::Final), "final");
        assert_eq!(checkpoint_type_to_string(CheckpointType::Custom), "custom");
    }

    #[test]
    fn test_checkpoint_type_from_string() {
        assert_eq!(
            checkpoint_type_from_string("initial"),
            CheckpointType::Initial
        );
        assert_eq!(
            checkpoint_type_from_string("pre_flight"),
            CheckpointType::PreFlight
        );
        assert_eq!(
            checkpoint_type_from_string("intent_received"),
            CheckpointType::IntentReceived
        );
        assert_eq!(
            checkpoint_type_from_string("intent_validated"),
            CheckpointType::IntentValidated
        );
        assert_eq!(
            checkpoint_type_from_string("rebase_started"),
            CheckpointType::RebaseStarted
        );
        assert_eq!(
            checkpoint_type_from_string("rebase_completed"),
            CheckpointType::RebaseCompleted
        );
        assert_eq!(checkpoint_type_from_string("final"), CheckpointType::Final);
        assert_eq!(
            checkpoint_type_from_string("custom"),
            CheckpointType::Custom
        );
        // Unknown values default to Initial
        assert_eq!(
            checkpoint_type_from_string("unknown"),
            CheckpointType::Initial
        );
    }

    #[test]
    fn test_checkpoint_status_to_string() {
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Pending),
            "pending"
        );
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Created),
            "created"
        );
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Active),
            "active"
        );
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Superseded),
            "superseded"
        );
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Expired),
            "expired"
        );
        assert_eq!(
            checkpoint_status_to_string(CheckpointStatus::Invalidated),
            "invalidated"
        );
    }

    #[test]
    fn test_checkpoint_status_from_string() {
        assert_eq!(
            checkpoint_status_from_string("pending"),
            CheckpointStatus::Pending
        );
        assert_eq!(
            checkpoint_status_from_string("created"),
            CheckpointStatus::Created
        );
        assert_eq!(
            checkpoint_status_from_string("active"),
            CheckpointStatus::Active
        );
        assert_eq!(
            checkpoint_status_from_string("superseded"),
            CheckpointStatus::Superseded
        );
        assert_eq!(
            checkpoint_status_from_string("expired"),
            CheckpointStatus::Expired
        );
        assert_eq!(
            checkpoint_status_from_string("invalidated"),
            CheckpointStatus::Invalidated
        );
        // Unknown values default to Pending
        assert_eq!(
            checkpoint_status_from_string("unknown"),
            CheckpointStatus::Pending
        );
    }
}
