use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ChangeChannel, CreateIntentRequest, CreateIntentResponse, CreateVersionRequest,
    CreateVersionResponse, Intent, IntentRebaseError, IntentStatus, IntentVersion, VersionStatus,
};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{compute_payload_hash, IntentRepository};

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
