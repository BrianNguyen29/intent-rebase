//! Tenant-scoped rule pack registry primitives
//!
//! This module provides the foundation for multi-tenant rule pack isolation.
//! Rule packs are scoped to a tenant_id, ensuring tenant A cannot access tenant B's packs.
//!
//! Bounded slice scope (P3-S3):
//! - Registry model and repository trait
//! - In-memory implementation for testing
//! - Tenant isolation tests
//!
//! Out of scope for this slice:
//! - Full upload/management API (Phase 4+)
//! - S3/object storage integration
//! - Rule evaluation engine rewiring

use crate::rule_pack::{RulePack, RulePackStatus, RulePackVersion};
use async_trait::async_trait;
use uuid::Uuid;

/// Errors specific to rule pack registry operations
#[derive(Debug, thiserror::Error)]
pub enum RulePackRegistryError {
    #[error("Rule pack not found: {0}")]
    NotFound(String),

    #[error("Tenant not authorized to access this rule pack")]
    Unauthorized,

    #[error("Version conflict: {0}")]
    VersionConflict(String),

    #[error("Invalid status transition: {0}")]
    InvalidStatusTransition(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Repository trait for tenant-scoped rule pack storage.
///
/// All methods require a tenant_id to enforce tenant isolation.
/// A tenant can only access their own rule packs.
#[async_trait]
pub trait TenantRulePackRepository: Send + Sync {
    /// List all rule packs for a tenant, optionally filtered by status.
    async fn list_packs(
        &self,
        tenant_id: Uuid,
        status_filter: Option<RulePackStatus>,
    ) -> Result<Vec<RulePack>, RulePackRegistryError>;

    /// Get a specific rule pack by version for a tenant.
    async fn get_pack(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
    ) -> Result<RulePack, RulePackRegistryError>;

    /// Get the active rule pack for a tenant.
    /// Returns the pack with Active status, or error if none exists.
    async fn get_active_pack(&self, tenant_id: Uuid) -> Result<RulePack, RulePackRegistryError>;

    /// Create a new rule pack for a tenant.
    /// The pack must have Active status initially.
    async fn create_pack(
        &self,
        tenant_id: Uuid,
        pack: RulePack,
    ) -> Result<RulePack, RulePackRegistryError>;

    /// Update the status of a rule pack (e.g., Active -> Deprecated).
    /// Only the pack owner (tenant) can update status.
    async fn update_pack_status(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
        new_status: RulePackStatus,
    ) -> Result<RulePack, RulePackRegistryError>;

    /// List all versions for a specific tenant.
    async fn list_versions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RulePackVersion>, RulePackRegistryError>;
}

/// In-memory implementation of TenantRulePackRepository for testing.
///
/// Uses per-tenant HashMaps for isolation.
pub struct InMemoryTenantRulePackRepository {
    /// Per-tenant rule pack storage.
    /// Structure: tenant_id -> (version -> pack)
    packs: std::sync::RwLock<
        std::collections::HashMap<Uuid, std::collections::HashMap<RulePackVersion, RulePack>>,
    >,
}

impl InMemoryTenantRulePackRepository {
    pub fn new() -> Self {
        Self {
            packs: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryTenantRulePackRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TenantRulePackRepository for InMemoryTenantRulePackRepository {
    async fn list_packs(
        &self,
        tenant_id: Uuid,
        status_filter: Option<RulePackStatus>,
    ) -> Result<Vec<RulePack>, RulePackRegistryError> {
        let packs = self.packs.read().unwrap();
        let tenant_packs: Vec<RulePack> = packs
            .get(&tenant_id)
            .map(|t| t.values().cloned().collect())
            .unwrap_or_default();

        match status_filter {
            Some(status) => Ok(tenant_packs
                .into_iter()
                .filter(|p| p.status == status)
                .collect()),
            None => Ok(tenant_packs),
        }
    }

    async fn get_pack(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
    ) -> Result<RulePack, RulePackRegistryError> {
        let packs = self.packs.read().unwrap();
        match packs.get(&tenant_id).and_then(|t| t.get(version)) {
            Some(pack) => Ok(pack.clone()),
            None => Err(RulePackRegistryError::NotFound(format!(
                "tenant={}, version={}",
                tenant_id, version
            ))),
        }
    }

    async fn get_active_pack(&self, tenant_id: Uuid) -> Result<RulePack, RulePackRegistryError> {
        let packs = self.packs.read().unwrap();
        let tenant_packs = packs.get(&tenant_id);

        match tenant_packs {
            Some(packs_map) => packs_map
                .values()
                .find(|p| p.status == RulePackStatus::Active)
                .cloned()
                .ok_or_else(|| {
                    RulePackRegistryError::NotFound(format!(
                        "no active pack for tenant={}",
                        tenant_id
                    ))
                }),
            None => Err(RulePackRegistryError::NotFound(format!(
                "no packs found for tenant={}",
                tenant_id
            ))),
        }
    }

    async fn create_pack(
        &self,
        tenant_id: Uuid,
        mut pack: RulePack,
    ) -> Result<RulePack, RulePackRegistryError> {
        // Enforce Active status on creation
        pack.status = RulePackStatus::Active;

        let mut packs = self.packs.write().unwrap();
        let tenant_packs = packs.entry(tenant_id).or_default();

        // Check if version already exists
        if tenant_packs.contains_key(&pack.version) {
            return Err(RulePackRegistryError::VersionConflict(format!(
                "version {} already exists for tenant {}",
                pack.version, tenant_id
            )));
        }

        tenant_packs.insert(pack.version.clone(), pack.clone());
        Ok(pack)
    }

    async fn update_pack_status(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
        new_status: RulePackStatus,
    ) -> Result<RulePack, RulePackRegistryError> {
        let mut packs = self.packs.write().unwrap();
        let tenant_packs = packs
            .get_mut(&tenant_id)
            .ok_or_else(|| RulePackRegistryError::NotFound(format!("tenant={}", tenant_id)))?;

        let pack = tenant_packs.get_mut(version).ok_or_else(|| {
            RulePackRegistryError::NotFound(format!("tenant={}, version={}", tenant_id, version))
        })?;

        // Validate status transition
        match (&pack.status, &new_status) {
            (RulePackStatus::Active, RulePackStatus::Deprecated) => {}
            (RulePackStatus::Active, RulePackStatus::Superseded) => {}
            (RulePackStatus::Deprecated, RulePackStatus::Active) => {}
            _ => {
                return Err(RulePackRegistryError::InvalidStatusTransition(format!(
                    "cannot transition from {:?} to {:?}",
                    pack.status, new_status
                )))
            }
        }

        pack.status = new_status;
        Ok(pack.clone())
    }

    async fn list_versions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RulePackVersion>, RulePackRegistryError> {
        let packs = self.packs.read().unwrap();
        let tenant_packs = packs.get(&tenant_id);

        match tenant_packs {
            Some(packs_map) => {
                let mut versions: Vec<RulePackVersion> = packs_map.keys().cloned().collect();
                versions.sort();
                Ok(versions)
            }
            None => Ok(vec![]),
        }
    }
}

// =============================================================================
// SQL-backed implementation (requires sqlx dependency)
// =============================================================================
//
// The SQL implementation below is provided for reference. To enable it:
// 1. Add sqlx to rebase-engine/Cargo.toml dependencies
// 2. Create the rule_packs table:
//
// ```sql
// CREATE TABLE rule_packs (
//     tenant_id UUID NOT NULL,
//     version VARCHAR(20) NOT NULL,
//     name VARCHAR(255) NOT NULL,
//     status VARCHAR(20) NOT NULL DEFAULT 'active',
//     risk_config JSONB NOT NULL,
//     propagation_config JSONB NOT NULL,
//     description TEXT,
//     created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
//     PRIMARY KEY (tenant_id, version)
// );
// ```
//
// NOTE: The SqlxTenantRulePackRepository below is commented out because
// rebase-engine does not depend on sqlx. It should be moved to intent-service
// or a dedicated rule-pack-service crate for production use.

/*
pub struct SqlxTenantRulePackRepository {
    pool: sqlx::PgPool,
}

impl SqlxTenantRulePackRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenantRulePackRepository for SqlxTenantRulePackRepository {
    async fn list_packs(
        &self,
        tenant_id: Uuid,
        status_filter: Option<RulePackStatus>,
    ) -> Result<Vec<RulePack>, RulePackRegistryError> {
        let status_str = status_filter.map(|s| match s {
            RulePackStatus::Draft => "draft",
            RulePackStatus::Active => "active",
            RulePackStatus::Deprecated => "deprecated",
            RulePackStatus::Superseded => "superseded",
        });

        let packs: Vec<SqlxRulePackRow> = match status_str {
            Some(status) => {
                sqlx::query_as::<_, SqlxRulePackRow>(
                    "SELECT tenant_id, version, name, status, risk_config, propagation_config, description
                     FROM rule_packs WHERE tenant_id = $1 AND status = $2"
                )
                .bind(tenant_id)
                .bind(status)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SqlxRulePackRow>(
                    "SELECT tenant_id, version, name, status, risk_config, propagation_config, description
                     FROM rule_packs WHERE tenant_id = $1"
                )
                .bind(tenant_id)
                .fetch_all(&self.pool)
                .await
            }
        }.map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?;

        packs.into_iter().map(|row| row.try_into()).collect()
    }

    async fn get_pack(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
    ) -> Result<RulePack, RulePackRegistryError> {
        let row = sqlx::query_as::<_, SqlxRulePackRow>(
            "SELECT tenant_id, version, name, status, risk_config, propagation_config, description
             FROM rule_packs WHERE tenant_id = $1 AND version = $2"
        )
        .bind(tenant_id)
        .bind(version.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?
        .ok_or_else(|| RulePackRegistryError::NotFound(format!("tenant={}, version={}", tenant_id, version)))?;

        row.try_into()
    }

    async fn get_active_pack(
        &self,
        tenant_id: Uuid,
    ) -> Result<RulePack, RulePackRegistryError> {
        let row = sqlx::query_as::<_, SqlxRulePackRow>(
            "SELECT tenant_id, version, name, status, risk_config, propagation_config, description
             FROM rule_packs WHERE tenant_id = $1 AND status = 'active'"
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?
        .ok_or_else(|| RulePackRegistryError::NotFound(format!("no active pack for tenant={}", tenant_id)))?;

        row.try_into()
    }

    async fn create_pack(
        &self,
        tenant_id: Uuid,
        pack: RulePack,
    ) -> Result<RulePack, RulePackRegistryError> {
        let risk_json = serde_json::to_string(&pack.risk)
            .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?;
        let propagation_json = serde_json::to_string(&pack.propagation)
            .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?;
        let status_str = match pack.status {
            RulePackStatus::Draft => "draft",
            RulePackStatus::Active => "active",
            RulePackStatus::Deprecated => "deprecated",
            RulePackStatus::Superseded => "superseded",
        };

        sqlx::query(
            "INSERT INTO rule_packs (tenant_id, version, name, status, risk_config, propagation_config, description)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(tenant_id)
        .bind(pack.version.to_string())
        .bind(&pack.name)
        .bind(status_str)
        .bind(&risk_json)
        .bind(&propagation_json)
        .bind(&pack.description)
        .execute(&self.pool)
        .await
        .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?;

        Ok(pack)
    }

    async fn update_pack_status(
        &self,
        tenant_id: Uuid,
        version: &RulePackVersion,
        new_status: RulePackStatus,
    ) -> Result<RulePack, RulePackRegistryError> {
        let status_str = match new_status {
            RulePackStatus::Draft => "draft",
            RulePackStatus::Active => "active",
            RulePackStatus::Deprecated => "deprecated",
            RulePackStatus::Superseded => "superseded",
        };

        let rows_affected = sqlx::query(
            "UPDATE rule_packs SET status = $1 WHERE tenant_id = $2 AND version = $3"
        )
        .bind(status_str)
        .bind(tenant_id)
        .bind(version.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(RulePackRegistryError::NotFound(format!(
                "tenant={}, version={}",
                tenant_id, version
            )));
        }

        self.get_pack(tenant_id, version).await
    }

    async fn list_versions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RulePackVersion>, RulePackRegistryError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT version FROM rule_packs WHERE tenant_id = $1 ORDER BY version"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))?;

        rows.into_iter()
            .filter_map(|(v,)| RulePackVersion::parse(&v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| RulePackRegistryError::StorageError(e.to_string()))
    }
}

#[derive(sqlx::FromRow)]
struct SqlxRulePackRow {
    tenant_id: Uuid,
    version: String,
    name: String,
    status: String,
    risk_config: serde_json::Value,
    propagation_config: serde_json::Value,
    description: Option<String>,
}

impl TryFrom<SqlxRulePackRow> for RulePack {
    type Error = RulePackRegistryError;

    fn try_from(row: SqlxRulePackRow) -> Result<Self, Self::Error> {
        let version = RulePackVersion::parse(&row.version)
            .ok_or_else(|| RulePackRegistryError::StorageError(format!("invalid version: {}", row.version)))?;
        let status = match row.status.as_str() {
            "draft" => RulePackStatus::Draft,
            "active" => RulePackStatus::Active,
            "deprecated" => RulePackStatus::Deprecated,
            "superseded" => RulePackStatus::Superseded,
            _ => return Err(RulePackRegistryError::StorageError(format!("invalid status: {}", row.status))),
        };
        let risk: crate::rule_pack::RulePackRiskConfig = serde_json::from_value(row.risk_config)
            .map_err(|e| RulePackRegistryError::StorageError(format!("risk_config parse error: {}", e)))?;
        let propagation: crate::rule_pack::RulePackPropagationConfig = serde_json::from_value(row.propagation_config)
            .map_err(|e| RulePackRegistryError::StorageError(format!("propagation_config parse error: {}", e)))?;

        Ok(RulePack {
            version,
            name: row.name,
            status,
            risk,
            propagation,
            description: row.description,
        })
    }
}
*/

#[cfg(test)]
mod tenant_isolation_tests {
    use super::*;
    use crate::rule_pack::{RulePack, RulePackRiskConfig};
    use std::sync::Arc;

    fn create_test_pack(tenant_id: Uuid, version: &str, name: &str) -> RulePack {
        RulePack {
            version: RulePackVersion::parse(version).unwrap(),
            name: name.to_string(),
            status: RulePackStatus::Active,
            risk: RulePackRiskConfig::default(),
            propagation: crate::rule_pack::RulePackPropagationConfig::default(),
            description: Some(format!("Test pack for {}", tenant_id)),
        }
    }

    #[tokio::test]
    async fn test_tenant_isolation_list_packs() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Create packs for tenant A
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v1.0.0", "pack-a-v1"))
            .await
            .unwrap();
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v1.1.0", "pack-a-v2"))
            .await
            .unwrap();

        // Create packs for tenant B
        repo.create_pack(tenant_b, create_test_pack(tenant_b, "v1.0.0", "pack-b-v1"))
            .await
            .unwrap();

        // Tenant A should only see their 2 packs
        let tenant_a_packs = repo.list_packs(tenant_a, None).await.unwrap();
        assert_eq!(tenant_a_packs.len(), 2);
        assert!(tenant_a_packs.iter().all(|p| p.name.starts_with("pack-a")));

        // Tenant B should only see their 1 pack
        let tenant_b_packs = repo.list_packs(tenant_b, None).await.unwrap();
        assert_eq!(tenant_b_packs.len(), 1);
        assert!(tenant_b_packs[0].name.starts_with("pack-b"));
    }

    #[tokio::test]
    async fn test_tenant_isolation_cross_tenant_access_blocked() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Tenant A creates a pack
        let pack = repo
            .create_pack(
                tenant_a,
                create_test_pack(tenant_a, "v1.0.0", "secret-pack"),
            )
            .await
            .unwrap();

        // Tenant B tries to access tenant A's pack - should get NotFound
        let result = repo.get_pack(tenant_b, &pack.version).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RulePackRegistryError::NotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_tenant_isolation_get_active_pack() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Tenant A creates an active pack
        repo.create_pack(
            tenant_a,
            create_test_pack(tenant_a, "v1.0.0", "active-pack-a"),
        )
        .await
        .unwrap();

        // Tenant B has no active pack
        let result = repo.get_active_pack(tenant_b).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RulePackRegistryError::NotFound(_)
        ));

        // Tenant A should find their active pack
        let active = repo.get_active_pack(tenant_a).await;
        assert!(active.is_ok());
        assert_eq!(active.unwrap().name, "active-pack-a");
    }

    #[tokio::test]
    async fn test_tenant_isolation_update_status_blocked() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Tenant A creates a pack
        let pack = repo
            .create_pack(
                tenant_a,
                create_test_pack(tenant_a, "v1.0.0", "pack-to-deprecate"),
            )
            .await
            .unwrap();

        // Tenant B tries to deprecate tenant A's pack - should get NotFound
        let result = repo
            .update_pack_status(tenant_b, &pack.version, RulePackStatus::Deprecated)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RulePackRegistryError::NotFound(_)
        ));

        // Tenant A can deprecate their own pack
        let deprecated = repo
            .update_pack_status(tenant_a, &pack.version, RulePackStatus::Deprecated)
            .await;
        assert!(deprecated.is_ok());
        assert_eq!(deprecated.unwrap().status, RulePackStatus::Deprecated);
    }

    #[tokio::test]
    async fn test_tenant_isolation_list_versions() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Tenant A creates multiple versions
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v1.0.0", "pack-a-v1"))
            .await
            .unwrap();
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v1.1.0", "pack-a-v2"))
            .await
            .unwrap();
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v2.0.0", "pack-a-v3"))
            .await
            .unwrap();

        // Tenant B has no versions
        let tenant_b_versions = repo.list_versions(tenant_b).await.unwrap();
        assert!(tenant_b_versions.is_empty());

        // Tenant A has 3 versions
        let tenant_a_versions = repo.list_versions(tenant_a).await.unwrap();
        assert_eq!(tenant_a_versions.len(), 3);
    }

    #[tokio::test]
    async fn test_status_filter_respects_tenant_boundary() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Tenant A creates active pack
        repo.create_pack(
            tenant_a,
            create_test_pack(tenant_a, "v1.0.0", "active-pack"),
        )
        .await
        .unwrap();

        // Tenant B creates pack then deprecates it (create_pack enforces Active, so use update)
        let b_pack = repo
            .create_pack(
                tenant_b,
                create_test_pack(tenant_b, "v1.0.0", "deprecated-pack"),
            )
            .await
            .unwrap();
        repo.update_pack_status(tenant_b, &b_pack.version, RulePackStatus::Deprecated)
            .await
            .unwrap();

        // Tenant A queries active packs - should only get their own active pack
        let active_packs = repo
            .list_packs(tenant_a, Some(RulePackStatus::Active))
            .await
            .unwrap();
        assert_eq!(active_packs.len(), 1);
        assert_eq!(active_packs[0].name, "active-pack");

        // Tenant B queries deprecated packs - should only get their own deprecated pack
        let deprecated_packs = repo
            .list_packs(tenant_b, Some(RulePackStatus::Deprecated))
            .await
            .unwrap();
        assert_eq!(deprecated_packs.len(), 1);
        assert_eq!(deprecated_packs[0].name, "deprecated-pack");
    }

    #[tokio::test]
    async fn test_version_conflict_per_tenant() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Both tenants can have v1.0.0 - no conflict across tenants
        repo.create_pack(tenant_a, create_test_pack(tenant_a, "v1.0.0", "pack-a-v1"))
            .await
            .unwrap();
        repo.create_pack(tenant_b, create_test_pack(tenant_b, "v1.0.0", "pack-b-v1"))
            .await
            .unwrap();

        // But within a tenant, duplicate version is blocked
        let result = repo
            .create_pack(tenant_a, create_test_pack(tenant_a, "v1.0.0", "pack-a-dup"))
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RulePackRegistryError::VersionConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_status_transition_validation() {
        let repo = Arc::new(InMemoryTenantRulePackRepository::new());
        let tenant_a = Uuid::new_v4();

        let pack = repo
            .create_pack(
                tenant_a,
                create_test_pack(tenant_a, "v1.0.0", "pack-to-transition"),
            )
            .await
            .unwrap();

        // Valid: Active -> Deprecated
        let deprecated = repo
            .update_pack_status(tenant_a, &pack.version, RulePackStatus::Deprecated)
            .await;
        assert!(deprecated.is_ok());

        // Invalid: Deprecated -> Draft (not a valid transition)
        let result = repo
            .update_pack_status(tenant_a, &pack.version, RulePackStatus::Draft)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RulePackRegistryError::InvalidStatusTransition(_)
        ));
    }
}
