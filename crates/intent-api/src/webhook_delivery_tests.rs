//! Webhook delivery unit tests (G7 — payload shape + headers)
//!
//! Bounded non-production slice: verifies payload serialization, header values,
//! and sanitization helpers without any HTTP client or async dispatch.
//!
//! See: docs/10-delivery/19-propagation-status-implementation-plan.md (R6 D9)

use crate::webhook_delivery::{
    build_webhook_payload, sanitize_failure_reason, WebhookHeaders, WebhookPayloadInput,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn test_payload_shape_matches_schema() {
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let delivery_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 42,
        version_hash: Some("sha256:abc123".to_string()),
        previous_version: Some(41),
        delivery_id,
        attempt_number: 1,
        subscription_id,
    });

    let json = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["event_type"], "intent_changed");
    assert_eq!(json["intent_id"], json!(intent_id));
    assert_eq!(json["tenant_id"], json!(tenant_id));
    assert_eq!(json["version"], 42);
    assert_eq!(json["version_hash"], "sha256:abc123");
    assert_eq!(json["previous_version"], 41);
    assert!(json.get("timestamp").is_some());
    assert_eq!(json["delivery_id"], json!(delivery_id));
    assert_eq!(json["attempt_number"], 1);
    assert_eq!(json["subscription_id"], json!(subscription_id));
}

#[test]
fn test_payload_omits_optional_fields_when_none() {
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let delivery_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 1,
        subscription_id,
    });

    let json = serde_json::to_value(&payload).unwrap();
    assert!(json.get("version_hash").is_none());
    assert!(json.get("previous_version").is_none());
}

#[test]
fn test_content_type_header_is_application_json() {
    let delivery_id = Uuid::new_v4();
    let headers = WebhookHeaders::new(delivery_id);
    assert_eq!(headers.content_type, "application/json");
}

#[test]
fn test_idempotency_key_is_delivery_id() {
    let delivery_id = Uuid::new_v4();
    let headers = WebhookHeaders::new(delivery_id);
    assert_eq!(headers.idempotency_key, delivery_id.to_string());
}

#[test]
fn test_attempt_number_is_one_on_initial_attempt() {
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let delivery_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 1,
        subscription_id,
    });
    assert_eq!(payload.attempt_number, 1);
}

#[test]
fn test_attempt_number_increments_on_retry() {
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let delivery_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let first = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 1,
        subscription_id,
    });
    let second = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 2,
        subscription_id,
    });

    assert_eq!(first.attempt_number, 1);
    assert_eq!(second.attempt_number, 2);
}

#[test]
fn test_signature_header_is_absent() {
    let delivery_id = Uuid::new_v4();
    let headers = WebhookHeaders::new(delivery_id);
    assert!(!headers.has_signature_header());
}

#[test]
fn test_sanitize_failure_reason_strips_full_url() {
    let raw = "Connection failed: https://example.com/webhook?secret=abc123";
    let sanitized = sanitize_failure_reason(raw);
    assert!(!sanitized.contains("https://example.com/webhook?secret=abc123"));
    assert!(sanitized.contains("[URL_REDACTED]"));
}

#[test]
fn test_sanitize_failure_reason_leaves_non_url_text_intact() {
    let raw = "DNS resolution timeout for host";
    let sanitized = sanitize_failure_reason(raw);
    assert_eq!(sanitized, "DNS resolution timeout for host");
}

#[test]
fn test_sanitize_failure_reason_strips_http_url() {
    let raw = "POST to http://internal.service:8080/callback failed";
    let sanitized = sanitize_failure_reason(raw);
    assert!(!sanitized.contains("http://internal.service:8080/callback"));
    assert!(sanitized.contains("[URL_REDACTED]"));
}

#[test]
fn test_sanitize_failure_reason_strips_multiple_urls() {
    let raw = "Tried https://a.com then http://b.com";
    let sanitized = sanitize_failure_reason(raw);
    assert_eq!(sanitized, "Tried [URL_REDACTED] then [URL_REDACTED]");
}
