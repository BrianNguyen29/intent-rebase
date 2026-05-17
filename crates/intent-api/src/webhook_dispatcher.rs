//! Webhook dispatcher (Phase 4a Slice 3)
//!
//! Bounded dispatch boundary: converts an outbox record into an HTTP
//! webhook delivery attempt with optional HMAC-SHA256 signing.
//!
//! Does NOT implement subscription CRUD or production secret management.
//! Retry/DLQ full lifecycle remains Slice 4/5 scope.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 3)

use async_trait::async_trait;
use std::sync::Arc;

use crate::webhook_delivery::{
    build_webhook_payload, sanitize_failure_reason, WebhookDeliveryResult, WebhookHeaders,
    WebhookPayloadInput, WebhookSendError, WebhookSender,
};
use crate::webhook_hmac::{build_canonical_string, sign_payload, WEBHOOK_HMAC_SECRET_ENV_VAR};
use crate::webhook_outbox_repo::WebhookOutboxRecord;

// =============================================================================
// Dispatch Failure Classification
// =============================================================================

/// Classification of a webhook dispatch failure for retry decisions.
///
/// Slice 5a: the worker uses this to decide whether to reschedule a retry
/// (Retryable with attempts remaining) or mark the record as failed
/// (Terminal or exhausted).
#[derive(Debug, Clone)]
pub enum WebhookDispatchFailure {
    /// Transient failure — the worker may reschedule a retry if attempts remain.
    Retryable { reason: String },
    /// Non-retryable failure — the worker should mark the record as failed immediately.
    Terminal { reason: String },
}

// =============================================================================
// Dispatcher Trait
// =============================================================================

#[async_trait]
pub trait WebhookDispatcher: Send + Sync {
    /// Dispatch a single outbox record.
    ///
    /// Returns `Ok(())` on successful delivery, or `Err(failure)` on failure.
    /// The caller (worker) uses `WebhookDispatchFailure` to decide whether to
    /// reschedule a retry or mark the record as failed.
    async fn dispatch(&self, record: &WebhookOutboxRecord) -> Result<(), WebhookDispatchFailure>;
}

// =============================================================================
// Delivery Dispatcher Implementation
// =============================================================================

/// Production-oriented dispatcher that builds payloads, signs with HMAC,
/// and sends via a `WebhookSender` abstraction.
pub struct WebhookDeliveryDispatcher {
    sender: Arc<dyn WebhookSender>,
}

impl WebhookDeliveryDispatcher {
    pub fn new(sender: Arc<dyn WebhookSender>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl WebhookDispatcher for WebhookDeliveryDispatcher {
    async fn dispatch(&self, record: &WebhookOutboxRecord) -> Result<(), WebhookDispatchFailure> {
        let url =
            record
                .webhook_url
                .as_deref()
                .ok_or_else(|| WebhookDispatchFailure::Terminal {
                    reason: "No webhook_url in outbox record".to_string(),
                })?;

        // TODO(Slice 4+): version info should be stored in the outbox record at creation time.
        let payload = build_webhook_payload(WebhookPayloadInput {
            intent_id: record.intent_id,
            tenant_id: record.tenant_id,
            version: 0,
            version_hash: None,
            previous_version: None,
            delivery_id: record.id,
            attempt_number: record.attempt_count + 1,
            subscription_id: record.subscription_id,
        });

        let body = serde_json::to_string(&payload).unwrap_or_default();
        let timestamp = payload.timestamp.to_rfc3339();

        let mut headers = WebhookHeaders::new(record.id);

        // HMAC signing (local-dev env secret only)
        if let Ok(secret) = std::env::var(WEBHOOK_HMAC_SECRET_ENV_VAR) {
            let canonical = build_canonical_string(&timestamp, &record.id.to_string(), &body);
            let signature = sign_payload(&secret, &canonical).map_err(|e| {
                WebhookDispatchFailure::Terminal {
                    reason: format!("HMAC sign error: {}", e),
                }
            })?;
            headers = headers.with_signature(signature);
        }

        let result = self.sender.send(url, &payload, &headers).await;

        match result {
            Ok(WebhookDeliveryResult::Success) => Ok(()),
            Ok(WebhookDeliveryResult::NonRetryableFailure { reason }) => {
                Err(WebhookDispatchFailure::Terminal {
                    reason: sanitize_failure_reason(&reason),
                })
            }
            Ok(WebhookDeliveryResult::RetryableFailure { reason }) => {
                Err(WebhookDispatchFailure::Retryable {
                    reason: sanitize_failure_reason(&reason),
                })
            }
            Ok(WebhookDeliveryResult::RateLimited { retry_after }) => {
                Err(WebhookDispatchFailure::Retryable {
                    reason: format!("rate limited: retry_after={:?}", retry_after),
                })
            }
            Err(WebhookSendError::Network(reason)) => Err(WebhookDispatchFailure::Retryable {
                reason: sanitize_failure_reason(&reason),
            }),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhook_delivery::{WebhookPayload, WebhookSendError};
    use std::sync::Mutex;

    struct MockWebhookSender {
        result: Result<WebhookDeliveryResult, WebhookSendError>,
        captured_headers: Mutex<Option<WebhookHeaders>>,
    }

    impl MockWebhookSender {
        fn new(result: Result<WebhookDeliveryResult, WebhookSendError>) -> Self {
            Self {
                result,
                captured_headers: Mutex::new(None),
            }
        }

        fn captured_headers(&self) -> Option<WebhookHeaders> {
            self.captured_headers.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl WebhookSender for MockWebhookSender {
        async fn send(
            &self,
            _url: &str,
            _payload: &WebhookPayload,
            headers: &WebhookHeaders,
        ) -> Result<WebhookDeliveryResult, WebhookSendError> {
            *self.captured_headers.lock().unwrap() = Some(headers.clone());
            self.result.clone()
        }
    }

    fn sample_record_with_url(url: &str) -> WebhookOutboxRecord {
        WebhookOutboxRecord::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
            Some(url.to_string()),
        )
    }

    #[test]
    fn test_dispatch_no_url_fails() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sender = Arc::new(MockWebhookSender::new(Ok(WebhookDeliveryResult::Success)));
        let dispatcher = WebhookDeliveryDispatcher::new(sender);
        let mut record = sample_record_with_url("http://example.com");
        record.webhook_url = None;

        let result = rt.block_on(dispatcher.dispatch(&record));
        assert!(result.is_err());
        match result.unwrap_err() {
            WebhookDispatchFailure::Terminal { reason } => {
                assert!(reason.contains("No webhook_url"));
            }
            other => panic!("expected Terminal failure, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_success() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sender = Arc::new(MockWebhookSender::new(Ok(WebhookDeliveryResult::Success)));
        let dispatcher = WebhookDeliveryDispatcher::new(sender);
        let record = sample_record_with_url("http://example.com");

        let result = rt.block_on(dispatcher.dispatch(&record));
        assert!(result.is_ok());
    }

    #[test]
    fn test_dispatch_failure() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let sender = Arc::new(MockWebhookSender::new(Ok(
            WebhookDeliveryResult::NonRetryableFailure {
                reason: "HTTP 404".to_string(),
            },
        )));
        let dispatcher = WebhookDeliveryDispatcher::new(sender);
        let record = sample_record_with_url("http://example.com");

        let result = rt.block_on(dispatcher.dispatch(&record));
        assert!(result.is_err());
        match result.unwrap_err() {
            WebhookDispatchFailure::Terminal { reason } => {
                assert!(reason.contains("404"));
            }
            other => panic!("expected Terminal failure, got {:?}", other),
        }
    }

    #[test]
    fn test_dispatch_adds_hmac_signature_when_secret_set() {
        temp_env::with_var(WEBHOOK_HMAC_SECRET_ENV_VAR, Some("test_secret_123"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let sender = Arc::new(MockWebhookSender::new(Ok(WebhookDeliveryResult::Success)));
            let dispatcher = WebhookDeliveryDispatcher::new(sender.clone());
            let record = sample_record_with_url("http://example.com");

            let result = rt.block_on(dispatcher.dispatch(&record));
            assert!(result.is_ok());

            let headers = sender.captured_headers().unwrap();
            assert!(headers.has_signature_header());
            let signature = headers.signature.unwrap();
            assert_eq!(signature.len(), 64);
            assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
        });
    }

    #[test]
    fn test_dispatch_no_hmac_when_secret_unset() {
        temp_env::with_var_unset(WEBHOOK_HMAC_SECRET_ENV_VAR, || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let sender = Arc::new(MockWebhookSender::new(Ok(WebhookDeliveryResult::Success)));
            let dispatcher = WebhookDeliveryDispatcher::new(sender.clone());
            let record = sample_record_with_url("http://example.com");

            let result = rt.block_on(dispatcher.dispatch(&record));
            assert!(result.is_ok());

            let headers = sender.captured_headers().unwrap();
            assert!(!headers.has_signature_header());
        });
    }
}
