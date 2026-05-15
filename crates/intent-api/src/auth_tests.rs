use crate::auth::{
    generate_test_token, rls_reset_tenant_context_sql, rls_set_tenant_context_sql,
    validate_tenant_id_for_rls, AuthConfig, AuthConfigError, Claims, RlsTenantClaims,
};
use intent_rebase_types::rls::RlsTenantContext;
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
// RLS Helper Tests (re-exported from intent-rebase-types)
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
// RlsTenantContext Tests (re-exported from intent-rebase-types)
// =====================================================================

#[test]
fn test_rls_tenant_context_new_valid() {
    let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let ctx = RlsTenantContext::new(tenant_id);
    assert!(ctx.is_ok());
    let ctx = ctx.unwrap();
    assert_eq!(ctx.tenant_id(), tenant_id);
}

#[test]
fn test_rls_tenant_context_new_nil_rejected() {
    let nil_uuid = uuid::Uuid::nil();
    let ctx = RlsTenantContext::new(nil_uuid);
    assert!(ctx.is_err());
    assert!(ctx.unwrap_err().contains("Nil UUID"));
}

#[test]
fn test_rls_tenant_context_clone() {
    let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let ctx = RlsTenantContext::new(tenant_id).unwrap();
    let ctx_clone = ctx.clone();
    assert_eq!(ctx.tenant_id(), ctx_clone.tenant_id());
}

#[test]
fn test_rls_tenant_context_debug() {
    let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let ctx = RlsTenantContext::new(tenant_id).unwrap();
    let debug_str = format!("{:?}", ctx);
    assert!(debug_str.contains("RlsTenantContext"));
    assert!(debug_str.contains("550e8400-e29b-41d4-a716-446655440000"));
}

// =====================================================================
// RlsTenantClaims Tests
// =====================================================================

#[test]
fn test_rls_tenant_claims_new_valid() {
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        roles: vec!["admin".to_string()],
        exp: 9999999999,
        iat: 0,
    };
    let result = RlsTenantClaims::new(claims.clone());
    assert!(result.is_ok());
    let tenant_claims = result.unwrap();
    assert_eq!(
        tenant_claims.tenant_id,
        uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
    );
    assert_eq!(tenant_claims.claims.sub, "user-1");
}

#[test]
fn test_rls_tenant_claims_new_nil_rejected() {
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "00000000-0000-0000-0000-000000000000".to_string(),
        roles: vec![],
        exp: 9999999999,
        iat: 0,
    };
    let result = RlsTenantClaims::new(claims);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Nil UUID"));
}

#[test]
fn test_rls_tenant_claims_new_invalid_uuid() {
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: "not-a-valid-uuid".to_string(),
        roles: vec![],
        exp: 9999999999,
        iat: 0,
    };
    let result = RlsTenantClaims::new(claims);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("not a valid UUID"));
}

#[test]
fn test_rls_tenant_claims_new_unchecked() {
    let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let claims = Claims {
        sub: "user-1".to_string(),
        tenant_id: tenant_id.to_string(),
        roles: vec![],
        exp: 9999999999,
        iat: 0,
    };
    let tenant_claims = RlsTenantClaims::new_unchecked(tenant_id, claims.clone());
    assert_eq!(tenant_claims.tenant_id, tenant_id);
    assert_eq!(tenant_claims.claims.sub, "user-1");
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
