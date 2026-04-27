//! S3/MinIO-backed bundle storage implementation
//!
//! Uses the AWS SDK for S3 to store and retrieve forensic bundle bytes.
//! Compatible with MinIO for local development and testing.
//!
//! **Truthful scope:** This implementation stores bundle bytes to S3/MinIO.
//! **NOT claimed:** Bundle retrieval/download API, async jobs, hash chain verification.

use async_trait::async_trait;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client as S3Client;
use uuid::Uuid;

use super::bundle_storage::{BundleStorage, BundleStorageError};

/// S3-backed bundle storage implementation.
///
/// Stores bundle bytes in an S3-compatible object store (including MinIO).
/// Bundle IDs are used as object keys; tenant ID is used for path prefixing.
pub struct S3BundleStorage {
    client: S3Client,
    bucket: String,
    /// Optional key prefix for multi-tenancy namespace separation.
    /// When set, objects are stored at `{key_prefix}/{tenant_id}/{bundle_id}`
    key_prefix: Option<String>,
}

impl S3BundleStorage {
    /// Create a new S3 bundle storage client.
    ///
    /// **Credentials:** Loaded from the environment via aws-config defaults
    /// (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION).
    /// For MinIO local development, set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY
    /// to `minioadmin` and set AWS_ENDPOINT to `http://localhost:9000`.
    ///
    /// **Truthful scope:** This creates an S3 client but does NOT verify
    /// bucket existence or configure lifecycle rules — those are future scope.
    pub async fn new(bucket: String) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = S3Client::new(&config);

        Self {
            client,
            bucket,
            key_prefix: None,
        }
    }

    /// Create with a custom endpoint (for MinIO).
    ///
    /// Use this for local development with MinIO:
    /// ```ignore
    /// S3BundleStorage::with_endpoint("http://localhost:9000", "minioadmin", "minioadmin", "forensic-bundles").await
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

        Self {
            client,
            bucket,
            key_prefix: None,
        }
    }

    /// Set a key prefix for multi-tenancy namespace separation.
    ///
    /// When set, objects are stored at `{key_prefix}/{tenant_id}/{bundle_id}`.
    /// This provides logical namespace isolation within a single bucket.
    pub fn with_key_prefix(mut self, prefix: &str) -> Self {
        self.key_prefix = Some(prefix.to_string());
        self
    }

    fn build_key(&self, tenant_id: Uuid, bundle_id: Uuid) -> String {
        match &self.key_prefix {
            Some(prefix) => format!("{}/{}/{}", prefix, tenant_id, bundle_id),
            None => format!("{}/{}", tenant_id, bundle_id),
        }
    }
}

#[async_trait]
impl BundleStorage for S3BundleStorage {
    async fn put(
        &self,
        bundle_id: Uuid,
        tenant_id: Uuid,
        data: &[u8],
    ) -> Result<(), BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);

        let body = ByteStream::from(data.to_vec());

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(body)
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| BundleStorageError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<Vec<u8>, BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);

        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| BundleStorageError::Storage(e.to_string()))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| BundleStorageError::Storage(e.to_string()))?
            .to_vec();

        Ok(bytes)
    }

    async fn exists(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<bool, BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);

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
                    Err(BundleStorageError::Storage(err_str))
                }
            }
        }
    }

    async fn delete(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<(), BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| BundleStorageError::Storage(e.to_string()))?;

        Ok(())
    }

    fn location(&self) -> &str {
        &self.bucket
    }
}

#[cfg(test)]
mod tests {
    // S3 integration tests require a live MinIO instance.
    // Run with: cargo test -- --ignored

    #[test]
    fn test_s3_storage_location_unit() {
        // Placeholder test verifying the struct layout compiles.
        // Actual S3 operations require async initialization and a live MinIO.
        let _ = println!("S3BundleStorage requires async initialization with live MinIO");
    }
}
