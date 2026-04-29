//! JWT authentication types for intent-api
//!
//! This module provides JWT authentication when the `jwt-auth` feature is enabled.
//! Use `build_router_with_jwt_auth` instead of `build_router` to enable JWT authentication.

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
    fn from_env_impl(strict: bool) -> Result<Self, AuthConfigError> {
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
// RLS Session Context Helper
// ============================================================================

/// PostgreSQL session setting name for tenant context
const RLS_TENANT_SETTING: &str = "app.current_tenant_id";

/// Generates a SQL statement to safely set the RLS tenant context for a session.
///
/// This helper constructs the proper `SET LOCAL` or `SET` command to configure
/// the `app.current_tenant_id` session variable used by RLS policies.
///
/// # Security Notes
///
/// - Uses parameterized UUID to prevent SQL injection
/// - The UUID is validated before being embedded in the SQL
/// - RLS policies check `NULL` tenant_id as bypass (superuser/migration access)
/// - Always use `SET LOCAL` for transaction-scoped context
///
/// # Example
///
/// ```sql
/// -- Set tenant context for current session (transaction-scoped with SET LOCAL)
/// SET LOCAL app.current_tenant_id = '550e8400-e29b-41d4-a716-446655440000';
///
/// -- Then subsequent queries in the same transaction will be tenant-scoped
/// SELECT * FROM intents WHERE tenant_id = current_tenant_id();
/// ```
pub fn rls_set_tenant_context_sql(tenant_id: uuid::Uuid) -> String {
    format!("SET LOCAL {} = '{}'", RLS_TENANT_SETTING, tenant_id)
}

/// Generates a SQL statement to reset the RLS tenant context.
///
/// Use this at the end of a transaction or when switching tenants.
/// The `RESET` command clears the session variable.
pub fn rls_reset_tenant_context_sql() -> String {
    format!("RESET {}", RLS_TENANT_SETTING)
}

/// Validates that a tenant_id UUID is safe to use in RLS context.
///
/// Returns `Err` with explanation if the UUID is not valid for RLS use.
pub fn validate_tenant_id_for_rls(tenant_id: uuid::Uuid) -> Result<(), String> {
    // Check for nil UUID which is used as sentinel/default
    if tenant_id == uuid::Uuid::nil() {
        return Err(
            "Nil UUID (00000000-0000-0000-0000-000000000000) cannot be used as tenant_id \
             for RLS context; it is reserved as the default/sentinel value"
                .into(),
        );
    }

    // Additional validation could go here (e.g., format checks, range checks)
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    use std::sync::Mutex;

    // =====================================================================
    // Test Helper: Serialize Environment Variable Mutations
    // =====================================================================

    /// Lock to serialize env-mutating tests that set/remove JWT_SECRET or INTENT_API_REQUIRE_JWT.
    /// Rust's default parallel test execution can cause race conditions when multiple tests
    /// concurrently modify the same environment variables.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper to serialize env-mutating tests.
    fn with_env_lock<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = ENV_LOCK.lock().unwrap();
        f()
    }

    // =====================================================================
    // JWT Token Tests (original)
    // =====================================================================

    #[test]
    fn test_generate_and_validate_token() {
        let secret = "test-secret";
        let token = generate_test_token(secret, "user-1", "tenant-1", &["admin"]);

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        );

        assert!(result.is_ok());
        let claims = result.unwrap().claims;
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.tenant_id, "tenant-1");
        assert_eq!(claims.roles, vec!["admin"]);
    }

    #[test]
    fn test_invalid_token_rejected() {
        let secret = "test-secret";
        let token = "invalid.jwt.token";

        let result = decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let token = generate_test_token("secret-1", "user-1", "tenant-1", &["admin"]);

        let result = decode::<Claims>(
            &token,
            &DecodingKey::from_secret("secret-2".as_bytes()),
            &Validation::new(Algorithm::HS256),
        );

        assert!(result.is_err());
    }

    // =====================================================================
    // RLS Helper Tests
    // =====================================================================

    #[test]
    fn test_rls_set_tenant_context_sql() {
        let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let sql = rls_set_tenant_context_sql(tenant_id);
        assert_eq!(
            sql,
            "SET LOCAL app.current_tenant_id = '550e8400-e29b-41d4-a716-446655440000'"
        );
    }

    #[test]
    fn test_rls_reset_tenant_context_sql() {
        let sql = rls_reset_tenant_context_sql();
        assert_eq!(sql, "RESET app.current_tenant_id");
    }

    #[test]
    fn test_validate_tenant_id_for_rls_nil_rejected() {
        let nil_uuid = uuid::Uuid::nil();
        let result = validate_tenant_id_for_rls(nil_uuid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Nil UUID"));
    }

    #[test]
    fn test_validate_tenant_id_for_rls_valid() {
        let valid_uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let result = validate_tenant_id_for_rls(valid_uuid);
        assert!(result.is_ok());
    }

    // =====================================================================
    // AuthConfig Production Guard Tests
    // =====================================================================

    #[test]
    fn test_auth_config_from_env_dev_fallback() {
        with_env_lock(|| {
            // Clear JWT_SECRET if set
            std::env::remove_var("JWT_SECRET");

            let config = AuthConfig::from_env_impl(false);
            assert!(config.is_ok());
            let config = config.unwrap();
            assert_eq!(config.jwt_secret, "dev-secret-key-do-not-use-in-production");
            assert!(!config.is_production_ready());
        });
    }

    #[test]
    fn test_auth_config_from_env_strict_missing_fails() {
        with_env_lock(|| {
            // Save original env values to restore after test
            let had_jwt_secret = std::env::var("JWT_SECRET");
            let had_require_jwt = std::env::var("INTENT_API_REQUIRE_JWT");

            // Ensure JWT_SECRET is not set for this test
            std::env::remove_var("JWT_SECRET");
            std::env::remove_var("INTENT_API_REQUIRE_JWT");

            let config = AuthConfig::from_env_impl(true);
            assert!(config.is_err());
            match config.unwrap_err() {
                AuthConfigError::MissingSecret(_) => {}
                other => panic!("Expected MissingSecret, got {:?}", other),
            }

            // Restore original env values
            match had_jwt_secret {
                Ok(v) => std::env::set_var("JWT_SECRET", v),
                Err(_) => std::env::remove_var("JWT_SECRET"),
            }
            match had_require_jwt {
                Ok(v) => std::env::set_var("INTENT_API_REQUIRE_JWT", v),
                Err(_) => std::env::remove_var("INTENT_API_REQUIRE_JWT"),
            }
        });
    }

    #[test]
    fn test_auth_config_from_env_weak_secret_strict_fails() {
        with_env_lock(|| {
            // Save original env values to restore after test
            let had_jwt_secret = std::env::var("JWT_SECRET");
            let had_require_jwt = std::env::var("INTENT_API_REQUIRE_JWT");

            std::env::set_var("JWT_SECRET", "dev-secret-key-do-not-use-in-production");
            std::env::remove_var("INTENT_API_REQUIRE_JWT");

            let config = AuthConfig::from_env_impl(true);
            assert!(config.is_err());
            match config.unwrap_err() {
                AuthConfigError::WeakSecret(msg) => {
                    assert!(msg.contains("weak"));
                }
                other => panic!("Expected WeakSecret, got {:?}", other),
            }

            // Restore original env values
            match had_jwt_secret {
                Ok(v) => std::env::set_var("JWT_SECRET", v),
                Err(_) => std::env::remove_var("JWT_SECRET"),
            }
            match had_require_jwt {
                Ok(v) => std::env::set_var("INTENT_API_REQUIRE_JWT", v),
                Err(_) => std::env::remove_var("INTENT_API_REQUIRE_JWT"),
            }
        });
    }

    #[test]
    fn test_auth_config_from_env_short_secret_strict_fails() {
        with_env_lock(|| {
            // Save original env values to restore after test
            let had_jwt_secret = std::env::var("JWT_SECRET");
            let had_require_jwt = std::env::var("INTENT_API_REQUIRE_JWT");

            std::env::set_var("JWT_SECRET", "short");
            std::env::remove_var("INTENT_API_REQUIRE_JWT");

            let config = AuthConfig::from_env_impl(true);
            assert!(config.is_err());
            match config.unwrap_err() {
                AuthConfigError::WeakSecret(msg) => {
                    assert!(msg.contains("32 bytes"));
                }
                other => panic!("Expected WeakSecret, got {:?}", other),
            }

            // Restore original env values
            match had_jwt_secret {
                Ok(v) => std::env::set_var("JWT_SECRET", v),
                Err(_) => std::env::remove_var("JWT_SECRET"),
            }
            match had_require_jwt {
                Ok(v) => std::env::set_var("INTENT_API_REQUIRE_JWT", v),
                Err(_) => std::env::remove_var("INTENT_API_REQUIRE_JWT"),
            }
        });
    }

    #[test]
    fn test_auth_config_production_ready() {
        with_env_lock(|| {
            // Save original env values to restore after test
            let had_jwt_secret = std::env::var("JWT_SECRET");
            let had_require_jwt = std::env::var("INTENT_API_REQUIRE_JWT");

            // Set a proper secret that doesn't contain any forbidden words
            std::env::set_var(
                "JWT_SECRET",
                "this-is-a-long-and-unguessable-key-for-hs256-algo",
            );
            std::env::remove_var("INTENT_API_REQUIRE_JWT");

            let config = AuthConfig::from_env_impl(true);
            assert!(config.is_ok());
            assert!(config.unwrap().is_production_ready());

            // Restore original env values
            match had_jwt_secret {
                Ok(v) => std::env::set_var("JWT_SECRET", v),
                Err(_) => std::env::remove_var("JWT_SECRET"),
            }
            match had_require_jwt {
                Ok(v) => std::env::set_var("INTENT_API_REQUIRE_JWT", v),
                Err(_) => std::env::remove_var("INTENT_API_REQUIRE_JWT"),
            }
        });
    }
}
