//! JWT authentication types for intent-api
//!
//! This module provides JWT authentication when the `jwt-auth` feature is enabled.
//! Use `build_router_with_jwt_auth` instead of `build_router` to enable JWT authentication.

use jsonwebtoken::Algorithm;
use serde::{Deserialize, Serialize};

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
}
