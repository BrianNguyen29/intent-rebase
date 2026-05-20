//! Forensic bundle repository trait and implementations
//!
//! Phase 3 Batch 3b (P4 bounded slice): Bundle persistence primitives and generation status tracking.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.
//!
//! **This slice scope:** BundleStatus tracking, repository trait, and in-memory implementation.
//! **Out of scope:** S3 storage, HTTP API, bundle generation, integrity verification, replay.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::bundle::{
    BundleIntegrity, BundlePurpose, BundleRetention, BundleStatus, BundleTimeRange, ForensicBundle,
};
use super::bundle_contents::BundleContents;

/// Repository trait for forensic bundle storage.
///
/// **P4 bounded slice scope:** Core CRUD methods with bundle status tracking.
/// S3 persistence, generation API, integrity verification, and replay are Phase 4 scope.
#[async_trait]
pub trait BundleRepository: Send + Sync {
    /// Create a new bundle record with Pending status.
    ///
    /// Returns an error if a bundle with the same ID already exists.
    async fn create(&self, bundle: ForensicBundle) -> Result<ForensicBundle, IntentRebaseError>;

    /// Get a bundle by its ID.
    async fn get(&self, bundle_id: Uuid) -> Result<ForensicBundle, IntentRebaseError>;

    /// List all bundles for a given tenant.
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError>;

    /// List all bundles for a given tenant with a specific status.
    async fn list_by_tenant_and_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError>;

    /// List all bundles for a given tenant and purpose.
    async fn list_by_tenant_and_purpose(
        &self,
        tenant_id: Uuid,
        purpose: BundlePurpose,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError>;

    /// Update bundle status with validation.
    ///
    /// Returns an error if the status transition is invalid.
    async fn update_status(
        &self,
        bundle_id: Uuid,
        new_status: BundleStatus,
    ) -> Result<ForensicBundle, IntentRebaseError>;

    /// Update bundle contents after generation completes.
    async fn update_contents(
        &self,
        bundle_id: Uuid,
        contents: BundleContents,
    ) -> Result<ForensicBundle, IntentRebaseError>;

    /// List all bundles in a terminal state (Ready or Failed) for a tenant.
    async fn list_terminal_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxBundleRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxBundleRepository> {
        None
    }
}

// =============================================================================
// In-memory implementation
// =============================================================================

/// In-memory implementation for testing and Phase 3 Batch 3b (P4 bounded slice).
pub struct InMemoryBundleRepository {
    bundles: RwLock<HashMap<Uuid, ForensicBundle>>,
    /// Secondary index: tenant_id -> list of bundle_ids
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: (tenant_id, status) -> list of bundle_ids
    by_status: RwLock<HashMap<(Uuid, BundleStatus), Vec<Uuid>>>,
    /// Secondary index: (tenant_id, purpose) -> list of bundle_ids
    by_purpose: RwLock<HashMap<(Uuid, BundlePurpose), Vec<Uuid>>>,
}

impl InMemoryBundleRepository {
    pub fn new() -> Self {
        Self {
            bundles: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
            by_status: RwLock::new(HashMap::new()),
            by_purpose: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryBundleRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BundleRepository for InMemoryBundleRepository {
    async fn create(&self, bundle: ForensicBundle) -> Result<ForensicBundle, IntentRebaseError> {
        let mut bundles = self.bundles.write().await;
        let mut by_tenant = self.by_tenant.write().await;
        let mut by_status = self.by_status.write().await;
        let mut by_purpose = self.by_purpose.write().await;

        // Check for duplicate bundle_id
        if bundles.contains_key(&bundle.bundle_id) {
            return Err(IntentRebaseError::Internal(format!(
                "bundle with id '{}' already exists",
                bundle.bundle_id
            )));
        }

        bundles.insert(bundle.bundle_id, bundle.clone());

        by_tenant
            .entry(bundle.tenant_id)
            .or_insert_with(Vec::new)
            .push(bundle.bundle_id);

        by_status
            .entry((bundle.tenant_id, bundle.status))
            .or_insert_with(Vec::new)
            .push(bundle.bundle_id);

        by_purpose
            .entry((bundle.tenant_id, bundle.purpose))
            .or_insert_with(Vec::new)
            .push(bundle.bundle_id);

        Ok(bundle)
    }

    async fn get(&self, bundle_id: Uuid) -> Result<ForensicBundle, IntentRebaseError> {
        let bundles = self.bundles.read().await;
        bundles
            .get(&bundle_id)
            .cloned()
            .ok_or(IntentRebaseError::ForensicBundleNotFound(bundle_id))
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let bundles = self.bundles.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();
        let mut result: Vec<ForensicBundle> = ids
            .iter()
            .filter_map(|id| bundles.get(id).cloned())
            .collect();

        // Sort by created_at descending
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }

    async fn list_by_tenant_and_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let bundles = self.bundles.read().await;
        let by_status = self.by_status.read().await;

        let ids = by_status
            .get(&(tenant_id, status))
            .cloned()
            .unwrap_or_default();
        let result: Vec<ForensicBundle> = ids
            .iter()
            .filter_map(|id| bundles.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn list_by_tenant_and_purpose(
        &self,
        tenant_id: Uuid,
        purpose: BundlePurpose,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let bundles = self.bundles.read().await;
        let by_purpose = self.by_purpose.read().await;

        let ids = by_purpose
            .get(&(tenant_id, purpose))
            .cloned()
            .unwrap_or_default();
        let result: Vec<ForensicBundle> = ids
            .iter()
            .filter_map(|id| bundles.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn update_status(
        &self,
        bundle_id: Uuid,
        new_status: BundleStatus,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let mut bundles = self.bundles.write().await;
        let mut by_status = self.by_status.write().await;

        let bundle = bundles
            .get_mut(&bundle_id)
            .ok_or(IntentRebaseError::ForensicBundleNotFound(bundle_id))?;

        // Validate status transition
        if !bundle.status.can_transition_to(new_status) {
            return Err(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", bundle.status),
                to_status: format!("{:?}", new_status),
                reason: "invalid status transition".to_string(),
            });
        }

        let old_status = bundle.status;
        let tenant_id = bundle.tenant_id;

        // Update status
        bundle.status = new_status;

        // Maintain by_status secondary index
        if let Some(old_list) = by_status.get_mut(&(tenant_id, old_status)) {
            old_list.retain(|&id| id != bundle_id);
        }
        by_status
            .entry((tenant_id, new_status))
            .or_insert_with(Vec::new)
            .push(bundle_id);

        Ok(bundle.clone())
    }

    async fn update_contents(
        &self,
        bundle_id: Uuid,
        contents: BundleContents,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let mut bundles = self.bundles.write().await;

        let bundle = bundles
            .get_mut(&bundle_id)
            .ok_or(IntentRebaseError::ForensicBundleNotFound(bundle_id))?;

        bundle.contents = contents;

        Ok(bundle.clone())
    }

    async fn list_terminal_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let bundles = self.bundles.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();
        let mut result: Vec<ForensicBundle> = ids
            .iter()
            .filter_map(|id| bundles.get(id).cloned())
            .filter(|b| b.status.is_terminal())
            .collect();

        // Sort by created_at descending
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }

    fn as_sqlx_repo(&self) -> Option<&SqlxBundleRepository> {
        None
    }
}

// =============================================================================
// SQLx-backed Bundle Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

/// SQL-backed repository for forensic bundle storage using PostgreSQL.
pub struct SqlxBundleRepository {
    pool: PgPool,
}

impl SqlxBundleRepository {
    /// Create a new SqlxBundleRepository.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn row_to_bundle(&self, row: PgRow) -> Result<ForensicBundle, IntentRebaseError> {
        let contents_json: serde_json::Value = row.get("contents");
        let contents: BundleContents = serde_json::from_value(contents_json)
            .map_err(|e| IntentRebaseError::Internal(format!("deserialize contents: {}", e)))?;

        let integrity_json: serde_json::Value = row.get("integrity");
        let integrity: BundleIntegrity = serde_json::from_value(integrity_json)
            .map_err(|e| IntentRebaseError::Internal(format!("deserialize integrity: {}", e)))?;

        let retention: Option<BundleRetention> = row
            .get::<Option<serde_json::Value>, _>("retention")
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| IntentRebaseError::Internal(format!("deserialize retention: {}", e)))?;

        let time_range = BundleTimeRange {
            start: row.get("time_range_start"),
            end: row.get("time_range_end"),
        };

        Ok(ForensicBundle {
            bundle_id: row.get("bundle_id"),
            tenant_id: row.get("tenant_id"),
            bundle_version: row.get("bundle_version"),
            created_at: row.get("created_at"),
            created_by: row.get("created_by"),
            time_range,
            purpose: bundle_purpose_from_string(&row.get::<String, _>("purpose"))?,
            status: bundle_status_from_string(&row.get::<String, _>("status"))?,
            contents,
            integrity,
            retention,
        })
    }
}

fn bundle_status_to_string(s: BundleStatus) -> &'static str {
    match s {
        BundleStatus::Pending => "pending",
        BundleStatus::Generating => "generating",
        BundleStatus::Ready => "ready",
        BundleStatus::Failed => "failed",
    }
}

fn bundle_status_from_string(s: &str) -> Result<BundleStatus, IntentRebaseError> {
    match s {
        "pending" => Ok(BundleStatus::Pending),
        "generating" => Ok(BundleStatus::Generating),
        "ready" => Ok(BundleStatus::Ready),
        "failed" => Ok(BundleStatus::Failed),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown bundle status: {}",
            other
        ))),
    }
}

fn bundle_purpose_to_string(p: BundlePurpose) -> &'static str {
    match p {
        BundlePurpose::IncidentInvestigation => "incident_investigation",
        BundlePurpose::ComplianceAudit => "compliance_audit",
        BundlePurpose::Legal => "legal",
    }
}

fn bundle_purpose_from_string(s: &str) -> Result<BundlePurpose, IntentRebaseError> {
    match s {
        "incident_investigation" => Ok(BundlePurpose::IncidentInvestigation),
        "compliance_audit" => Ok(BundlePurpose::ComplianceAudit),
        "legal" => Ok(BundlePurpose::Legal),
        other => Err(IntentRebaseError::Internal(format!(
            "unknown bundle purpose: {}",
            other
        ))),
    }
}

#[async_trait]
impl BundleRepository for SqlxBundleRepository {
    async fn create(&self, bundle: ForensicBundle) -> Result<ForensicBundle, IntentRebaseError> {
        let contents_json = serde_json::to_value(&bundle.contents)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize contents: {}", e)))?;
        let integrity_json = serde_json::to_value(&bundle.integrity)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize integrity: {}", e)))?;
        let retention_json = bundle
            .retention
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok());

        sqlx::query(
            r#"
            INSERT INTO forensic_bundles (
                bundle_id, tenant_id, bundle_version, created_at, created_by,
                time_range_start, time_range_end, purpose, status,
                contents, integrity, retention
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(bundle.bundle_id)
        .bind(bundle.tenant_id)
        .bind(&bundle.bundle_version)
        .bind(bundle.created_at)
        .bind(&bundle.created_by)
        .bind(bundle.time_range.start)
        .bind(bundle.time_range.end)
        .bind(bundle_purpose_to_string(bundle.purpose))
        .bind(bundle_status_to_string(bundle.status))
        .bind(contents_json)
        .bind(integrity_json)
        .bind(retention_json)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert forensic bundle: {}", e)))?;

        Ok(bundle)
    }

    async fn get(&self, bundle_id: Uuid) -> Result<ForensicBundle, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE bundle_id = $1
            "#,
        )
        .bind(bundle_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch forensic bundle: {}", e)))?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let limit = limit.unwrap_or(100) as i32;
        let rows = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list forensic bundles by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_bundle(r)).collect()
    }

    async fn list_by_tenant_and_status(
        &self,
        tenant_id: Uuid,
        status: BundleStatus,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE tenant_id = $1 AND status = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_status_to_string(status))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list forensic bundles by tenant and status: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_bundle(r)).collect()
    }

    async fn list_by_tenant_and_purpose(
        &self,
        tenant_id: Uuid,
        purpose: BundlePurpose,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE tenant_id = $1 AND purpose = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_purpose_to_string(purpose))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list forensic bundles by tenant and purpose: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_bundle(r)).collect()
    }

    async fn update_status(
        &self,
        bundle_id: Uuid,
        new_status: BundleStatus,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        // Fetch current bundle to validate transition
        let current = self.get(bundle_id).await?;
        if !current.status.can_transition_to(new_status) {
            return Err(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", current.status),
                to_status: format!("{:?}", new_status),
                reason: "invalid status transition".to_string(),
            });
        }

        let row = sqlx::query(
            r#"
            UPDATE forensic_bundles
            SET status = $2
            WHERE bundle_id = $1
            RETURNING bundle_id, tenant_id, bundle_version, created_at, created_by,
                      time_range_start, time_range_end, purpose, status,
                      contents, integrity, retention
            "#,
        )
        .bind(bundle_id)
        .bind(bundle_status_to_string(new_status))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update forensic bundle status: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }

    async fn update_contents(
        &self,
        bundle_id: Uuid,
        contents: BundleContents,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let contents_json = serde_json::to_value(&contents)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize contents: {}", e)))?;

        let row = sqlx::query(
            r#"
            UPDATE forensic_bundles
            SET contents = $2
            WHERE bundle_id = $1
            RETURNING bundle_id, tenant_id, bundle_version, created_at, created_by,
                      time_range_start, time_range_end, purpose, status,
                      contents, integrity, retention
            "#,
        )
        .bind(bundle_id)
        .bind(contents_json)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update forensic bundle contents: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }

    async fn list_terminal_bundles(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let limit = limit.unwrap_or(100) as i32;
        let rows = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE tenant_id = $1 AND status IN ('ready', 'failed')
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list terminal forensic bundles: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_bundle(r)).collect()
    }

    fn as_sqlx_repo(&self) -> Option<&SqlxBundleRepository> {
        Some(self)
    }
}

// =============================================================================
// Transaction helper methods for RLS-aware operations
// =============================================================================

impl SqlxBundleRepository {
    /// Create a new bundle record within an external transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    pub async fn create_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        bundle: ForensicBundle,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let contents_json = serde_json::to_value(&bundle.contents)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize contents: {}", e)))?;
        let integrity_json = serde_json::to_value(&bundle.integrity)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize integrity: {}", e)))?;
        let retention_json = bundle
            .retention
            .as_ref()
            .and_then(|r| serde_json::to_value(r).ok());

        sqlx::query(
            r#"
            INSERT INTO forensic_bundles (
                bundle_id, tenant_id, bundle_version, created_at, created_by,
                time_range_start, time_range_end, purpose, status,
                contents, integrity, retention
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(bundle.bundle_id)
        .bind(bundle.tenant_id)
        .bind(&bundle.bundle_version)
        .bind(bundle.created_at)
        .bind(&bundle.created_by)
        .bind(bundle.time_range.start)
        .bind(bundle.time_range.end)
        .bind(bundle_purpose_to_string(bundle.purpose))
        .bind(bundle_status_to_string(bundle.status))
        .bind(contents_json)
        .bind(integrity_json)
        .bind(retention_json)
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert forensic bundle: {}", e)))?;

        Ok(bundle)
    }

    /// Get a bundle by its ID within an external transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    pub async fn get_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        bundle_id: Uuid,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE bundle_id = $1
            "#,
        )
        .bind(bundle_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch forensic bundle: {}", e)))?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }

    /// List all bundles for a given tenant within an external transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    pub async fn list_by_tenant_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<ForensicBundle>, IntentRebaseError> {
        let limit = limit.unwrap_or(100) as i32;
        let rows = sqlx::query(
            r#"
            SELECT bundle_id, tenant_id, bundle_version, created_at, created_by,
                   time_range_start, time_range_end, purpose, status,
                   contents, integrity, retention
            FROM forensic_bundles
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list forensic bundles by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_bundle(r)).collect()
    }

    /// Update bundle status with validation within an external transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    pub async fn update_status_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        bundle_id: Uuid,
        new_status: BundleStatus,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        // Fetch current bundle to validate transition
        let current = self.get_with_tx(tx, bundle_id).await?;
        if !current.status.can_transition_to(new_status) {
            return Err(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", current.status),
                to_status: format!("{:?}", new_status),
                reason: "invalid status transition".to_string(),
            });
        }

        let row = sqlx::query(
            r#"
            UPDATE forensic_bundles
            SET status = $2
            WHERE bundle_id = $1
            RETURNING bundle_id, tenant_id, bundle_version, created_at, created_by,
                      time_range_start, time_range_end, purpose, status,
                      contents, integrity, retention
            "#,
        )
        .bind(bundle_id)
        .bind(bundle_status_to_string(new_status))
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update forensic bundle status: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }

    /// Update bundle contents after generation completes within an external transaction.
    ///
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// which sets the RLS tenant context before any operations.
    pub async fn update_contents_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        bundle_id: Uuid,
        contents: BundleContents,
    ) -> Result<ForensicBundle, IntentRebaseError> {
        let contents_json = serde_json::to_value(&contents)
            .map_err(|e| IntentRebaseError::Internal(format!("serialize contents: {}", e)))?;

        let row = sqlx::query(
            r#"
            UPDATE forensic_bundles
            SET contents = $2
            WHERE bundle_id = $1
            RETURNING bundle_id, tenant_id, bundle_version, created_at, created_by,
                      time_range_start, time_range_end, purpose, status,
                      contents, integrity, retention
            "#,
        )
        .bind(bundle_id)
        .bind(contents_json)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update forensic bundle contents: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_bundle(r),
            None => Err(IntentRebaseError::ForensicBundleNotFound(bundle_id)),
        }
    }
}
