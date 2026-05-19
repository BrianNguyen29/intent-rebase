//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 1: First slice implementation with in-memory repository.
//! Repository trait allows swapping to SQL-backed implementation.

pub mod approval_request_repo;
pub mod checkpoint_repo;
pub mod checkpoint_service;
pub mod event_consumer;
pub mod in_memory_intent_repository;
pub mod intent_service;
pub mod policy_snapshot_repo;
pub mod propagation_record_repo;
pub mod s3_snapshot_storage;
pub mod sqlx_approval_request_repo;
pub mod sqlx_repository;

#[cfg(test)]
mod approval_request_repo_tests;

use async_trait::async_trait;
use intent_rebase_types::{
    CreateIntentRequest, CreateIntentResponse, CreateVersionRequest, CreateVersionResponse, Intent,
    IntentRebaseError, IntentVersion,
};
use uuid::Uuid;

pub use approval_request_repo::{
    ApprovalRequest, ApprovalRequestRepository, ApprovalRequestStatus,
    InMemoryApprovalRequestRepository,
};
pub use checkpoint_repo::{
    CheckpointRepository, InMemoryCheckpointRepository, SqlxCheckpointRepository,
};
pub use checkpoint_service::CheckpointService;
pub use in_memory_intent_repository::InMemoryIntentRepository;
pub use intent_service::IntentService;
pub use policy_snapshot_repo::{
    InMemoryPolicySnapshotRepository, PolicySnapshotRepository, SqlxPolicySnapshotRepository,
};
pub use propagation_record_repo::{
    InMemoryPropagationRecordRepository, PropagationRecordRepository,
    SqlxPropagationRecordRepository,
};
pub use s3_snapshot_storage::{
    InMemorySnapshotStorage, S3SnapshotStorage, SnapshotStorage, SnapshotStorageError,
};
pub use sqlx_approval_request_repo::SqlxApprovalRequestRepository;
pub use sqlx_repository::SqlxIntentRepository;

// Re-export tenant extraction for internal use
pub use sqlx_repository::TenantResolver;

/// Repository trait for intent storage
/// Allows for in-memory (tests) or SQL-backed implementations
#[async_trait]
pub trait IntentRepository: Send + Sync {
    /// Create a new intent with its initial version (transactional)
    /// This is the primary method for intent creation - it creates both intent and v1 atomically
    async fn create_intent_tx(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError>;

    async fn get_intent(&self, id: Uuid) -> Result<Intent, IntentRebaseError>;

    /// Create a new version with optimistic concurrency control
    /// expected_version: the version number the caller believes is current
    /// expected_row_version: the row_version the caller last observed
    /// Returns ConcurrencyConflict if the intent has been modified since read
    async fn create_version_with_occ(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: i32,
        expected_row_version: i32,
    ) -> Result<CreateVersionResponse, IntentRebaseError>;

    async fn get_version(&self, id: Uuid) -> Result<IntentVersion, IntentRebaseError>;
    async fn get_versions_by_intent(
        &self,
        intent_id: Uuid,
    ) -> Result<Vec<IntentVersion>, IntentRebaseError>;
    async fn get_version_by_intent_and_number(
        &self,
        intent_id: Uuid,
        version_number: i32,
    ) -> Result<IntentVersion, IntentRebaseError>;

    /// Get intent with FOR UPDATE lock (for OCC workflows)
    /// Returns (intent, row_version) tuple
    async fn get_intent_for_update(&self, id: Uuid) -> Result<(Intent, i32), IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxIntentRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxIntentRepository> {
        None
    }
}

/// Compute SHA-256 hash of the payload for integrity verification
pub(crate) fn compute_payload_hash(payload: &intent_rebase_types::IntentPayload) -> String {
    use sha2::{Digest, Sha256};

    let json = serde_json::to_string(payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
        IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
        IntentScope, IntentStatus, RiskTier, SourceRef, Urgency,
    };
    use rebase_engine::Severity;
    use std::sync::Arc;

    fn create_test_payload() -> IntentPayload {
        IntentPayload {
            objective: IntentObjective {
                summary: "Test intent".to_string(),
                success_statement: "Success".to_string(),
                domain: "testing".to_string(),
            },
            scope: IntentScope {
                in_scope: vec!["item1".to_string()],
                out_of_scope: vec![],
            },
            constraints: IntentConstraints {
                functional: vec![],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![],
                forbidden_actions: vec![],
                approval_requirements: vec![],
            },
            preferences: IntentPreferences { tradeoffs: vec![] },
            references: IntentReferences {
                specs: vec![],
                tickets: vec![],
                repos: vec![],
                policies: vec![],
            },
            assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Medium,
                urgency: Urgency::Medium,
                confidence: 0.9,
            },
        }
    }

    fn create_test_request() -> CreateIntentRequest {
        CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        }
    }

    #[tokio::test]
    async fn test_create_intent() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let result = service.create_intent(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.current_version, 1);
        assert_eq!(response.status, IntentStatus::Active);

        // Verify intent was stored
        let head = service.get_intent_head(response.intent_id).await;
        assert!(head.is_ok());
    }

    #[tokio::test]
    async fn test_create_version() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        // Create initial intent
        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create new version
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "Updated constraints".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        let version_result = service
            .create_version(intent_id, version_request, None, None)
            .await;
        assert!(version_result.is_ok());
        let version_response = version_result.unwrap();
        assert_eq!(version_response.version_number, 2);

        // Verify versions list
        let versions = service.list_versions(intent_id).await;
        assert!(versions.is_ok());
        assert_eq!(versions.unwrap().total, 2);
    }

    #[tokio::test]
    async fn test_get_intent_head() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();

        let head = service.get_intent_head(response.intent_id).await;
        assert!(head.is_ok());
        let head = head.unwrap();
        assert_eq!(head.intent.id, response.intent_id);
        assert_eq!(head.version.version_number, 1);
    }

    #[tokio::test]
    async fn test_list_versions() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        let versions = service.list_versions(intent_id).await.unwrap();
        assert_eq!(versions.total, 1);
        assert_eq!(versions.versions.len(), 1);
    }

    #[tokio::test]
    async fn test_list_versions_descending_order() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Create version 3
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v3".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        let versions = service.list_versions(intent_id).await.unwrap();
        assert_eq!(versions.total, 3);
        // Should be descending order (3, 2, 1)
        assert_eq!(versions.versions[0].version_number, 3);
        assert_eq!(versions.versions[1].version_number, 2);
        assert_eq!(versions.versions[2].version_number, 1);
    }

    #[tokio::test]
    async fn test_get_specific_version() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Get version 1 specifically
        let v1 = service.get_version(intent_id, 1).await;
        assert!(v1.is_ok());
        assert_eq!(v1.unwrap().version_number, 1);

        // Get version 2 specifically
        let v2 = service.get_version(intent_id, 2).await;
        assert!(v2.is_ok());
        assert_eq!(v2.unwrap().version_number, 2);

        // Get nonexistent version
        let v99 = service.get_version(intent_id, 99).await;
        assert!(v99.is_err());
    }

    #[tokio::test]
    async fn test_sha256_hash_format() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();

        let head = service.get_intent_head(response.intent_id).await.unwrap();
        let hash = head.version.hash;

        // SHA-256 produces 64 character hex string
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_intent() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo);

        let result = service.get_intent_head(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_in_memory_repo_persistence() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service1 = IntentService::new(repo.clone());
        let service2 = IntentService::new(repo);

        let request = create_test_request();
        let response = service1.create_intent(request).await.unwrap();

        // Second service instance should see the same data
        let head = service2.get_intent_head(response.intent_id).await;
        assert!(head.is_ok());
    }

    // OCC tests - Optimistic Concurrency Control

    #[tokio::test]
    async fn test_occ_stale_version_rejected() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Try to create version 2 with OCC expecting version 1 (but current is 1, so this should work)
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        // First update with correct version should succeed
        let result = service
            .create_version(intent_id, version_request.clone(), Some(1), Some(0))
            .await;
        assert!(result.is_ok());

        // Now try to create another version expecting version 1 (stale)
        // but current is 2, so this should fail with ConcurrencyConflict
        let stale_result = service
            .create_version(intent_id, version_request.clone(), Some(1), Some(0))
            .await;
        assert!(stale_result.is_err());
        assert!(matches!(
            stale_result.unwrap_err(),
            IntentRebaseError::ConcurrencyConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_occ_omitted_headers_defaults_to_current_state() {
        // When OCC headers are omitted (None, None), the service uses the current server
        // state as expected values. This allows the operation to succeed when no one
        // else has modified the intent, but is unsafe if concurrent modifications exist.
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        // Without headers, the service defaults to current version (1) and row_version (0)
        // This succeeds because no concurrent modification has occurred
        let result = service
            .create_version(intent_id, version_request.clone(), None, None)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().version_number, 2);

        // Subsequent call without headers still succeeds (in-memory repo is single-threaded)
        // but the SQL repo would detect this as a conflict since row_version changed
        let result2 = service
            .create_version(intent_id, version_request, None, None)
            .await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().version_number, 3);
    }

    #[tokio::test]
    async fn test_occ_correct_version_succeeds() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Get the head to see current state
        let head = service.get_intent_head(intent_id).await.unwrap();

        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        // Create version with correct OCC values
        let result = service
            .create_version(
                intent_id,
                version_request,
                Some(head.intent.current_version),
                Some(head.row_version),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().version_number, 2);
    }

    #[tokio::test]
    async fn test_occ_row_version_not_tracked_in_memory() {
        // NOTE: In-memory repo does NOT properly track row_version.
        // This test documents that the wrong row_version is ignored in this implementation.
        // The SQL repository properly enforces row_version checks.
        // This test verifies the current (limiting) behavior, not the desired behavior.
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        let head = service.get_intent_head(intent_id).await.unwrap();

        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        // Use correct version but wrong row_version
        // In-memory repo ignores row_version, so this succeeds despite wrong value
        // SQL repo would properly reject this with ConcurrencyConflict
        let result = service
            .create_version(
                intent_id,
                version_request,
                Some(head.intent.current_version),
                Some(head.row_version + 999), // wrong row_version
            )
            .await;

        // In-memory behavior: succeeds because row_version is not enforced
        assert!(result.is_ok());
        assert_eq!(result.unwrap().version_number, 2);
    }

    // === Diff Tests ===

    #[tokio::test]
    async fn test_compute_diff_same_versions_error() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Try to diff v1 with v1 - should fail (from_version must be less than to_version)
        let result = service.compute_diff(intent_id, 1, 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("must be less than"));
    }

    #[tokio::test]
    async fn test_compute_diff_reversed_versions_error() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Try to diff v2 with v1 (reversed order) - should fail
        let result = service.compute_diff(intent_id, 2, 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("must be less than"));
    }

    #[tokio::test]
    async fn test_compute_diff_nonexistent_intent_error() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let result = service.compute_diff(Uuid::new_v4(), 1, 2).await;
        assert!(result.is_err());
        // The in-memory repo's get_versions_by_intent doesn't verify intent exists,
        // so we get InvalidIntentVersion rather than IntentNotFound
        let err = result.unwrap_err();
        assert!(err.to_string().contains("version") || err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_compute_diff_nonexistent_version_error() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Try to diff v1 with v99 - should fail with version not found
        let result = service.compute_diff(intent_id, 1, 99).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compute_diff_no_change() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create version 2 with identical payload
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2 identical".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Diff v1 to v2 should show no changes
        let result = service.compute_diff(intent_id, 1, 2).await;
        assert!(result.is_ok());
        let (_, _, diff, risk) = result.unwrap();

        // No changes in any section
        assert!(diff.scope.in_scope.added.is_empty());
        assert!(diff.scope.in_scope.removed.is_empty());
        assert!(diff.scope.out_of_scope.added.is_empty());
        assert!(diff.scope.out_of_scope.removed.is_empty());
        assert!(diff.constraints.functional.is_empty());
        assert!(diff.constraints.non_functional.is_empty());
        assert!(diff.constraints.policy.is_empty());
        assert!(diff.constraints.budget.is_empty());
        assert!(diff.constraints.time.is_empty());
        assert!(diff.acceptance_criteria.required.is_empty());
        assert!(diff.acceptance_criteria.optional.is_empty());
        assert!(diff.authority.allowed_actions.is_empty());
        assert!(diff.authority.forbidden_actions.is_empty());
        assert!(diff.authority.approval_requirements.is_empty());

        // Risk should be low with full confidence
        assert_eq!(risk.severity, Severity::Low);
        assert_eq!(risk.confidence, 1.0);
        assert!(!risk.manual_review);
    }

    #[tokio::test]
    async fn test_compute_diff_with_scope_change() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Create version 2 with scope change
        let mut payload = create_test_payload();
        payload.scope.in_scope.push("new item".to_string());

        let version_request = CreateVersionRequest {
            payload,
            change_reason: "added scope item".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Diff v1 to v2 should show scope change
        let result = service.compute_diff(intent_id, 1, 2).await;
        assert!(result.is_ok());
        let (_, _, diff, risk) = result.unwrap();

        // Scope has changes
        assert_eq!(diff.scope.in_scope.added, vec!["new item"]);
        assert!(diff.scope.in_scope.removed.is_empty());

        // Risk should be medium (scope changes are medium)
        assert_eq!(risk.severity, Severity::Medium);
        // Scope changes have no clause_ids, so confidence is 0.5 (below 0.7 threshold),
        // which triggers manual_review
        assert!(risk.manual_review);
    }

    #[tokio::test]
    async fn test_parent_version_id_set_on_v2_create() {
        // Verify that creating v2 sets parent_version_id to v1.id
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        // Create initial intent (v1)
        let request = create_test_request();
        let response = service.create_intent(request).await.unwrap();
        let intent_id = response.intent_id;

        // Get v1 to capture its ID
        let v1 = service.get_version(intent_id, 1).await.unwrap();
        let v1_id = v1.id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        let _v2_response = service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Verify v2's parent_version_id points to v1.id
        let v2 = service.get_version(intent_id, 2).await.unwrap();
        assert_eq!(v2.parent_version_id, Some(v1_id));

        // Create version 3 to verify chain continues
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v3".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        let _v3_response = service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Verify v3's parent_version_id points to v2.id
        let v2_for_v3 = service.get_version(intent_id, 2).await.unwrap();
        let v3 = service.get_version(intent_id, 3).await.unwrap();
        assert_eq!(v3.parent_version_id, Some(v2_for_v3.id));

        // Verify v1.parent_version_id is still None
        let v1_after = service.get_version(intent_id, 1).await.unwrap();
        assert_eq!(v1_after.parent_version_id, None);
    }

    // === Tenant Context Tests ===

    #[tokio::test]
    async fn test_create_intent_preserves_explicit_tenant_id() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let explicit_tenant_id = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let request = CreateIntentRequest {
            tenant_id: Some(explicit_tenant_id),
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let result = service.create_intent(request).await;
        assert!(result.is_ok());

        let head = service
            .get_intent_head(result.unwrap().intent_id)
            .await
            .unwrap();
        // Explicit tenant_id must be preserved, not replaced with random UUID
        assert_eq!(head.intent.tenant_id, explicit_tenant_id);
    }

    #[tokio::test]
    async fn test_create_intent_fallback_tenant_id_is_valid() {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        // Request with no explicit tenant_id (None)
        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let result = service.create_intent(request).await;
        assert!(result.is_ok());

        let head = service
            .get_intent_head(result.unwrap().intent_id)
            .await
            .unwrap();
        // Fallback tenant_id must be non-nil and valid UUID
        assert_ne!(head.intent.tenant_id, Uuid::nil());
    }

    #[tokio::test]
    async fn test_create_intent_none_tenant_id_differs_each_call() {
        // Verify that each call without explicit tenant_id gets a unique UUID
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = IntentService::new(repo.clone());

        let request1 = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test1".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let request2 = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test2".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let result1 = service.create_intent(request1).await.unwrap();
        let result2 = service.create_intent(request2).await.unwrap();

        let head1 = service.get_intent_head(result1.intent_id).await.unwrap();
        let head2 = service.get_intent_head(result2.intent_id).await.unwrap();

        // Each intent without explicit tenant_id should get a unique tenant_id
        assert_ne!(head1.intent.tenant_id, head2.intent.tenant_id);
    }
}
