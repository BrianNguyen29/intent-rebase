//! Tenant repository trait and implementations
//!
//! Phase 3 P3-S5: Tenant record storage scaffold.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::tenant::{Tenant, TenantStatus};

/// Repository trait for tenant storage.
///
/// **P3-S5 scope:** Core CRUD methods with tenant-scoped queries.
/// SQL persistence, offboarding deletion flows, and quota management are future scope.
#[async_trait]
pub trait TenantRepository: Send + Sync {
    /// Create a new tenant record.
    ///
    /// Returns an error if a tenant with the same slug already exists.
    async fn create(&self, tenant: Tenant) -> Result<Tenant, IntentRebaseError>;

    /// Get a tenant by its ID.
    async fn get(&self, tenant_id: Uuid) -> Result<Tenant, IntentRebaseError>;

    /// Get a tenant by its slug.
    async fn get_by_slug(&self, slug: &str) -> Result<Tenant, IntentRebaseError>;

    /// List all tenants with a given status.
    async fn list_by_status(&self, status: TenantStatus) -> Result<Vec<Tenant>, IntentRebaseError>;

    /// Update a tenant's status.
    async fn update_status(
        &self,
        tenant_id: Uuid,
        new_status: TenantStatus,
    ) -> Result<Tenant, IntentRebaseError>;

    /// List all tenants (paginated).
    async fn list_all(&self, limit: Option<usize>) -> Result<Vec<Tenant>, IntentRebaseError>;
}

// =============================================================================
// In-memory implementation
// =============================================================================

/// In-memory implementation for testing and Phase 3 P3-S5.
pub struct InMemoryTenantRepository {
    tenants: RwLock<HashMap<Uuid, Tenant>>,
    /// Secondary index: slug -> tenant_id
    by_slug: RwLock<HashMap<String, Uuid>>,
    /// Secondary index: status -> list of tenant_ids
    by_status: RwLock<HashMap<TenantStatus, Vec<Uuid>>>,
}

impl InMemoryTenantRepository {
    pub fn new() -> Self {
        Self {
            tenants: RwLock::new(HashMap::new()),
            by_slug: RwLock::new(HashMap::new()),
            by_status: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryTenantRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TenantRepository for InMemoryTenantRepository {
    async fn create(&self, tenant: Tenant) -> Result<Tenant, IntentRebaseError> {
        let mut tenants = self.tenants.write().await;
        let mut by_slug = self.by_slug.write().await;
        let mut by_status = self.by_status.write().await;

        // Check slug uniqueness
        if by_slug.contains_key(&tenant.slug) {
            return Err(IntentRebaseError::Internal(format!(
                "tenant slug '{}' already exists",
                tenant.slug
            )));
        }

        tenants.insert(tenant.id, tenant.clone());
        by_slug.insert(tenant.slug.clone(), tenant.id);
        by_status
            .entry(tenant.status)
            .or_insert_with(Vec::new)
            .push(tenant.id);

        Ok(tenant)
    }

    async fn get(&self, tenant_id: Uuid) -> Result<Tenant, IntentRebaseError> {
        let tenants = self.tenants.read().await;
        tenants
            .get(&tenant_id)
            .cloned()
            .ok_or(IntentRebaseError::TenantNotFound(tenant_id))
    }

    async fn get_by_slug(&self, slug: &str) -> Result<Tenant, IntentRebaseError> {
        let tenants = self.tenants.read().await;
        let by_slug = self.by_slug.read().await;

        let tenant_id = by_slug
            .get(slug)
            .copied()
            .ok_or(IntentRebaseError::TenantNotFoundBySlug(slug.to_string()))?;

        tenants
            .get(&tenant_id)
            .cloned()
            .ok_or(IntentRebaseError::TenantNotFound(tenant_id))
    }

    async fn list_by_status(&self, status: TenantStatus) -> Result<Vec<Tenant>, IntentRebaseError> {
        let tenants = self.tenants.read().await;
        let by_status = self.by_status.read().await;

        let ids = by_status.get(&status).cloned().unwrap_or_default();
        let result: Vec<Tenant> = ids
            .iter()
            .filter_map(|id| tenants.get(id).cloned())
            .collect();

        Ok(result)
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        new_status: TenantStatus,
    ) -> Result<Tenant, IntentRebaseError> {
        let mut tenants = self.tenants.write().await;
        let mut by_status = self.by_status.write().await;

        let tenant = tenants
            .get_mut(&tenant_id)
            .ok_or(IntentRebaseError::TenantNotFound(tenant_id))?;

        let old_status = tenant.status;
        tenant.status = new_status;
        tenant.updated_at = chrono::Utc::now();

        // Maintain by_status secondary index
        if let Some(old_list) = by_status.get_mut(&old_status) {
            old_list.retain(|&id| id != tenant_id);
        }
        by_status
            .entry(new_status)
            .or_insert_with(Vec::new)
            .push(tenant_id);

        Ok(tenant.clone())
    }

    async fn list_all(&self, limit: Option<usize>) -> Result<Vec<Tenant>, IntentRebaseError> {
        let tenants = self.tenants.read().await;
        let mut result: Vec<Tenant> = tenants.values().cloned().collect();

        // Sort by created_at descending
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(l) = limit {
            result.truncate(l);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::TenantRegion;
    use std::sync::Arc;

    fn create_test_tenant(slug: &str) -> Tenant {
        Tenant::new(
            format!("{} Corp", slug),
            slug.to_string(),
            TenantRegion::UsEast1,
        )
    }

    #[tokio::test]
    async fn test_create_tenant() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant = create_test_tenant("acme");

        let result = repo.create(tenant.clone()).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, tenant.id);
        assert_eq!(created.name, "acme Corp");
        assert_eq!(created.slug, "acme");
        assert_eq!(created.status, TenantStatus::Provisioning);
    }

    #[tokio::test]
    async fn test_create_tenant_duplicate_slug() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant1 = create_test_tenant("acme");
        let tenant2 = create_test_tenant("acme");

        repo.create(tenant1).await.unwrap();
        let result = repo.create(tenant2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_tenant() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant = create_test_tenant("acme");
        let id = tenant.id;

        repo.create(tenant).await.unwrap();

        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_tenant_not_found() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_by_slug() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant = create_test_tenant("acme");

        repo.create(tenant).await.unwrap();

        let result = repo.get_by_slug("acme").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().slug, "acme");
    }

    #[tokio::test]
    async fn test_get_by_slug_not_found() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let result = repo.get_by_slug("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant1 = create_test_tenant("acme1");
        let tenant2 = create_test_tenant("acme2");

        repo.create(tenant1).await.unwrap();
        repo.create(tenant2).await.unwrap();

        let result = repo.list_by_status(TenantStatus::Provisioning).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);

        let result = repo.list_by_status(TenantStatus::Active).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant = create_test_tenant("acme");
        let id = tenant.id;

        repo.create(tenant).await.unwrap();

        let result = repo.update_status(id, TenantStatus::Active).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, TenantStatus::Active);
    }

    #[tokio::test]
    async fn test_list_all() {
        let repo = Arc::new(InMemoryTenantRepository::new());

        for i in 0..5 {
            let tenant = create_test_tenant(&format!("tenant-{}", i));
            repo.create(tenant).await.unwrap();
        }

        let result = repo.list_all(None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 5);

        let result = repo.list_all(Some(2)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_tenant_status_transitions() {
        let repo = Arc::new(InMemoryTenantRepository::new());
        let tenant = create_test_tenant("acme");
        let id = tenant.id;

        repo.create(tenant).await.unwrap();

        // Provisioning -> Active
        repo.update_status(id, TenantStatus::Active).await.unwrap();

        // Active -> Suspended
        repo.update_status(id, TenantStatus::Suspended)
            .await
            .unwrap();

        // Suspended -> Active (reactivation)
        repo.update_status(id, TenantStatus::Active).await.unwrap();

        // Active -> Offboarding
        repo.update_status(id, TenantStatus::Offboarding)
            .await
            .unwrap();

        let result = repo.get(id).await.unwrap();
        assert_eq!(result.status, TenantStatus::Offboarding);
    }
}
