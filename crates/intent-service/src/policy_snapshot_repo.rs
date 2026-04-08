//! Policy snapshot repository for Phase 2 governance bounded slice
//!
//! Provides storage for policy_snapshot table records that create point-in-time
//! immutable records of approval policy at intent approval time.
//!
//! Bounded slice: Only schema + types + repository + minimal lifecycle linkage.
//! S3 upload, scope canonicalization, revalidation, and re-approval workflow are out of scope.

use async_trait::async_trait;
use intent_rebase_types::{IntentRebaseError, PolicySnapshot, ScopeDefinition, ScopeType};
use std::collections::HashMap;
#[allow(unused_imports)]
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for policy snapshot storage
#[async_trait]
pub trait PolicySnapshotRepository: Send + Sync {
    /// Create a new policy snapshot
    async fn create_snapshot(
        &self,
        snapshot: PolicySnapshot,
    ) -> Result<PolicySnapshot, IntentRebaseError>;

    /// Get a policy snapshot by ID
    async fn get_snapshot(&self, id: Uuid) -> Result<PolicySnapshot, IntentRebaseError>;

    /// Get the latest policy snapshot for an intent (by version, descending)
    async fn get_latest_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError>;

    /// Get a policy snapshot for a specific intent version
    async fn get_by_intent_version(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError>;

    /// List all policy snapshots for an intent (ordered by version descending)
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PolicySnapshot>, IntentRebaseError>;
}

/// In-memory policy snapshot repository for Phase 2 bounded slice testing
pub struct InMemoryPolicySnapshotRepository {
    snapshots: RwLock<HashMap<Uuid, PolicySnapshot>>,
    by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryPolicySnapshotRepository {
    pub fn new() -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryPolicySnapshotRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PolicySnapshotRepository for InMemoryPolicySnapshotRepository {
    async fn create_snapshot(
        &self,
        snapshot: PolicySnapshot,
    ) -> Result<PolicySnapshot, IntentRebaseError> {
        let mut snapshots = self.snapshots.write().await;
        let mut by_intent = self.by_intent.write().await;

        // Store snapshot
        snapshots.insert(snapshot.id, snapshot.clone());

        // Index by intent
        by_intent
            .entry(snapshot.intent_id)
            .or_insert_with(Vec::new)
            .push(snapshot.id);

        Ok(snapshot)
    }

    async fn get_snapshot(&self, id: Uuid) -> Result<PolicySnapshot, IntentRebaseError> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::PolicySnapshotNotFound(id))
    }

    async fn get_latest_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError> {
        let snapshots = self.snapshots.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<PolicySnapshot> = ids
            .iter()
            .filter_map(|id| snapshots.get(id).cloned())
            .filter(|s| s.tenant_id == tenant_id)
            .collect();

        // Sort by intent_version descending (newest first)
        result.sort_by(|a, b| b.intent_version.cmp(&a.intent_version));

        Ok(result.into_iter().next())
    }

    async fn get_by_intent_version(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError> {
        let snapshots = self.snapshots.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let result: Option<PolicySnapshot> = ids
            .iter()
            .filter_map(|id| snapshots.get(id).cloned())
            .find(|s| s.tenant_id == tenant_id && s.intent_version == intent_version);

        Ok(result)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PolicySnapshot>, IntentRebaseError> {
        let snapshots = self.snapshots.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<PolicySnapshot> = ids
            .iter()
            .filter_map(|id| snapshots.get(id).cloned())
            .filter(|s| s.tenant_id == tenant_id)
            .collect();

        // Sort by intent_version descending (newest first)
        result.sort_by(|a, b| b.intent_version.cmp(&a.intent_version));

        Ok(result)
    }
}

// =============================================================================
// SQLx-backed Policy Snapshot Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

/// SQL-backed repository for policy snapshot persistence using PostgreSQL.
/// Follows the same patterns as SqlxApprovalRequestRepository.
pub struct SqlxPolicySnapshotRepository {
    pool: PgPool,
}

impl SqlxPolicySnapshotRepository {
    /// Create a new SqlxPolicySnapshotRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to a PolicySnapshot domain object
    fn row_to_snapshot(&self, row: PgRow) -> Result<PolicySnapshot, IntentRebaseError> {
        let scope_type_str: String = row.get("scope_type");
        let affected_resources: serde_json::Value = row.get("affected_resources");
        let required_approvers: serde_json::Value = row.get("required_approvers");
        let min_approvals: i32 = row.get("min_approvals");

        let scope_definition = ScopeDefinition {
            scope_type: scope_type_from_string(&scope_type_str),
            affected_resources: affected_resources
                .as_array()
                .map(|arr| arr.to_vec())
                .unwrap_or_default(),
            required_approvers: required_approvers
                .as_array()
                .map(|arr| arr.to_vec())
                .unwrap_or_default(),
            min_approvals,
        };

        Ok(PolicySnapshot {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            intent_id: row.get("intent_id"),
            intent_version: row.get("intent_version"),
            rule_pack_version: row.get("rule_pack_version"),
            scope_definition,
            scope_hash: row.get("scope_hash"),
            snapshot_uri: row.get("snapshot_uri"),
            created_at: row.get("created_at"),
            canonicalized_at: row.get("canonicalized_at"),
        })
    }

    /// Insert a new policy snapshot into the database
    async fn insert_snapshot(
        &self,
        snapshot: &PolicySnapshot,
    ) -> Result<PolicySnapshot, IntentRebaseError> {
        let affected_resources =
            serde_json::to_value(&snapshot.scope_definition.affected_resources).map_err(|e| {
                IntentRebaseError::SerializationError(format!("affected_resources: {}", e))
            })?;
        let required_approvers =
            serde_json::to_value(&snapshot.scope_definition.required_approvers).map_err(|e| {
                IntentRebaseError::SerializationError(format!("required_approvers: {}", e))
            })?;
        let scope_type_str = scope_type_to_string(&snapshot.scope_definition.scope_type);

        sqlx::query(
            r#"
            INSERT INTO policy_snapshot (
                id, tenant_id, intent_id, intent_version,
                rule_pack_version, scope_type, affected_resources,
                required_approvers, min_approvals, scope_hash, snapshot_uri,
                created_at, canonicalized_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(snapshot.id)
        .bind(snapshot.tenant_id)
        .bind(snapshot.intent_id)
        .bind(snapshot.intent_version)
        .bind(&snapshot.rule_pack_version)
        .bind(scope_type_str)
        .bind(affected_resources)
        .bind(required_approvers)
        .bind(snapshot.scope_definition.min_approvals)
        .bind(&snapshot.scope_hash)
        .bind(&snapshot.snapshot_uri)
        .bind(snapshot.created_at)
        .bind(snapshot.canonicalized_at)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert policy snapshot: {}", e)))?;

        Ok(snapshot.clone())
    }
}

#[async_trait]
impl PolicySnapshotRepository for SqlxPolicySnapshotRepository {
    async fn create_snapshot(
        &self,
        snapshot: PolicySnapshot,
    ) -> Result<PolicySnapshot, IntentRebaseError> {
        self.insert_snapshot(&snapshot).await
    }

    async fn get_snapshot(&self, id: Uuid) -> Result<PolicySnapshot, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version,
                rule_pack_version, scope_type, affected_resources,
                required_approvers, min_approvals, scope_hash, snapshot_uri,
                created_at, canonicalized_at
            FROM policy_snapshot
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch policy snapshot: {}", e)))?;

        match row {
            Some(r) => self.row_to_snapshot(r),
            None => Err(IntentRebaseError::PolicySnapshotNotFound(id)),
        }
    }

    async fn get_latest_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version,
                rule_pack_version, scope_type, affected_resources,
                required_approvers, min_approvals, scope_hash, snapshot_uri,
                created_at, canonicalized_at
            FROM policy_snapshot
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY intent_version DESC
            LIMIT 1
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("get latest policy snapshot by intent: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_snapshot(r).map(Some),
            None => Ok(None),
        }
    }

    async fn get_by_intent_version(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<Option<PolicySnapshot>, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version,
                rule_pack_version, scope_type, affected_resources,
                required_approvers, min_approvals, scope_hash, snapshot_uri,
                created_at, canonicalized_at
            FROM policy_snapshot
            WHERE intent_id = $1 AND intent_version = $2 AND tenant_id = $3
            "#,
        )
        .bind(intent_id)
        .bind(intent_version)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("get policy snapshot by intent version: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_snapshot(r).map(Some),
            None => Ok(None),
        }
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<PolicySnapshot>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version,
                rule_pack_version, scope_type, affected_resources,
                required_approvers, min_approvals, scope_hash, snapshot_uri,
                created_at, canonicalized_at
            FROM policy_snapshot
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY intent_version DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list policy snapshots by intent: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_snapshot(r)).collect()
    }
}

// =============================================================================
// Helper functions for scope type enum conversion
// =============================================================================

fn scope_type_to_string(scope_type: &ScopeType) -> &'static str {
    match scope_type {
        ScopeType::Full => "full",
        ScopeType::Partial => "partial",
        ScopeType::None => "none",
    }
}

/// Decode a scope type string from the database into a ScopeType enum.
fn scope_type_from_string(s: &str) -> ScopeType {
    match s {
        "full" => ScopeType::Full,
        "partial" => ScopeType::Partial,
        "none" => ScopeType::None,
        _ => ScopeType::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot() -> PolicySnapshot {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let scope = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };

        PolicySnapshot::new(
            tenant_id,
            intent_id,
            1,
            "v1.0.0".to_string(),
            scope,
            "abc123".to_string(),
        )
    }

    #[tokio::test]
    async fn test_create_snapshot() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let snapshot = create_test_snapshot();
        let id = snapshot.id;

        let result = repo.create_snapshot(snapshot).await;
        assert!(result.is_ok());

        // Verify stored
        let stored = repo.get_snapshot(id).await.unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.intent_version, 1);
    }

    #[tokio::test]
    async fn test_get_latest_by_intent() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create snapshots for versions 1, 2, 3
        for version in 1..=3 {
            let scope = ScopeDefinition::default();
            let snapshot = PolicySnapshot::new(
                tenant_id,
                intent_id,
                version,
                "v1.0.0".to_string(),
                scope,
                format!("hash{}", version),
            );
            repo.create_snapshot(snapshot).await.unwrap();
        }

        let latest = repo
            .get_latest_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().intent_version, 3);
    }

    #[tokio::test]
    async fn test_get_by_intent_version() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let scope = ScopeDefinition::default();
        let snapshot = PolicySnapshot::new(
            tenant_id,
            intent_id,
            5,
            "v2.0.0".to_string(),
            scope,
            "xyz789".to_string(),
        );
        repo.create_snapshot(snapshot).await.unwrap();

        let found = repo
            .get_by_intent_version(intent_id, 5, tenant_id)
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().intent_version, 5);
    }

    #[tokio::test]
    async fn test_get_by_intent_version_not_found() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let found = repo
            .get_by_intent_version(intent_id, 999, tenant_id)
            .await
            .unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create snapshots for versions 1, 2, 3
        for version in 1..=3 {
            let scope = ScopeDefinition::default();
            let snapshot = PolicySnapshot::new(
                tenant_id,
                intent_id,
                version,
                "v1.0.0".to_string(),
                scope,
                format!("hash{}", version),
            );
            repo.create_snapshot(snapshot).await.unwrap();
        }

        let snapshots = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(snapshots.len(), 3);
        // Should be ordered by version descending
        assert_eq!(snapshots[0].intent_version, 3);
        assert_eq!(snapshots[1].intent_version, 2);
        assert_eq!(snapshots[2].intent_version, 1);
    }

    #[tokio::test]
    async fn test_list_by_intent_filters_tenant() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create snapshot for tenant 1
        let scope = ScopeDefinition::default();
        let snapshot1 = PolicySnapshot::new(
            tenant_1,
            intent_id,
            1,
            "v1.0.0".to_string(),
            scope.clone(),
            "hash1".to_string(),
        );
        repo.create_snapshot(snapshot1).await.unwrap();

        // Create snapshot for tenant 2
        let snapshot2 = PolicySnapshot::new(
            tenant_2,
            intent_id,
            1,
            "v1.0.0".to_string(),
            scope,
            "hash2".to_string(),
        );
        repo.create_snapshot(snapshot2).await.unwrap();

        // List for tenant 1 should only return tenant 1's snapshot
        let snapshots_1 = repo.list_by_intent(intent_id, tenant_1).await.unwrap();
        assert_eq!(snapshots_1.len(), 1);
        assert_eq!(snapshots_1[0].tenant_id, tenant_1);

        // List for tenant 2 should only return tenant 2's snapshot
        let snapshots_2 = repo.list_by_intent(intent_id, tenant_2).await.unwrap();
        assert_eq!(snapshots_2.len(), 1);
        assert_eq!(snapshots_2[0].tenant_id, tenant_2);
    }

    #[tokio::test]
    async fn test_get_snapshot_not_found() {
        let repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let id = Uuid::new_v4();
        let result = repo.get_snapshot(id).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::PolicySnapshotNotFound(found_id) if found_id == id
        ));
    }
}

// =============================================================================
// SqlxPolicySnapshotRepository unit tests (helper function tests)
// These test the enum conversion logic without requiring a database connection.
// =============================================================================

#[cfg(test)]
mod sqlx_policy_snapshot_tests {
    use super::*;

    #[test]
    fn test_scope_type_to_string() {
        assert_eq!(scope_type_to_string(&ScopeType::Full), "full");
        assert_eq!(scope_type_to_string(&ScopeType::Partial), "partial");
        assert_eq!(scope_type_to_string(&ScopeType::None), "none");
    }

    #[test]
    fn test_scope_type_from_string() {
        assert_eq!(scope_type_from_string("full"), ScopeType::Full);
        assert_eq!(scope_type_from_string("partial"), ScopeType::Partial);
        assert_eq!(scope_type_from_string("none"), ScopeType::None);
        // Unknown values default to None
        assert_eq!(scope_type_from_string("unknown"), ScopeType::None);
    }
}
