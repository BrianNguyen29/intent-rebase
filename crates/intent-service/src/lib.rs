//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 1: First slice implementation with in-memory repository.
//! Repository trait allows swapping to SQL-backed implementation.

pub mod sqlx_repository;

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ChangeChannel, CreateIntentRequest, CreateIntentResponse, CreateVersionRequest,
    CreateVersionResponse, Intent, IntentHeadResponse, IntentRebaseError, IntentStatus,
    IntentVersion, ListVersionsResponse, VersionStatus,
};
use rebase_engine::{compute_diff_with_risk_sync, DiffRiskAnalysis, IntentVersionDiff};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use sqlx_repository::SqlxIntentRepository;

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

        let version_id = Uuid::new_v4();
        let version = IntentVersion {
            id: version_id,
            intent_id,
            version_number: new_version_number,
            parent_version_id: None, // TODO: link to previous version
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
}

impl IntentService {
    pub fn new(repo: Arc<dyn IntentRepository>) -> Self {
        Self { repo }
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
    /// This allows clients to detect concurrent modifications and retry.
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
}
