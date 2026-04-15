//! Bundle storage trait and implementations
//!
//! Provides the storage abstraction for forensic bundle persistence.
//! An in-memory implementation is provided for tests; an S3/MinIO
//! implementation is provided for production use.
//!
//! **This slice scope:** Bundle manifest and content storage via S3/MinIO seam.
//! **Out of scope:** Async job orchestration, bundle retrieval/download API,
//! hash chain integrity verification, replay.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Bundle storage error types
#[derive(Debug, thiserror::Error)]
pub enum BundleStorageError {
    #[error("bundle not found: {0}")]
    NotFound(Uuid),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Trait for forensic bundle object storage.
///
/// Implementations are responsible for storing and retrieving
/// the serialized bundle bytes from an object store (S3, MinIO, etc.).
#[async_trait]
pub trait BundleStorage: Send + Sync {
    /// Store a bundle's serialized bytes.
    async fn put(
        &self,
        bundle_id: Uuid,
        tenant_id: Uuid,
        data: &[u8],
    ) -> Result<(), BundleStorageError>;

    /// Retrieve a bundle's serialized bytes by ID.
    async fn get(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<Vec<u8>, BundleStorageError>;

    /// Check if a bundle exists in storage.
    async fn exists(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<bool, BundleStorageError>;

    /// Delete a bundle from storage.
    async fn delete(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<(), BundleStorageError>;

    /// Returns the storage location identifier (e.g., S3 bucket name).
    fn location(&self) -> &str;
}

// =============================================================================
// In-memory implementation (for tests)
// =============================================================================

/// In-memory bundle storage for unit/integration tests.
///
/// **Test-only scope:** This implementation stores bundles in memory
/// and is NOT suitable for production use where durability is required.
pub struct InMemoryBundleStorage {
    store: RwLock<HashMap<Uuid, Vec<u8>>>,
    /// Secondary index: tenant_id -> set of bundle_ids
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    location: String,
}

impl InMemoryBundleStorage {
    pub fn new(location: &str) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
            location: location.to_string(),
        }
    }
}

#[async_trait]
impl BundleStorage for InMemoryBundleStorage {
    async fn put(
        &self,
        bundle_id: Uuid,
        tenant_id: Uuid,
        data: &[u8],
    ) -> Result<(), BundleStorageError> {
        let mut store = self.store.write().await;
        let mut by_tenant = self.by_tenant.write().await;

        store.insert(bundle_id, data.to_vec());

        by_tenant
            .entry(tenant_id)
            .or_insert_with(Vec::new)
            .push(bundle_id);

        Ok(())
    }

    async fn get(&self, bundle_id: Uuid, _tenant_id: Uuid) -> Result<Vec<u8>, BundleStorageError> {
        let store = self.store.read().await;
        store
            .get(&bundle_id)
            .cloned()
            .ok_or(BundleStorageError::NotFound(bundle_id))
    }

    async fn exists(&self, bundle_id: Uuid, _tenant_id: Uuid) -> Result<bool, BundleStorageError> {
        let store = self.store.read().await;
        Ok(store.contains_key(&bundle_id))
    }

    async fn delete(&self, bundle_id: Uuid, _tenant_id: Uuid) -> Result<(), BundleStorageError> {
        let mut store = self.store.write().await;
        store.remove(&bundle_id);
        Ok(())
    }

    fn location(&self) -> &str {
        &self.location
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_in_memory_put_and_get() {
        let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
        let bundle_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let data = b"test bundle content";

        storage.put(bundle_id, tenant_id, data).await.unwrap();

        let retrieved = storage.get(bundle_id, tenant_id).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_in_memory_get_not_found() {
        let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
        let bundle_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let result = storage.get(bundle_id, tenant_id).await;
        assert!(matches!(result, Err(BundleStorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_in_memory_exists() {
        let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
        let bundle_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        assert!(!storage.exists(bundle_id, tenant_id).await.unwrap());

        storage.put(bundle_id, tenant_id, b"data").await.unwrap();

        assert!(storage.exists(bundle_id, tenant_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_delete() {
        let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
        let bundle_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        storage.put(bundle_id, tenant_id, b"data").await.unwrap();
        assert!(storage.exists(bundle_id, tenant_id).await.unwrap());

        storage.delete(bundle_id, tenant_id).await.unwrap();
        assert!(!storage.exists(bundle_id, tenant_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_location() {
        let storage = InMemoryBundleStorage::new("my-test-bucket");
        assert_eq!(storage.location(), "my-test-bucket");
    }
}
