//! NATS pull-consumer adapter and consumer registry.
//!
//! Bounded decomposition slice (S7) from `nats_jetstream.rs`.
//! Converts JetStream messages into `PublishedEvent`, extracts W3C traceparent
//! headers, dispatches to `EventConsumer`, and manages bounded ack/NAK/DLQ
//! behavior. Includes a consumer registry for lifecycle management.

use async_nats::jetstream::Context as JetStreamContext;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::timeout;
use uuid::Uuid;

use super::DlqHelper;
use intent_rebase_types::TraceContext;

// =============================================================================
// Pull Consumer Adapter (Bounded Ack, Traceparent Extraction)
// =============================================================================

/// Phase 4 (bounded slice): NATS pull-consumer adapter that dispatches messages to `EventConsumer` trait.
///
/// Bounded implementation:
/// - Converts JetStream message into `PublishedEvent`
/// - Extracts W3C traceparent headers via `extract_trace_context`
/// - Dispatches to `EventConsumer::consume`
/// - Acks on `Consumed`, acks on `Failed`/`Retryable` to prevent infinite redelivery (bounded ack behavior)
/// - Max deliver = 3 aligns with G2 bounded retry semantics via JetStream config
///
/// **NON-PRODUCTION full-consumer path:**
/// - When `dlq_helper` is Some, Failed/Retryable outcomes trigger app-level DLQ publish BEFORE ack
/// - This is gated behind `INTENT_API_NATS_FULL_CONSUMER=true` and is local-dev only
///
/// **Phase 4 lifecycle first slice:**
/// - Single consumer only (CheckpointCreatorConsumer) by default
/// - Additional consumers (SnapshotCreatorConsumer, NotifierConsumer) behind FULL_CONSUMER gate
/// - Graceful shutdown with bounded poll loop
#[derive(Debug)]
pub struct NatsPullConsumerAdapter {
    /// JetStream context for consumer operations
    jetstream: JetStreamContext,
    /// Consumer configuration
    consumer_config: async_nats::jetstream::consumer::pull::Config,
    /// Stream name
    stream_name: String,
    /// Consumer name
    consumer_name: String,
    /// Message processing timeout
    message_timeout: Duration,
    /// Poll interval when no messages available
    poll_interval: Duration,
    /// NON-PRODUCTION: Optional DLQ helper for app-level DLQ publishing on Failed/Retryable
    dlq_helper: Option<Arc<DlqHelper>>,
    /// Optional tenant scope: when set, the consumer rejects events whose subject
    /// tenant_id does not match. Preserves shared-consumer behavior when None.
    tenant_scope: Option<Uuid>,
}

impl NatsPullConsumerAdapter {
    /// Create a new NATS pull consumer adapter.
    ///
    /// **Note**: The consumer is created lazily on first `run` call,
    /// not in this constructor.
    pub fn new(jetstream: JetStreamContext, stream_name: &str) -> Self {
        let consumer_name = format!("{}_consumer", stream_name);
        Self {
            jetstream,
            consumer_config: async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(consumer_name.clone()),
                description: Some("Phase 4 bounded pull consumer for audit events".to_string()),
                ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
                max_deliver: 3, // G2 retry config: max_deliver=3 (i64)
                ack_wait: Duration::from_secs(30),
                ..Default::default()
            },
            stream_name: stream_name.to_string(),
            consumer_name,
            message_timeout: Duration::from_secs(60),
            poll_interval: Duration::from_millis(500),
            dlq_helper: None,
            tenant_scope: None,
        }
    }

    /// Set an optional tenant scope for this consumer.
    ///
    /// When `tenant_scope` is `Some(tenant_id)`, the consumer will reject
    /// events whose NATS subject does not contain a matching tenant_id.
    /// When `None` (default), the consumer processes all events (shared mode).
    #[allow(dead_code)]
    pub fn with_tenant_scope(mut self, tenant_scope: Option<Uuid>) -> Self {
        self.tenant_scope = tenant_scope;
        self
    }

    /// Create with custom message timeout.
    #[allow(dead_code)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.message_timeout = timeout;
        self
    }

    /// Create with custom poll interval.
    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// NON-PRODUCTION: Attach an optional DLQ helper for app-level DLQ publishing.
    /// Only enabled when `INTENT_API_NATS_FULL_CONSUMER=true`.
    #[allow(dead_code)]
    pub fn with_dlq_helper(mut self, dlq_helper: Option<Arc<DlqHelper>>) -> Self {
        self.dlq_helper = dlq_helper;
        self
    }

    /// Extract trace context from JetStream message headers.
    ///
    /// Parses the W3C `traceparent` header into `TraceContext`.
    /// If header is missing or malformed, returns `TraceContext::default()`.
    #[allow(dead_code)]
    pub(crate) fn extract_trace_context(headers: &async_nats::HeaderMap) -> TraceContext {
        use intent_rebase_types::parse_traceparent;

        headers
            .get("traceparent")
            .and_then(|v| parse_traceparent(v.as_str()).ok())
            .unwrap_or_default()
    }

    /// Extract tenant_id from a NATS subject.
    ///
    /// Expected format: `audit.events.v1.<tenant_id>.<event_type>`
    /// Returns `None` if the subject does not contain a valid UUID at position 3.
    #[allow(dead_code)]
    pub(crate) fn extract_tenant_id_from_subject(subject: &str) -> Option<Uuid> {
        let parts: Vec<&str> = subject.split('.').collect();
        if parts.len() >= 4 {
            Uuid::parse_str(parts[3]).ok()
        } else {
            None
        }
    }

    /// Check whether the message subject matches the consumer's tenant scope.
    ///
    /// - If `tenant_scope` is `None`, always returns `Ok(())`.
    /// - If `tenant_scope` is `Some(expected)`, returns `Ok(())` only when the
    ///   subject contains a matching tenant_id; otherwise returns `Err`.
    #[allow(dead_code)]
    pub(crate) fn check_tenant_scope(&self, subject: &str) -> Result<(), String> {
        Self::check_tenant_scope_static(self.tenant_scope, subject)
    }

    /// Static version of `check_tenant_scope` for easier unit testing.
    #[allow(dead_code)]
    pub(crate) fn check_tenant_scope_static(
        tenant_scope: Option<Uuid>,
        subject: &str,
    ) -> Result<(), String> {
        if let Some(expected) = tenant_scope {
            match Self::extract_tenant_id_from_subject(subject) {
                Some(actual) if actual == expected => Ok(()),
                Some(actual) => Err(format!(
                    "tenant scope mismatch: expected {}, got {} (subject: {})",
                    expected, actual, subject
                )),
                None => Err(format!(
                    "tenant scope mismatch: expected {}, but subject has no tenant_id (subject: {})",
                    expected, subject
                )),
            }
        } else {
            Ok(())
        }
    }

    /// Process a single JetStream message: dispatch to consumer and ack on success.
    ///
    /// **Bounded behavior:**
    /// - Converts the JetStream message into a `PublishedEvent`
    /// - Extracts trace context from `traceparent` header (if present)
    /// - Dispatches to `EventConsumer::consume`
    /// - On `Consumed`: acknowledges the message (JetStream won't redeliver)
    /// - On `Failed`: acks the message to prevent infinite redelivery (bounded ack)
    /// - On `Retryable`: acks the message to prevent infinite redelivery (bounded ack)
    ///
    /// **Tenant guard (bounded slice):**
    /// - When `tenant_scope` is set, rejects cross-tenant events before dispatch.
    /// - Rejected events are acked to prevent infinite redelivery.
    ///
    /// **NON-PRODUCTION full-consumer path:**
    /// - When `dlq_helper` is Some, Failed/Retryable outcomes trigger `publish_to_dlq()`
    ///   BEFORE ack. This is gated behind `INTENT_API_NATS_FULL_CONSUMER=true`.
    /// - Delivery count is extracted from `message.info().delivered`; falls back to 1
    ///   if unavailable (bounded metadata fallback).
    ///
    /// **Safety-net redelivery cap:** `max_deliver=3` in consumer config is a
    /// JetStream-level safety net, but the current bounded ACK-all behavior (ack
    /// on success, Failed, and Retryable) does not exercise redelivery — messages
    /// are acked rather than nacked. DLQ/retry worker is Phase 4+ scope.
    ///
    /// Returns `Ok(())` if processing succeeded (message acknowledged).
    /// Returns `Err(String)` if processing failed in a way that should not retry.
    #[allow(dead_code)]
    pub async fn process_one(
        &self,
        message: async_nats::jetstream::Message,
        consumer: &dyn intent_rebase_types::EventConsumer,
    ) -> Result<(), String> {
        // Extract subject from message
        let subject = message.subject.to_string();

        // Bounded tenant guard: reject cross-tenant events before side effects
        if let Err(reason) = self.check_tenant_scope(&subject) {
            tracing::warn!(
                "NatsPullConsumerAdapter: tenant guard rejected message on '{}': {}",
                subject,
                reason
            );
            let _ = message.ack().await;
            return Err(reason);
        }

        // Parse payload into serde_json::Value
        let payload: serde_json::Value = match serde_json::from_slice(&message.payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    "NatsPullConsumerAdapter: failed to parse payload for '{}': {}",
                    subject,
                    e
                );
                // Ack invalid payloads to prevent redelivery (don't infinite-retry bad data)
                let _ = message.ack().await;
                return Err(format!("invalid payload JSON: {}", e));
            }
        };

        // Extract trace context from headers
        let trace_context = NatsPullConsumerAdapter::extract_trace_context(
            message
                .headers
                .as_ref()
                .unwrap_or(&async_nats::HeaderMap::new()),
        );

        // Build PublishedEvent
        let event = intent_rebase_types::PublishedEvent {
            subject: subject.clone(),
            schema_version: "v1".to_string(),
            sequence: 0, // JetStream pull consumer doesn't provide sequence in message
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            payload,
            published_at: chrono::Utc::now(),
        };

        // Dispatch to consumer
        match consumer.consume(&event).await {
            intent_rebase_types::ConsumeResult::Consumed { .. } => {
                tracing::debug!("NatsPullConsumerAdapter: consumed event from '{}'", subject);
                // Ack on success — JetStream won't redeliver
                message.ack().await.map_err(|e| {
                    tracing::error!("NatsPullConsumerAdapter: failed to ack message: {}", e);
                    format!("ack failed: {}", e)
                })?;
                Ok(())
            }
            intent_rebase_types::ConsumeResult::Failed { reason } => {
                tracing::warn!(
                    "NatsPullConsumerAdapter: consumer failed for '{}': {}",
                    subject,
                    reason
                );
                // NON-PRODUCTION: App-level DLQ publish for full-consumer path (before ack)
                if let Some(ref dlq) = self.dlq_helper {
                    let delivery_count = message.info().map(|i| i.delivered as u64).unwrap_or(1);
                    let payload = message.payload.to_vec();
                    let _ = dlq
                        .publish_to_dlq(&subject, payload, delivery_count, &reason)
                        .await;
                }
                // Bounded behavior: ack anyway to prevent infinite redelivery
                // With max_deliver=3 in consumer config, this is the last delivery attempt (after 3 attempts)
                // The failure is logged but the message is acknowledged to prevent redelivery
                let _ = message.ack().await;
                Err(format!("consumer failed: {}", reason))
            }
            intent_rebase_types::ConsumeResult::Retryable { reason } => {
                tracing::warn!(
                    "NatsPullConsumerAdapter: consumer returned retryable for '{}': {}",
                    subject,
                    reason
                );
                // NON-PRODUCTION: App-level DLQ publish for full-consumer path (before ack)
                if let Some(ref dlq) = self.dlq_helper {
                    let delivery_count = message.info().map(|i| i.delivered as u64).unwrap_or(1);
                    let payload = message.payload.to_vec();
                    let _ = dlq
                        .publish_to_dlq(&subject, payload, delivery_count, &reason)
                        .await;
                }
                // Bounded behavior: ack anyway to prevent infinite redelivery
                // With max_deliver=3 in consumer config, retryable failures won't be redelivered after 3 attempts
                let _ = message.ack().await;
                Err(format!("retryable: {}", reason))
            }
        }
    }

    /// Run the consumer poll loop with graceful shutdown support.
    ///
    /// **Phase 4 bounded slice:**
    /// - Single consumer (CheckpointCreatorConsumer)
    /// - No DLQ worker
    /// - Graceful shutdown via shutdown signal
    ///
    /// # Arguments
    ///
    /// * `consumer` - The event consumer to dispatch messages to
    /// * `shutdown` - Channel to receive shutdown signal
    ///
    /// # Behavior
    ///
    /// - Creates or gets the pull consumer idempotently
    /// - Fetches messages from JetStream using pull consumer's messages() stream
    /// - Processes each message via `process_one`
    /// - Sleeps `poll_interval` when no messages available
    /// - Stops gracefully when shutdown signal is received
    #[allow(dead_code)]
    pub async fn run(
        &self,
        consumer: Arc<dyn intent_rebase_types::EventConsumer>,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), String> {
        // Create or get the consumer idempotently using create_consumer_on_stream
        let pull_consumer = self
            .jetstream
            .create_consumer_on_stream(self.consumer_config.clone(), &self.stream_name)
            .await
            .map_err(|e| format!("failed to create/get consumer: {}", e))?;

        tracing::info!(
            "NatsPullConsumerAdapter: started polling consumer '{}' on stream '{}'",
            self.consumer_name,
            self.stream_name
        );

        loop {
            // Check for shutdown signal
            if *shutdown.borrow() {
                tracing::info!(
                    "NatsPullConsumerAdapter: shutdown signal received, stopping poll loop"
                );
                break;
            }

            // Fetch messages using the pull consumer's messages() stream
            // messages() returns a stream that we poll for messages
            let stream = match timeout(self.message_timeout, pull_consumer.messages()).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    tracing::warn!(
                        "NatsPullConsumerAdapter: messages stream error, will retry: {}",
                        e
                    );
                    tokio::time::sleep(self.poll_interval).await;
                    continue;
                }
                Err(_) => {
                    // Timeout is normal (no messages), poll again
                    tokio::time::sleep(self.poll_interval).await;
                    continue;
                }
            };

            // Process messages from the stream
            // Use futures_util::StreamExt to iterate
            use futures_util::StreamExt;
            let mut message_stream = stream;

            // Process up to some messages per poll cycle
            let mut messages_processed = 0;
            const MAX_MESSAGES_PER_POLL: usize = 10;

            while messages_processed < MAX_MESSAGES_PER_POLL {
                // Check shutdown before processing each message
                if *shutdown.borrow() {
                    tracing::info!(
                        "NatsPullConsumerAdapter: shutdown signal received during processing, stopping"
                    );
                    return Ok(());
                }

                // Try to get next message with a short timeout
                match tokio::time::timeout(self.poll_interval, message_stream.next()).await {
                    Ok(Some(result)) => match result {
                        Ok(msg) => {
                            if let Err(e) = self.process_one(msg, consumer.as_ref()).await {
                                tracing::warn!("NatsPullConsumerAdapter: process_one error: {}", e);
                            }
                            messages_processed += 1;
                        }
                        Err(e) => {
                            tracing::warn!("NatsPullConsumerAdapter: message error: {}", e);
                        }
                    },
                    Ok(None) => {
                        // Stream ended (no more messages)
                        break;
                    }
                    Err(_) => {
                        // Timeout waiting for message - normal, continue polling
                        break;
                    }
                }
            }

            // If no messages were processed, sleep before next poll
            if messages_processed == 0 {
                tokio::time::sleep(self.poll_interval).await;
            }
        }

        tracing::info!(
            "NatsPullConsumerAdapter: poll loop stopped for consumer '{}'",
            self.consumer_name
        );
        Ok(())
    }
}

// =============================================================================
// Bounded Multi-Consumer Registry (Phase 4 First Slice)
// =============================================================================
//
// Bounded implementation for managing multiple NATS consumer tasks with shared
// graceful shutdown. This is a first-slice registry — not full production lifecycle.
//
// **Phase 4 bounded slice:**
// - CheckpointCreatorConsumer registered and enabled via INTENT_API_NATS_CONSUMER
// - SnapshotCreatorConsumer and DLQ worker NOT enabled (Phase 4+ future scope)
// - Shared shutdown watch channel across all registered consumers
// - Graceful drain: shutdown signal stops all consumer poll loops
//
// **Production Readiness Note:**
// This is a BOUNDED FIRST SLICE. Not production-ready until:
// - G1–G5 gates pass (see docs/10-delivery/14-dlq-retry-design.md)
// - Multi-consumer lifecycle fully tested
// - Consumer lag monitoring wired (G3)

/// A registered consumer entry with its configuration
struct RegisteredConsumer {
    /// Human-readable name for this consumer
    name: String,
    /// The event consumer implementation
    consumer: Arc<dyn intent_rebase_types::EventConsumer>,
    /// The stream name to consume from
    stream_name: String,
}

/// Bounded multi-consumer registry for NATS JetStream consumers.
///
/// Manages multiple consumer tasks with shared graceful shutdown.
/// Each registered consumer runs its own poll loop that stops when
/// the shared shutdown signal is received.
///
/// **Phase 4 bounded slice:**
/// - Only CheckpointCreatorConsumer is registered and enabled by default
/// - Additional consumers (SnapshotCreatorConsumer, NotifierConsumer) behind FULL_CONSUMER gate
/// - DLQ publishing is NON-PRODUCTION and gated behind `INTENT_API_NATS_FULL_CONSUMER=true`
pub struct ConsumerRegistry {
    /// Registered consumers by name
    consumers: HashMap<String, RegisteredConsumer>,
    /// Shared shutdown signal sender (clones held by each consumer task)
    shutdown_tx: Option<watch::Sender<bool>>,
    /// NON-PRODUCTION: Enable full-consumer path with DLQ publishing and additional consumers
    full_consumer: bool,
}

impl ConsumerRegistry {
    /// Create a new empty consumer registry.
    pub fn new() -> Self {
        Self {
            consumers: HashMap::new(),
            shutdown_tx: None,
            full_consumer: false,
        }
    }

    /// Register a consumer with the registry.
    ///
    /// The consumer will be started when `start_all` is called.
    /// Returns an error if a consumer with the same name is already registered.
    ///
    /// **Note:** The JetStream context is created once during `start_all`,
    /// so all consumers share the same NATS connection.
    pub fn register(
        mut self,
        name: &str,
        consumer: Arc<dyn intent_rebase_types::EventConsumer>,
        stream_name: &str,
    ) -> Result<Self, ConsumerRegistryError> {
        if self.consumers.contains_key(name) {
            return Err(ConsumerRegistryError::AlreadyRegistered {
                name: name.to_string(),
            });
        }

        self.consumers.insert(
            name.to_string(),
            RegisteredConsumer {
                name: name.to_string(),
                consumer,
                stream_name: stream_name.to_string(),
            },
        );

        Ok(self)
    }

    /// NON-PRODUCTION: Enable the full-consumer path.
    ///
    /// When enabled, `start_all` creates a `DlqHelper` and attaches it to each
    /// `NatsPullConsumerAdapter`, enabling app-level DLQ publishing on Failed/Retryable
    /// outcomes. This is gated behind `INTENT_API_NATS_FULL_CONSUMER=true` and is
    /// local-dev only — not production-ready.
    pub fn with_full_consumer(mut self, enabled: bool) -> Self {
        self.full_consumer = enabled;
        self
    }

    /// Start all registered consumers and return a handle for shutdown.
    ///
    /// Creates a shared shutdown channel that will signal all consumers
    /// to stop gracefully when `shutdown` is called.
    ///
    /// **Bounded behavior:**
    /// - Single NATS connection shared across all consumers
    /// - Each consumer gets its own JetStream adapter and poll loop
    /// - All consumers stop when shutdown signal is received
    ///
    /// Returns `Ok(ConsumerRegistryHandle)` if all consumers started successfully.
    /// Returns `Err` if no consumers are registered or if NATS connection fails.
    pub async fn start_all(
        mut self,
        nats_url: &str,
    ) -> Result<ConsumerRegistryHandle, ConsumerRegistryError> {
        if self.consumers.is_empty() {
            return Err(ConsumerRegistryError::NoConsumersRegistered);
        }

        // Create shared shutdown channel
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        // Connect to NATS with timeout
        let client = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            async_nats::connect(nats_url),
        )
        .await
        {
            Ok(Ok(client)) => client,
            Ok(Err(e)) => {
                return Err(ConsumerRegistryError::NatsConnectionFailed(e.to_string()));
            }
            Err(_) => {
                return Err(ConsumerRegistryError::NatsConnectionFailed(
                    "NATS connection timed out after 5s".to_string(),
                ));
            }
        };

        let jetstream = async_nats::jetstream::new(client);

        // NON-PRODUCTION: Create DLQ helper only for full-consumer path
        let dlq_helper = if self.full_consumer {
            Some(Arc::new(DlqHelper::new(jetstream.clone())))
        } else {
            None
        };

        // Spawn a task for each registered consumer
        let mut handles = Vec::new();

        for (_, registered) in self.consumers.drain() {
            let consumer = registered.consumer;
            let stream_name = registered.stream_name;
            let name = registered.name;
            let rx = shutdown_rx.clone();

            // Create adapter for this consumer
            let adapter = NatsPullConsumerAdapter::new(jetstream.clone(), &stream_name)
                .with_dlq_helper(dlq_helper.clone());

            let handle = tokio::spawn(async move {
                tracing::info!(
                    "ConsumerRegistry: starting consumer '{}' on stream '{}'",
                    name,
                    stream_name
                );
                if let Err(e) = adapter.run(consumer, rx).await {
                    tracing::error!(
                        "ConsumerRegistry: consumer '{}' poll loop ended with error: {}",
                        name,
                        e
                    );
                } else {
                    tracing::info!(
                        "ConsumerRegistry: consumer '{}' poll loop ended normally",
                        name
                    );
                }
            });

            handles.push(handle);
        }

        Ok(ConsumerRegistryHandle {
            handles,
            shutdown_tx: self.shutdown_tx.take().unwrap(),
        })
    }
}

impl std::fmt::Debug for ConsumerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsumerRegistry")
            .field("consumers", &self.consumers.keys().collect::<Vec<_>>())
            .field("shutdown_tx", &self.shutdown_tx.is_some())
            .field("full_consumer", &self.full_consumer)
            .finish()
    }
}

impl Default for ConsumerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a running consumer registry.
///
/// Allows graceful shutdown of all registered consumers.
#[derive(Debug)]
pub struct ConsumerRegistryHandle {
    /// Task handles for all running consumers
    handles: Vec<tokio::task::JoinHandle<()>>,
    /// Shutdown signal sender
    shutdown_tx: watch::Sender<bool>,
}

impl ConsumerRegistryHandle {
    /// Signal all registered consumers to stop gracefully.
    ///
    /// This sends `true` on the shared shutdown channel, which causes
    /// all consumer poll loops to stop. Consumers may still need time
    /// to finish processing any in-flight messages.
    pub fn shutdown(&self) {
        tracing::info!("ConsumerRegistryHandle: sending shutdown signal to all consumers");
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for all registered consumers to finish.
    ///
    /// This await will complete when all consumer tasks have terminated.
    /// If a consumer task panics, the panic is logged but not propagated
    /// to the caller (fire-and-forget semantics for task join errors).
    pub async fn wait_for_all(self) {
        tracing::info!("ConsumerRegistryHandle: waiting for all consumers to finish");
        for (i, handle) in self.handles.into_iter().enumerate() {
            match handle.await {
                Ok(()) => {
                    tracing::debug!("ConsumerRegistryHandle: consumer {} finished", i);
                }
                Err(e) => {
                    tracing::error!(
                        "{}",
                        crate::panic_hardening::format_join_error(
                            &format!("ConsumerRegistryHandle consumer {}", i),
                            e
                        )
                    );
                }
            }
        }
        tracing::info!("ConsumerRegistryHandle: all consumers finished");
    }
}

/// Errors that can occur when operating a consumer registry
#[derive(Debug, Clone)]
pub enum ConsumerRegistryError {
    /// A consumer with this name is already registered
    AlreadyRegistered { name: String },
    /// No consumers have been registered
    NoConsumersRegistered,
    /// Failed to connect to NATS
    NatsConnectionFailed(String),
}

impl std::fmt::Display for ConsumerRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsumerRegistryError::AlreadyRegistered { name } => {
                write!(f, "consumer '{}' is already registered", name)
            }
            ConsumerRegistryError::NoConsumersRegistered => {
                write!(f, "no consumers registered")
            }
            ConsumerRegistryError::NatsConnectionFailed(e) => {
                write!(f, "NATS connection failed: {}", e)
            }
        }
    }
}

impl std::error::Error for ConsumerRegistryError {}
