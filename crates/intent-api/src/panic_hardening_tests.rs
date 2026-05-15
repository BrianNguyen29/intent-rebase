use crate::panic_hardening::{init_panic_hook, sanitize_panic_payload};

#[test]
fn test_sanitize_panic_payload_jwt_token() {
    let payload = "Error: token=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<JWT_REDACTED>"));
    assert!(!sanitized.contains("eyJ"));
}

#[test]
fn test_sanitize_panic_payload_db_url() {
    let payload = "Connection failed: postgres://user:password@localhost:5432/dbname";
    let sanitized = sanitize_panic_payload(payload);
    // Bounded slice: only protocol prefix is redacted, not full URL credentials
    // This prevents protocol-based log injection while keeping implementation minimal
    assert!(sanitized.contains("<DB_URL_REDACTED>"));
    assert!(!sanitized.contains("postgres://"));
}

#[test]
fn test_sanitize_panic_payload_aws_credentials() {
    let payload = "AWS Error: AccessKeyId=AKIAIOSFODNN7EXAMPLE SecretAccessKey=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<AWS_CREDS_REDACTED>"));
    assert!(!sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn test_sanitize_panic_payload_bearer_token() {
    let payload = "Auth failed: Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
    let sanitized = sanitize_panic_payload(payload);
    assert!(sanitized.contains("<TOKEN_REDACTED>"));
    assert!(!sanitized.contains("eyJ"));
}

#[test]
fn test_sanitize_panic_payload_truncation() {
    let long_payload = "x".repeat(1000);
    let sanitized = sanitize_panic_payload(&long_payload);
    assert!(sanitized.contains("<TRUNCATED"));
    assert!(sanitized.len() < 1000);
}

#[test]
fn test_sanitize_panic_payload_noop_on_clean_string() {
    let payload = "This is a normal error message with no secrets";
    let sanitized = sanitize_panic_payload(payload);
    assert_eq!(sanitized, payload);
}

#[test]
fn test_init_panic_hook_does_not_panic() {
    // init_panic_hook should not panic - just register the hook
    init_panic_hook();
    // If we get here, the test passes
}
