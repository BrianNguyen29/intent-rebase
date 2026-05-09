// ============================================================================
// API Key Authentication Scaffold (Phase 1)
// ============================================================================
// Extracted from lib.rs (Phase 1 bounded decomposition slice)

use axum::{
    body::Body, extract::FromRequestParts, http::StatusCode, middleware::Next,
    response::IntoResponse,
};

/// API key extracted from X-API-Key header.
/// Phase 1: This is stored in request extensions but NOT validated.
/// Phase 2: Will integrate with actual API key validation and tenant resolution.
#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

/// Extension key for storing API key in request extensions.
#[derive(Clone, Copy)]
pub struct ApiKeyExtensionKey;

impl std::fmt::Display for ApiKeyExtensionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKeyExtensionKey")
    }
}

/// Rejection type for API key extraction — implements IntoResponse for axum compatibility.
#[derive(Debug)]
pub struct ApiKeyRejection(pub String);

impl IntoResponse for ApiKeyRejection {
    fn into_response(self) -> axum::response::Response {
        let body = crate::ApiError {
            error: crate::ErrorDetails {
                code: "INVALID_API_KEY".to_string(),
                message: self.0,
                retryable: false,
                details: None,
            },
        };
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for ApiKey
where
    S: Clone + Send + Sync,
{
    type Rejection = ApiKeyRejection;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Phase 1: Look for X-API-Key header, return empty if not present
        // Phase 2: This will become mandatory and validated against a key store
        match parts.headers.get("x-api-key") {
            Some(value) => {
                let key = value
                    .to_str()
                    .map_err(|_| ApiKeyRejection("X-API-Key header is not valid UTF-8".into()))?;
                Ok(ApiKey(key.to_string()))
            }
            None => {
                // Phase 1: Return empty API key (no blocking)
                // The middleware logs this for observability
                Ok(ApiKey(String::new()))
            }
        }
    }
}

/// Middleware that extracts X-API-Key header and stores it in request extensions.
/// Phase 1: This middleware logs the presence/absence of API keys but does NOT block requests.
/// Phase 2: Will validate API keys and enforce authentication.
pub async fn api_key_extractor_middleware(
    request: axum::http::Request<Body>,
    next: Next,
) -> axum::response::Response {
    let api_key = request
        .headers()
        .get("x-api-key")
        .map(|v| v.to_str().unwrap_or("<invalid utf8>"));

    match api_key {
        Some(key) if !key.is_empty() => {
            tracing::debug!("API key present: {}...", &key[..key.len().min(8)]);
        }
        _ => {
            tracing::debug!("No API key present in request (Phase 1 - allowed)");
        }
    }

    // Phase 1: Pass through without blocking
    // Phase 2: Add actual validation here
    next.run(request).await
}
