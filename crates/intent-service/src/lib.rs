//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 1: First slice implementation with in-memory repository.
//! Repository trait allows swapping to SQL-backed implementation.

pub mod checkpoint_repo;
pub mod sqlx_repository;

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    AffectedItem, AffectedItemsPreview, ChangeChannel, Checkpoint, CreateIntentRequest,
    CreateIntentResponse, CreateVersionRequest, CreateVersionResponse, Intent, IntentHeadResponse,
    IntentRebaseError, IntentStatus, IntentVersion, ListVersionsResponse, NodeType, VersionStatus,
};
use rebase_engine::{compute_diff_with_risk_sync, DiffRiskAnalysis, IntentVersionDiff, RebasePlan};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use checkpoint_repo::{
    CheckpointRepository, InMemoryCheckpointRepository, SqlxCheckpointRepository,
};
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

#[async_trait]
impl IntentRepository for InMemoryIntentRepository {
    async fn create_intent_tx(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        let intent_id = Uuid::new_v4();
        let now = Utc::now();
        let tenant_id = Uuid::new_v4(); // TODO: extract from auth context

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

        result.sort_by(|a, b| a.version_number.cmp(&b.version_number));
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
}

impl IntentService {
    pub fn new(repo: Arc<dyn IntentRepository>) -> Self {
        Self {
            repo,
            graph_service: None,
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
        }
    }

    /// Create a new intent with initial version (transactional)
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
        self.repo
            .create_version_with_occ(intent_id, request, exp_ver, exp_row_ver)
            .await
    }

    /// Get the current (head) version of an intent
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
        versions.sort_by(|a, b| b.version_number.cmp(&a.version_number));

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

// =============================================================================
// Checkpoint Service — Phase 2 checkpoint lifecycle management
// =============================================================================

use intent_rebase_types::CheckpointType;

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
            status: intent_rebase_types::CheckpointStatus::Pending,
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
            status: intent_rebase_types::CheckpointStatus::Pending,
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
            .update_status(checkpoint_id, intent_rebase_types::CheckpointStatus::Active)
            .await
    }

    /// Supersede a checkpoint (mark it as superseded by a newer checkpoint).
    pub async fn supersede_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo
            .update_status(
                checkpoint_id,
                intent_rebase_types::CheckpointStatus::Superseded,
            )
            .await
    }

    /// Invalidate a checkpoint due to an error or invalid state.
    pub async fn invalidate_checkpoint(
        &self,
        checkpoint_id: Uuid,
    ) -> Result<Checkpoint, IntentRebaseError> {
        self.repo
            .update_status(
                checkpoint_id,
                intent_rebase_types::CheckpointStatus::Invalidated,
            )
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
mod checkpoint_service_tests {
    use super::*;
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
                    i as i32 + 1,
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
                    i as i32 + 1,
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
                    i as i32 + 1,
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
        assert_eq!(
            activated.unwrap().status,
            intent_rebase_types::CheckpointStatus::Active
        );
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
        assert_eq!(
            superseded.unwrap().status,
            intent_rebase_types::CheckpointStatus::Superseded
        );
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
        assert_eq!(
            invalidated.unwrap().status,
            intent_rebase_types::CheckpointStatus::Invalidated
        );
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
        expired_checkpoint.status = intent_rebase_types::CheckpointStatus::Active;

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
}
