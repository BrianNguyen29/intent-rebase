//! S3/MinIO-backed policy snapshot blob storage implementation
//!
//! Uses the AWS SDK for S3 to store and retrieve policy snapshot JSON blobs.
//! Compatible with MinIO for local development and testing.
//!
//! **Truthful scope:** This implementation provides a write-only S3 storage seam
//! for policy snapshot blobs with memory fallback. Object Lock enforcement,
//! chain-hash verification, and 100-year retention are NOT claimed.
//!
//! **Key structure:** `{tenant_id}/{intent_id}/v{intent_version}/snapshot.json`
//! **URI structure:** `s3://ire-policy-snapshots/{tenant_id}/{intent_id}/v{intent_version}/snapshot.json`

use async_trait::async_trait;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use intent_rebase_types::PolicySnapshot;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Snapshot storage error types
#[derive(Debug, thiserror::Error)]
pub enum SnapshotStorageError {
    #[error("snapshot not found: intent_id={0}, version={1}")]
    NotFound(Uuid, i32),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("integrity error: {0}")]
    Integrity(String),
}

/// Trait for policy snapshot blob storage.
///
/// Implementations are responsible for storing and retrieving
/// serialized policy snapshot JSON blobs to an object store (S3, MinIO, etc.).
#[async_trait]
pub trait SnapshotStorage: Send + Sync {
    /// Store a policy snapshot blob to S3.
    /// Returns the S3 URI of the stored object.
    async fn put(
        &self,
        snapshot: &PolicySnapshot,
        blob_bytes: &[u8],
    ) -> Result<String, SnapshotStorageError>;

    /// Check if a policy snapshot blob exists in storage.
    async fn exists(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<bool, SnapshotStorageError>;

    /// Returns the bucket name.
    fn bucket(&self) -> &str;
}

// =============================================================================
// In-memory implementation (for tests and memory:// fallback)
// =============================================================================

/// In-memory snapshot storage for unit/integration tests.
///
/// **Test-only scope:** This implementation stores snapshots in memory
/// and is NOT suitable for production use where durability is required.
/// This exists to satisfy the `memory://` URI fallback requirement.
pub struct InMemorySnapshotStorage {
    store: RwLock<HashMap<String, Vec<u8>>>,
    location: String,
}

impl InMemorySnapshotStorage {
    /// Create a new in-memory snapshot storage.
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            location: "memory://policy-snapshots".to_string(),
        }
    }

    fn build_key(tenant_id: Uuid, intent_id: Uuid, intent_version: i32) -> String {
        format!(
            "{}/{}/v{}/snapshot.json",
            tenant_id.to_string().replace("-", ""),
            intent_id.to_string().replace("-", ""),
            intent_version
        )
    }
}

impl Default for InMemorySnapshotStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SnapshotStorage for InMemorySnapshotStorage {
    async fn put(
        &self,
        snapshot: &PolicySnapshot,
        blob_bytes: &[u8],
    ) -> Result<String, SnapshotStorageError> {
        let key = Self::build_key(
            snapshot.tenant_id,
            snapshot.intent_id,
            snapshot.intent_version,
        );
        let mut store = self.store.write().await;
        store.insert(key.clone(), blob_bytes.to_vec());
        Ok(format!(
            "memory://policy-snapshots/{}/{}/v{}",
            snapshot.tenant_id, snapshot.intent_id, snapshot.intent_version
        ))
    }

    async fn exists(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<bool, SnapshotStorageError> {
        let key = Self::build_key(tenant_id, intent_id, intent_version);
        let store = self.store.read().await;
        Ok(store.contains_key(&key))
    }

    fn bucket(&self) -> &str {
        &self.location
    }
}

// =============================================================================
// S3/MinIO implementation (for production)
// =============================================================================

/// S3-backed snapshot storage implementation.
///
/// Stores policy snapshot JSON blobs in an S3-compatible object store (including MinIO).
/// Object keys follow the pattern: `{tenant_id}/{intent_id}/v{intent_version}/snapshot.json`
pub struct S3SnapshotStorage {
    client: S3Client,
    bucket: String,
}

impl S3SnapshotStorage {
    /// Create a new S3 snapshot storage client.
    ///
    /// **Credentials:** Loaded from the environment via aws-config defaults
    /// (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION).
    /// For MinIO local development, set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY
    /// to `minioadmin` and set AWS_ENDPOINT to `http://localhost:9000`.
    pub async fn new(bucket: String) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = S3Client::new(&config);

        Self { client, bucket }
    }

    /// Create with a custom endpoint (for MinIO).
    ///
    /// Use this for local development with MinIO:
    /// ```ignore
    /// S3SnapshotStorage::with_endpoint("http://localhost:9000", "minioadmin", "minioadmin", "ire-policy-snapshots").await
    /// ```
    #[allow(dead_code)]
    pub async fn with_endpoint(
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: String,
    ) -> Self {
        let creds = Credentials::new(access_key, secret_key, None, None, "explicit");
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .credentials_provider(creds)
            .region(aws_config::Region::new("us-east-1"))
            .endpoint_url(endpoint)
            .load()
            .await;

        let client = S3Client::new(&config);

        Self { client, bucket }
    }

    /// Build the S3 object key for a policy snapshot.
    ///
    /// Key format: `{tenant_id}/{intent_id}/v{intent_version}/snapshot.json`
    /// All UUIDs are lowercase without dashes.
    pub fn build_key(tenant_id: Uuid, intent_id: Uuid, intent_version: i32) -> String {
        format!(
            "{}/{}/v{}/snapshot.json",
            tenant_id.to_string().replace("-", ""),
            intent_id.to_string().replace("-", ""),
            intent_version
        )
    }

    /// Build the S3 URI for a policy snapshot.
    ///
    /// URI format: `s3://ire-policy-snapshots/{tenant_id}/{intent_id}/v{intent_version}/snapshot.json`
    pub fn build_uri(&self, tenant_id: Uuid, intent_id: Uuid, intent_version: i32) -> String {
        format!(
            "s3://{}/{}/{}/v{}/snapshot.json",
            self.bucket,
            tenant_id.to_string().replace("-", ""),
            intent_id.to_string().replace("-", ""),
            intent_version
        )
    }

    /// Compute SHA256 checksum of blob bytes.
    ///
    /// Uses `sha2::Sha256` to compute the checksum, matching the aws-sdk-s3
    /// `checksum_sha256` field behavior.
    pub fn compute_checksum(blob_bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(blob_bytes);
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

#[async_trait]
impl SnapshotStorage for S3SnapshotStorage {
    async fn put(
        &self,
        snapshot: &PolicySnapshot,
        blob_bytes: &[u8],
    ) -> Result<String, SnapshotStorageError> {
        let key = Self::build_key(
            snapshot.tenant_id,
            snapshot.intent_id,
            snapshot.intent_version,
        );

        // Compute SHA256 checksum for integrity
        let checksum = Self::compute_checksum(blob_bytes);

        let body = ByteStream::from(blob_bytes.to_vec());

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .content_type("application/json")
            .checksum_sha256(&checksum)
            .send()
            .await
            .map_err(|e| SnapshotStorageError::Storage(e.to_string()))?;

        Ok(self.build_uri(
            snapshot.tenant_id,
            snapshot.intent_id,
            snapshot.intent_version,
        ))
    }

    async fn exists(
        &self,
        intent_id: Uuid,
        intent_version: i32,
        tenant_id: Uuid,
    ) -> Result<bool, SnapshotStorageError> {
        let key = Self::build_key(tenant_id, intent_id, intent_version);

        let result = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                // Check if it's a "not found" error
                if err_str.contains("NoSuchKey")
                    || err_str.contains("NotFound")
                    || err_str.contains("404")
                {
                    Ok(false)
                } else {
                    Err(SnapshotStorageError::Storage(err_str))
                }
            }
        }
    }

    fn bucket(&self) -> &str {
        &self.bucket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_snapshot() -> PolicySnapshot {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let scope = intent_rebase_types::ScopeDefinition::default();

        PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope)
    }

    #[test]
    fn test_build_key_format() {
        let tenant_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let intent_id = Uuid::parse_str("9f4b2e5a-8c3d-4b1e-9a2c-8d7f6e5b4a3d").unwrap();
        let intent_version = 3;

        let key = S3SnapshotStorage::build_key(tenant_id, intent_id, intent_version);

        // UUIDs without dashes
        assert_eq!(
            key,
            "550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3d/v3/snapshot.json"
        );
    }

    #[test]
    fn test_compute_checksum() {
        let data = b"test snapshot content";
        let checksum = S3SnapshotStorage::compute_checksum(data);

        // SHA256 produces 64 hex characters
        assert_eq!(checksum.len(), 64);
        // Verify it's deterministic
        assert_eq!(checksum, S3SnapshotStorage::compute_checksum(data));
    }

    #[test]
    fn test_in_memory_storage_location() {
        let storage = InMemorySnapshotStorage::new();
        assert_eq!(storage.bucket(), "memory://policy-snapshots");
    }

    #[tokio::test]
    async fn test_in_memory_put_and_exists() {
        let storage = Arc::new(InMemorySnapshotStorage::new());
        let snapshot = create_test_snapshot();
        let blob = br#"{"test": "snapshot"}"#;

        let uri = storage.put(&snapshot, blob).await.unwrap();
        assert!(uri.starts_with("memory://policy-snapshots/"));

        let exists = storage
            .exists(
                snapshot.intent_id,
                snapshot.intent_version,
                snapshot.tenant_id,
            )
            .await
            .unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn test_in_memory_exists_not_found() {
        let storage = Arc::new(InMemorySnapshotStorage::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let exists = storage.exists(intent_id, 999, tenant_id).await.unwrap();
        assert!(!exists);
    }

    #[test]
    fn test_s3_key_determinism() {
        // Same inputs must produce same key
        let tenant_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let intent_id = Uuid::parse_str("9f4b2e5a-8c3d-4b1e-9a2c-8d7f6e5b4a3d").unwrap();

        let key1 = S3SnapshotStorage::build_key(tenant_id, intent_id, 1);
        let key2 = S3SnapshotStorage::build_key(tenant_id, intent_id, 1);
        assert_eq!(key1, key2);

        // Different versions produce different keys
        let key_v1 = S3SnapshotStorage::build_key(tenant_id, intent_id, 1);
        let key_v2 = S3SnapshotStorage::build_key(tenant_id, intent_id, 2);
        assert_ne!(key_v1, key_v2);
    }
}
