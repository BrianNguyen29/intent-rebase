//! Checkpoint service — Phase 2 checkpoint lifecycle management
//!
//! This service layer sits between the API/adapter layer and the repository,
//! providing checkpoint lifecycle operations including create, query, and expire.
//! It does NOT handle Temporal SDK integration - that belongs in the runtime-adapter crate.

use intent_rebase_types::{Checkpoint, CheckpointStatus, CheckpointType, IntentRebaseError};
use std::sync::Arc;
use uuid::Uuid;

use crate::CheckpointRepository;

/// CheckpointService handles checkpoint lifecycle operations for the runtime adapter.
///
/// This service layer sits between the API/adapter layer and the repository,
/// providing checkpoint lifecycle operations including create, query, and expire.
/// It does NOT handle Temporal SDK integration - that belongs in the runtime-adapter crate.
pub struct CheckpointService {
    repo: Arc<dyn CheckpointRepository>,
    /// Default TTL for checkpoints that don't specify one
    default_ttl: Option<chrono::Duration>,
}

impl CheckpointService {
    /// Create a new CheckpointService with the given repository.
    pub fn new(repo: Arc<dyn CheckpointRepository>) -> Self {
        Self {
            repo,
            default_ttl: None,
        }
    }

    /// Create a new CheckpointService with a custom default TTL for checkpoints.
    pub fn with_default_ttl(repo: Arc<dyn CheckpointRepository>, ttl: chrono::Duration) -> Self {
        Self {
            repo,
            default_ttl: Some(ttl),
        }
    }

    /// Create a new checkpoint for an intent version.
    ///
    /// The checkpoint captures the workflow state at a specific point in the rebase lifecycle.
    /// If `expires_in` is provided, the checkpoint will expire after that duration.
    /// Otherwise, uses the service's default_ttl, or never expires if no default is set.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_checkpoint(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        workflow_id: Uuid,
        tenant_id: Uuid,
        checkpoint_type: CheckpointType,
        workflow_state: serde_json::Value,
        expires_in: Option<chrono::Duration>,
        metadata: Option<serde_json::Value>,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let expires_at = expires_in.map(|d| chrono::Utc::now() + d);

        let checkpoint = Checkpoint {
            checkpoint_id: Uuid::new_v4(),
            intent_id,
            intent_version,
            workflow_id,
            tenant_id,
            workflow_state,
            checkpoint_type,
            created_at: chrono::Utc::now(),
            expires_at,
            status: CheckpointStatus::Pending,
            metadata: metadata.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
        };

        self.repo.create_checkpoint(checkpoint).await
    }

    /// Create a checkpoint using the service's default TTL.
    pub async fn create_checkpoint_with_defaults(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        workflow_id: Uuid,
        tenant_id: Uuid,
        checkpoint_type: CheckpointType,
        workflow_state: serde_json::Value,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let expires_at = self.default_ttl.map(|d| chrono::Utc::now() + d);

        let checkpoint = Checkpoint {
            checkpoint_id: Uuid::new_v4(),
            intent_id,
            intent_version,
            workflow_id,
            tenant_id,
            workflow_state,
            checkpoint_type,
            created_at: chrono::Utc::now(),
            expires_at,
            status: CheckpointStatus::Pending,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        };

        self.repo.create_checkpoint(checkpoint).await
    }

    /// Get a checkpoint by its ID.
    pub async fn get_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo.get_checkpoint(checkpoint_id).await
    }

    /// List all checkpoints for a workflow, ordered by creation time descending.
    pub async fn list_checkpoints_by_workflow(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        self.repo.list_by_workflow(workflow_id, tenant_id).await
    }

    /// List all checkpoints for an intent, ordered by creation time descending.
    pub async fn list_checkpoints_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        self.repo.list_by_intent(intent_id, tenant_id).await
    }

    /// Get the latest checkpoint for a workflow.
    pub async fn get_latest_checkpoint(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.repo.list_by_workflow(workflow_id, tenant_id).await?;
        Ok(checkpoints.into_iter().next())
    }

    /// Get the latest checkpoint for an intent version.
    pub async fn get_checkpoint_for_version(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<Option<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.repo.list_by_intent(intent_id, tenant_id).await?;
        Ok(checkpoints
            .into_iter()
            .find(|c| c.intent_version == intent_version))
    }

    /// Activate a checkpoint (mark it as active and ready for replay).
    pub async fn activate_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo
            .update_status(checkpoint_id, CheckpointStatus::Active)
            .await
    }

    /// Supersede a checkpoint (mark it as superseded by a newer checkpoint).
    pub async fn supersede_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo
            .update_status(checkpoint_id, CheckpointStatus::Superseded)
            .await
    }

    /// Invalidate a checkpoint due to an error or invalid state.
    pub async fn invalidate_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo
            .update_status(checkpoint_id, CheckpointStatus::Invalidated)
            .await
    }

    /// Run checkpoint expiration job.
    ///
    /// This should be called periodically (e.g., by a background worker)
    /// to mark expired checkpoints and reclaim resources.
    ///
    /// Returns the number of checkpoints that were expired.
    pub async fn run_expiration(&self) -> Result<usize, IntentRebaseError> {
        self.repo.expire_checkpoints().await
    }

    /// Get checkpoints by type for a workflow.
    pub async fn list_checkpoints_by_type(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
        checkpoint_type: CheckpointType,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.repo.list_by_workflow(workflow_id, tenant_id).await?;
        Ok(checkpoints
            .into_iter()
            .filter(|c| c.checkpoint_type == checkpoint_type)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryCheckpointRepository;
    use intent_rebase_types::CheckpointType;

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
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Initial,
                serde_json::json!({}),
                Some(chrono::Duration::hours(1)),
                None,
            )
            .await;

        assert!(checkpoint.is_ok());
        let checkpoint = checkpoint.unwrap();
        assert_eq!(checkpoint.intent_id, intent_id);
        assert_eq!(checkpoint.workflow_id, workflow_id);
        assert_eq!(checkpoint.tenant_id, tenant_id);
        assert_eq!(checkpoint.intent_version, 1);
        assert_eq!(checkpoint.checkpoint_type, CheckpointType::Initial);
        assert!(checkpoint.expires_at.is_some());
    }

    #[tokio::test]
    async fn test_create_checkpoint_with_defaults() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service =
            CheckpointService::with_default_ttl(repo.clone(), chrono::Duration::hours(24));

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = service
            .create_checkpoint_with_defaults(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::PreFlight,
                serde_json::json!({"step": 1}),
            )
            .await;

        assert!(checkpoint.is_ok());
        let checkpoint = checkpoint.unwrap();
        assert!(checkpoint.expires_at.is_some()); // Should use default TTL
    }

    #[tokio::test]
    async fn test_create_checkpoint_no_expiry() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // No expiry specified
        let checkpoint = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Final,
                serde_json::json!({}),
                None,
                None,
            )
            .await;

        assert!(checkpoint.is_ok());
        let checkpoint = checkpoint.unwrap();
        assert!(checkpoint.expires_at.is_none()); // Should never expire
    }

    #[tokio::test]
    async fn test_get_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let created = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let retrieved = service.get_checkpoint(created.checkpoint_id).await;
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().checkpoint_id, created.checkpoint_id);
    }

    #[tokio::test]
    async fn test_list_checkpoints_by_workflow() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create multiple checkpoints
        for i in 0..3 {
            service
                .create_checkpoint(
                    intent_id,
                    i + 1,
                    workflow_id,
                    tenant_id,
                    CheckpointType::Initial,
                    serde_json::json!({}),
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        let checkpoints = service
            .list_checkpoints_by_workflow(workflow_id, tenant_id)
            .await;
        assert!(checkpoints.is_ok());
        assert_eq!(checkpoints.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_checkpoints_by_intent() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create multiple checkpoints for same intent (different versions)
        for i in 0..3 {
            service
                .create_checkpoint(
                    intent_id,
                    i + 1,
                    workflow_id,
                    tenant_id,
                    CheckpointType::Initial,
                    serde_json::json!({}),
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        let checkpoints = service
            .list_checkpoints_by_intent(intent_id, tenant_id)
            .await;
        assert!(checkpoints.is_ok());
        assert_eq!(checkpoints.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_get_latest_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create checkpoints with slight delay to ensure different timestamps
        for i in 0..3 {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            service
                .create_checkpoint(
                    intent_id,
                    i + 1,
                    workflow_id,
                    tenant_id,
                    CheckpointType::Initial,
                    serde_json::json!({}),
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        let latest = service.get_latest_checkpoint(workflow_id, tenant_id).await;
        assert!(latest.is_ok());
        let latest = latest.unwrap();
        assert!(latest.is_some());
        // Latest should be the one with highest created_at (version 3 in this case)
        assert_eq!(latest.unwrap().intent_version, 3);
    }

    #[tokio::test]
    async fn test_get_checkpoint_for_version() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create checkpoints for versions 1 and 2
        service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let v2 = service
            .create_checkpoint(
                intent_id,
                2,
                workflow_id,
                tenant_id,
                CheckpointType::PreFlight,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let found = service
            .get_checkpoint_for_version(intent_id, 2, tenant_id)
            .await;
        assert!(found.is_ok());
        assert_eq!(found.unwrap().unwrap().checkpoint_id, v2.checkpoint_id);

        // Version 3 doesn't exist
        let not_found = service
            .get_checkpoint_for_version(intent_id, 3, tenant_id)
            .await;
        assert!(not_found.is_ok());
        assert!(not_found.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_activate_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::IntentReceived,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let activated = service.activate_checkpoint(checkpoint.checkpoint_id).await;
        assert!(activated.is_ok());
        assert_eq!(activated.unwrap().status, CheckpointStatus::Active);
    }

    #[tokio::test]
    async fn test_supersede_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let superseded = service.supersede_checkpoint(checkpoint.checkpoint_id).await;
        assert!(superseded.is_ok());
        assert_eq!(superseded.unwrap().status, CheckpointStatus::Superseded);
    }

    #[tokio::test]
    async fn test_invalidate_checkpoint() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::RebaseStarted,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let invalidated = service
            .invalidate_checkpoint(checkpoint.checkpoint_id)
            .await;
        assert!(invalidated.is_ok());
        assert_eq!(invalidated.unwrap().status, CheckpointStatus::Invalidated);
    }

    #[tokio::test]
    async fn test_run_expiration() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create an already-expired checkpoint
        let mut expired_checkpoint =
            create_test_checkpoint(intent_id, workflow_id, tenant_id, CheckpointType::Final);
        expired_checkpoint.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        expired_checkpoint.status = CheckpointStatus::Active;

        repo.create_checkpoint(expired_checkpoint).await.unwrap();

        // Run expiration
        let count = service.run_expiration().await;
        assert!(count.is_ok());
        assert_eq!(count.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_list_checkpoints_by_type() {
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create checkpoints of different types
        service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_id,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        service
            .create_checkpoint(
                intent_id,
                2,
                workflow_id,
                tenant_id,
                CheckpointType::PreFlight,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        service
            .create_checkpoint(
                intent_id,
                3,
                workflow_id,
                tenant_id,
                CheckpointType::PreFlight,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        let pre_flight_checkpoints = service
            .list_checkpoints_by_type(workflow_id, tenant_id, CheckpointType::PreFlight)
            .await;
        assert!(pre_flight_checkpoints.is_ok());
        assert_eq!(pre_flight_checkpoints.unwrap().len(), 2);

        let initial_checkpoints = service
            .list_checkpoints_by_type(workflow_id, tenant_id, CheckpointType::Initial)
            .await;
        assert!(initial_checkpoints.is_ok());
        assert_eq!(initial_checkpoints.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_tenant_isolation() {
        // Checkpoints from different tenants should not leak
        let repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = CheckpointService::new(repo.clone());

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create checkpoint for tenant 1
        service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_1,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        // Create checkpoint for tenant 2
        service
            .create_checkpoint(
                intent_id,
                1,
                workflow_id,
                tenant_2,
                CheckpointType::Initial,
                serde_json::json!({}),
                None,
                None,
            )
            .await
            .unwrap();

        // Each tenant should only see their own checkpoints
        let tenant_1_checkpoints = service
            .list_checkpoints_by_workflow(workflow_id, tenant_1)
            .await;
        assert_eq!(tenant_1_checkpoints.unwrap().len(), 1);

        let tenant_2_checkpoints = service
            .list_checkpoints_by_workflow(workflow_id, tenant_2)
            .await;
        assert_eq!(tenant_2_checkpoints.unwrap().len(), 1);
    }
}
