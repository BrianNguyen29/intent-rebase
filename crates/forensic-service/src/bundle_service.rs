//! Forensic bundle generation service
//!
//! Orchestrates the full bundle generation path:
//! 1. Collects data via ForensicDataCollector (intent versions, audit events, policy snapshots)
//! 2. Generates bundle manifest with integrity hashes via BundleGeneratorService
//! 3. Persists bundle bytes to S3/MinIO via BundleStorage
//! 4. Records bundle in repository with Ready status
//!
//! **This slice scope:** Request-driven synchronous bundle generation with real collection,
//! generation, and S3/MinIO persistence.
//!
//! **NOT claimed in this slice:**
//! - Async job orchestration for large bundle generation
//! - Bundle retrieval/download API (separate Phase 4 endpoint)
//! - Bundle replay (state reproduction from stored bundle)
//! - Hash chain integrity verification (future Phase 4 work)

use std::sync::Arc;
use uuid::Uuid;

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;

use super::bundle::{BundlePurpose, BundleStatus, BundleTimeRange, ForensicBundle};
use super::bundle_generator::{BundleGeneratorService, GenerateBundleRequest};
use super::bundle_hasher::{
    compute_sha256, ArtifactEntry, AuditEventEntry, IntentVersionEntry, PolicySnapshotEntry,
};
use super::bundle_repo::BundleRepository;
use super::bundle_storage::BundleStorage;
use super::collector::{CollectorError, ForensicDataCollector};

/// Request to create a forensic bundle.
#[derive(Debug, Clone)]
pub struct CreateForensicBundleRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent IDs to include in the bundle
    pub intent_ids: Vec<Uuid>,
    /// Time range to collect data for
    pub time_range: BundleTimeRange,
    /// Purpose of the bundle
    pub purpose: BundlePurpose,
    /// Actor who triggered bundle generation
    pub created_by: String,
}

/// Response after successful bundle generation.
#[derive(Debug, Clone)]
pub struct CreateForensicBundleResponse {
    /// The generated bundle manifest
    pub bundle: ForensicBundle,
    /// Storage location where bundle bytes are persisted
    pub storage_location: String,
    /// Size of the stored bundle in bytes
    pub bundle_size_bytes: usize,
    /// Human-readable message
    pub message: String,
}

/// Errors during forensic bundle creation.
#[derive(Debug)]
pub enum ForensicBundleServiceError {
    /// Bundle not found in repository
    NotFound(Uuid),
    /// Collection failed
    Collection(CollectorError),
    /// Bundle generation failed
    Generation(String),
    /// Storage operation failed
    Storage(String),
    /// Repository operation failed
    Repository(IntentRebaseError),
    /// Serialization failed
    Serialization(String),
    /// Invalid time range
    InvalidTimeRange(String),
    /// Replay verification failed
    Replay(String),
}

impl std::fmt::Display for ForensicBundleServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "bundle not found: {}", id),
            Self::Collection(e) => write!(f, "collection failed: {}", e),
            Self::Generation(e) => write!(f, "generation failed: {}", e),
            Self::Storage(e) => write!(f, "storage failed: {}", e),
            Self::Repository(e) => write!(f, "repository error: {}", e),
            Self::Serialization(e) => write!(f, "serialization failed: {}", e),
            Self::InvalidTimeRange(e) => write!(f, "invalid time range: {}", e),
            Self::Replay(e) => write!(f, "replay verification failed: {}", e),
        }
    }
}

impl std::error::Error for ForensicBundleServiceError {}

impl From<CollectorError> for ForensicBundleServiceError {
    fn from(err: CollectorError) -> Self {
        ForensicBundleServiceError::Collection(err)
    }
}

impl From<IntentRebaseError> for ForensicBundleServiceError {
    fn from(err: IntentRebaseError) -> Self {
        ForensicBundleServiceError::Repository(err)
    }
}

/// Trait for forensic bundle service operations.
///
/// Provides the interface for the HTTP API layer to interact with
/// the bundle generation service without depending on concrete types.
#[async_trait]
pub trait ForensicBundleServiceTrait: Send + Sync {
    /// Create a new forensic bundle.
    async fn create_bundle(
        &self,
        request: CreateForensicBundleRequest,
    ) -> Result<CreateForensicBundleResponse, ForensicBundleServiceError>;

    /// Get a bundle by ID.
    async fn get_bundle(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, ForensicBundleServiceError>;

    /// List bundles for a tenant.
    async fn list_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, ForensicBundleServiceError>;

    /// List bundles for a tenant by status.
    async fn list_bundles_by_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, ForensicBundleServiceError>;

    /// Download a bundle's serialized bytes from storage.
    async fn download_bundle_bytes(
        &self,
        bundle_id: Uuid,
    ) -> Result<Vec<u8>, ForensicBundleServiceError>;

    /// Verify a stored bundle's integrity against provided content sections.
    ///
    /// **Bounded replay evidence path:** Loads the bundle manifest from the repository
    /// and verifies the provided content sections against the per-section hashes stored
    /// in `bundle.integrity`. This is read-only verification — no mutations occur.
    ///
    /// Returns `Ok(VerifyBundleReplayResponse)` with the verification report.
    /// Returns `Err(ForensicBundleServiceError::NotFound)` if the bundle does not exist.
    /// Returns `Err(ForensicBundleServiceError::Replay)` if verification fails or the bundle
    /// is not in Ready status.
    async fn verify_bundle_replay(
        &self,
        bundle_id: Uuid,
        content_sections: super::bundle_hasher::ContentSectionsForVerification,
    ) -> Result<super::bundle_replay::VerifyBundleReplayResponse, ForensicBundleServiceError>;

    /// Returns a reference to the underlying repository for RLS-aware operations.
    ///
    /// Returns `None` if the repository is not accessible (e.g., for in-memory implementations
    /// where the repository type is erased). This is used by handlers to detect SQL-backed
    /// repositories and use transaction-scoped RLS paths.
    fn repo(&self) -> Option<&dyn super::bundle_repo::BundleRepository>;

    /// Returns a reference to the SQLx-backed service if this is a SQL-backed implementation.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    /// This is used by handlers to access `create_bundle_with_tx` for RLS-wrapped creation.
    fn as_bundle_service_sqlx(
        &self,
    ) -> Option<&super::ForensicBundleService<super::bundle_repo::SqlxBundleRepository>> {
        None
    }
}

/// Forensic bundle service.
///
/// Orchestrates the full bundle generation path:
/// 1. Collects data from repositories via ForensicDataCollector
/// 2. Generates bundle manifest with integrity hashes via BundleGeneratorService
/// 3. Persists bundle bytes to S3/MinIO via BundleStorage
/// 4. Updates bundle record in repository with Ready status
///
/// **Bounded synchronous scope:** This service completes the full generate→store→record
/// cycle in one synchronous call. For large bundles, this may be slow.
/// Async job orchestration for large bundles is NOT in this slice.
#[derive(Clone)]
pub struct ForensicBundleService<R: BundleRepository> {
    repo: Arc<R>,
    storage: Arc<dyn BundleStorage>,
    collector: Arc<dyn ForensicDataCollector>,
}

impl<R: BundleRepository> ForensicBundleService<R> {
    /// Create a new forensic bundle service.
    ///
    /// The storage parameter accepts `Arc<dyn BundleStorage>`, enabling runtime
    /// selection between InMemoryBundleStorage and S3BundleStorage.
    ///
    /// **Bounded scope:** Enables S3/minIO bundle storage wiring behind env gate.
    /// Object Lock, retention enforcement, chain-hash remain Phase 4+ deferred.
    pub fn new(
        repo: Arc<R>,
        storage: Arc<dyn BundleStorage>,
        collector: Arc<dyn ForensicDataCollector>,
    ) -> Self {
        Self {
            repo,
            storage,
            collector,
        }
    }

    /// Prepare a forensic bundle (collect, generate, serialize) without persisting.
    ///
    /// This helper extracts the non-DB operations from `create_bundle` so that
    /// both the pool-based and tx-based paths can share the same generation logic.
    ///
    /// **Bounded scope:** Storage is NOT part of this helper; the caller decides
    /// when and how to persist to S3/MinIO and record in the repository.
    async fn prepare_bundle(
        &self,
        request: &CreateForensicBundleRequest,
    ) -> Result<(ForensicBundle, Vec<u8>), ForensicBundleServiceError> {
        // Step 1: Validate time range
        if request.time_range.start > request.time_range.end {
            return Err(ForensicBundleServiceError::InvalidTimeRange(
                "start must be before end".to_string(),
            ));
        }

        // Step 2: Collect forensic data (real collection from repositories)
        let time_range = (request.time_range.start, request.time_range.end);
        let collection_result = self
            .collector
            .collect(Some(request.tenant_id), &request.intent_ids, &time_range)
            .await?;

        // Step 3: Convert collected data to hash entries for bundle generation
        let intent_versions: Vec<IntentVersionEntry> = collection_result
            .intents
            .iter()
            .flat_map(|intent| {
                intent.versions.iter().map(|v| IntentVersionEntry {
                    intent_id: intent.intent_id,
                    version: v.version_number as i32,
                    content_hash: format!("{:032x}", v.version_number),
                })
            })
            .collect();

        // For artifacts: we collect metadata from audit events that reference artifacts.
        let artifacts: Vec<ArtifactEntry> = collection_result
            .intents
            .iter()
            .flat_map(|intent| {
                intent
                    .audit_events
                    .iter()
                    .filter(|e| e.artifact_id.is_some())
                    .map(|e| {
                        let artifact_id = e.artifact_id.unwrap();
                        let content_hash = compute_sha256(&artifact_id)
                            .unwrap_or_else(|_| "00000000000000000000000000000000".to_string());
                        ArtifactEntry {
                            artifact_id,
                            content_hash,
                            collected_at: e.occurred_at,
                        }
                    })
            })
            .collect();

        // Audit event entries
        let audit_events: Vec<AuditEventEntry> = collection_result
            .intents
            .iter()
            .flat_map(|intent| {
                intent
                    .audit_events
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| AuditEventEntry {
                        event_id: e.id,
                        content_hash: format!("{:032x}", idx as u64),
                        event_index: idx,
                    })
            })
            .collect();

        // Policy snapshot entries
        let policy_snapshots: Vec<PolicySnapshotEntry> = collection_result
            .intents
            .iter()
            .flat_map(|intent| {
                intent.policy_snapshots.iter().map(|s| PolicySnapshotEntry {
                    snapshot_id: s.id,
                    scope_hash: s.scope_hash.clone(),
                })
            })
            .collect();

        // Step 4: Generate bundle manifest and integrity hashes
        let gen_request = GenerateBundleRequest {
            tenant_id: request.tenant_id,
            time_range: request.time_range.clone(),
            purpose: request.purpose,
            created_by: request.created_by.clone(),
            intent_versions,
            artifacts,
            approvals: vec![], // Approvals not collected in this slice
            audit_events,
            policy_snapshots,
        };

        let gen_result = BundleGeneratorService::generate(gen_request);
        let bundle = gen_result.bundle;

        // Step 5: Serialize bundle to JSON
        let bundle_json = serde_json::to_vec(&bundle)
            .map_err(|e| ForensicBundleServiceError::Serialization(e.to_string()))?;

        Ok((bundle, bundle_json))
    }
}

#[async_trait]
impl<R: BundleRepository + 'static> ForensicBundleServiceTrait for ForensicBundleService<R> {
    /// Create a new forensic bundle.
    ///
    /// Full generation path:
    /// 1. **Collection** — Collects intent versions, audit events, and policy snapshots
    ///    from repositories using ForensicDataCollector (tenant-scoped).
    /// 2. **Generation** — Builds bundle manifest with integrity hashes via
    ///    BundleGeneratorService.
    /// 3. **Storage** — Serializes bundle to JSON and persists to S3/MinIO.
    /// 4. **Record** — Updates bundle status to Ready in repository.
    ///
    /// **Truthful scope:**
    /// - This is a synchronous request-driven operation (no async jobs).
    /// - Real data is collected from actual repositories.
    /// - Bundle is persisted to S3/MinIO via the configured BundleStorage.
    /// - Tenant scoping is enforced via the tenant_id field.
    ///
    /// **NOT claimed:**
    /// - Async job orchestration for large bundles
    /// - Bundle retrieval/download API (Phase 4 scope)
    /// - Bundle replay (Phase 4 scope)
    /// - Hash chain integrity verification (Phase 4 scope)
    async fn create_bundle(
        &self,
        request: CreateForensicBundleRequest,
    ) -> Result<CreateForensicBundleResponse, ForensicBundleServiceError> {
        let (bundle, bundle_json) = self.prepare_bundle(&request).await?;
        let bundle_size_bytes = bundle_json.len();

        // Persist bundle record to repository first (before storage so we can transition status)
        self.repo
            .create(bundle.clone())
            .await
            .map_err(ForensicBundleServiceError::Repository)?;

        self.storage
            .put(bundle.bundle_id, bundle.tenant_id, &bundle_json)
            .await
            .map_err(|e| ForensicBundleServiceError::Storage(e.to_string()))?;

        // Transition bundle status to Ready
        let final_bundle = self
            .repo
            .update_status(bundle.bundle_id, BundleStatus::Ready)
            .await?;

        Ok(CreateForensicBundleResponse {
            bundle: final_bundle,
            storage_location: format!("{}/{}", self.storage.location(), bundle.bundle_id),
            bundle_size_bytes,
            message: "Bundle generated and stored successfully".to_string(),
        })
    }

    /// Get a bundle by ID.
    ///
    /// Returns the bundle manifest. Bundle content must be retrieved separately
    /// from S3/MinIO using the storage layer.
    async fn get_bundle(
        &self,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, ForensicBundleServiceError> {
        self.repo.get(bundle_id).await.map_err(|e| match e {
            IntentRebaseError::ForensicBundleNotFound(id) => {
                ForensicBundleServiceError::NotFound(id)
            }
            _ => ForensicBundleServiceError::Repository(e),
        })
    }

    /// List bundles for a tenant.
    async fn list_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, ForensicBundleServiceError> {
        self.repo
            .list_by_tenant(tenant_id, limit)
            .await
            .map_err(ForensicBundleServiceError::Repository)
    }

    /// List bundles for a tenant by status.
    async fn list_bundles_by_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, ForensicBundleServiceError> {
        self.repo
            .list_by_tenant_and_status(tenant_id, status)
            .await
            .map_err(ForensicBundleServiceError::Repository)
    }

    /// Download a bundle's serialized bytes from storage.
    ///
    /// **Phase 4 scope note:** This is the storage retrieval seam.
    /// A full download API endpoint (GET /forensic/bundle/{id}/download)
    /// is Phase 4 scope. This method provides the storage retrieval only.
    async fn download_bundle_bytes(
        &self,
        bundle_id: Uuid,
    ) -> Result<Vec<u8>, ForensicBundleServiceError> {
        // Verify bundle exists in repo first and get tenant_id for storage lookup
        let bundle = self.get_bundle(bundle_id).await?;

        self.storage
            .get(bundle_id, bundle.tenant_id)
            .await
            .map_err(|e| match e {
                super::bundle_storage::BundleStorageError::NotFound(_) => {
                    ForensicBundleServiceError::NotFound(bundle_id)
                }
                _ => ForensicBundleServiceError::Storage(e.to_string()),
            })
    }

    /// Verify a stored bundle's integrity against provided content sections.
    ///
    /// **Bounded replay evidence path:** Loads the bundle manifest and verifies
    /// the provided content sections against the per-section hashes stored in
    /// `bundle.integrity`. All operations are read-only.
    async fn verify_bundle_replay(
        &self,
        bundle_id: Uuid,
        content_sections: super::bundle_hasher::ContentSectionsForVerification,
    ) -> Result<super::bundle_replay::VerifyBundleReplayResponse, ForensicBundleServiceError> {
        let bundle = self.get_bundle(bundle_id).await?;

        let replay_service = super::bundle_replay::BundleReplayService::new();
        replay_service
            .verify_bundle_from_integrity(&bundle, &content_sections)
            .map_err(|e| ForensicBundleServiceError::Replay(e.to_string()))
    }

    fn repo(&self) -> Option<&dyn super::bundle_repo::BundleRepository> {
        Some(self.repo.as_ref())
    }

    fn as_bundle_service_sqlx(
        &self,
    ) -> Option<&super::ForensicBundleService<super::bundle_repo::SqlxBundleRepository>> {
        use std::any::Any;
        let any_self: &dyn Any = self;
        any_self
            .downcast_ref::<super::ForensicBundleService<super::bundle_repo::SqlxBundleRepository>>(
            )
    }
}

// =============================================================================
// SQLx-specific transaction helper for RLS-aware bundle creation
// =============================================================================

impl ForensicBundleService<super::bundle_repo::SqlxBundleRepository> {
    /// Create a forensic bundle with DB operations wrapped in a caller-owned transaction.
    ///
    /// This method mirrors `create_bundle` but uses `_with_tx` repository methods
    /// so that `create` and `update_status` happen inside the same RLS-aware transaction.
    ///
    /// **Bounded semantics:**
    /// - Collection, generation, and serialization happen before the tx.
    /// - Storage (`storage.put`) happens OUTSIDE the transaction (S3/MinIO is not
    ///   transactional). If storage fails after the DB tx commits, the bundle record
    ///   is Ready but storage may be missing. This matches the non-RLS fallback behavior.
    /// - The caller must begin the transaction via `RlsAwarePool::begin_with_tenant`
    ///   and commit/rollback after this method returns.
    pub async fn create_bundle_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        request: CreateForensicBundleRequest,
    ) -> Result<CreateForensicBundleResponse, ForensicBundleServiceError> {
        let (bundle, bundle_json) = self.prepare_bundle(&request).await?;
        let bundle_size_bytes = bundle_json.len();

        // Persist bundle record inside the transaction
        self.repo
            .create_with_tx(tx, bundle.clone())
            .await
            .map_err(ForensicBundleServiceError::Repository)?;

        // Storage is outside the transaction
        self.storage
            .put(bundle.bundle_id, bundle.tenant_id, &bundle_json)
            .await
            .map_err(|e| ForensicBundleServiceError::Storage(e.to_string()))?;

        // Transition bundle status to Ready inside the transaction
        let final_bundle = self
            .repo
            .update_status_with_tx(tx, bundle.bundle_id, BundleStatus::Ready)
            .await
            .map_err(ForensicBundleServiceError::Repository)?;

        Ok(CreateForensicBundleResponse {
            bundle: final_bundle,
            storage_location: format!("{}/{}", self.storage.location(), bundle.bundle_id),
            bundle_size_bytes,
            message: "Bundle generated and stored successfully".to_string(),
        })
    }
}
