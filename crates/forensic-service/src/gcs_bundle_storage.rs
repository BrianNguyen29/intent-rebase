//! GCS-backed bundle storage implementation using metadata-server OAuth.
//!
//! Uses the GCS JSON API (not S3 interoperability) to store and retrieve
//! forensic bundle bytes. Authentication is via the GKE metadata-server
//! token endpoint — no HMAC keys or long-lived secrets are required.
//!
//! **Scope:**
//! - Stores bundle bytes in a dedicated GCS bucket.
//! - Uses `reqwest` for HTTP calls to the GCS JSON API.
//! - Token caching with TTL refresh.
//! - No S3 interoperability / HMAC keys.
//!
//! **NOT claimed:**
//! - Object Lock / Bucket Lock enforcement (retention policy is UNLOCKED).
//! - Lifecycle rules, tiering, or automatic expiry.
//! - Multi-region replication or cross-bucket sync.

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::bundle_storage::{BundleStorage, BundleStorageError};

/// Metadata server endpoint for OAuth access token.
const METADATA_TOKEN_URL: &str =
    "http://169.254.169.254/computeMetadata/v1/instance/service-accounts/default/token";

/// GCS JSON API base.
const GCS_API_BASE: &str = "https://storage.googleapis.com/storage/v1";

/// Cached access token with expiry.
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    /// Instant after which the token should be considered expired.
    /// We refresh with a 60-second safety margin before true expiry.
    expires_at: Instant,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        // Refresh 60 seconds before actual expiry to avoid edge races.
        Instant::now() > self.expires_at - Duration::from_secs(60)
    }
}

/// GCS-backed bundle storage using metadata-server OAuth.
///
/// Objects are stored at `tenants/{tenant_id}/bundles/{bundle_id}`.
/// The bucket must already exist and the GKE service account must have
/// `roles/storage.objectAdmin` (or more restrictive) on the bucket.
#[derive(Debug)]
pub struct GcsBundleStorage {
    bucket: String,
    client: reqwest::Client,
    token_cache: Arc<RwLock<Option<CachedToken>>>,
}

impl GcsBundleStorage {
    /// Create a new GCS bundle storage client.
    ///
    /// The bucket must exist. The running GKE workload must have access to
    /// the metadata server (169.254.169.254) to fetch an OAuth token.
    pub fn new(bucket: String) -> Self {
        Self {
            bucket,
            client: reqwest::Client::new(),
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Fetch or return a cached access token from the GKE metadata server.
    async fn get_token(&self) -> Result<String, BundleStorageError> {
        // Fast path: check cached token
        {
            let cache = self.token_cache.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_expired() {
                    return Ok(cached.token.clone());
                }
            }
        }

        // Slow path: fetch from metadata server
        let mut headers = HeaderMap::new();
        headers.insert("Metadata-Flavor", HeaderValue::from_static("Google"));

        let response = self
            .client
            .get(METADATA_TOKEN_URL)
            .headers(headers)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                BundleStorageError::Storage(format!("metadata token request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            return Err(BundleStorageError::Storage(format!(
                "metadata token request failed: HTTP {} — {}",
                status, body
            )));
        }

        let body: serde_json::Value = response.json().await.map_err(|e| {
            BundleStorageError::Storage(format!("metadata token JSON parse failed: {}", e))
        })?;

        let token = body["access_token"]
            .as_str()
            .ok_or_else(|| {
                BundleStorageError::Storage(
                    "metadata token response missing 'access_token' field".to_string(),
                )
            })?
            .to_string();

        let expires_in = body["expires_in"].as_u64().unwrap_or(3600);

        let cached = CachedToken {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(expires_in),
        };

        let mut cache = self.token_cache.write().await;
        *cache = Some(cached);

        Ok(token)
    }

    /// Build the GCS object name for a bundle.
    fn build_key(&self, tenant_id: Uuid, bundle_id: Uuid) -> String {
        format!("tenants/{}/bundles/{}", tenant_id, bundle_id)
    }

    /// Build the GCS JSON API media upload URL for POST/PUT of object bytes.
    fn media_url(&self, key: &str) -> String {
        // Media uploads use the dedicated /upload endpoint, not the regular API base.
        // https://cloud.google.com/storage/docs/json_api/v1/how-tos/upload
        let encoded = urlencoding::encode(key);
        format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.bucket, encoded
        )
    }

    /// Build the GCS JSON API object metadata URL (no `alt=media`).
    fn metadata_url(&self, key: &str) -> String {
        let encoded = urlencoding::encode(key);
        format!("{}/b/{}/o/{}", GCS_API_BASE, self.bucket, encoded)
    }

    /// Build the GCS JSON API object URL for GET/DELETE.
    fn object_url(&self, key: &str) -> String {
        let encoded = urlencoding::encode(key);
        format!("{}/b/{}/o/{}", GCS_API_BASE, self.bucket, encoded)
    }

    /// Build request headers with authorization.
    async fn auth_headers(&self) -> Result<HeaderMap, BundleStorageError> {
        let token = self.get_token().await?;
        let mut headers = HeaderMap::new();
        let auth_value = HeaderValue::from_str(&format!("Bearer {}", token))
            .map_err(|e| BundleStorageError::Storage(format!("invalid auth header: {}", e)))?;
        headers.insert(AUTHORIZATION, auth_value);
        Ok(headers)
    }

    /// Execute a GCS request with bounded retry for transient errors.
    ///
    /// Retries up to 3 times with exponential backoff (100ms, 200ms, 400ms)
    /// for HTTP 429 (rate limit) and 5xx (server errors). Also retries on
    /// metadata-token 401/403 to handle near-expiry token races.
    ///
    /// On each attempt (including retries), auth headers are fetched freshly so
    /// that a 401/403 token-refresh path automatically uses the new token.
    async fn gcs_request_with_retry(
        &self,
        method: reqwest::Method,
        url: &str,
        extra_headers: Option<HeaderMap>,
        body: Option<Vec<u8>>,
        timeout_secs: u64,
    ) -> Result<reqwest::Response, BundleStorageError> {
        let mut backoff = Duration::from_millis(100);
        const MAX_RETRIES: usize = 3;

        for attempt in 0..=MAX_RETRIES {
            let mut headers = self.auth_headers().await?;
            if let Some(ref extra) = extra_headers {
                for (k, v) in extra.iter() {
                    headers.insert(k, v.clone());
                }
            }

            let mut request = self
                .client
                .request(method.clone(), url)
                .headers(headers)
                .timeout(Duration::from_secs(timeout_secs));

            if let Some(ref data) = body {
                request = request.body(data.clone());
            }

            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt < MAX_RETRIES && e.is_timeout() {
                        tracing::warn!(
                            "GCS request timeout (attempt {}/{}), retrying in {:?}",
                            attempt + 1,
                            MAX_RETRIES,
                            backoff
                        );
                        tokio::time::sleep(backoff).await;
                        backoff *= 2;
                        continue;
                    }
                    return Err(BundleStorageError::Storage(format!(
                        "GCS request failed: {}",
                        e
                    )));
                }
            };

            let status = response.status();
            if status.is_success() || status.as_u16() == 404 {
                return Ok(response);
            }

            // Retry on transient errors: 429, 5xx, 401/403 (token expiry race)
            let is_retryable = status.as_u16() == 429
                || status.is_server_error()
                || (status.as_u16() == 401 || status.as_u16() == 403);

            if is_retryable && attempt < MAX_RETRIES {
                tracing::warn!(
                    "GCS returned HTTP {} (attempt {}/{}), retrying in {:?}",
                    status,
                    attempt + 1,
                    MAX_RETRIES,
                    backoff
                );
                // On 401/403, force token refresh so the next loop iteration
                // fetches a fresh token via auth_headers().
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    let mut cache = self.token_cache.write().await;
                    *cache = None;
                }
                tokio::time::sleep(backoff).await;
                backoff *= 2;
                continue;
            }

            return Ok(response);
        }

        // Unreachable because loop returns inside, but compiler needs it
        Err(BundleStorageError::Storage(
            "GCS request exceeded max retries".to_string(),
        ))
    }
}

#[async_trait]
impl BundleStorage for GcsBundleStorage {
    async fn put(
        &self,
        bundle_id: Uuid,
        tenant_id: Uuid,
        data: &[u8],
    ) -> Result<(), BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);
        let url = self.media_url(&key);
        let mut extra = HeaderMap::new();
        extra.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let response = self
            .gcs_request_with_retry(
                reqwest::Method::POST,
                &url,
                Some(extra),
                Some(data.to_vec()),
                30,
            )
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            Err(BundleStorageError::Storage(format!(
                "GCS upload failed: HTTP {} — {}",
                status, body
            )))
        }
    }

    async fn get(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<Vec<u8>, BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);
        let url = format!("{}?alt=media", self.object_url(&key));

        let response = self
            .gcs_request_with_retry(reqwest::Method::GET, &url, None, None, 30)
            .await?;

        if response.status().is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| {
                    BundleStorageError::Storage(format!("GCS download body read failed: {}", e))
                })?
                .to_vec();
            Ok(bytes)
        } else if response.status().as_u16() == 404 {
            Err(BundleStorageError::NotFound(bundle_id))
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            Err(BundleStorageError::Storage(format!(
                "GCS download failed: HTTP {} — {}",
                status, body
            )))
        }
    }

    async fn exists(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<bool, BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);
        // Use metadata URL (no alt=media) to avoid downloading object body.
        let url = self.metadata_url(&key);

        let response = self
            .gcs_request_with_retry(reqwest::Method::HEAD, &url, None, None, 10)
            .await?;

        if response.status().is_success() {
            Ok(true)
        } else if response.status().as_u16() == 404 {
            Ok(false)
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            Err(BundleStorageError::Storage(format!(
                "GCS exists check failed: HTTP {} — {}",
                status, body
            )))
        }
    }

    async fn delete(&self, bundle_id: Uuid, tenant_id: Uuid) -> Result<(), BundleStorageError> {
        let key = self.build_key(tenant_id, bundle_id);
        let url = self.object_url(&key);

        let response = self
            .gcs_request_with_retry(reqwest::Method::DELETE, &url, None, None, 30)
            .await?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            // 404 is acceptable for idempotent delete
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".to_string());
            Err(BundleStorageError::Storage(format!(
                "GCS delete failed: HTTP {} — {}",
                status, body
            )))
        }
    }

    fn location(&self) -> &str {
        &self.bucket
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcs_build_key() {
        let storage = GcsBundleStorage::new("my-bucket".to_string());
        let tenant_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let bundle_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let key = storage.build_key(tenant_id, bundle_id);
        assert_eq!(
            key,
            "tenants/550e8400-e29b-41d4-a716-446655440000/bundles/6ba7b810-9dad-11d1-80b4-00c04fd430c8"
        );
    }

    #[test]
    fn test_gcs_media_url_encoding() {
        let storage = GcsBundleStorage::new("test-bucket".to_string());
        let url = storage.media_url("tenants/550e8400-e29b-41d4-a716-446655440000/bundles/6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        // URL-encoded path separators
        assert!(url.contains("%2F"));
        assert!(url.starts_with("https://storage.googleapis.com/upload/storage/v1/b/test-bucket/o"));
        assert!(url.contains("uploadType=media"));
    }

    #[test]
    fn test_gcs_metadata_url_encoding() {
        let storage = GcsBundleStorage::new("test-bucket".to_string());
        let url = storage.metadata_url("tenants/550e8400-e29b-41d4-a716-446655440000/bundles/6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert!(url.contains("%2F"));
        assert!(url.starts_with("https://storage.googleapis.com/storage/v1/b/test-bucket/o"));
        // No alt=media query parameter
        assert!(!url.contains("alt=media"));
    }

    #[test]
    fn test_gcs_object_url_encoding() {
        let storage = GcsBundleStorage::new("test-bucket".to_string());
        let url = storage.object_url("tenants/550e8400-e29b-41d4-a716-446655440000/bundles/6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        assert!(url.contains("%2F"));
        assert!(url.starts_with("https://storage.googleapis.com/storage/v1/b/test-bucket/o"));
    }

    #[test]
    fn test_gcs_location() {
        let storage = GcsBundleStorage::new("forensic-evidence-ferrum-497801-ed2c5bdd".to_string());
        assert_eq!(
            storage.location(),
            "forensic-evidence-ferrum-497801-ed2c5bdd"
        );
    }

    #[test]
    fn test_cached_token_expiry() {
        let token = CachedToken {
            token: "test-token".to_string(),
            expires_at: Instant::now() + Duration::from_secs(120),
        };
        assert!(!token.is_expired());

        let expired = CachedToken {
            token: "test-token".to_string(),
            expires_at: Instant::now() + Duration::from_secs(30),
        };
        // 60-second safety margin means 30s remaining = expired
        assert!(expired.is_expired());
    }
}
