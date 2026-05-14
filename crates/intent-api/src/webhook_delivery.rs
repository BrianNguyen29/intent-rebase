//! Webhook delivery scaffolding (B3/B4/B5 — internal payload/header builders + async skeleton + dispatcher)
//!
//! Bounded non-production slice: provides pure data builders for webhook
//! payloads and header values, plus an internal async delivery skeleton
//! and env-gated dispatcher integration that is NOT wired into production flow.
//!
//! See: docs/10-delivery/19-propagation-status-implementation-plan.md (R6 D9, R2, R5, R7)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::{IntentRebaseError, PropagationStatus};
use intent_service::PropagationRecordRepository;
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

// =============================================================================
// Payload & Headers
// =============================================================================

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

// =============================================================================
// Env Gate
// =============================================================================

/// Environment variable name for the webhook delivery enablement gate.
pub const WEBHOOK_DELIVERY_ENV_VAR: &str = "INTENT_API_WEBHOOK_DELIVERY";

/// Parse the webhook delivery env gate.
///
/// - Explicit "true" / "1" / "yes" → enabled
/// - Unset, empty, or any other value → disabled (conservative default)
///
/// R7 D13: default disabled outside local/dev; conservative fail-closed.
#[allow(dead_code)]
pub fn is_webhook_delivery_enabled() -> bool {
    matches!(
        std::env::var(WEBHOOK_DELIVERY_ENV_VAR),
        Ok(v) if v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes")
    )
}

// =============================================================================
// Timeout & Retry Constants (R5)
// =============================================================================

/// TCP + TLS handshake establishment timeout.
pub const WEBHOOK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Total per-delivery attempt timeout (connect + send + wait-for-response).
pub const WEBHOOK_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Hard ceiling for all attempts including retries.
pub const WEBHOOK_MAX_TOTAL_DURATION: Duration = Duration::from_secs(120);

/// Exponential backoff base delay.
pub const WEBHOOK_BACKOFF_BASE_DELAY: Duration = Duration::from_secs(2);

/// Exponential backoff multiplier.
pub const WEBHOOK_BACKOFF_MULTIPLIER: f64 = 2.0;

/// Exponential backoff maximum delay.
pub const WEBHOOK_BACKOFF_MAX_DELAY: Duration = Duration::from_secs(30);

/// Maximum delivery attempts (initial + retries).
pub const WEBHOOK_MAX_ATTEMPTS: u32 = 3;

/// Retry-After header cap for 429 responses.
pub const WEBHOOK_RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

// =============================================================================
// Error Classification
// =============================================================================

/// Classification of a webhook delivery error for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookErrorCategory {
    /// Success — no error.
    Success,
    /// Retryable: HTTP 5xx, connect timeout, request timeout, DNS failure,
    /// connection refused, connection reset.
    Retryable,
    /// Non-retryable: HTTP 4xx (except 429), malformed URL, TLS cert failure,
    /// unresolvable host.
    NonRetryable,
    /// Special case: HTTP 429 Too Many Requests — retry once with backoff.
    RateLimited,
}

/// Classify an HTTP status code into an error category.
#[allow(dead_code)]
pub fn classify_status_code(status: u16) -> WebhookErrorCategory {
    match status {
        200..=299 => WebhookErrorCategory::Success,
        429 => WebhookErrorCategory::RateLimited,
        500..=599 => WebhookErrorCategory::Retryable,
        _ => WebhookErrorCategory::NonRetryable,
    }
}

// =============================================================================
// Backoff / Jitter
// =============================================================================

/// Compute the backoff delay for a given attempt number using exponential
/// backoff with full jitter.
///
/// Formula: delay = min(base * multiplier^(attempt-1), max_delay)
/// Jitter: rand::random::<f64>() * delay
#[allow(dead_code)]
pub fn compute_backoff_delay(attempt_number: u32) -> Duration {
    let raw_delay_secs = WEBHOOK_BACKOFF_BASE_DELAY.as_secs_f64()
        * WEBHOOK_BACKOFF_MULTIPLIER
            .powi(i32::try_from(attempt_number.saturating_sub(1)).unwrap_or(0));
    let capped_delay_secs = raw_delay_secs.min(WEBHOOK_BACKOFF_MAX_DELAY.as_secs_f64());
    let jittered_secs = rand::random::<f64>() * capped_delay_secs;
    Duration::from_secs_f64(jittered_secs)
}

// =============================================================================
// Async Delivery Skeleton
// =============================================================================

/// Bounded result of a webhook delivery attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum WebhookDeliveryResult {
    /// HTTP 2xx response — delivery acknowledged.
    Success,
    /// Retryable failure — caller may retry if attempts remain.
    RetryableFailure { reason: String },
    /// Non-retryable failure — caller should not retry.
    NonRetryableFailure { reason: String },
    /// Rate limited (429) — caller may retry once per R5 policy.
    RateLimited { retry_after: Option<Duration> },
}

/// Error returned by the webhook sender abstraction.
#[derive(Debug, Clone)]
pub enum WebhookSendError {
    Network(String),
}

/// Abstraction over HTTP webhook transport for testability.
#[async_trait]
pub trait WebhookSender: Send + Sync {
    async fn send(
        &self,
        url: &str,
        payload: &WebhookPayload,
        headers: &WebhookHeaders,
    ) -> Result<WebhookDeliveryResult, WebhookSendError>;
}

#[async_trait]
impl WebhookSender for reqwest::Client {
    async fn send(
        &self,
        url: &str,
        payload: &WebhookPayload,
        headers: &WebhookHeaders,
    ) -> Result<WebhookDeliveryResult, WebhookSendError> {
        send_webhook(self, url, payload, headers)
            .await
            .map_err(|e| WebhookSendError::Network(e.to_string()))
    }
}

/// Build a `reqwest::Client` configured with webhook timeout constants.
#[allow(dead_code)]
pub fn build_webhook_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(WEBHOOK_CONNECT_TIMEOUT)
        .timeout(WEBHOOK_REQUEST_TIMEOUT)
        .build()
        .expect("reqwest client with basic timeouts should always build")
}

/// Send a webhook payload to the given URL.
///
/// Bounded skeleton: performs the HTTP POST and classifies the result.
/// NOT wired into application flow — called only by future dispatcher code.
#[allow(dead_code)]
pub async fn send_webhook(
    client: &reqwest::Client,
    url: &str,
    payload: &WebhookPayload,
    headers: &WebhookHeaders,
) -> Result<WebhookDeliveryResult, reqwest::Error> {
    let body = serde_json::to_string(payload).unwrap_or_default();

    let response = client
        .post(url)
        .header("Content-Type", &headers.content_type)
        .header("X-Idempotency-Key", &headers.idempotency_key)
        .body(body)
        .send()
        .await?;

    let status = response.status().as_u16();
    let category = classify_status_code(status);

    match category {
        WebhookErrorCategory::Success => Ok(WebhookDeliveryResult::Success),
        WebhookErrorCategory::RateLimited => {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
                .map(|d| d.min(WEBHOOK_RETRY_AFTER_CAP));
            Ok(WebhookDeliveryResult::RateLimited { retry_after })
        }
        WebhookErrorCategory::Retryable => Ok(WebhookDeliveryResult::RetryableFailure {
            reason: format!("HTTP {}", status),
        }),
        WebhookErrorCategory::NonRetryable => Ok(WebhookDeliveryResult::NonRetryableFailure {
            reason: format!("HTTP {}", status),
        }),
    }
}

// =============================================================================
// Retry Loop (B8)
// =============================================================================

/// Abstraction over sleep for testability.
#[async_trait]
pub trait Sleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

/// Production sleeper using tokio::time::sleep.
pub struct TokioSleeper;

#[async_trait]
impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

/// Check whether sleeping for `delay` would exceed the max total duration.
pub(crate) fn would_exceed_max_duration(elapsed: Duration, delay: Duration) -> bool {
    elapsed.saturating_add(delay) > WEBHOOK_MAX_TOTAL_DURATION
}

/// Send a webhook with bounded retries.
///
/// - Max attempts: `WEBHOOK_MAX_ATTEMPTS` (3 total: initial + 2 retries)
/// - Retryable failures (5xx, network errors): exponential backoff with full jitter
/// - 429 RateLimited: uses Retry-After header when present, capped at 60s
/// - Non-retryable failures (4xx except 429): no retry, immediate failure
/// - Max total duration: `WEBHOOK_MAX_TOTAL_DURATION` (120s hard ceiling)
///
/// Bounded B8: retries are in-process sequential; no external queue or worker.
#[allow(dead_code)]
pub async fn send_webhook_with_retries(
    sender: &dyn WebhookSender,
    url: &str,
    payload: &WebhookPayload,
    headers: &WebhookHeaders,
    sleeper: &dyn Sleeper,
) -> Result<WebhookDeliveryResult, WebhookSendError> {
    let start = Instant::now();
    let mut attempt: u32 = 1;

    loop {
        let result = sender.send(url, payload, headers).await;

        match result {
            Ok(WebhookDeliveryResult::Success) => {
                return Ok(WebhookDeliveryResult::Success);
            }
            Ok(WebhookDeliveryResult::NonRetryableFailure { reason }) => {
                return Ok(WebhookDeliveryResult::NonRetryableFailure { reason });
            }
            Ok(WebhookDeliveryResult::RateLimited { retry_after }) => {
                if attempt >= WEBHOOK_MAX_ATTEMPTS {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: format!(
                            "rate limited after {} attempts, retry_after={:?}",
                            attempt, retry_after
                        ),
                    });
                }
                let delay = retry_after
                    .unwrap_or_else(|| compute_backoff_delay(attempt))
                    .min(WEBHOOK_RETRY_AFTER_CAP);
                if would_exceed_max_duration(start.elapsed(), delay) {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: "timeout: max_total_duration_exceeded".to_string(),
                    });
                }
                sleeper.sleep(delay).await;
                attempt += 1;
            }
            Ok(WebhookDeliveryResult::RetryableFailure { reason }) => {
                if attempt >= WEBHOOK_MAX_ATTEMPTS {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: format!("retry exhausted: {}", reason),
                    });
                }
                let delay = compute_backoff_delay(attempt);
                if would_exceed_max_duration(start.elapsed(), delay) {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: "timeout: max_total_duration_exceeded".to_string(),
                    });
                }
                sleeper.sleep(delay).await;
                attempt += 1;
            }
            Err(WebhookSendError::Network(reason)) => {
                if attempt >= WEBHOOK_MAX_ATTEMPTS {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: format!("network error after {} attempts: {}", attempt, reason),
                    });
                }
                let delay = compute_backoff_delay(attempt);
                if would_exceed_max_duration(start.elapsed(), delay) {
                    return Ok(WebhookDeliveryResult::NonRetryableFailure {
                        reason: "timeout: max_total_duration_exceeded".to_string(),
                    });
                }
                sleeper.sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

// =============================================================================
// Subscription Resolver (minimal scaffolding)
// =============================================================================

/// Minimal webhook subscription record (B1 schema mirror).
#[derive(Debug, Clone)]
pub struct WebhookSubscription {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub subscription_id: Uuid,
    pub webhook_url: String,
    pub downstream_system_id: Option<String>,
}

/// Resolver for webhook subscriptions by intent.
///
/// Bounded scaffolding: no DB-backed implementation yet.
/// Future Slice 3+ work will replace the in-memory/empty variants with a
/// real repository querying `webhook_subscriptions` (migration 018).
#[async_trait]
pub trait WebhookSubscriptionResolver: Send + Sync {
    async fn resolve_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscription>, IntentRebaseError>;
}

/// Empty resolver — always returns no subscriptions.
///
/// Used in production paths until a real subscription repository is wired.
pub struct EmptyWebhookSubscriptionResolver;

#[async_trait]
impl WebhookSubscriptionResolver for EmptyWebhookSubscriptionResolver {
    async fn resolve_by_intent(
        &self,
        _tenant_id: Uuid,
        _intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscription>, IntentRebaseError> {
        Ok(Vec::new())
    }
}

/// In-memory resolver for tests.
pub struct InMemoryWebhookSubscriptionResolver {
    subscriptions: std::sync::Mutex<Vec<WebhookSubscription>>,
}

impl InMemoryWebhookSubscriptionResolver {
    pub fn new() -> Self {
        Self {
            subscriptions: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn add(&self, sub: WebhookSubscription) {
        self.subscriptions.lock().unwrap().push(sub);
    }
}

impl Default for InMemoryWebhookSubscriptionResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebhookSubscriptionResolver for InMemoryWebhookSubscriptionResolver {
    async fn resolve_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscription>, IntentRebaseError> {
        let subs = self.subscriptions.lock().unwrap();
        Ok(subs
            .iter()
            .filter(|s| s.tenant_id == tenant_id && s.intent_id == intent_id)
            .cloned()
            .collect())
    }
}

/// SQL-backed resolver querying migration 018 `webhook_subscriptions`.
///
/// Bounded B6: uses `sqlx::query` (not `query!`) so no compile-time DB is required.
/// Tenant filtering is application-layer mandatory; RLS on the table is defense-in-depth.
pub struct SqlxWebhookSubscriptionResolver {
    pool: sqlx::PgPool,
}

impl SqlxWebhookSubscriptionResolver {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookSubscriptionResolver for SqlxWebhookSubscriptionResolver {
    async fn resolve_by_intent(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
    ) -> Result<Vec<WebhookSubscription>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, subscription_id, webhook_url, downstream_system_id
            FROM webhook_subscriptions
            WHERE tenant_id = $1 AND intent_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "Failed to resolve webhook subscriptions: {}",
                e
            ))
        })?;

        let mut subscriptions = Vec::with_capacity(rows.len());
        for row in rows {
            subscriptions.push(WebhookSubscription {
                id: row.try_get("id").map_err(|e| {
                    IntentRebaseError::StorageError(format!("Invalid id column: {}", e))
                })?,
                tenant_id: row.try_get("tenant_id").map_err(|e| {
                    IntentRebaseError::StorageError(format!("Invalid tenant_id column: {}", e))
                })?,
                intent_id: row.try_get("intent_id").map_err(|e| {
                    IntentRebaseError::StorageError(format!("Invalid intent_id column: {}", e))
                })?,
                subscription_id: row.try_get("subscription_id").map_err(|e| {
                    IntentRebaseError::StorageError(format!(
                        "Invalid subscription_id column: {}",
                        e
                    ))
                })?,
                webhook_url: row.try_get("webhook_url").map_err(|e| {
                    IntentRebaseError::StorageError(format!("Invalid webhook_url column: {}", e))
                })?,
                downstream_system_id: row.try_get("downstream_system_id").ok(),
            });
        }

        Ok(subscriptions)
    }
}

// =============================================================================
// Dispatcher
// =============================================================================

/// Dispatch webhooks for all propagation records of an intent.
///
/// Bounded B5 integration:
/// - Records delivery attempt via repository
/// - Builds payload/headers for each matching subscription
/// - Sends via the injected `WebhookSender`
/// - Records delivery outcome via repository
///
/// NOT wired into application flow by default — gated by `is_webhook_delivery_enabled`.
#[allow(dead_code)]
pub async fn dispatch_webhooks_for_intent(
    repo: &Arc<dyn PropagationRecordRepository>,
    sender: &dyn WebhookSender,
    resolver: &dyn WebhookSubscriptionResolver,
    tenant_id: Uuid,
    intent_id: Uuid,
    version: i32,
) {
    let records = match repo.list_by_intent(intent_id, tenant_id).await {
        Ok(recs) => recs,
        Err(e) => {
            tracing::warn!("Failed to list propagation records for dispatch: {}", e);
            return;
        }
    };

    let subscriptions = match resolver.resolve_by_intent(tenant_id, intent_id).await {
        Ok(subs) => subs,
        Err(e) => {
            tracing::warn!("Failed to resolve webhook subscriptions: {}", e);
            return;
        }
    };

    for sub in subscriptions {
        let record = records
            .iter()
            .find(|r| {
                r.downstream_system_id == sub.downstream_system_id.clone().unwrap_or_default()
            })
            .cloned();

        let record = match record {
            Some(r) => r,
            None => {
                tracing::debug!(
                    "No propagation record for downstream system {:?}, skipping",
                    sub.downstream_system_id
                );
                continue;
            }
        };

        let updated = match repo.record_delivery_attempt(record.id, tenant_id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "Failed to record delivery attempt for record {}: {}",
                    record.id,
                    e
                );
                continue;
            }
        };

        let payload = build_webhook_payload(WebhookPayloadInput {
            intent_id,
            tenant_id,
            version,
            version_hash: None,
            previous_version: None,
            delivery_id: Uuid::new_v4(),
            attempt_number: updated.delivery_attempt_count,
            subscription_id: sub.subscription_id,
        });
        let headers = WebhookHeaders::new(payload.delivery_id);

        let result = sender.send(&sub.webhook_url, &payload, &headers).await;

        match result {
            Ok(WebhookDeliveryResult::Success) => {
                if let Err(e) = repo
                    .record_delivery_outcome(
                        record.id,
                        tenant_id,
                        PropagationStatus::Acknowledged,
                        None,
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to record delivery outcome for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
            Ok(WebhookDeliveryResult::RetryableFailure { reason }) => {
                if let Err(e) = repo
                    .record_delivery_outcome(
                        record.id,
                        tenant_id,
                        PropagationStatus::Failed,
                        Some(sanitize_failure_reason(&reason)),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to record delivery outcome for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
            Ok(WebhookDeliveryResult::NonRetryableFailure { reason }) => {
                if let Err(e) = repo
                    .record_delivery_outcome(
                        record.id,
                        tenant_id,
                        PropagationStatus::Failed,
                        Some(sanitize_failure_reason(&reason)),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to record delivery outcome for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
            Ok(WebhookDeliveryResult::RateLimited { retry_after }) => {
                let reason = format!("rate limited, retry_after={:?}", retry_after);
                if let Err(e) = repo
                    .record_delivery_outcome(
                        record.id,
                        tenant_id,
                        PropagationStatus::Failed,
                        Some(sanitize_failure_reason(&reason)),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to record delivery outcome for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
            Err(WebhookSendError::Network(reason)) => {
                if let Err(e) = repo
                    .record_delivery_outcome(
                        record.id,
                        tenant_id,
                        PropagationStatus::Failed,
                        Some(sanitize_failure_reason(&reason)),
                    )
                    .await
                {
                    tracing::warn!(
                        "Failed to record delivery outcome for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
        }
    }
}
