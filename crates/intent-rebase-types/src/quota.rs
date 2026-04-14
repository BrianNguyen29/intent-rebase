//! Phase 3 Batch 3a — P3-S2 Bounded Quota Enforcement Slice
//!
//! Provides tenant resource quotas with in-memory enforcement on highest-value create paths.
//!
//! Scope (P3-S2 bounded slice):
//! - Quota model with tenant-level limits
//! - In-memory quota repository (no SQL dependency for initial slice)
//! - Enforced on: intent creation, artifact ingest
//! - NOT in scope: full quota admin APIs, multi-region, policy engine redesign
//!
//! Design notes:
//! - Uses simple in-memory counts keyed by (tenant_id, resource_type)
//! - Hard limits enforced at create time
//! - Future: SQL-backed quota tracking with async quota reservation

use crate::error::IntentRebaseError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A quota limit for a specific resource type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuotaLimit {
    /// Resource type identifier (e.g., "intents", "artifacts")
    pub resource: String,
    /// Maximum allowed count (hard limit)
    pub limit: i32,
    /// Optional scope (e.g., "global", "workflow_id specific")
    pub scope: Option<String>,
}

impl QuotaLimit {
    pub fn new(resource: impl Into<String>, limit: i32) -> Self {
        Self {
            resource: resource.into(),
            limit,
            scope: None,
        }
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

/// A count of a resource for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCount {
    pub tenant_id: Uuid,
    pub resource: String,
    pub current: i32,
}

impl QuotaCount {
    pub fn new(tenant_id: Uuid, resource: impl Into<String>, current: i32) -> Self {
        Self {
            tenant_id,
            resource: resource.into(),
            current,
        }
    }
}

/// Repository trait for quota storage
/// Allows in-memory (Phase 1) or SQL-backed implementations
#[async_trait]
pub trait QuotaRepository: Send + Sync {
    /// Get the current count for a tenant+resource
    async fn get_count(&self, tenant_id: Uuid, resource: &str) -> Result<i32, IntentRebaseError>;

    /// Increment count for a tenant+resource
    async fn increment(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError>;

    /// Decrement count for a tenant+resource
    async fn decrement(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError>;

    /// Set count for a tenant+resource
    async fn set_count(&self, tenant_id: Uuid, resource: &str, count: i32) -> Result<(), IntentRebaseError>;

    /// Get all limits (for a tenant or global defaults)
    async fn get_limits(&self, tenant_id: Uuid) -> Result<Vec<QuotaLimit>, IntentRebaseError>;

    /// Check if a resource has a specific limit value (returns limit or None if no custom limit)
    async fn get_limit(&self, tenant_id: Uuid, resource: &str) -> Result<Option<i32>, IntentRebaseError>;
}

/// In-memory quota repository for Phase 1 testing and single-instance deployments
pub struct InMemoryQuotaRepository {
    /// Counts keyed by (tenant_id, resource)
    counts: RwLock<HashMap<(Uuid, String), i32>>,
    /// Limits keyed by tenant_id -> resource -> limit
    limits: RwLock<HashMap<Uuid, HashMap<String, i32>>>,
    /// Global default limits
    defaults: RwLock<HashMap<String, i32>>,
}

impl InMemoryQuotaRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom limit for a tenant (overrides global default)
    pub async fn set_limit(&self, tenant_id: Uuid, resource: &str, limit: i32) {
        let mut limits = self.limits.write().await;
        limits
            .entry(tenant_id)
            .or_default()
            .insert(resource.to_string(), limit);
    }

    /// Set a global default limit
    pub async fn set_default(&self, resource: &str, limit: i32) {
        let mut defaults = self.defaults.write().await;
        defaults.insert(resource.to_string(), limit);
    }

    /// Get effective limit for a tenant+resource (custom or default)
    pub async fn get_effective_limit(&self, tenant_id: Uuid, resource: &str) -> i32 {
        // Check tenant-specific limit first
        {
            let limits = self.limits.read().await;
            if let Some(tenant_limits) = limits.get(&tenant_id) {
                if let Some(limit) = tenant_limits.get(resource) {
                    return *limit;
                }
            }
        }
        // Fall back to global default
        let defaults = self.defaults.read().await;
        defaults.get(resource).copied().unwrap_or(i32::MAX)
    }
}

impl Default for InMemoryQuotaRepository {
    fn default() -> Self {
        Self {
            counts: RwLock::new(HashMap::new()),
            limits: RwLock::new(HashMap::new()),
            defaults: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl QuotaRepository for InMemoryQuotaRepository {
    async fn get_count(&self, tenant_id: Uuid, resource: &str) -> Result<i32, IntentRebaseError> {
        let counts = self.counts.read().await;
        Ok(*counts.get(&(tenant_id, resource.to_string())).unwrap_or(&0))
    }

    async fn increment(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError> {
        let mut counts = self.counts.write().await;
        let key = (tenant_id, resource.to_string());
        let current = *counts.get(&key).unwrap_or(&0);
        counts.insert(key, current + 1);
        Ok(())
    }

    async fn decrement(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError> {
        let mut counts = self.counts.write().await;
        let key = (tenant_id, resource.to_string());
        if let Some(current) = counts.get_mut(&key) {
            *current = (*current - 1).max(0);
        }
        Ok(())
    }

    async fn set_count(&self, tenant_id: Uuid, resource: &str, count: i32) -> Result<(), IntentRebaseError> {
        let mut counts = self.counts.write().await;
        counts.insert((tenant_id, resource.to_string()), count.max(0));
        Ok(())
    }

    async fn get_limits(&self, tenant_id: Uuid) -> Result<Vec<QuotaLimit>, IntentRebaseError> {
        let limits = self.limits.read().await;
        let defaults = self.defaults.read().await;
        let mut result = Vec::new();

        // Add tenant-specific limits
        if let Some(tenant_limits) = limits.get(&tenant_id) {
            for (resource, limit) in tenant_limits {
                result.push(QuotaLimit {
                    resource: resource.clone(),
                    limit: *limit,
                    scope: None,
                });
            }
        }

        // Add global defaults for resources not overridden by tenant
        for (resource, limit) in defaults.iter() {
            if !result.iter().any(|l: &QuotaLimit| l.resource == *resource) {
                result.push(QuotaLimit {
                    resource: resource.clone(),
                    limit: *limit,
                    scope: None,
                });
            }
        }

        Ok(result)
    }

    async fn get_limit(&self, tenant_id: Uuid, resource: &str) -> Result<Option<i32>, IntentRebaseError> {
        // Check tenant-specific limit first
        {
            let limits = self.limits.read().await;
            if let Some(tenant_limits) = limits.get(&tenant_id) {
                if let Some(limit) = tenant_limits.get(resource) {
                    return Ok(Some(*limit));
                }
            }
        }
        // Fall back to global default
        let defaults = self.defaults.read().await;
        Ok(defaults.get(resource).copied())
    }
}

/// Quota service for checking and enforcing tenant quotas
pub struct QuotaService {
    repo: Arc<dyn QuotaRepository>,
    /// Default limits applied when no custom limit is set
    default_limits: HashMap<String, i32>,
}

impl QuotaService {
    pub fn new(repo: Arc<dyn QuotaRepository>) -> Self {
        let mut default_limits = HashMap::new();
        default_limits.insert("intents".to_string(), 10000);
        default_limits.insert("artifacts".to_string(), 100000);
        Self {
            repo,
            default_limits,
        }
    }

    /// Check if a tenant is within quota for a resource
    /// Returns Ok(()) if within limits, Err(QuotaExceeded) if over
    pub async fn check_quota(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError> {
        let current = self.repo.get_count(tenant_id, resource).await?;
        let limit = self.get_effective_limit(tenant_id, resource).await;

        if current >= limit {
            return Err(IntentRebaseError::QuotaExceeded {
                tenant_id,
                resource: resource.to_string(),
                current,
                limit,
            });
        }
        Ok(())
    }

    /// Enforce quota - checks and increments atomically
    /// Returns Ok(new_count) on success, Err(QuotaExceeded) if over limit
    pub async fn enforce(&self, tenant_id: Uuid, resource: &str) -> Result<i32, IntentRebaseError> {
        let current = self.repo.get_count(tenant_id, resource).await?;
        let limit = self.get_effective_limit(tenant_id, resource).await;

        if current >= limit {
            return Err(IntentRebaseError::QuotaExceeded {
                tenant_id,
                resource: resource.to_string(),
                current,
                limit,
            });
        }

        self.repo.increment(tenant_id, resource).await?;
        Ok(current + 1)
    }

    /// Release quota - decrements count
    pub async fn release(&self, tenant_id: Uuid, resource: &str) -> Result<(), IntentRebaseError> {
        self.repo.decrement(tenant_id, resource).await
    }

    /// Get effective limit for tenant+resource
    async fn get_effective_limit(&self, tenant_id: Uuid, resource: &str) -> i32 {
        if let Ok(Some(limit)) = self.repo.get_limit(tenant_id, resource).await {
            return limit;
        }
        self.default_limits.get(resource).copied().unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_enforcement_intents_under_limit() {
        let repo = Arc::new(InMemoryQuotaRepository::new());
        let tenant_id = Uuid::new_v4();
        repo.set_count(tenant_id, "intents", 5).await.unwrap();
        let service = QuotaService::new(repo.clone());

        let result = service.enforce(tenant_id, "intents").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 6);
    }

    #[tokio::test]
    async fn test_quota_enforcement_intents_at_limit() {
        let repo = Arc::new(InMemoryQuotaRepository::new());
        let tenant_id = Uuid::new_v4();
        repo.set_count(tenant_id, "intents", 10000).await.unwrap();
        let service = QuotaService::new(repo.clone());

        let result = service.enforce(tenant_id, "intents").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::QuotaExceeded { .. }));
    }

    #[tokio::test]
    async fn test_quota_release() {
        let repo = Arc::new(InMemoryQuotaRepository::new());
        let service = QuotaService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        repo.set_count(tenant_id, "intents", 5).await.unwrap();

        service.release(tenant_id, "intents").await.unwrap();
        let count = repo.get_count(tenant_id, "intents").await.unwrap();
        assert_eq!(count, 4);
    }

    #[tokio::test]
    async fn test_custom_tenant_limit() {
        let repo = Arc::new(InMemoryQuotaRepository::new());
        let tenant_id = Uuid::new_v4();
        // Set limit using async method
        repo.set_limit(tenant_id, "intents", 100).await;
        let service = QuotaService::new(repo.clone());

        repo.set_count(tenant_id, "intents", 100).await.unwrap();

        let result = service.enforce(tenant_id, "intents").await;
        assert!(result.is_err());
        if let Err(IntentRebaseError::QuotaExceeded { limit, .. }) = result {
            assert_eq!(limit, 100);
        }
    }

    #[tokio::test]
    async fn test_default_limits() {
        let repo = Arc::new(InMemoryQuotaRepository::new());
        let service = QuotaService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        // Default limit for intents is 10000

        repo.set_count(tenant_id, "intents", 9999).await.unwrap();
        let result = service.enforce(tenant_id, "intents").await;
        assert!(result.is_ok());

        repo.set_count(tenant_id, "intents", 10000).await.unwrap();
        let result = service.enforce(tenant_id, "intents").await;
        assert!(result.is_err());
    }
}