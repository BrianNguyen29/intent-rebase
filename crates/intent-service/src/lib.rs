//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 1: First slice implementation with in-memory repository.
//! Repository trait allows swapping to SQL-backed implementation.

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ChangeChannel, CreateIntentRequest, CreateIntentResponse, CreateVersionRequest,
    CreateVersionResponse, Intent, IntentHeadResponse, IntentRebaseError, IntentStatus,
    IntentVersion, ListVersionsResponse, VersionStatus,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for intent storage
/// Allows for in-memory (tests) or SQL-backed implementations
#[async_trait]
pub trait IntentRepository: Send + Sync {
    async fn create_intent(&self, intent: Intent) -> Result<Intent, IntentRebaseError>;
    async fn get_intent(&self, id: Uuid) -> Result<Intent, IntentRebaseError>;
    async fn update_intent(&self, intent: Intent) -> Result<Intent, IntentRebaseError>;

    async fn create_version(
        &self,
        version: IntentVersion,
    ) -> Result<IntentVersion, IntentRebaseError>;
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
    async fn create_intent(&self, intent: Intent) -> Result<Intent, IntentRebaseError> {
        let mut intents = self.intents.write().await;
        intents.insert(intent.id, intent.clone());
        Ok(intent)
    }

    async fn get_intent(&self, id: Uuid) -> Result<Intent, IntentRebaseError> {
        let intents = self.intents.read().await;
        intents
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::IntentNotFound(id))
    }

    async fn update_intent(&self, intent: Intent) -> Result<Intent, IntentRebaseError> {
        let mut intents = self.intents.write().await;
        if intents.contains_key(&intent.id) {
            intents.insert(intent.id, intent.clone());
            Ok(intent)
        } else {
            Err(IntentRebaseError::IntentNotFound(intent.id))
        }
    }

    async fn create_version(
        &self,
        version: IntentVersion,
    ) -> Result<IntentVersion, IntentRebaseError> {
        // Always acquire versions_by_intent before versions to prevent deadlocks
        let mut versions_by_intent = self.versions_by_intent.write().await;
        let mut versions = self.versions.write().await;

        versions.insert(version.id, version.clone());
        versions_by_intent
            .entry(version.intent_id)
            .or_insert_with(Vec::new)
            .push(version.id);

        Ok(version)
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
}

/// IntentService handles intent lifecycle operations
pub struct IntentService {
    repo: Arc<dyn IntentRepository>,
}

impl IntentService {
    pub fn new(repo: Arc<dyn IntentRepository>) -> Self {
        Self { repo }
    }

    /// Create a new intent with initial version
    pub async fn create_intent(
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

        // Persist both
        self.repo.create_intent(intent.clone()).await?;
        self.repo.create_version(version).await?;

        Ok(CreateIntentResponse {
            intent_id,
            current_version: 1,
            status: IntentStatus::Active,
        })
    }

    /// Create a new version of an existing intent
    pub async fn create_version(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        // Get current intent to increment version
        let intent = self.repo.get_intent(intent_id).await?;
        let new_version_number = intent.current_version + 1;
        let now = Utc::now();

        // Compute payload hash
        let payload_hash = compute_payload_hash(&request.payload);

        // Create new version
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

        // Persist version
        self.repo.create_version(version.clone()).await?;

        // Update intent's current version
        let mut updated_intent = intent;
        updated_intent.current_version = new_version_number;
        self.repo.update_intent(updated_intent).await?;

        Ok(CreateVersionResponse {
            intent_version_id: version_id,
            intent_id,
            version_number: new_version_number,
            status: VersionStatus::Active,
        })
    }

    /// Get the current (head) version of an intent
    pub async fn get_intent_head(
        &self,
        intent_id: Uuid,
    ) -> Result<IntentHeadResponse, IntentRebaseError> {
        let intent = self.repo.get_intent(intent_id).await?;
        let version = self
            .repo
            .get_version_by_intent_and_number(intent_id, intent.current_version)
            .await?;

        Ok(IntentHeadResponse { intent, version })
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

        let version_result = service.create_version(intent_id, version_request).await;
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
            .create_version(intent_id, version_request)
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
            .create_version(intent_id, version_request)
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
            .create_version(intent_id, version_request)
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
}
