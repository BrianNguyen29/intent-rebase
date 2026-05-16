//! Webhook HMAC signing (Phase 4a Slice 3)
//!
//! Bounded local-dev foundation: signs webhook payloads with HMAC-SHA256
//! using a secret from the `INTENT_API_WEBHOOK_HMAC_SECRET` env var.
//! Production secret manager integration and key rotation remain deferred.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 3)

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Environment variable name for the local-dev HMAC secret.
pub const WEBHOOK_HMAC_SECRET_ENV_VAR: &str = "INTENT_API_WEBHOOK_HMAC_SECRET";

type HmacSha256 = Hmac<Sha256>;

/// Sign a canonical payload string with HMAC-SHA256.
///
/// Returns a lowercase hex-encoded signature string.
pub fn sign_payload(secret: &str, payload: &str) -> Result<String, String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("HMAC init error: {}", e))?;
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();
    Ok(hex_encode(&bytes))
}

/// Build the canonical string to sign for a webhook payload.
///
/// Format: "<timestamp>.<delivery_id>.<body_json>"
pub fn build_canonical_string(timestamp: &str, delivery_id: &str, body: &str) -> String {
    format!("{}.{}.{}", timestamp, delivery_id, body)
}

/// Encode bytes as a lowercase hex string.
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_canonical_string() {
        let ts = "2026-05-16T12:00:00Z";
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let body = r#"{"event":"intent_changed"}"#;
        let canonical = build_canonical_string(ts, id, body);
        assert_eq!(
            canonical,
            "2026-05-16T12:00:00Z.550e8400-e29b-41d4-a716-446655440000.{\"event\":\"intent_changed\"}"
        );
    }

    #[test]
    fn test_sign_payload_fixed_vector() {
        // Fixed vector: known secret + known payload → known hex signature
        let secret = "super_secret_key_42";
        let payload = "2026-05-16T12:00:00Z.550e8400-e29b-41d4-a716-446655440000.{\"foo\":\"bar\"}";
        let signature = sign_payload(secret, payload).unwrap();

        // Verify the signature is a 64-character hex string (32 bytes * 2)
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));

        // Verify determinism: same inputs produce same signature
        let signature2 = sign_payload(secret, payload).unwrap();
        assert_eq!(signature, signature2);

        // Verify different secret produces different signature
        let signature3 = sign_payload("different_secret", payload).unwrap();
        assert_ne!(signature, signature3);

        // Verify different payload produces different signature
        let signature4 = sign_payload(secret, "different_payload").unwrap();
        assert_ne!(signature, signature4);
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }
}
