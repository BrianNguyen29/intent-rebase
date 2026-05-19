//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 1: First slice implementation with in-memory repository.
//! Repository trait allows swapping to SQL-backed implementation.

pub mod approval_request_repo;
pub mod checkpoint_repo;
pub mod checkpoint_service;
pub mod event_consumer;
pub mod policy_snapshot_repo;
pub mod propagation_record_repo;
pub mod s3_snapshot_storage;
pub mod sqlx_approval_request_repo;
pub mod sqlx_repository;

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    get_current_trace_context, AffectedItem, AffectedItemsPreview, ApprovalCancelledAuditPayload,
    AuditRepository, ChangeChannel, CreateIntentRequest, CreateIntentResponse,
    CreateVersionRequest, CreateVersionResponse, Intent, IntentHeadResponse, IntentRebaseError,
    IntentStatus, IntentVersion, ListVersionsResponse, NodeType, VersionStatus,
};
use rebase_engine::{compute_diff_with_risk_sync, DiffRiskAnalysis, IntentVersionDiff, RebasePlan};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use approval_request_repo::{
    ApprovalRequest, ApprovalRequestRepository, ApprovalRequestStatus,
    InMemoryApprovalRequestRepository,
};
pub use checkpoint_repo::{
    CheckpointRepository, InMemoryCheckpointRepository, SqlxCheckpointRepository,
};
pub use checkpoint_service::CheckpointService;
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

/// In-memory implementation for testing and Phase 1
pub struct InMemoryIntentRepository {
    intents: RwLock<HashMap<Uuid, Intent>>,
    versions: RwLock<HashMap<Uuid, IntentVersion>>,
    versions_by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryIntentRepository {
    pub fn new() -> Self {
        Self {
            intents: RwLock::new(HashMap::new()),
            versions: RwLock::new(HashMap::new()),
            versions_by_intent: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryIntentRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Generates a default tenant ID for in-memory repository when no explicit tenant context is provided.
///
/// This is a fallback for Phase 1 in-memory repository only.
/// In production with SQL-backed repository, tenant context should be extracted from auth middleware.
fn generate_default_tenant_id() -> Uuid {
    Uuid::new_v4()
}

#[async_trait]
impl IntentRepository for InMemoryIntentRepository {
    async fn create_intent_tx(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        let intent_id = Uuid::new_v4();
        let now = Utc::now();
        // Use explicit tenant_id from request if provided; otherwise generate a default.
        // In-memory repository has no auth context to extract tenant from.
        // SQL-backed repository should use TenantResolver from sqlx_repository module.
        let tenant_id = request.tenant_id.unwrap_or_else(generate_default_tenant_id);

        // Create the intent document
        let intent = Intent {
            id: intent_id,
            tenant_id,
            workflow_id: request.workflow_id,
            current_version: 1,
            status: IntentStatus::Active,
            created_at: now,
            created_by: request.created_by.clone(),
            source_refs: request.source_refs.clone(),
            tags: request.tags.clone(),
        };

        // Create initial version
        let version_id = Uuid::new_v4();
        let payload_hash = compute_payload_hash(&request.payload);

        let version = IntentVersion {
            id: version_id,
            intent_id,
            version_number: 1,
            parent_version_id: None,
            created_at: now,
            created_by: request.created_by.clone(),
            change_reason: "Initial creation".to_string(),
            change_channel: ChangeChannel::UserEdit,
            status: VersionStatus::Active,
            hash: payload_hash,
            payload: request.payload,
        };

        // Persist both atomically
        let mut intents = self.intents.write().await;
        let mut versions = self.versions.write().await;
        let mut versions_by_intent = self.versions_by_intent.write().await;

        intents.insert(intent.id, intent);
        versions.insert(version.id, version);
        versions_by_intent.insert(intent_id, vec![version_id]);

        Ok(CreateIntentResponse {
            intent_id,
            current_version: 1,
            status: IntentStatus::Active,
        })
    }

    async fn get_intent(&self, id: Uuid) -> Result<Intent, IntentRebaseError> {
        let intents = self.intents.read().await;
        intents
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::IntentNotFound(id))
    }

    async fn create_version_with_occ(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: i32,
        _expected_row_version: i32,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let mut intents = self.intents.write().await;
        let mut versions = self.versions.write().await;
        let mut versions_by_intent = self.versions_by_intent.write().await;

        let intent = intents
            .get(&intent_id)
            .ok_or(IntentRebaseError::IntentNotFound(intent_id))?;

        // OCC check (in-memory version check only, row_version not tracked)
        if intent.current_version != expected_version {
            return Err(IntentRebaseError::ConcurrencyConflict(intent_id));
        }

        let new_version_number = intent.current_version + 1;
        let now = Utc::now();
        let payload_hash = compute_payload_hash(&request.payload);

        // Look up the previous version to set parent_version_id
        let parent_version_id = versions_by_intent
            .get(&intent_id)
            .and_then(|ids| ids.last())
            .copied();

        let version_id = Uuid::new_v4();
        let version = IntentVersion {
            id: version_id,
            intent_id,
            version_number: new_version_number,
            parent_version_id,
            created_at: now,
            created_by: request.created_by.clone(),
            change_reason: request.change_reason.clone(),
            change_channel: request.change_channel.clone(),
            status: VersionStatus::Active,
            hash: payload_hash,
            payload: request.payload,
        };

        // Update intent's current version
        let mut updated_intent = intent.clone();
        updated_intent.current_version = new_version_number;
        intents.insert(intent_id, updated_intent);

        // Persist version
        versions.insert(version_id, version.clone());
        versions_by_intent
            .entry(intent_id)
            .or_insert_with(Vec::new)
            .push(version_id);

        Ok(CreateVersionResponse {
            intent_version_id: version_id,
            intent_id,
            version_number: new_version_number,
            status: VersionStatus::Active,
        })
    }

    async fn get_version(&self, id: Uuid) -> Result<IntentVersion, IntentRebaseError> {
        let versions = self.versions.read().await;
        versions
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::IntentVersionNotFound(id))
    }

    async fn get_versions_by_intent(
        &self,
        intent_id: Uuid,
    ) -> Result<Vec<IntentVersion>, IntentRebaseError> {
        let versions_by_intent = self.versions_by_intent.read().await;
        let versions = self.versions.read().await;

        let version_ids = versions_by_intent
            .get(&intent_id)
            .cloned()
            .unwrap_or_default();
        let mut result: Vec<IntentVersion> = version_ids
            .iter()
            .filter_map(|id| versions.get(id).cloned())
            .collect();

        result.sort_by_key(|a| a.version_number);
        Ok(result)
    }

    async fn get_version_by_intent_and_number(
        &self,
        intent_id: Uuid,
        version_number: i32,
    ) -> Result<IntentVersion, IntentRebaseError> {
        let versions = self.get_versions_by_intent(intent_id).await?;
        versions
            .into_iter()
            .find(|v| v.version_number == version_number)
            .ok_or_else(|| {
                IntentRebaseError::InvalidIntentVersion(format!(
                    "version {} not found for intent {}",
                    version_number, intent_id
                ))
            })
    }

    async fn get_intent_for_update(&self, id: Uuid) -> Result<(Intent, i32), IntentRebaseError> {
        // In-memory repo doesn't track row_version, return 0 as placeholder
        let intent = self.get_intent(id).await?;
        Ok((intent, 0))
    }
}

/// IntentService handles intent lifecycle operations
pub struct IntentService {
    repo: Arc<dyn IntentRepository>,
    /// Optional graph service for impact classification
    graph_service: Option<Arc<graph_service::GraphService>>,
    /// Optional approval request repository for cancelling pending approvals on version change
    approval_repo: Option<Arc<dyn ApprovalRequestRepository>>,
    /// Optional audit repository for recording cancellation events
    audit_repo: Option<Arc<dyn AuditRepository>>,
    /// System actor ID used for system-initiated cancellations
    system_actor_id: String,
    /// Phase 3 P3-S5: Optional RLS-aware pool for tenant-scoped transactions.
    /// When Some, RLS-aware methods are available for JWT-authenticated requests.
    rls_pool: Option<graph_service::RlsAwarePool>,
}

impl IntentService {
    pub fn new(repo: Arc<dyn IntentRepository>) -> Self {
        Self {
            repo,
            graph_service: None,
            approval_repo: None,
            audit_repo: None,
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with optional graph service for graph-integrated features
    pub fn with_graph_service(
        repo: Arc<dyn IntentRepository>,
        graph_service: Arc<graph_service::GraphService>,
    ) -> Self {
        Self {
            repo,
            graph_service: Some(graph_service),
            approval_repo: None,
            audit_repo: None,
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with approval and audit repositories for Phase 2b bounded slice.
    /// When approval_repo is provided, creating a new intent version will automatically cancel
    /// any pending approval requests for that intent.
    pub fn with_approval_and_audit(
        repo: Arc<dyn IntentRepository>,
        approval_repo: Arc<dyn ApprovalRequestRepository>,
        audit_repo: Arc<dyn AuditRepository>,
    ) -> Self {
        Self {
            repo,
            graph_service: None,
            approval_repo: Some(approval_repo),
            audit_repo: Some(audit_repo),
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with all optional services for Phase 2b bounded slice.
    pub fn with_all_services(
        repo: Arc<dyn IntentRepository>,
        graph_service: Arc<graph_service::GraphService>,
        approval_repo: Arc<dyn ApprovalRequestRepository>,
        audit_repo: Arc<dyn AuditRepository>,
    ) -> Self {
        Self {
            repo,
            graph_service: Some(graph_service),
            approval_repo: Some(approval_repo),
            audit_repo: Some(audit_repo),
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Set the RLS-aware pool for tenant-scoped transactions.
    ///
    /// Phase 3 P3-S5: Enables RLS-aware methods for JWT-authenticated requests.
    /// This should be called after constructing the service when using SQL-backed
    /// repositories with RLS enabled.
    pub fn with_rls_pool(mut self, pool: graph_service::RlsAwarePool) -> Self {
        self.rls_pool = Some(pool);
        self
    }

    /// Create a new intent with initial version (transactional)
    #[tracing::instrument(skip(self))]
    pub async fn create_intent(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        self.repo.create_intent_tx(request).await
    }

    /// Create a new version of an existing intent with optimistic concurrency control
    ///
    /// If `expected_version` and `expected_row_version` are provided (non-zero), performs OCC check:
    /// - Returns `ConcurrencyConflict` if the intent's current version or row_version doesn't match
    ///   This allows clients to detect concurrent modifications and retry.
    ///
    /// Phase 2b bounded slice: When approval_repo is configured, this method will automatically
    /// cancel any pending approval requests for the intent when a new version is created.
    #[tracing::instrument(skip(self))]
    pub async fn create_version(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: Option<i32>,
        expected_row_version: Option<i32>,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let exp_ver = expected_version.unwrap_or(intent.current_version);
        let exp_row_ver = expected_row_version.unwrap_or(row_version);

        // Capture old version number before creating new version for cancellation
        let old_version = intent.current_version;

        let result = self
            .repo
            .create_version_with_occ(intent_id, request, exp_ver, exp_row_ver)
            .await?;

        // Phase 2b bounded slice: Cancel pending approval requests if approval_repo is configured
        if let Some(approval_repo) = &self.approval_repo {
            let tenant_id = intent.tenant_id;
            let cancellation_reason = format!(
                "Intent version changed from v{} to v{}",
                old_version, result.version_number
            );

            // Cancel all pending approval requests for this intent
            let cancelled_count = approval_repo
                .cancel_pending_by_intent(
                    intent_id,
                    tenant_id,
                    &self.system_actor_id,
                    &cancellation_reason,
                )
                .await
                .unwrap_or(0);

            // Emit audit event if audit_repo is configured and we cancelled any requests
            if cancelled_count > 0 {
                if let Some(audit_repo) = &self.audit_repo {
                    let audit_payload = ApprovalCancelledAuditPayload {
                        intent_id,
                        cancelled_version_from: old_version,
                        cancelled_version_to: result.version_number,
                        decision_class: "D/E".to_string(), // High risk decisions require approval
                        cancelled_by: self.system_actor_id.clone(),
                        cancellation_reason,
                        cancelled_count,
                    };
                    let _ = audit_repo
                        .record_approval_cancelled(
                            tenant_id,
                            &self.system_actor_id,
                            intent_id,
                            audit_payload,
                            get_current_trace_context(),
                        )
                        .await;
                }
            }
        }

        Ok(result)
    }

    // =============================================================================
    // RLS-aware methods (Phase 3 P3-S5 bounded slice)
    // =============================================================================

    /// Returns true if RLS pool is configured.
    pub fn has_rls_pool(&self) -> bool {
        self.rls_pool.is_some()
    }

    /// Create a new intent with initial version using RLS-aware transaction.
    ///
    /// Phase 3 P3-S5: This method wraps intent creation in an RLS-set transaction
    /// when `rls_pool` is configured. The tenant_id is extracted from the JWT claims
    /// and validated before beginning the transaction.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `rls_pool` is not configured (caller should fall back to non-RLS)
    /// - tenant_id is nil (RLS validation failure)
    /// - Transaction fails to begin or commit
    /// - Intent creation fails
    #[tracing::instrument(skip(self, request))]
    pub async fn create_intent_with_rls(
        &self,
        request: CreateIntentRequest,
        tenant_id: Uuid,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        let rls_pool = self
            .rls_pool
            .as_ref()
            .ok_or_else(|| IntentRebaseError::Internal("RLS pool not configured".to_string()))?;

        // Validate tenant_id for RLS use
        intent_rebase_types::rls::validate_tenant_id_for_rls(tenant_id).map_err(|e| {
            IntentRebaseError::Internal(format!("invalid tenant_id for RLS: {}", e))
        })?;

        let mut tx = rls_pool.begin_with_tenant(tenant_id).await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin RLS transaction: {}", e))
        })?;

        // Get the SQL repository and create intent within the transaction
        let sql_repo = self.repo.as_sqlx_repo().ok_or_else(|| {
            IntentRebaseError::Internal("RLS requires SQL-backed repository".to_string())
        })?;

        let result = sql_repo
            .create_intent_with_tx(&mut tx, request, tenant_id)
            .await;

        match result {
            Ok(response) => {
                tx.commit().await.map_err(|e| {
                    IntentRebaseError::StorageError(format!(
                        "failed to commit RLS transaction: {}",
                        e
                    ))
                })?;
                Ok(response)
            }
            Err(e) => {
                // Transaction will be rolled back on drop, but we log the error
                tracing::error!(error = %e, "RLS intent creation failed, rolling back transaction");
                Err(e)
            }
        }
    }

    /// Create a new version of an existing intent using RLS-aware transaction.
    ///
    /// Phase 3 P3-S5: This method wraps version creation in an RLS-set transaction
    /// when `rls_pool` is configured. The tenant_id is extracted from the JWT claims
    /// and validated before beginning the transaction.
    ///
    /// If `expected_version` and `expected_row_version` are provided (non-zero), performs OCC check.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `rls_pool` is not configured (caller should fall back to non-RLS)
    /// - tenant_id is nil (RLS validation failure)
    /// - Transaction fails to begin or commit
    /// - Intent not found or version creation fails
    #[tracing::instrument(skip(self, request))]
    pub async fn create_version_with_rls(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: Option<i32>,
        expected_row_version: Option<i32>,
        tenant_id: Uuid,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let rls_pool = self
            .rls_pool
            .as_ref()
            .ok_or_else(|| IntentRebaseError::Internal("RLS pool not configured".to_string()))?;

        // Validate tenant_id for RLS use
        intent_rebase_types::rls::validate_tenant_id_for_rls(tenant_id).map_err(|e| {
            IntentRebaseError::Internal(format!("invalid tenant_id for RLS: {}", e))
        })?;

        let mut tx = rls_pool.begin_with_tenant(tenant_id).await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin RLS transaction: {}", e))
        })?;

        // Get the SQL repository
        let sql_repo = self.repo.as_sqlx_repo().ok_or_else(|| {
            IntentRebaseError::Internal("RLS requires SQL-backed repository".to_string())
        })?;

        // First get the intent to check current version
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let exp_ver = expected_version.unwrap_or(intent.current_version);
        let exp_row_ver = expected_row_version.unwrap_or(row_version);

        // Capture old version number for potential approval cancellation
        let old_version = intent.current_version;

        let result = sql_repo
            .create_version_with_tx(&mut tx, intent_id, request, exp_ver, exp_row_ver)
            .await;

        match result {
            Ok(version_result) => {
                // Phase 2b: Cancel pending approvals if configured
                if let Some(approval_repo) = &self.approval_repo {
                    let cancellation_reason = format!(
                        "Intent version changed from v{} to v{}",
                        old_version, version_result.version_number
                    );

                    let cancelled_count = approval_repo
                        .cancel_pending_by_intent(
                            intent_id,
                            tenant_id,
                            &self.system_actor_id,
                            &cancellation_reason,
                        )
                        .await
                        .unwrap_or(0);

                    if cancelled_count > 0 {
                        if let Some(audit_repo) = &self.audit_repo {
                            let audit_payload = ApprovalCancelledAuditPayload {
                                intent_id,
                                cancelled_version_from: old_version,
                                cancelled_version_to: version_result.version_number,
                                decision_class: "D/E".to_string(),
                                cancelled_by: self.system_actor_id.clone(),
                                cancellation_reason,
                                cancelled_count,
                            };
                            let _ = audit_repo
                                .record_approval_cancelled(
                                    tenant_id,
                                    &self.system_actor_id,
                                    intent_id,
                                    audit_payload,
                                    get_current_trace_context(),
                                )
                                .await;
                        }
                    }
                }

                tx.commit().await.map_err(|e| {
                    IntentRebaseError::StorageError(format!(
                        "failed to commit RLS transaction: {}",
                        e
                    ))
                })?;
                Ok(version_result)
            }
            Err(e) => {
                tracing::error!(error = %e, "RLS version creation failed, rolling back transaction");
                Err(e)
            }
        }
    }

    /// Get the current (head) version of an intent
    #[tracing::instrument(skip(self))]
    pub async fn get_intent_head(
        &self,
        intent_id: Uuid,
    ) -> Result<IntentHeadResponse, IntentRebaseError> {
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let version = self
            .repo
            .get_version_by_intent_and_number(intent_id, intent.current_version)
            .await?;

        Ok(IntentHeadResponse {
            intent,
            version,
            row_version,
        })
    }

    /// Get a specific version of an intent by version number
    pub async fn get_version(
        &self,
        intent_id: Uuid,
        version_number: i32,
    ) -> Result<IntentVersion, IntentRebaseError> {
        self.repo
            .get_version_by_intent_and_number(intent_id, version_number)
            .await
    }

    /// List all versions of an intent (descending order per API spec)
    pub async fn list_versions(
        &self,
        intent_id: Uuid,
    ) -> Result<ListVersionsResponse, IntentRebaseError> {
        // Verify intent exists
        let _intent = self.repo.get_intent(intent_id).await?;
        let mut versions = self.repo.get_versions_by_intent(intent_id).await?;

        // Sort descending by version number (newest first)
        versions.sort_by_key(|b| std::cmp::Reverse(b.version_number));

        Ok(ListVersionsResponse {
            intent_id,
            total: versions.len(),
            versions,
        })
    }

    /// Compute diff between two versions of an intent
    ///
    /// Validates that both versions exist, belong to the same intent,
    /// and have valid ordering (from_version < to_version).
    /// Returns (from_version, to_version, diff, risk) tuple.
    #[tracing::instrument(skip(self))]
    pub async fn compute_diff(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<
        (
            IntentVersion,
            IntentVersion,
            IntentVersionDiff,
            DiffRiskAnalysis,
        ),
        IntentRebaseError,
    > {
        // Validate version ordering before fetching
        if from_version >= to_version {
            return Err(IntentRebaseError::InvalidIntentVersion(format!(
                "from_version ({}) must be less than to_version ({})",
                from_version, to_version
            )));
        }

        // Fetch both versions
        let from = self
            .repo
            .get_version_by_intent_and_number(intent_id, from_version)
            .await?;
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Compute diff with risk analysis using the synchronous function
        // The sync function is safe here since it only does in-memory computation
        let (diff, risk) = compute_diff_with_risk_sync(&from, &to)?;

        Ok((from, to, diff, risk))
    }

    /// Compute rebase preview between two versions of an intent
    ///
    /// Validates that both versions exist, belong to the same intent,
    /// and have valid ordering (from_version < to_version).
    /// Returns a rebase plan with decision class, rationale, and section decisions.
    ///
    /// This is a preview-only endpoint that does NOT include:
    /// - affected_items (requires graph integration - Phase 2)
    /// - deferred fields (Phase 2)
    #[tracing::instrument(skip(self))]
    pub async fn compute_rebase_preview(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<RebasePlan, IntentRebaseError> {
        // Validate version ordering before fetching
        if from_version >= to_version {
            return Err(IntentRebaseError::InvalidIntentVersion(format!(
                "from_version ({}) must be less than to_version ({})",
                from_version, to_version
            )));
        }

        // Fetch both versions
        let from = self
            .repo
            .get_version_by_intent_and_number(intent_id, from_version)
            .await?;
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Compute diff with risk analysis
        let (diff, risk) = compute_diff_with_risk_sync(&from, &to)?;

        // Generate rebase plan using the planner (Phase 1 baseline)
        // Note: This only uses diff+risk - no graph integration in Phase 1
        let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

        Ok(plan)
    }

    /// Compute rebase preview with graph-integrated affected items.
    ///
    /// This method extends `compute_rebase_preview` by enriching the response
    /// with graph-based impact classification when graph service is available.
    ///
    /// If graph service is unavailable or the IntentVersion node is not found in the graph,
    /// the affected_items will have status=Unavailable but the endpoint will NOT fail.
    /// This ensures the rebase preview remains reliable even when graph coverage is incomplete.
    ///
    /// The affected_items are classified starting from the `to_version` IntentVersion node.
    #[tracing::instrument(skip(self))]
    pub async fn compute_rebase_preview_with_graph(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<RebasePlan, IntentRebaseError> {
        // First, compute the base rebase plan
        let plan = self
            .compute_rebase_preview(intent_id, from_version, to_version)
            .await?;

        // If no graph service is available, return the plan with unavailable status
        let graph_service = match &self.graph_service {
            Some(gs) => gs,
            None => return Ok(plan),
        };

        // Get the to_version to find its graph node
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Try to classify affected items from the to_version
        let classification_result = graph_service
            .classify_affected_items_from_intent_version(to.id, Some(3))
            .await;

        match classification_result {
            Ok(Some(result)) => {
                // Graph classification succeeded - build affected items from result
                let (artifacts, approvals, side_effects) =
                    Self::classify_nodes_by_type(&result.classified_nodes);

                let affected_items =
                    AffectedItemsPreview::from_classification(artifacts, approvals, side_effects);

                // Create new plan with enriched affected_items
                let enriched_plan = RebasePlan {
                    decision_class: plan.decision_class,
                    rationale: plan.rationale,
                    section_decisions: plan.section_decisions,
                    affected_items,
                    deferred: plan.deferred,
                    manual_review_recommended: plan.manual_review_recommended,
                    risk_tier: plan.risk_tier,
                    risk_level: plan.risk_level,
                };

                Ok(enriched_plan)
            }
            Ok(None) | Err(_) => {
                // Graph node not found or classification failed - return with unavailable status
                // Note: We intentionally do NOT fail the endpoint here
                Ok(plan)
            }
        }
    }

    /// Helper to classify graph nodes by type from a classification result
    fn classify_nodes_by_type(
        classified_nodes: &[intent_rebase_types::ClassifiedNode],
    ) -> (Vec<AffectedItem>, Vec<AffectedItem>, Vec<AffectedItem>) {
        let mut artifacts = Vec::new();
        let mut approvals = Vec::new();
        let mut side_effects = Vec::new();

        for classified in classified_nodes {
            let item = AffectedItem {
                node_id: classified.node.id,
                label: classified.node.label.clone(),
                impact: classified.impact.clone(),
                reason: classified.reason.clone(),
                external_ref: classified.node.external_ref.clone(),
            };

            match classified.node.node_type {
                NodeType::Artifact => artifacts.push(item),
                NodeType::Approval => approvals.push(item),
                NodeType::SideEffect => side_effects.push(item),
                _ => {} // Skip other node types for affected items preview
            }
        }

        (artifacts, approvals, side_effects)
    }
}

/// Compute SHA-256 hash of the payload for integrity verification
fn compute_payload_hash(payload: &intent_rebase_types::IntentPayload) -> String {
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
        AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
        IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier,
        SourceRef, Urgency,
    };
    use rebase_engine::Severity;

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
