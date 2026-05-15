//! JWT authentication types for intent-api
//!
//! This module provides JWT authentication when the `jwt-auth` feature is enabled.
//!
//! For in-memory or testing setups, use [`crate::router::build_router_with_jwt_auth`].
//! For production deployments that need JWT together with SQL-backed audit/approval
//! repositories (and optional RLS), use
//! [`crate::router::build_router_with_sql_audit_and_approval_jwt`] instead.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::IntoResponse,
};
use jsonwebtoken::Algorithm;
use serde::{Deserialize, Serialize};

/// Minimum secret length for HS256 (256 bits = 32 bytes)
const MIN_SECRET_LENGTH: usize = 32;

/// Weak secrets that must not be used in production
const FORBIDDEN_SECRETS: &[&str] = &[
    "dev-secret-key-do-not-use-in-production",
    "secret",
    "password",
    "changeme",
    "development",
];

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,        // Subject (user/service ID)
    pub tenant_id: String,  // Tenant ID
    pub roles: Vec<String>, // User roles
    pub exp: usize,         // Expiration time
    pub iat: usize,         // Issued at
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// JWT secret key (HS256). In production, load from environment variable.
    pub jwt_secret: String,
    /// Algorithm used for JWT verification
    pub algorithm: Algorithm,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-key-do-not-use-in-production".to_string()),
            algorithm: Algorithm::HS256,
        }
    }
}

impl AuthConfig {
    /// Creates AuthConfig from environment variables (lenient mode).
    ///
    /// # Lenient Behavior
    ///
    /// This function is lenient — it does NOT enforce strict production checks:
    /// - Returns a dev fallback secret if `JWT_SECRET` is not set (with a warning)
    /// - Warns if secret is too short or matches weak patterns, but allows startup
    ///
    /// # Strict Enforcement
    ///
    /// For strict production enforcement, use `from_env_impl(true)` directly, or
    /// set `INTENT_API_REQUIRE_JWT=true` which triggers strict validation via this
    /// function in `main.rs` startup guard.
    ///
    /// # Errors
    ///
    /// Returns `AuthConfigError` only in strict mode (`from_env_impl(true)`) when:
    /// - `JWT_SECRET` environment variable is not set
    /// - Secret is shorter than 32 bytes
    /// - Secret matches a known weak/development secret
    pub fn from_env() -> Result<Self, AuthConfigError> {
        Self::from_env_impl(false)
    }

    /// Internal implementation with optional strict mode.
    pub(crate) fn from_env_impl(strict: bool) -> Result<Self, AuthConfigError> {
        let jwt_secret = match std::env::var("JWT_SECRET") {
            Ok(secret) => secret,
            Err(_) if !strict => {
                tracing::warn!("JWT_SECRET not set, using dev fallback (NOT for production use)");
                return Ok(Self {
                    jwt_secret: "dev-secret-key-do-not-use-in-production".to_string(),
                    algorithm: Algorithm::HS256,
                });
            }
            Err(_) => {
                return Err(AuthConfigError::MissingSecret(
                    "JWT_SECRET environment variable is not set".into(),
                ));
            }
        };

        // Check minimum length
        if jwt_secret.len() < MIN_SECRET_LENGTH {
            if strict {
                return Err(AuthConfigError::WeakSecret(format!(
                    "JWT_SECRET must be at least {} bytes for HS256, got {} bytes",
                    MIN_SECRET_LENGTH,
                    jwt_secret.len()
                )));
            } else {
                tracing::warn!(
                    "JWT_SECRET is shorter than recommended {} bytes (got {}), \
                    this is insecure for production",
                    MIN_SECRET_LENGTH,
                    jwt_secret.len()
                );
            }
        }

        // Check for forbidden/weak secrets
        let lower_secret = jwt_secret.to_lowercase();
        for forbidden in FORBIDDEN_SECRETS {
            if lower_secret.contains(forbidden) {
                if strict {
                    return Err(AuthConfigError::WeakSecret(format!(
                        "JWT_SECRET appears to be a weak/forbidden secret: '{}'",
                        forbidden
                    )));
                } else {
                    tracing::warn!(
                        "JWT_SECRET contains weak pattern '{}', this is insecure for production",
                        forbidden
                    );
                }
            }
        }

        if strict {
            tracing::info!("JWT production guard passed: JWT_SECRET is properly configured");
        }

        Ok(Self {
            jwt_secret,
            algorithm: Algorithm::HS256,
        })
    }

    /// Returns true if the current secret is considered secure for production.
    ///
    /// A secret is production-ready if it:
    /// - Is at least 32 bytes long
    /// - Does not contain weak/forbidden patterns
    pub fn is_production_ready(&self) -> bool {
        self.jwt_secret.len() >= MIN_SECRET_LENGTH
            && !FORBIDDEN_SECRETS
                .iter()
                .any(|f| self.jwt_secret.to_lowercase().contains(*f))
    }
}

/// Errors that can occur when loading AuthConfig in production mode
#[derive(Debug, Clone)]
pub enum AuthConfigError {
    /// JWT_SECRET environment variable is not set
    MissingSecret(String),
    /// Secret is too short or matches a known weak pattern
    WeakSecret(String),
}

impl std::fmt::Display for AuthConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthConfigError::MissingSecret(msg) => {
                write!(f, "JWT configuration error: {}", msg)
            }
            AuthConfigError::WeakSecret(msg) => {
                write!(f, "JWT configuration error: {}", msg)
            }
        }
    }
}

impl std::error::Error for AuthConfigError {}

// ============================================================================
// RLS Tenant Claims Extractor (Phase 3 P3-S5 Bounded Slice)
// ============================================================================

/// Re-export RLS helpers from intent-rebase-types for backward compatibility.
/// These were moved to intent-rebase-types to allow sharing across crates.
pub use intent_rebase_types::rls::{
    rls_reset_tenant_context_sql, rls_set_tenant_context_sql, validate_tenant_id_for_rls,
    RlsTenantContext,
};

/// Extension to store RlsTenantClaims in axum request extensions.
/// This is set by the JWT auth middleware after validating the token.
#[derive(Clone, Debug)]
pub struct RlsTenantClaims {
    /// The validated tenant ID from the JWT token (guaranteed non-nil)
    pub tenant_id: uuid::Uuid,
    /// The full claims from the JWT token
    pub claims: Claims,
}

impl RlsTenantClaims {
    /// Creates a new RlsTenantClaims from a Claims struct.
    ///
    /// The tenant_id is parsed from claims.tenant_id and validated for RLS use.
    /// Returns error if the tenant_id is not a valid UUID or is the nil UUID.
    pub fn new(claims: Claims) -> Result<Self, String> {
        let tenant_uuid = uuid::Uuid::parse_str(&claims.tenant_id)
            .map_err(|e| format!("tenant_id in JWT is not a valid UUID: {}", e))?;
        validate_tenant_id_for_rls(tenant_uuid)?;
        Ok(Self {
            tenant_id: tenant_uuid,
            claims,
        })
    }

    /// Creates a new RlsTenantClaims without validation (for testing or internal use).
    #[cfg(test)]
    pub fn new_unchecked(tenant_id: uuid::Uuid, claims: Claims) -> Self {
        Self { tenant_id, claims }
    }
}

/// Error type for RlsTenantClaims extraction failures.
#[derive(Debug)]
pub struct RlsTenantClaimsExtractionError(pub String);

impl std::fmt::Display for RlsTenantClaimsExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RlsTenantClaims extraction failed: {}", self.0)
    }
}

impl std::error::Error for RlsTenantClaimsExtractionError {}

impl IntoResponse for RlsTenantClaimsExtractionError {
    fn into_response(self) -> axum::response::Response {
        let body = crate::ApiError {
            error: crate::ErrorDetails {
                code: "UNAUTHORIZED".to_string(),
                message: self.0,
                retryable: false,
                details: None,
            },
        };
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

/// Error type for tenant mismatch detection.
#[derive(Debug)]
pub struct TenantMismatchError {
    pub jwt_tenant_id: uuid::Uuid,
    pub request_tenant_id: uuid::Uuid,
}

impl std::fmt::Display for TenantMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
            self.jwt_tenant_id, self.request_tenant_id
        )
    }
}

impl std::error::Error for TenantMismatchError {}

impl IntoResponse for TenantMismatchError {
    fn into_response(self) -> axum::response::Response {
        let body = crate::ApiError {
            error: crate::ErrorDetails {
                code: "TENANT_MISMATCH".to_string(),
                message: self.to_string(),
                retryable: false,
                details: None,
            },
        };
        (StatusCode::FORBIDDEN, axum::Json(body)).into_response()
    }
}

/// Extracts RlsTenantClaims from request extensions.
///
/// This is used in handlers to get the validated JWT tenant claims.
/// The claims must have been inserted by the jwt_auth_async middleware.
#[async_trait::async_trait]
impl<S> FromRequestParts<S> for RlsTenantClaims
where
    S: Clone + Send + Sync,
{
    type Rejection = RlsTenantClaimsExtractionError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims>()
            .ok_or(RlsTenantClaimsExtractionError(
                "JWT claims not found in request extensions. Ensure JWT auth middleware is applied."
                    .into(),
            ))
            .and_then(|claims| {
                RlsTenantClaims::new(claims.clone()).map_err(RlsTenantClaimsExtractionError)
            })
    }
}

/// Optional RLS tenant claims extractor.
///
/// Returns `Some(RlsTenantClaims)` when valid JWT claims are present in extensions.
/// Returns `None` when no JWT claims are found (no auth, invalid token, etc.).
///
/// This allows handlers to gracefully fall back to non-RLS paths when JWT auth
/// is not present, rather than returning 401/403.
#[derive(Clone, Debug)]
pub struct OptionalRlsTenantClaims(pub Option<RlsTenantClaims>);

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for OptionalRlsTenantClaims
where
    S: Clone + Send + Sync,
{
    type Rejection = RlsTenantClaimsExtractionError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let claims = parts.extensions.get::<Claims>();
        match claims {
            Some(claims) => {
                match RlsTenantClaims::new(claims.clone()) {
                    Ok(rls_claims) => Ok(OptionalRlsTenantClaims(Some(rls_claims))),
                    Err(_) => Ok(OptionalRlsTenantClaims(None)), // Invalid tenant_id in claims, but treat as no JWT
                }
            }
            None => Ok(OptionalRlsTenantClaims(None)),
        }
    }
}

/// JWT token generation utility (for testing and dev)
pub fn generate_test_token(secret: &str, sub: &str, tenant_id: &str, roles: &[&str]) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: sub.to_string(),
        tenant_id: tenant_id.to_string(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
        exp: now + 3600, // 1 hour
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}
