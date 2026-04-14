//! Bundle generation service
//!
//! Phase 3 Batch 3b (P4 bounded slice): Orchestrates forensic bundle creation
//! by transitioning bundle status through Pending -> Generating -> Ready/Failed.
//!
//! **This slice scope:** Bundle generation service with status management.
//! **Out of scope:** S3 storage, actual content collection, integrity hashing,
//! bundle download/export, and replay functionality.

use uuid::Uuid;

use intent_rebase_types::IntentRebaseError;

use super::bundle::{BundlePurpose, BundleStatus, BundleTimeRange, ForensicBundle};
use super::bundle_contents::BundleContents;
use super::bundle_repo::BundleRepository;

/// Request to create a new forensic bundle.
#[derive(Debug, Clone)]
pub struct CreateBundleRequest {
    pub tenant_id: Uuid,
    pub time_range: BundleTimeRange,
    pub purpose: BundlePurpose,
    pub created_by: String,
}

/// Response after successfully initiating bundle creation.
#[derive(Debug, Clone)]
pub struct CreateBundleResponse {
    pub bundle: ForensicBundle,
    pub message: String,
}

/// Errors that can occur during bundle generation.
#[derive(Debug)]
pub enum BundleGenError {
    /// Bundle not found in repository
    NotFound(Uuid),
    /// Invalid status transition
    InvalidTransition {
        from: BundleStatus,
        to: BundleStatus,
        reason: String,
    },
    /// Serialization error (e.g., JSON encoding failure)
    Serialization(String),
    /// Repository-level error
    Repository(IntentRebaseError),
}

impl From<IntentRebaseError> for BundleGenError {
    fn from(err: IntentRebaseError) -> Self {
        BundleGenError::Repository(err)
    }
}

/// Bundle generation service.
///
/// Orchestrates forensic bundle lifecycle by:
/// 1. Creating bundle manifest with Pending status
/// 2. Transitioning to Generating (simulated generation start)
/// 3. Transitioning to Ready (simulated completion)
///
/// **Bounded slice note:** Actual content collection is Phase 4 scope.
/// This service manages status transitions only, using placeholders for
/// content counts until Phase 4 integration.
#[derive(Clone)]
pub struct BundleGenerationService<R: BundleRepository> {
    repo: Arc<R>,
}

impl<R: BundleRepository> BundleGenerationService<R> {
    /// Create a new BundleGenerationService.
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// Initiate bundle creation: creates manifest with Pending status.
    ///
    /// Returns the created bundle with Pending status.
    pub async fn initiate_bundle(
        &self,
        request: CreateBundleRequest,
    ) -> Result<CreateBundleResponse, BundleGenError> {
        // Create bundle manifest with Pending status
        let bundle = ForensicBundle::new(
            request.tenant_id,
            request.time_range,
            request.purpose,
            BundleContents::default(), // Placeholder until Phase 4 content collection
            &request.created_by,
            None,
        );

        let created = self.repo.create(bundle).await.map_err(BundleGenError::Repository)?;

        Ok(CreateBundleResponse {
            bundle: created,
            message: "Bundle creation initiated. Use transition_to_generating to start generation."
                .to_string(),
        })
    }

    /// Transition bundle from Pending to Generating.
    ///
    /// **Bounded slice:** This is a no-op placeholder for actual generation logic.
    /// In Phase 4, this will trigger actual content collection.
    pub async fn transition_to_generating(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, BundleGenError> {
        let bundle = self
            .repo
            .update_status(bundle_id, BundleStatus::Generating)
            .await
            .map_err(|e| match e {
                IntentRebaseError::ForensicBundleNotFound(id) => {
                    BundleGenError::NotFound(id)
                }
                IntentRebaseError::InvalidForensicBundleStatusTransition {
                    from_status,
                    to_status,
                    reason,
                } => BundleGenError::InvalidTransition {
                    from: match from_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Pending,
                    },
                    to: match to_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Generating,
                    },
                    reason,
                },
                _ => BundleGenError::Repository(e),
            })?;

        Ok(bundle)
    }

    /// Transition bundle from Generating to Ready (generation completed).
    ///
    /// **Bounded slice:** This is a placeholder. Actual bundle assembly,
    /// integrity hashing, and S3 storage are Phase 4 scope.
    pub async fn transition_to_ready(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, BundleGenError> {
        let bundle = self
            .repo
            .update_status(bundle_id, BundleStatus::Ready)
            .await
            .map_err(|e| match e {
                IntentRebaseError::ForensicBundleNotFound(id) => {
                    BundleGenError::NotFound(id)
                }
                IntentRebaseError::InvalidForensicBundleStatusTransition {
                    from_status,
                    to_status,
                    reason,
                } => BundleGenError::InvalidTransition {
                    from: match from_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Generating,
                    },
                    to: match to_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Ready,
                    },
                    reason,
                },
                _ => BundleGenError::Repository(e),
            })?;

        Ok(bundle)
    }

    /// Transition bundle from Generating to Failed.
    ///
    /// Use this when generation encounters a terminal error.
    pub async fn transition_to_failed(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, BundleGenError> {
        let bundle = self
            .repo
            .update_status(bundle_id, BundleStatus::Failed)
            .await
            .map_err(|e| match e {
                IntentRebaseError::ForensicBundleNotFound(id) => {
                    BundleGenError::NotFound(id)
                }
                IntentRebaseError::InvalidForensicBundleStatusTransition {
                    from_status,
                    to_status,
                    reason,
                } => BundleGenError::InvalidTransition {
                    from: match from_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Generating,
                    },
                    to: match to_status.as_str() {
                        "Pending" => BundleStatus::Pending,
                        "Generating" => BundleStatus::Generating,
                        "Ready" => BundleStatus::Ready,
                        "Failed" => BundleStatus::Failed,
                        _ => BundleStatus::Failed,
                    },
                    reason,
                },
                _ => BundleGenError::Repository(e),
            })?;

        Ok(bundle)
    }

    /// Complete bundle creation: transitions through full Pending -> Generating -> Ready cycle.
    ///
    /// **Bounded slice:** This simulates the full generation flow for testing.
    /// In Phase 4, this will involve actual content collection and S3 storage.
    pub async fn complete_bundle_creation(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, BundleGenError> {
        // Transition Pending -> Generating
        self.transition_to_generating(bundle_id).await?;

        // Transition Generating -> Ready
        self.transition_to_ready(bundle_id).await
    }

    /// Get a bundle by ID.
    pub async fn get_bundle(&self, bundle_id: Uuid) -> Result<ForensicBundle, BundleGenError> {
        self.repo
            .get(bundle_id)
            .await
            .map_err(|e| match e {
                IntentRebaseError::ForensicBundleNotFound(id) => BundleGenError::NotFound(id),
                _ => BundleGenError::Repository(e),
            })
    }

    /// List all bundles for a tenant.
    pub async fn list_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, BundleGenError> {
        self.repo
            .list_by_tenant(tenant_id, limit)
            .await
            .map_err(BundleGenError::Repository)
    }

    /// List bundles by tenant and status.
    pub async fn list_bundles_by_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, BundleGenError> {
        self.repo
            .list_by_tenant_and_status(tenant_id, status)
            .await
            .map_err(BundleGenError::Repository)
    }

    /// Download a bundle as serialized JSON bytes.
    ///
    /// Returns the bundle manifest serialized to JSON format, suitable for
    /// local storage or download. The JSON can be used to reconstruct the
    /// bundle or verify its contents.
    ///
    /// **Bounded slice scope:** Returns the bundle manifest only (no actual
    /// content collection). S3 storage and retrieval are Phase 4 scope.
    pub async fn download_bundle(
        &self,
        bundle_id: Uuid,
    ) -> Result<Vec<u8>, BundleGenError> {
        let bundle = self.get_bundle(bundle_id).await?;
        serde_json::to_vec_pretty(&bundle)
            .map_err(|e| BundleGenError::Serialization(format!("JSON serialization failed: {}", e)))
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle_repo::InMemoryBundleRepository;
    use chrono::Utc;

    fn create_test_request(tenant_id: Uuid) -> CreateBundleRequest {
        CreateBundleRequest {
            tenant_id,
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "test-user".to_string(),
        }
    }

    #[tokio::test]
    async fn test_initiate_bundle() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let response = service.initiate_bundle(request).await.unwrap();

        assert_eq!(response.bundle.tenant_id, tenant_id);
        assert_eq!(response.bundle.status, BundleStatus::Pending);
        assert_eq!(response.bundle.purpose, BundlePurpose::IncidentInvestigation);
    }

    #[tokio::test]
    async fn test_transition_to_generating() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        let bundle = service.transition_to_generating(bundle_id).await.unwrap();

        assert_eq!(bundle.status, BundleStatus::Generating);
    }

    #[tokio::test]
    async fn test_transition_to_ready() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        service.transition_to_generating(bundle_id).await.unwrap();
        let bundle = service.transition_to_ready(bundle_id).await.unwrap();

        assert_eq!(bundle.status, BundleStatus::Ready);
    }

    #[tokio::test]
    async fn test_transition_to_failed() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        service.transition_to_generating(bundle_id).await.unwrap();
        let bundle = service.transition_to_failed(bundle_id).await.unwrap();

        assert_eq!(bundle.status, BundleStatus::Failed);
    }

    #[tokio::test]
    async fn test_complete_bundle_creation() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        let bundle = service.complete_bundle_creation(bundle_id).await.unwrap();

        assert_eq!(bundle.status, BundleStatus::Ready);
    }

    #[tokio::test]
    async fn test_get_bundle() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        let bundle = service.get_bundle(bundle_id).await.unwrap();

        assert_eq!(bundle.bundle_id, bundle_id);
    }

    #[tokio::test]
    async fn test_get_bundle_not_found() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let fake_id = Uuid::new_v4();

        let result = service.get_bundle(fake_id).await;

        assert!(matches!(result, Err(BundleGenError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_list_bundles() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();

        for _ in 0..3 {
            let request = create_test_request(tenant_id);
            service.initiate_bundle(request).await.unwrap();
        }

        let bundles = service.list_bundles(tenant_id, None).await.unwrap();

        assert_eq!(bundles.len(), 3);
    }

    #[tokio::test]
    async fn test_list_bundles_by_status() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();

        let request1 = create_test_request(tenant_id);
        let resp1 = service.initiate_bundle(request1).await.unwrap();

        let request2 = create_test_request(tenant_id);
        let resp2 = service.initiate_bundle(request2).await.unwrap();
        service
            .transition_to_generating(resp2.bundle.bundle_id)
            .await
            .unwrap();

        let pending = service
            .list_bundles_by_status(tenant_id, BundleStatus::Pending)
            .await
            .unwrap();
        let generating = service
            .list_bundles_by_status(tenant_id, BundleStatus::Generating)
            .await
            .unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(generating.len(), 1);
    }

    #[tokio::test]
    async fn test_invalid_transition_pending_to_ready() {
        let repo = Arc::new(InMemoryBundleRepository::new());
        let service = BundleGenerationService::new(repo);
        let tenant_id = Uuid::new_v4();
        let request = create_test_request(tenant_id);

        let init_response = service.initiate_bundle(request).await.unwrap();
        let bundle_id = init_response.bundle.bundle_id;

        // Try invalid transition: Pending -> Ready (must go through Generating)
        let result = service.transition_to_ready(bundle_id).await;

        assert!(matches!(result, Err(BundleGenError::InvalidTransition { .. })));
    }
}
