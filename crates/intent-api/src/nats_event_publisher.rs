//! NATS event publisher for Phase 2b bounded core publisher slice
//!
//! ## Design Goals
//!
//! - **Bounded**: Only publishes events that are already persisted to audit storage.
//!   Audit persistence is the source of truth; publishing is best-effort notification.
//! - **Fail-open**: Publishing errors are logged but don't fail the operation.
//! - **Core publish only**: Uses NATS core publish (fire-and-forget, at-most-once delivery).
//!   JetStream streams/consumers/DLQ are Phase 3 scope.
//! - **W3C trace-context injection**: Injects traceparent/tracestate headers when
//!   trace context is available from the current span.
//! - **Bounded timeouts**: 2s connect timeout, 1s publish timeout.
//! - **Optional retry**: One retry with exponential backoff on publish failure.
//!
//! ## Configuration
//!
//! - `NATS_URL`: NATS server URL (e.g., `nats://localhost:4222`). If not set,
//!   the publisher logs a warning and operates in no-op mode.
//!
//! ## What is NOT implemented (Phase 3 scope)
//!
//! - JetStream stream creation and configuration
//! - NATS consumers with real subscription management
//! - Dead-letter queue (DLQ) for failed event processing
//! - Consumer groups and parallel processing
//! - Production durability guarantees
//! - Live NATS server integration tests (bounded unit tests exist; live integration tests
//!   require a running NATS server and are Phase 3 scope)
//! - True connection liveness monitoring (is_ready is a configuration check only)

use async_nats::Client;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};

use intent_rebase_types::{EventPublisher, EventSubject, PublishResult, TraceContext};

/// Phase 2b: NATS event publisher with W3C trace-context injection.
///
/// This publisher uses NATS core publish (fire-and-forget, at-most-once delivery).
/// JetStream streams/consumers/DLQ are Phase 3 scope.
///
/// ## Bounded Behavior
///
/// - Connection is established lazily on first publish attempt
/// - Failed publishes return `PublishResult::Skipped` (fail-open)
/// - Trace headers are injected when trace context is available
///
/// ## Configuration
///
/// - `NATS_URL` env var: NATS server URL (required for connection)
/// - Connect timeout: 2 seconds
/// - Publish timeout: 1 second
/// - One retry with exponential backoff (100ms base, 500ms max)
#[derive(Debug)]
pub struct NatsEventPublisher {
    /// NATS client (lazily initialized)
    client: Arc<RwLock<Option<Client>>>,
    /// Connect timeout
    connect_timeout: Duration,
    /// Publish timeout per message
    publish_timeout: Duration,
    /// Base backoff duration for retry
    base_backoff: Duration,
    /// Max backoff duration for retry
    max_backoff: Duration,
}

impl NatsEventPublisher {
    /// Create a new NatsEventPublisher with default timeouts.
    ///
    /// Default timeouts:
    /// - Connect: 2 seconds
    /// - Publish: 1 second
    /// - Retry backoff: 100ms base, 500ms max
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            connect_timeout: Duration::from_secs(2),
            publish_timeout: Duration::from_secs(1),
            base_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(500),
        }
    }

    /// Create a NatsEventPublisher with custom timeouts.
    #[allow(dead_code)]
    pub fn with_timeouts(
        connect_timeout: Duration,
        publish_timeout: Duration,
        base_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            connect_timeout,
            publish_timeout,
            base_backoff,
            max_backoff,
        }
    }

    /// Get the NATS URL from environment.
    fn get_nats_url() -> Option<String> {
        std::env::var("NATS_URL").ok().filter(|s| !s.is_empty())
    }

    /// Connect to NATS with timeout.
    async fn connect(&self) -> Result<Client, String> {
        let url = match Self::get_nats_url() {
            Some(url) => url,
            None => {
                tracing::warn!("NATS_URL not set — NatsEventPublisher operating in no-op mode");
                return Err("NATS_URL not configured".to_string());
            }
        };

        tracing::debug!(
            "Connecting to NATS at {} (timeout: {:?})",
            url,
            self.connect_timeout
        );

        let result = timeout(self.connect_timeout, async_nats::connect(&url)).await;

        match result {
            Ok(Ok(client)) => Ok(client),
            Ok(Err(e)) => {
                tracing::warn!("NATS connection failed: {:?}", e);
                Err(format!("NATS connection failed: {}", e))
            }
            Err(_) => {
                tracing::warn!("NATS connection timed out after {:?}", self.connect_timeout);
                Err(format!(
                    "NATS connection timed out after {:?}",
                    self.connect_timeout
                ))
            }
        }
    }

    /// Get or create a NATS client connection.
    async fn get_client(&self) -> Result<Client, String> {
        // Fast path: check if already connected
        {
            let client_guard = self.client.read().await;
            if let Some(ref client) = *client_guard {
                return Ok(client.clone());
            }
        }

        // Slow path: connect
        let client = self.connect().await?;

        // Store the client
        {
            let mut client_guard = self.client.write().await;
            *client_guard = Some(client.clone());
        }

        Ok(client)
    }

    /// Build W3C traceparent header value from trace context.
    ///
    /// Format: `00-{trace_id}-{span_id}-{trace_flags}`
    /// trace_flags: "01" = sampled, "00" = not sampled
    fn build_traceparent(trace_id: &str, span_id: &str, sampled: bool) -> String {
        let flags = if sampled { "01" } else { "00" };
        format!("00-{}-{}-{}", trace_id, span_id, flags)
    }

    /// Publish with retry using exponential backoff.
    async fn publish_with_retry(
        &self,
        client: &Client,
        subject: &str,
        payload: Vec<u8>,
        headers: async_nats::HeaderMap,
    ) -> Result<(), String> {
        let mut backoff = self.base_backoff;
        // Convert subject to String to avoid lifetime issues
        let subject_string = subject.to_string();

        // First attempt
        let result = timeout(
            self.publish_timeout,
            client.publish_with_headers(
                subject_string.clone(),
                headers.clone(),
                payload.clone().into(),
            ),
        )
        .await;

        match result {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => {
                tracing::warn!("NATS publish failed (first attempt): {:?}", e);
            }
            Err(_) => {
                tracing::warn!("NATS publish timed out (first attempt)");
            }
        }

        // Retry with backoff
        sleep(backoff).await;
        backoff = backoff.saturating_mul(2).min(self.max_backoff);

        tracing::debug!("Retrying NATS publish (backoff: {:?})", backoff);

        let result = timeout(
            self.publish_timeout,
            client.publish_with_headers(subject_string.clone(), headers, payload.into()),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                tracing::debug!("NATS publish succeeded on retry");
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::warn!("NATS publish failed (retry): {:?}", e);
                Err(format!("NATS publish failed: {}", e))
            }
            Err(_) => {
                tracing::warn!("NATS publish timed out (retry)");
                Err("NATS publish timed out".to_string())
            }
        }
    }
}

impl Default for NatsEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(
        &self,
        subject: &EventSubject,
        payload: &serde_json::Value,
        trace_context: TraceContext,
    ) -> PublishResult {
        // Get or create connection
        let client = match self.get_client().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "NatsEventPublisher: failed to connect to NATS: {} — skipping publish to '{}'",
                    e,
                    subject.subject
                );
                return PublishResult::Skipped {
                    reason: format!("NATS connection failed: {}", e),
                };
            }
        };

        // Serialize payload
        let payload_bytes = match serde_json::to_vec(payload) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!(
                    "NatsEventPublisher: failed to serialize payload for '{}': {}",
                    subject.subject,
                    e
                );
                return PublishResult::Skipped {
                    reason: format!("payload serialization failed: {}", e),
                };
            }
        };

        // Build headers with W3C trace context if available
        let mut headers = async_nats::HeaderMap::new();

        if trace_context.is_some() {
            // traceparent header
            if let (Some(ref trace_id), Some(ref span_id)) =
                (&trace_context.trace_id, &trace_context.span_id)
            {
                // Phase 2b limitation: TraceContext lacks a `sampled` field, so we cannot
                // reflect the actual sampling decision. We default to sampled=true (trace_flags=01)
                // for all published events. This is a known Phase 2b gap — proper sampling
                // propagation requires TraceContext to carry a sampled flag (future work).
                let traceparent = Self::build_traceparent(trace_id, span_id, true);
                headers.insert("traceparent", traceparent.as_str());
                tracing::debug!(
                    "NatsEventPublisher: injected traceparent for subject '{}': {}",
                    subject.subject,
                    traceparent
                );
            }

            // tracestate header (if present in context, but TraceContext doesn't carry it)
            // Note: Full tracestate propagation is future scope
        } else {
            tracing::debug!(
                "NatsEventPublisher: no trace context for subject '{}' — publishing without trace headers",
                subject.subject
            );
        }

        // Publish with retry
        match self
            .publish_with_retry(&client, &subject.subject, payload_bytes, headers)
            .await
        {
            Ok(()) => {
                tracing::debug!(
                    "NatsEventPublisher: published event to '{}' (schema={})",
                    subject.subject,
                    subject.schema_version
                );
                // Note: We don't have sequence numbers from core NATS publish
                // Sequence tracking requires JetStream (Phase 3)
                PublishResult::Published {
                    subject: subject.subject.clone(),
                    sequence: 0, // Core publish doesn't provide sequence
                }
            }
            Err(e) => {
                tracing::warn!(
                    "NatsEventPublisher: publish failed for '{}': {} — skipping",
                    subject.subject,
                    e
                );
                PublishResult::Skipped {
                    reason: format!("NATS publish failed: {}", e),
                }
            }
        }
    }

    /// Check if the publisher is ready to publish.
    ///
    /// Returns `true` if `NATS_URL` is configured, `false` otherwise.
    ///
    /// **Semantics (Phase 2b bounded):**
    /// - This is a **configuration check**, not a connection liveness check.
    /// - Returns `true` when `NATS_URL` is set to a non-empty value.
    /// - Does **NOT** verify that a NATS server is reachable or that a connection
    ///   has been established. The actual connection state is ephemeral — the client
    ///   is lazily initialized on first publish and may drop at any time.
    /// - This is acceptable for Phase 2b fail-open design: publishing errors are
    ///   logged and return `PublishResult::Skipped`, so readiness semantics don't
    ///   affect operation correctness.
    ///
    /// **Future (Phase 3):** A true liveness check (e.g., async `is_connected()`)
    /// would require NATS connection health monitoring, which is Phase 3 scope
    /// (JetStream connection management).
    fn is_ready(&self) -> bool {
        Self::get_nats_url().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_traceparent() {
        let tp = NatsEventPublisher::build_traceparent(
            "0af7651916cd43dd8448eb211c80319c",
            "b7ad6b7169203331",
            true,
        );
        assert_eq!(
            tp,
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );

        let tp_unsampled = NatsEventPublisher::build_traceparent(
            "0af7651916cd43dd8448eb211c80319c",
            "b7ad6b7169203331",
            false,
        );
        assert_eq!(
            tp_unsampled,
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-00"
        );
    }

    #[tokio::test]
    async fn test_nats_publisher_no_url() {
        // Use temp_env for deterministic parallel test isolation
        // Set env var synchronously before the async block, then test
        let original = std::env::var("NATS_URL").ok();
        std::env::remove_var("NATS_URL");

        let publisher = NatsEventPublisher::new();
        let subject = EventSubject::from_audit_event(uuid::Uuid::new_v4(), "RebaseApplied");
        let payload = serde_json::json!({ "test": true });

        let result = publisher
            .publish(&subject, &payload, TraceContext::default())
            .await;

        // Restore original
        match original {
            Some(v) => std::env::set_var("NATS_URL", v),
            None => std::env::remove_var("NATS_URL"),
        }

        match result {
            PublishResult::Skipped { reason } => {
                assert!(
                    reason.contains("NATS_URL not configured")
                        || reason.contains("connection failed")
                );
            }
            _ => panic!("Expected Skipped result"),
        }
    }

    #[tokio::test]
    async fn test_nats_publisher_is_ready_no_url() {
        // Use temp_env for deterministic parallel test isolation - empty string
        temp_env::with_var("NATS_URL", Some(""), || {
            let publisher = NatsEventPublisher::new();
            assert!(
                !publisher.is_ready(),
                "Publisher should not be ready with empty NATS_URL"
            );
        });
    }

    #[tokio::test]
    async fn test_nats_publisher_is_ready_with_url() {
        // Use temp_env for deterministic parallel test isolation - valid URL
        temp_env::with_var("NATS_URL", Some("nats://localhost:4222"), || {
            let publisher = NatsEventPublisher::new();
            // is_ready checks if URL is set, not if connection succeeds
            assert!(publisher.is_ready());
        });
    }
}
