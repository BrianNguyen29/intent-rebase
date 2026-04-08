//! SQL-backed intent repository using sqlx
//!
//! Phase 1: Implements IntentRepository trait with PostgreSQL backend.
//! Uses optimistic concurrency control (OCC) for version creation.

use crate::IntentRepository;
use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ActorRef, ChangeChannel, CreateIntentRequest, CreateIntentResponse, CreateVersionRequest,
    CreateVersionResponse, Intent, IntentPayload, IntentRebaseError, IntentStatus, IntentVersion,
    SourceRef, VersionStatus,
};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

/// Tenant resolution trait for extracting tenant identity from request context.
///
/// This trait provides a seam for tenant extraction that can be implemented
/// differently based on auth infrastructure (JWT claims, API key metadata, etc.).
/// Using this trait avoids hardcoding `Uuid::new_v4()` placeholder in SQL paths.
pub trait TenantResolver: Send + Sync {
    /// Resolve tenant ID from the current request context.
    /// Returns None if tenant cannot be determined (caller should handle auth error).
    fn resolve_tenant_id(&self) -> Option<Uuid>;

    /// Resolve tenant ID or return a default/placeholder.
    /// Use this only when tenant resolution failure should not block the operation.
    fn resolve_tenant_id_or_default(&self) -> Uuid {
        self.resolve_tenant_id().unwrap_or_else(Uuid::new_v4)
    }
}

/// Default tenant resolver that always returns None (placeholder behavior).
/// Production implementations should extract from actual auth context.
pub struct DefaultTenantResolver;

impl TenantResolver for DefaultTenantResolver {
    fn resolve_tenant_id(&self) -> Option<Uuid> {
        None
    }
}

/// Simple tenant resolver that extracts from a provided UUID.
/// Useful for testing and internal service-to-service calls.
pub struct StaticTenantResolver {
    tenant_id: Uuid,
}

impl StaticTenantResolver {
    pub fn new(tenant_id: Uuid) -> Self {
        Self { tenant_id }
    }
}

impl TenantResolver for StaticTenantResolver {
    fn resolve_tenant_id(&self) -> Option<Uuid> {
        Some(self.tenant_id)
    }
}

/// SQL-backed repository for intent storage
pub struct SqlxIntentRepository {
    pool: PgPool,
    tenant_resolver: Box<dyn TenantResolver>,
}

impl SqlxIntentRepository {
    /// Create a new SqlxIntentRepository with default tenant resolver.
    /// Use this for testing or when tenant resolution is handled at a higher layer.
    pub fn new(pool: PgPool) -> Self {
        Self::with_tenant_resolver(pool, DefaultTenantResolver)
    }

    /// Create a new SqlxIntentRepository with a custom tenant resolver.
    pub fn with_tenant_resolver(pool: PgPool, resolver: impl TenantResolver + 'static) -> Self {
        Self {
            pool,
            tenant_resolver: Box::new(resolver),
        }
    }

    /// Create a new intent with initial version (transactional)
    pub async fn create_intent_tx(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin transaction: {}", e))
        })?;

        let intent_id = Uuid::new_v4();
        let now = Utc::now();
        let tenant_id = self.tenant_resolver.resolve_tenant_id_or_default();

        // Insert intent
        let source_refs_json = serde_json::to_value(&request.source_refs)
            .map_err(|e| IntentRebaseError::SerializationError(format!("source_refs: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO intents (intent_id, tenant_id, workflow_id, current_version, status,
                created_at, created_by_actor_type, created_by_actor_id, source_refs, tags, row_version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1)
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .bind(request.workflow_id)
        .bind(1)
        .bind("active")
        .bind(now)
        .bind(&request.created_by.actor_type)
        .bind(&request.created_by.actor_id)
        .bind(source_refs_json)
        .bind(&request.tags)
        .execute(&mut *tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert intent: {}", e)))?;

        // Create initial version
        let version_id = Uuid::new_v4();
        let payload_hash = compute_payload_hash(&request.payload);
        let payload_json = serde_json::to_value(&request.payload)
            .map_err(|e| IntentRebaseError::SerializationError(format!("payload: {}", e)))?;

        sqlx::query(
            r#"
            INSERT INTO intent_versions (intent_version_id, intent_id, version_number,
                parent_version_id, created_at, created_by_actor_type, created_by_actor_id,
                change_reason, change_channel, status, hash, payload)
            VALUES ($1, $2, 1, NULL, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(version_id)
        .bind(intent_id)
        .bind(now)
        .bind(&request.created_by.actor_type)
        .bind(&request.created_by.actor_id)
        .bind("Initial creation")
        .bind("user_edit")
        .bind("active")
        .bind(&payload_hash)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert version: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| IntentRebaseError::StorageError(format!("commit: {}", e)))?;

        Ok(CreateIntentResponse {
            intent_id,
            current_version: 1,
            status: IntentStatus::Active,
        })
    }

    /// Create a new version with optimistic concurrency control
    ///
    /// Uses transactional compare-and-swap on `intents.current_version` with row_version bump.
    /// If the intent has been modified since it was read, returns `ConcurrencyConflict`.
    pub async fn create_version_with_occ(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: i32,
        expected_row_version: i32,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin transaction: {}", e))
        })?;

        // Check current version with OCC
        let row = sqlx::query(
            r#"
            SELECT current_version, row_version
            FROM intents
            WHERE intent_id = $1
            FOR UPDATE
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch intent: {}", e)))?;

        let row = row.ok_or(IntentRebaseError::IntentNotFound(intent_id))?;

        let current_version: i32 = row.get("current_version");
        let current_row_version: i32 = row.get("row_version");

        // OCC check: version must match what caller expected
        if current_version != expected_version {
            return Err(IntentRebaseError::ConcurrencyConflict(intent_id));
        }

        if current_row_version != expected_row_version {
            return Err(IntentRebaseError::ConcurrencyConflict(intent_id));
        }

        let new_version_number = current_version + 1;
        let now = Utc::now();
        let payload_hash = compute_payload_hash(&request.payload);
        let payload_json = serde_json::to_value(&request.payload)
            .map_err(|e| IntentRebaseError::SerializationError(format!("payload: {}", e)))?;

        // Insert new version
        let version_id = Uuid::new_v4();
        let change_channel_str = change_channel_to_string(&request.change_channel);

        sqlx::query(
            r#"
            INSERT INTO intent_versions (intent_version_id, intent_id, version_number,
                parent_version_id, created_at, created_by_actor_type, created_by_actor_id,
                change_reason, change_channel, status, hash, payload)
            VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(version_id)
        .bind(intent_id)
        .bind(new_version_number)
        .bind(now)
        .bind(&request.created_by.actor_type)
        .bind(&request.created_by.actor_id)
        .bind(&request.change_reason)
        .bind(change_channel_str)
        .bind("active")
        .bind(&payload_hash)
        .bind(payload_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert version: {}", e)))?;

        // Update intent's current version with OCC
        let updated = sqlx::query(
            r#"
            UPDATE intents
            SET current_version = $1, row_version = row_version + 1
            WHERE intent_id = $2 AND row_version = $3
            "#,
        )
        .bind(new_version_number)
        .bind(intent_id)
        .bind(expected_row_version)
        .execute(&mut *tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("update intent: {}", e)))?;

        if updated.rows_affected() == 0 {
            return Err(IntentRebaseError::ConcurrencyConflict(intent_id));
        }

        tx.commit()
            .await
            .map_err(|e| IntentRebaseError::StorageError(format!("commit: {}", e)))?;

        Ok(CreateVersionResponse {
            intent_version_id: version_id,
            intent_id,
            version_number: new_version_number,
            status: VersionStatus::Active,
        })
    }

    async fn get_intent_by_id(&self, id: Uuid) -> Result<Intent, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT intent_id, tenant_id, workflow_id, current_version, status,
                created_at, created_by_actor_type, created_by_actor_id, source_refs, tags, row_version
            FROM intents WHERE intent_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch intent: {}", e)))?;

        match row {
            Some(r) => self.row_to_intent(r),
            None => Err(IntentRebaseError::IntentNotFound(id)),
        }
    }

    async fn get_version_by_id(&self, id: Uuid) -> Result<IntentVersion, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT intent_version_id, intent_id, version_number, parent_version_id,
                created_at, created_by_actor_type, created_by_actor_id, change_reason,
                change_channel, status, hash, payload
            FROM intent_versions WHERE intent_version_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch version: {}", e)))?;

        match row {
            Some(r) => self.row_to_version(r),
            None => Err(IntentRebaseError::IntentVersionNotFound(id)),
        }
    }

    fn row_to_intent(&self, row: PgRow) -> Result<Intent, IntentRebaseError> {
        let source_refs_json: serde_json::Value = row.get("source_refs");
        let source_refs: Vec<SourceRef> = serde_json::from_value(source_refs_json)
            .map_err(|e| IntentRebaseError::SerializationError(format!("source_refs: {}", e)))?;
        let tags: Vec<String> = row.get("tags");

        Ok(Intent {
            id: row.get("intent_id"),
            tenant_id: row.get("tenant_id"),
            workflow_id: row.get("workflow_id"),
            current_version: row.get("current_version"),
            status: status_from_string(&row.get::<String, _>("status")),
            created_at: row.get("created_at"),
            created_by: ActorRef {
                actor_type: row.get("created_by_actor_type"),
                actor_id: row.get("created_by_actor_id"),
            },
            source_refs,
            tags,
        })
    }

    fn row_to_version(&self, row: PgRow) -> Result<IntentVersion, IntentRebaseError> {
        let payload_json: serde_json::Value = row.get("payload");
        let payload: IntentPayload = serde_json::from_value(payload_json)
            .map_err(|e| IntentRebaseError::SerializationError(format!("payload: {}", e)))?;

        let change_channel_str: String = row.get("change_channel");
        let status_str: String = row.get("status");

        Ok(IntentVersion {
            id: row.get("intent_version_id"),
            intent_id: row.get("intent_id"),
            version_number: row.get("version_number"),
            parent_version_id: row.get("parent_version_id"),
            created_at: row.get("created_at"),
            created_by: ActorRef {
                actor_type: row.get("created_by_actor_type"),
                actor_id: row.get("created_by_actor_id"),
            },
            change_reason: row.get("change_reason"),
            change_channel: change_channel_from_string(&change_channel_str),
            status: version_status_from_string(&status_str),
            hash: row.get("hash"),
            payload,
        })
    }
}

#[async_trait]
impl IntentRepository for SqlxIntentRepository {
    async fn create_intent_tx(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        // Delegate to the existing transactional method
        self.create_intent_tx(request).await
    }

    async fn get_intent(&self, id: Uuid) -> Result<Intent, IntentRebaseError> {
        self.get_intent_by_id(id).await
    }

    async fn create_version_with_occ(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: i32,
        expected_row_version: i32,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        self.create_version_with_occ(intent_id, request, expected_version, expected_row_version)
            .await
    }

    async fn get_version(&self, id: Uuid) -> Result<IntentVersion, IntentRebaseError> {
        self.get_version_by_id(id).await
    }

    async fn get_versions_by_intent(
        &self,
        intent_id: Uuid,
    ) -> Result<Vec<IntentVersion>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT intent_version_id, intent_id, version_number, parent_version_id,
                created_at, created_by_actor_type, created_by_actor_id, change_reason,
                change_channel, status, hash, payload
            FROM intent_versions
            WHERE intent_id = $1
            ORDER BY version_number ASC
            "#,
        )
        .bind(intent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch versions: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| self.row_to_version(r))
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn get_version_by_intent_and_number(
        &self,
        intent_id: Uuid,
        version_number: i32,
    ) -> Result<IntentVersion, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT intent_version_id, intent_id, version_number, parent_version_id,
                created_at, created_by_actor_type, created_by_actor_id, change_reason,
                change_channel, status, hash, payload
            FROM intent_versions
            WHERE intent_id = $1 AND version_number = $2
            "#,
        )
        .bind(intent_id)
        .bind(version_number)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch version: {}", e)))?;

        match row {
            Some(r) => self.row_to_version(r),
            None => Err(IntentRebaseError::InvalidIntentVersion(format!(
                "version {} not found for intent {}",
                version_number, intent_id
            ))),
        }
    }

    async fn get_intent_for_update(&self, id: Uuid) -> Result<(Intent, i32), IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT intent_id, tenant_id, workflow_id, current_version, status,
                created_at, created_by_actor_type, created_by_actor_id, source_refs, tags, row_version
            FROM intents
            WHERE intent_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch intent: {}", e)))?;

        match row {
            Some(r) => {
                let row_version: i32 = r.get("row_version");
                let intent = self.row_to_intent(r)?;
                Ok((intent, row_version))
            }
            None => Err(IntentRebaseError::IntentNotFound(id)),
        }
    }
}

// Helper functions

fn compute_payload_hash(payload: &IntentPayload) -> String {
    use sha2::{Digest, Sha256};
    let json = serde_json::to_string(payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

fn change_channel_to_string(channel: &ChangeChannel) -> &'static str {
    match channel {
        ChangeChannel::UserEdit => "user_edit",
        ChangeChannel::Webhook => "webhook",
        ChangeChannel::PolicyUpdate => "policy_update",
        ChangeChannel::SystemNormalization => "system_normalization",
    }
}

fn change_channel_from_string(s: &str) -> ChangeChannel {
    match s {
        "webhook" => ChangeChannel::Webhook,
        "policy_update" => ChangeChannel::PolicyUpdate,
        "system_normalization" => ChangeChannel::SystemNormalization,
        _ => ChangeChannel::UserEdit,
    }
}

fn status_from_string(s: &str) -> IntentStatus {
    match s {
        "archived" => IntentStatus::Archived,
        "superseded" => IntentStatus::Superseded,
        _ => IntentStatus::Active,
    }
}

fn version_status_from_string(s: &str) -> VersionStatus {
    match s {
        "draft" => VersionStatus::Draft,
        "rejected" => VersionStatus::Rejected,
        "superseded" => VersionStatus::Superseded,
        _ => VersionStatus::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_channel_to_string() {
        assert_eq!(
            change_channel_to_string(&ChangeChannel::UserEdit),
            "user_edit"
        );
        assert_eq!(change_channel_to_string(&ChangeChannel::Webhook), "webhook");
        assert_eq!(
            change_channel_to_string(&ChangeChannel::PolicyUpdate),
            "policy_update"
        );
        assert_eq!(
            change_channel_to_string(&ChangeChannel::SystemNormalization),
            "system_normalization"
        );
    }

    #[test]
    fn test_change_channel_from_string() {
        assert_eq!(
            change_channel_from_string("user_edit"),
            ChangeChannel::UserEdit
        );
        assert_eq!(
            change_channel_from_string("webhook"),
            ChangeChannel::Webhook
        );
        assert_eq!(
            change_channel_from_string("policy_update"),
            ChangeChannel::PolicyUpdate
        );
        assert_eq!(
            change_channel_from_string("system_normalization"),
            ChangeChannel::SystemNormalization
        );
        assert_eq!(
            change_channel_from_string("unknown"),
            ChangeChannel::UserEdit
        );
    }

    #[test]
    fn test_status_from_string() {
        assert_eq!(status_from_string("active"), IntentStatus::Active);
        assert_eq!(status_from_string("archived"), IntentStatus::Archived);
        assert_eq!(status_from_string("superseded"), IntentStatus::Superseded);
        assert_eq!(status_from_string("unknown"), IntentStatus::Active);
    }

    #[test]
    fn test_version_status_from_string() {
        assert_eq!(version_status_from_string("active"), VersionStatus::Active);
        assert_eq!(version_status_from_string("draft"), VersionStatus::Draft);
        assert_eq!(
            version_status_from_string("rejected"),
            VersionStatus::Rejected
        );
        assert_eq!(
            version_status_from_string("superseded"),
            VersionStatus::Superseded
        );
        assert_eq!(version_status_from_string("unknown"), VersionStatus::Active);
    }
}
