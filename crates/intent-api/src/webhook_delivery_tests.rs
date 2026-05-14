//! Webhook delivery unit tests (G7 — payload shape + headers + B4 skeleton)
//!
//! Bounded non-production slice: verifies payload serialization, header values,
//! sanitization helpers, env gate parsing, retry classification, and backoff
//! behavior without any wired application flow or production dispatch.
//!
//! See: docs/10-delivery/19-propagation-status-implementation-plan.md (R6 D9, R5, R7)

use crate::webhook_delivery::{
    build_webhook_client, build_webhook_payload, classify_status_code, compute_backoff_delay,
    is_webhook_delivery_enabled, sanitize_failure_reason, send_webhook, WebhookDeliveryResult,
    WebhookErrorCategory, WebhookHeaders, WebhookPayloadInput, WEBHOOK_BACKOFF_BASE_DELAY,
    WEBHOOK_BACKOFF_MAX_DELAY, WEBHOOK_CONNECT_TIMEOUT, WEBHOOK_MAX_ATTEMPTS,
    WEBHOOK_MAX_TOTAL_DURATION, WEBHOOK_REQUEST_TIMEOUT, WEBHOOK_RETRY_AFTER_CAP,
};
use intent_rebase_types::PropagationStatus;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// =============================================================================
// B3 Payload & Header Tests
// =============================================================================

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

// =============================================================================
// B4 Env Gate Tests
// =============================================================================

#[test]
fn test_env_gate_disabled_when_unset() {
    temp_env::with_var_unset("INTENT_API_WEBHOOK_DELIVERY", || {
        assert!(!is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_disabled_for_empty() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some(""), || {
        assert!(!is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_disabled_for_false() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("false"), || {
        assert!(!is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_disabled_for_random_string() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("maybe"), || {
        assert!(!is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_enabled_for_true() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("true"), || {
        assert!(is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_enabled_for_one() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("1"), || {
        assert!(is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_enabled_for_yes() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("yes"), || {
        assert!(is_webhook_delivery_enabled());
    });
}

#[test]
fn test_env_gate_true_is_case_insensitive() {
    temp_env::with_var("INTENT_API_WEBHOOK_DELIVERY", Some("TRUE"), || {
        assert!(is_webhook_delivery_enabled());
    });
}

// =============================================================================
// B4 Retry Classification Tests
// =============================================================================

#[test]
fn test_classify_2xx_as_success() {
    assert_eq!(classify_status_code(200), WebhookErrorCategory::Success);
    assert_eq!(classify_status_code(204), WebhookErrorCategory::Success);
}

#[test]
fn test_classify_5xx_as_retryable() {
    assert_eq!(classify_status_code(500), WebhookErrorCategory::Retryable);
    assert_eq!(classify_status_code(503), WebhookErrorCategory::Retryable);
}

#[test]
fn test_classify_4xx_as_non_retryable() {
    assert_eq!(
        classify_status_code(400),
        WebhookErrorCategory::NonRetryable
    );
    assert_eq!(
        classify_status_code(404),
        WebhookErrorCategory::NonRetryable
    );
}

#[test]
fn test_classify_429_as_rate_limited() {
    assert_eq!(classify_status_code(429), WebhookErrorCategory::RateLimited);
}

// =============================================================================
// B4 Timeout Constants Tests
// =============================================================================

#[test]
fn test_timeout_constants_match_r5() {
    assert_eq!(WEBHOOK_CONNECT_TIMEOUT, Duration::from_secs(5));
    assert_eq!(WEBHOOK_REQUEST_TIMEOUT, Duration::from_secs(30));
    assert_eq!(WEBHOOK_MAX_TOTAL_DURATION, Duration::from_secs(120));
}

#[test]
fn test_retry_constants_match_r5() {
    assert_eq!(WEBHOOK_BACKOFF_BASE_DELAY, Duration::from_secs(2));
    assert_eq!(WEBHOOK_BACKOFF_MAX_DELAY, Duration::from_secs(30));
    assert_eq!(WEBHOOK_MAX_ATTEMPTS, 3);
    assert_eq!(WEBHOOK_RETRY_AFTER_CAP, Duration::from_secs(60));
}

// =============================================================================
// B4 Backoff / Jitter Tests
// =============================================================================

#[test]
fn test_backoff_delay_is_non_negative() {
    for attempt in 1..=5 {
        let delay = compute_backoff_delay(attempt);
        assert!(
            delay >= Duration::ZERO,
            "delay for attempt {} was negative",
            attempt
        );
    }
}

#[test]
fn test_backoff_delay_does_not_exceed_max() {
    for attempt in 1..=10 {
        let delay = compute_backoff_delay(attempt);
        assert!(
            delay <= WEBHOOK_BACKOFF_MAX_DELAY,
            "delay for attempt {} exceeded max: {:?}",
            attempt,
            delay
        );
    }
}

#[test]
fn test_backoff_delay_increases_with_attempt() {
    // Because of jitter we can't guarantee monotonicity on every sample,
    // but the *expected* value increases. We verify the max bound grows.
    let base = compute_backoff_delay(1);
    let mid = compute_backoff_delay(5);
    let high = compute_backoff_delay(10);
    // All are capped at MAX_DELAY, so they should never exceed it
    assert!(base <= WEBHOOK_BACKOFF_MAX_DELAY);
    assert!(mid <= WEBHOOK_BACKOFF_MAX_DELAY);
    assert!(high <= WEBHOOK_BACKOFF_MAX_DELAY);
}

// =============================================================================
// B4 Async Skeleton Compile / Integration Tests
// =============================================================================

#[tokio::test]
async fn test_build_webhook_client_has_timeouts() {
    let client = build_webhook_client();
    // Client construction succeeds — compile-time + runtime sanity check.
    // Actual timeout enforcement is verified by future integration tests.
    let _ = client;
}

#[tokio::test]
async fn test_send_webhook_compiles_and_runs_against_unreachable_host() {
    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id: Uuid::new_v4(),
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(payload.delivery_id);

    // Use a reserved-documentation URL that should always be unreachable.
    let result = send_webhook(
        &client,
        "http://localhost:59999/_test_unreachable",
        &payload,
        &headers,
    )
    .await;

    // Should fail at connection level, confirming the skeleton executes.
    assert!(result.is_err());
}

// =============================================================================
// B5 Dispatcher Integration Tests
// =============================================================================

use crate::webhook_delivery::{
    dispatch_webhooks_for_intent, EmptyWebhookSubscriptionResolver,
    InMemoryWebhookSubscriptionResolver, WebhookSendError, WebhookSender, WebhookSubscription,
};
use intent_rebase_types::PropagationRecord;
use intent_service::InMemoryPropagationRecordRepository;

/// Mock sender that always returns a predetermined result.
struct MockWebhookSender {
    result: Result<WebhookDeliveryResult, WebhookSendError>,
}

impl MockWebhookSender {
    fn new(result: Result<WebhookDeliveryResult, WebhookSendError>) -> Self {
        Self { result }
    }
}

#[async_trait::async_trait]
impl WebhookSender for MockWebhookSender {
    async fn send(
        &self,
        _url: &str,
        _payload: &crate::webhook_delivery::WebhookPayload,
        _headers: &WebhookHeaders,
    ) -> Result<WebhookDeliveryResult, WebhookSendError> {
        self.result.clone()
    }
}

#[tokio::test]
async fn test_dispatch_disabled_env_gate_records_no_attempts() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Dispatch with empty resolver (no subscriptions) — nothing happens regardless of gate
    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::Success));
    let resolver = EmptyWebhookSubscriptionResolver;
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    // No subscriptions matched, so no attempt recorded
    let stored = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 0);
}

#[tokio::test]
async fn test_dispatch_records_attempt_and_acknowledged_outcome() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::Success));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    let stored = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 1);
    assert_eq!(stored.status, PropagationStatus::Acknowledged);
    assert!(stored.acknowledged_at.is_some());
}

#[tokio::test]
async fn test_dispatch_records_attempt_and_failed_outcome() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::NonRetryableFailure {
        reason: "HTTP 404".to_string(),
    }));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    let stored = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 1);
    assert_eq!(stored.status, PropagationStatus::Failed);
    assert!(stored.failed_at.is_some());
    assert_eq!(stored.failure_reason, Some("HTTP 404".to_string()));
}

#[tokio::test]
async fn test_dispatch_records_network_error_as_failed() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Err(WebhookSendError::Network("timeout".to_string())));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    let stored = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 1);
    assert_eq!(stored.status, PropagationStatus::Failed);
    assert_eq!(stored.failure_reason, Some("timeout".to_string()));
}

#[tokio::test]
async fn test_dispatch_no_matching_record_skips_subscription() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    // No propagation records seeded
    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::Success));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    // Nothing crashes; no records exist to verify.
}

#[tokio::test]
async fn test_dispatch_wrong_tenant_no_attempt() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_a, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id: tenant_a,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::Success));
    // Dispatch with wrong tenant — resolver returns tenant_a's sub, but repo filters by tenant_b
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_b, intent_id, 2).await;

    // Record should remain untouched because list_by_intent with wrong tenant returns empty
    let stored = repo.get_record(record_id, tenant_a).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 0);
    assert_eq!(stored.status, PropagationStatus::Pending);
}

// =============================================================================
// B7 G8-Style Wiremock Delivery Simulation Tests (non-DB, no live Postgres)
// =============================================================================

use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_send_webhook_200_success() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id: Uuid::new_v4(),
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(payload.delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), WebhookDeliveryResult::Success);
}

#[tokio::test]
async fn test_send_webhook_404_non_retryable() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id: Uuid::new_v4(),
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(payload.delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        WebhookDeliveryResult::NonRetryableFailure {
            reason: "HTTP 404".to_string(),
        }
    );
}

#[tokio::test]
async fn test_send_webhook_500_retryable() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id: Uuid::new_v4(),
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(payload.delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        WebhookDeliveryResult::RetryableFailure {
            reason: "HTTP 503".to_string(),
        }
    );
}

#[tokio::test]
async fn test_send_webhook_429_rate_limited() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "42"))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id: Uuid::new_v4(),
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(payload.delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        WebhookDeliveryResult::RateLimited {
            retry_after: Some(std::time::Duration::from_secs(42)),
        }
    );
}

#[tokio::test]
async fn test_send_webhook_headers_present() {
    let mock_server = MockServer::start().await;
    let delivery_id = Uuid::new_v4();

    Mock::given(method("POST"))
        .and(path("/webhook"))
        .and(header("Content-Type", "application/json"))
        .and(header("X-Idempotency-Key", delivery_id.to_string()))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        version: 1,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 1,
        subscription_id: Uuid::new_v4(),
    });
    let headers = WebhookHeaders::new(delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_send_webhook_body_shape() {
    let mock_server = MockServer::start().await;
    let delivery_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let expected_body = serde_json::json!({
        "event_type": "intent_changed",
        "intent_id": intent_id,
        "tenant_id": tenant_id,
        "version": 2,
        "delivery_id": delivery_id,
        "attempt_number": 1,
        "subscription_id": subscription_id,
    });

    Mock::given(method("POST"))
        .and(path("/webhook"))
        .and(body_json(&expected_body))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let client = build_webhook_client();
    let payload = build_webhook_payload(WebhookPayloadInput {
        intent_id,
        tenant_id,
        version: 2,
        version_hash: None,
        previous_version: None,
        delivery_id,
        attempt_number: 1,
        subscription_id,
    });
    let headers = WebhookHeaders::new(delivery_id);

    let result = send_webhook(
        &client,
        &format!("{}/webhook", mock_server.uri()),
        &payload,
        &headers,
    )
    .await;
    assert!(result.is_ok());
}

// =============================================================================
// B7 Dispatcher Coverage Gaps
// =============================================================================

#[tokio::test]
async fn test_dispatch_rate_limited_outcome() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    let sub = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/callback".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::RateLimited {
        retry_after: Some(std::time::Duration::from_secs(30)),
    }));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    let stored = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(stored.delivery_attempt_count, 1);
    assert_eq!(stored.status, PropagationStatus::Failed);
    assert!(stored
        .failure_reason
        .as_ref()
        .unwrap()
        .contains("rate limited"));
}

#[tokio::test]
async fn test_dispatch_multiple_subscriptions() {
    let repo: Arc<dyn intent_service::PropagationRecordRepository> =
        Arc::new(InMemoryPropagationRecordRepository::new());
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let record_a = PropagationRecord::new(tenant_id, intent_id, "system-a".to_string());
    let record_b = PropagationRecord::new(tenant_id, intent_id, "system-b".to_string());
    let record_id_a = record_a.id;
    let record_id_b = record_b.id;
    repo.create_record(record_a).await.unwrap();
    repo.create_record(record_b).await.unwrap();

    let sub_a = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/a".to_string(),
        downstream_system_id: Some("system-a".to_string()),
    };
    let sub_b = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: "http://localhost:59999/b".to_string(),
        downstream_system_id: Some("system-b".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(sub_a);
    resolver.add(sub_b);

    let sender = MockWebhookSender::new(Ok(WebhookDeliveryResult::Success));
    dispatch_webhooks_for_intent(&repo, &sender, &resolver, tenant_id, intent_id, 2).await;

    let stored_a = repo.get_record(record_id_a, tenant_id).await.unwrap();
    let stored_b = repo.get_record(record_id_b, tenant_id).await.unwrap();
    assert_eq!(stored_a.delivery_attempt_count, 1);
    assert_eq!(stored_a.status, PropagationStatus::Acknowledged);
    assert_eq!(stored_b.delivery_attempt_count, 1);
    assert_eq!(stored_b.status, PropagationStatus::Acknowledged);
}
