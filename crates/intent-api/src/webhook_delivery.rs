//! Webhook delivery scaffolding (B3 — internal payload/header builders)
//!
//! Bounded non-production slice: provides pure data builders for webhook
//! payloads and header values. No HTTP client, no async dispatch, no
//! env gate, and no production readiness claims.
//!
//! See: docs/10-delivery/19-propagation-status-implementation-plan.md (R6 D9)

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Webhook payload posted to subscription URLs.
///
/// Matches the proposed JSON schema from Slice 3 design:
/// event_type, intent_id, tenant_id, version, version_hash, previous_version,
/// timestamp, delivery_id, attempt_number, subscription_id.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WebhookPayload {
    pub event_type: String,
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<i32>,
    pub timestamp: DateTime<Utc>,
    pub delivery_id: Uuid,
    pub attempt_number: i32,
    pub subscription_id: Uuid,
}

/// Input parameters for building a webhook payload.
///
/// Collapses the former 8-argument function into a single struct argument
/// to satisfy clippy::too_many_arguments while keeping call sites readable.
#[derive(Debug, Clone)]
pub struct WebhookPayloadInput {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    pub version_hash: Option<String>,
    pub previous_version: Option<i32>,
    pub delivery_id: Uuid,
    pub attempt_number: i32,
    pub subscription_id: Uuid,
}

/// Build a webhook payload for an intent change event.
#[allow(dead_code)]
pub fn build_webhook_payload(input: WebhookPayloadInput) -> WebhookPayload {
    WebhookPayload {
        event_type: "intent_changed".to_string(),
        intent_id: input.intent_id,
        tenant_id: input.tenant_id,
        version: input.version,
        version_hash: input.version_hash,
        previous_version: input.previous_version,
        timestamp: Utc::now(),
        delivery_id: input.delivery_id,
        attempt_number: input.attempt_number,
        subscription_id: input.subscription_id,
    }
}

/// HTTP header name/value pairs for a webhook delivery.
#[derive(Debug, Clone, PartialEq)]
pub struct WebhookHeaders {
    pub content_type: String,
    pub idempotency_key: String,
}

impl WebhookHeaders {
    /// Build headers for a webhook delivery.
    ///
    /// X-Webhook-Signature is intentionally absent because HMAC signing
    /// is deferred (Slice 3 design note).
    #[allow(dead_code)]
    pub fn new(delivery_id: Uuid) -> Self {
        Self {
            content_type: "application/json".to_string(),
            idempotency_key: delivery_id.to_string(),
        }
    }

    /// Returns true if the signature header should be present.
    /// Currently always false (deferred).
    #[allow(dead_code)]
    pub fn has_signature_header(&self) -> bool {
        false
    }
}

/// Sanitize a failure reason to prevent leaking full URLs or PII.
///
/// Bounded helper: strips anything that looks like an HTTP/HTTPS URL,
/// replacing it with `[URL_REDACTED]`.
#[allow(dead_code)]
pub fn sanitize_failure_reason(reason: &str) -> String {
    let mut result = reason.to_string();
    for prefix in ["http://", "https://"] {
        while let Some(start) = result.find(prefix) {
            let end = result[start..]
                .find(|c: char| c.is_whitespace() || c == '\'' || c == '"')
                .map(|i| start + i)
                .unwrap_or(result.len());
            result.replace_range(start..end, "[URL_REDACTED]");
        }
    }
    result
}
