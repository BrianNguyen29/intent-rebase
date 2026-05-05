//! Event publishing abstraction for Phase 2b bounded slice
//!
//! ## Design Goals
//!
//! - **Bounded**: Only publishes events that are already persisted to audit storage.
//!   Audit persistence is the source of truth; publishing is best-effort notification.
//! - **Fail-open**: Publishing errors are logged but don't fail the operation.
//! - **Testable**: In-memory mock publisher allows verification without external NATS.
//! - **Bounded consumers**: Phase 2b adds a minimal in-memory consumer abstraction for testing
//!   the event→checkpoint path. Full consumer infrastructure (NATS subscription, DLQ, startup wiring)
//!   is Phase 3.
//! - **Bounded trace continuity**: Phase 3 adds trace_id/span_id to `EventEnvelope` and
//!   `PublishedEvent` for in-process trace correlation. Cross-process propagation is future scope.
//! - **W3C trace-context injection into outbound NATS messages
//!
//! ## Subject Naming Convention (Phase 2b bounded slice)
//!
//! Subjects follow the pattern documented in ADR-04:
//! - `audit.events.v1.<tenant_id>.<event_type>` — audit events v1
//!
//! Versioning: v1 prefix in subject; full v2 migration path documented in Phase 3.
//!
//! ## Event Consumer Notes (Phase 2b bounded slice)
//!
//! Phase 2b adds a minimal in-memory consumer abstraction to demonstrate the event→checkpoint
//! path. This is bounded to in-memory consumers for testing only. The abstraction defines the
//! consumer contract, and an in-memory implementation proves the path works.
//!
//! **What is bounded (Phase 2b)**:
//! - `EventConsumer` trait: async consumer contract
//! - `InMemoryEventConsumer`: in-memory consumer buffer for testing
//! - `CheckpointCreatorConsumer` in intent-service: concrete consumer that creates checkpoints
//!
//! **What is NOT implemented (Phase 3)**:
//! - NATS-based consumers with real subscription management
//! - Dead-letter queue (DLQ) for failed event processing
//! - Full consumer startup wiring and lifecycle management
//! - Consumer groups and parallel processing

use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use super::TraceContext;

/// Phase 2b: Subject naming convention for published events.
///
/// Subjects follow: `audit.events.v1.<tenant_id>.<event_type>`
///
/// Versioning: v1 prefix — full v2 migration path is Phase 3.
#[derive(Debug, Clone)]
pub struct EventSubject {
    /// Full subject string (e.g., "audit.events.v1.<tenant_id>.RebaseApplied")
    pub subject: String,
    /// Schema version (v1 for Phase 2b)
    pub schema_version: &'static str,
    /// Event type used in subject construction
    pub event_type: String,
    /// Tenant ID used in subject construction
    pub tenant_id: Uuid,
}

impl EventSubject {
    /// Phase 2b: Construct a subject string from audit event components.
    ///
    /// Format: `audit.events.v1.<tenant_id>.<event_type>`
    ///
    /// Note: This is the bounded Phase 2b subject format. Full subject
    /// configuration (streams, retention) is Phase 3 NATS configuration.
    pub fn from_audit_event(tenant_id: Uuid, event_type: &str) -> Self {
        Self {
            subject: format!("audit.events.v1.{}.{}", tenant_id, event_type),
            schema_version: "v1",
            event_type: event_type.to_string(),
            tenant_id,
        }
    }
}

/// Payload envelope for published events.
///
/// Phase 2b: Wraps the audit event payload with metadata for tracing/replay.
/// Phase 3: Added trace_id/span_id for bounded trace continuity.
#[derive(Debug, Clone, Serialize)]
pub struct EventEnvelope<T: Serialize> {
    /// Subject this event was published to
    pub subject: String,
    /// Schema version for migration support
    pub schema_version: &'static str,
    /// Timestamp when event was published
    pub published_at: chrono::DateTime<chrono::Utc>,
    /// Sequence number for ordering (monotonic per subject)
    pub sequence: u64,
    /// Trace context for correlation (Phase 3 bounded trace continuity slice)
    pub trace_id: Option<String>,
    /// Span context for correlation (Phase 3 bounded trace continuity slice)
    pub span_id: Option<String>,
    /// The actual event payload
    pub payload: T,
}

impl<T: Serialize> EventEnvelope<T> {
    /// Create a new envelope (sequence is assigned by publisher)
    ///
    /// Phase 3 bounded trace continuity slice: accepts `TraceContext` to carry
    /// trace_id/span_id into the published envelope.
    pub fn new(
        subject: String,
        schema_version: &'static str,
        payload: T,
        trace_context: TraceContext,
    ) -> Self {
        Self {
            subject,
            schema_version,
            published_at: chrono::Utc::now(),
            sequence: 0, // Publisher assigns actual sequence
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            payload,
        }
    }
}

/// Result of a publish operation.
///
/// Phase 2b: Publishing is best-effort. Errors are logged but don't fail the operation.
#[derive(Debug)]
pub enum PublishResult {
    /// Event was published successfully
    Published { subject: String, sequence: u64 },
    /// Publishing failed but the operation continues (fail-open)
    Skipped { reason: String },
}

/// Event publisher trait for Phase 2b bounded slice.
///
/// Implementations:
/// - `NoOpEventPublisher` — no-op, used when event streaming is disabled
/// - `InMemoryEventPublisher` — mock publisher for tests
/// - `NatsEventPublisher` — real NATS JetStream publisher (Phase 3)
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish an event to the event stream.
    ///
    /// Phase 2b: This is fail-open. Errors are logged and `PublishResult::Skipped`
    /// is returned, but the caller continues normally.
    ///
    /// Phase 3 bounded trace continuity slice: `trace_context` is captured in the
    /// published event record for correlation. Pass `TraceContext::default()` if
    /// no trace context is available.
    ///
    /// Returns `PublishResult` indicating success or skip-with-reason.
    async fn publish(
        &self,
        subject: &EventSubject,
        payload: &serde_json::Value,
        trace_context: TraceContext,
    ) -> PublishResult;

    /// Check if the publisher is ready (connection established, etc.)
    ///
    /// Phase 2b: Returns `true` for `NoOpEventPublisher` and `InMemoryEventPublisher`.
    /// `NatsEventPublisher` would check NATS connection health.
    fn is_ready(&self) -> bool;
}

// =============================================================================
// No-Op Publisher (used when event streaming is disabled or unavailable)
// =============================================================================

/// Phase 2b: No-op event publisher for when event streaming is disabled.
///
/// This publisher does nothing — events are silently dropped.
/// Used when:
/// - Event streaming is intentionally disabled
/// - NATS is not configured
/// - Phase 3 NATS integration is not yet available
///
/// Behavior: `is_ready()` returns `true`, `publish()` always returns `Skipped`.
pub struct NoOpEventPublisher;

impl NoOpEventPublisher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher for NoOpEventPublisher {
    async fn publish(
        &self,
        subject: &EventSubject,
        _payload: &serde_json::Value,
        _trace_context: TraceContext,
    ) -> PublishResult {
        tracing::debug!(
            "NoOpEventPublisher: dropping event for subject '{}' (event streaming disabled)",
            subject.subject
        );
        PublishResult::Skipped {
            reason: "event streaming disabled".to_string(),
        }
    }

    fn is_ready(&self) -> bool {
        true
    }
}

// =============================================================================
// In-Memory Mock Publisher (for testing without external infrastructure)
// =============================================================================

/// Phase 2b: In-memory event publisher for testing.
///
/// This publisher stores events in memory, allowing tests to verify:
/// - Events were published to correct subjects
/// - Payload structure is correct
/// - Sequence numbers are monotonic
///
/// Does NOT require external NATS server.
///
/// Usage in tests:
/// ```ignore
/// let publisher = Arc::new(InMemoryEventPublisher::new());
/// // ... use publisher in test setup ...
/// let events = publisher.get_events_for_subject("audit.events.v1.*");
/// assert_eq!(events.len(), 1);
/// ```
#[derive(Debug)]
pub struct InMemoryEventPublisher {
    /// Stored events keyed by subject pattern (supports wildcards in lookup)
    events: tokio::sync::RwLock<std::collections::HashMap<String, Vec<PublishedEvent>>>,
    /// Sequence counter per subject prefix
    sequences: tokio::sync::RwLock<std::collections::HashMap<String, u64>>,
    /// Whether publish calls should fail (for testing error handling)
    fail_publish: std::sync::atomic::AtomicBool,
    /// Whether publisher reports ready
    ready: std::sync::atomic::AtomicBool,
}

/// A published event record for test verification
///
/// Phase 3 bounded trace continuity slice: includes trace_id and span_id.
#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub subject: String,
    pub schema_version: String,
    pub sequence: u64,
    /// Trace ID for correlation (Phase 3 bounded trace continuity slice)
    pub trace_id: Option<String>,
    /// Span ID for correlation (Phase 3 bounded trace continuity slice)
    pub span_id: Option<String>,
    pub payload: serde_json::Value,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

impl InMemoryEventPublisher {
    /// Create a new in-memory publisher
    pub fn new() -> Self {
        Self {
            events: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            sequences: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            fail_publish: std::sync::atomic::AtomicBool::new(false),
            ready: std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create a publisher that starts in not-ready state
    pub fn not_ready() -> Self {
        Self {
            events: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            sequences: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            fail_publish: std::sync::atomic::AtomicBool::new(false),
            ready: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set whether publish calls should fail (for testing error handling)
    #[cfg(test)]
    pub fn set_fail_publish(&self, fail: bool) {
        self.fail_publish
            .store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get all events published to a subject (supports wildcard `*`)
    ///
    /// Uses subject pattern matching. For exact match use `get_events_for_subject`.
    pub async fn get_events(&self) -> Vec<PublishedEvent> {
        let events = self.events.read().await;
        let mut result: Vec<PublishedEvent> = Vec::new();
        for (_, evts) in events.iter() {
            result.extend(evts.clone());
        }
        // Sort by sequence for deterministic ordering
        result.sort_by_key(|a| a.sequence);
        result
    }

    /// Get all events published to a specific subject
    pub async fn get_events_for_subject(&self, subject: &str) -> Vec<PublishedEvent> {
        let events = self.events.read().await;
        events.get(subject).cloned().unwrap_or_default()
    }

    /// Get the count of events published to a specific subject
    pub async fn count_for_subject(&self, subject: &str) -> usize {
        let events = self.events.read().await;
        events.get(subject).map(|v| v.len()).unwrap_or(0)
    }

    /// Clear all stored events (for test isolation)
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
        let mut sequences = self.sequences.write().await;
        sequences.clear();
    }

    /// Get total event count across all subjects
    pub async fn total_count(&self) -> usize {
        let events = self.events.read().await;
        events.values().map(|v| v.len()).sum()
    }

    /// Check if any events have been published
    pub async fn has_events(&self) -> bool {
        let events = self.events.read().await;
        !events.is_empty()
    }
}

impl Default for InMemoryEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher for InMemoryEventPublisher {
    async fn publish(
        &self,
        subject: &EventSubject,
        payload: &serde_json::Value,
        trace_context: TraceContext,
    ) -> PublishResult {
        // Check fail flag (for testing error handling)
        if self.fail_publish.load(std::sync::atomic::Ordering::SeqCst) {
            return PublishResult::Skipped {
                reason: "simulated publish failure".to_string(),
            };
        }

        let mut sequences = self.sequences.write().await;
        let sequence = sequences.entry(subject.subject.clone()).or_insert(0);
        *sequence += 1;
        let seq = *sequence;
        drop(sequences);

        let event = PublishedEvent {
            subject: subject.subject.clone(),
            schema_version: subject.schema_version.to_string(),
            sequence: seq,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            payload: payload.clone(),
            published_at: chrono::Utc::now(),
        };

        let mut events = self.events.write().await;
        events
            .entry(subject.subject.clone())
            .or_insert_with(Vec::new)
            .push(event);

        tracing::debug!(
            "InMemoryEventPublisher: published event to '{}' (seq={})",
            subject.subject,
            seq
        );

        PublishResult::Published {
            subject: subject.subject.clone(),
            sequence: seq,
        }
    }

    fn is_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// =============================================================================
// Event Consumer Abstraction (Phase 2b bounded slice)
// =============================================================================

/// Result of a consume operation.
///
/// Phase 2b: Consuming is best-effort. Errors are logged but don't fail the operation.
#[derive(Debug)]
pub enum ConsumeResult {
    /// Event was consumed successfully
    Consumed { subject: String, sequence: u64 },
    /// Consumer failed to process the event but will retry
    Retryable { reason: String },
    /// Consumer failed and will not retry (no DLQ in Phase 2b)
    Failed { reason: String },
}

/// Phase 2b: Event consumer trait for in-memory consumers.
///
/// This trait defines the contract for consuming events from the in-memory event buffer.
/// It is used to test the event→action path (e.g., event→checkpoint creation).
///
/// **Bounded to in-memory consumers for testing only (Phase 2b)**:
/// - Full NATS-based consumer infrastructure is Phase 3
/// - DLQ and retry logic are Phase 3
/// - Consumer startup wiring is Phase 3
///
/// Implementations:
/// - `InMemoryEventConsumer` — mock consumer for tests
/// - `CheckpointCreatorConsumer` — concrete consumer in intent-service (creates checkpoints)
#[async_trait]
pub trait EventConsumer: Send + Sync {
    /// Consume a published event.
    ///
    /// Phase 2b: This is best-effort. Errors are logged and `ConsumeResult::Failed`
    /// is returned. No DLQ in Phase 2b.
    ///
    /// Returns `ConsumeResult` indicating success, retry, or failure.
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult;
}

// =============================================================================
// In-Memory Event Consumer (for testing without external infrastructure)
// =============================================================================

/// Phase 2b: In-memory event consumer for testing.
///
/// This consumer stores consumed events in memory, allowing tests to verify:
/// - Events were consumed from correct subjects
/// - Payload structure is correct
/// - Sequence numbers are monotonic
///
/// Does NOT require external NATS server.
///
/// **This is bounded to in-memory consumers for testing only.**
/// Full NATS-based consumers are Phase 3.
///
/// Usage in tests:
/// ```ignore
/// let publisher = Arc::new(InMemoryEventPublisher::new());
/// let consumer = Arc::new(InMemoryEventConsumer::new());
/// // ... publish events, then consume them ...
/// consumer.consume(&event).await;
/// let consumed = consumer.get_consumed();
/// assert_eq!(consumed.len(), 1);
/// ```
#[derive(Debug)]
pub struct InMemoryEventConsumer {
    /// Stored consumed events keyed by subject pattern (supports wildcards in lookup)
    consumed: tokio::sync::RwLock<Vec<ConsumedEvent>>,
    /// Whether consume calls should fail (for testing error handling)
    fail_consume: std::sync::atomic::AtomicBool,
}

/// A consumed event record for test verification
#[derive(Debug, Clone)]
pub struct ConsumedEvent {
    pub subject: String,
    pub schema_version: String,
    pub sequence: u64,
    pub payload: serde_json::Value,
    pub consumed_at: chrono::DateTime<chrono::Utc>,
}

impl InMemoryEventConsumer {
    /// Create a new in-memory consumer
    pub fn new() -> Self {
        Self {
            consumed: tokio::sync::RwLock::new(Vec::new()),
            fail_consume: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Set whether consume calls should fail (for testing error handling)
    #[cfg(test)]
    pub fn set_fail_consume(&self, fail: bool) {
        self.fail_consume
            .store(fail, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get all consumed events
    pub async fn get_consumed(&self) -> Vec<ConsumedEvent> {
        let consumed = self.consumed.read().await;
        consumed.clone()
    }

    /// Get consumed events for a specific subject
    pub async fn get_consumed_for_subject(&self, subject: &str) -> Vec<ConsumedEvent> {
        let consumed = self.consumed.read().await;
        consumed
            .iter()
            .filter(|e| e.subject == subject)
            .cloned()
            .collect()
    }

    /// Get the count of consumed events
    pub async fn consumed_count(&self) -> usize {
        let consumed = self.consumed.read().await;
        consumed.len()
    }

    /// Clear all consumed events (for test isolation)
    pub async fn clear(&self) {
        let mut consumed = self.consumed.write().await;
        consumed.clear();
    }

    /// Check if any events have been consumed
    pub async fn has_consumed(&self) -> bool {
        let consumed = self.consumed.read().await;
        !consumed.is_empty()
    }
}

impl Default for InMemoryEventConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventConsumer for InMemoryEventConsumer {
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult {
        // Check fail flag (for testing error handling)
        if self.fail_consume.load(std::sync::atomic::Ordering::SeqCst) {
            return ConsumeResult::Failed {
                reason: "simulated consume failure".to_string(),
            };
        }

        let consumed_event = ConsumedEvent {
            subject: event.subject.clone(),
            schema_version: event.schema_version.clone(),
            sequence: event.sequence,
            payload: event.payload.clone(),
            consumed_at: chrono::Utc::now(),
        };

        let mut consumed = self.consumed.write().await;
        consumed.push(consumed_event);

        tracing::debug!(
            "InMemoryEventConsumer: consumed event from '{}' (seq={})",
            event.subject,
            event.sequence
        );

        ConsumeResult::Consumed {
            subject: event.subject.clone(),
            sequence: event.sequence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_noop_publisher_is_always_ready() {
        let publisher = NoOpEventPublisher::new();
        assert!(publisher.is_ready());
    }

    #[tokio::test]
    async fn test_noop_publisher_skips_events() {
        let publisher = NoOpEventPublisher::new();
        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        let payload = serde_json::json!({ "test": true });

        let result = publisher
            .publish(&subject, &payload, TraceContext::default())
            .await;
        match result {
            PublishResult::Skipped { reason } => {
                assert!(reason.contains("disabled"));
            }
            _ => panic!("Expected Skipped result"),
        }
    }

    #[tokio::test]
    async fn test_inmemory_publisher_publishes_and_stores() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        let payload = serde_json::json!({
            "from_version": 1,
            "to_version": 2,
            "decision_class": "B"
        });
        let trace_ctx = TraceContext::new(
            Some("0af7651916cd43dd8448eb211c80319c".to_string()),
            Some("b7ad6b7169203331".to_string()),
        );

        let result = publisher
            .publish(&subject, &payload, trace_ctx.clone())
            .await;
        match result {
            PublishResult::Published {
                subject: s,
                sequence,
            } => {
                assert_eq!(s, subject.subject);
                assert_eq!(sequence, 1);
            }
            _ => panic!("Expected Published result"),
        }

        // Verify stored
        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[0].schema_version, "v1");
        // Verify trace context was captured
        assert_eq!(events[0].trace_id, trace_ctx.trace_id);
        assert_eq!(events[0].span_id, trace_ctx.span_id);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_sequences_are_monotonic() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        // Publish 3 events
        for i in 1..=3 {
            let payload = serde_json::json!({ "index": i });
            publisher
                .publish(&subject, &payload, TraceContext::default())
                .await;
        }

        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[2].sequence, 3);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_multiple_subjects() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let subject1 = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let subject2 = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");

        publisher
            .publish(&subject1, &serde_json::json!({}), TraceContext::default())
            .await;
        publisher
            .publish(&subject2, &serde_json::json!({}), TraceContext::default())
            .await;
        publisher
            .publish(&subject1, &serde_json::json!({}), TraceContext::default())
            .await;

        assert_eq!(publisher.count_for_subject(&subject1.subject).await, 2);
        assert_eq!(publisher.count_for_subject(&subject2.subject).await, 1);
        assert_eq!(publisher.total_count().await, 3);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_clear() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");

        publisher
            .publish(&subject, &serde_json::json!({}), TraceContext::default())
            .await;
        assert!(publisher.has_events().await);

        publisher.clear().await;
        assert!(!publisher.has_events().await);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_not_ready() {
        let publisher = InMemoryEventPublisher::not_ready();
        assert!(!publisher.is_ready());
    }

    #[tokio::test]
    async fn test_event_subject_format() {
        let tenant_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");

        assert_eq!(subject.schema_version, "v1");
        assert_eq!(subject.event_type, "RebaseApplied");
        assert_eq!(
            subject.subject,
            "audit.events.v1.550e8400-e29b-41d4-a716-446655440000.RebaseApplied"
        );
    }

    // =====================================================================
    // EventConsumer tests (Phase 2b bounded slice)
    // =====================================================================

    #[tokio::test]
    async fn test_inmemory_consumer_consumes_and_stores() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        let payload = serde_json::json!({
            "from_version": 1,
            "to_version": 2,
            "decision_class": "B"
        });

        // Publish event
        publisher
            .publish(&subject, &payload, TraceContext::default())
            .await;

        // Get published event and consume it
        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 1);

        let result = consumer.consume(&events[0]).await;
        match result {
            ConsumeResult::Consumed {
                subject: s,
                sequence,
            } => {
                assert_eq!(s, subject.subject);
                assert_eq!(sequence, 1);
            }
            _ => panic!("Expected Consumed result"),
        }

        // Verify consumed
        let consumed = consumer.get_consumed().await;
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed[0].sequence, 1);
    }

    #[tokio::test]
    async fn test_inmemory_consumer_multiple_events() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");

        // Publish 3 events
        for i in 1..=3 {
            let payload = serde_json::json!({ "index": i });
            publisher
                .publish(&subject, &payload, TraceContext::default())
                .await;
        }

        // Get all events and consume them
        let events = publisher.get_events_for_subject(&subject.subject).await;
        for event in &events {
            consumer.consume(event).await;
        }

        let consumed = consumer.get_consumed().await;
        assert_eq!(consumed.len(), 3);
        assert_eq!(consumed[0].sequence, 1);
        assert_eq!(consumed[1].sequence, 2);
        assert_eq!(consumed[2].sequence, 3);
    }

    #[tokio::test]
    async fn test_inmemory_consumer_consumed_count() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        publisher
            .publish(&subject, &serde_json::json!({}), TraceContext::default())
            .await;
        publisher
            .publish(&subject, &serde_json::json!({}), TraceContext::default())
            .await;

        let events = publisher.get_events_for_subject(&subject.subject).await;
        consumer.consume(&events[0]).await;
        assert_eq!(consumer.consumed_count().await, 1);

        consumer.consume(&events[1]).await;
        assert_eq!(consumer.consumed_count().await, 2);
    }

    #[tokio::test]
    async fn test_inmemory_consumer_clear() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        publisher
            .publish(&subject, &serde_json::json!({}), TraceContext::default())
            .await;

        let events = publisher.get_events_for_subject(&subject.subject).await;
        consumer.consume(&events[0]).await;
        assert!(consumer.has_consumed().await);

        consumer.clear().await;
        assert!(!consumer.has_consumed().await);
    }

    #[tokio::test]
    async fn test_inmemory_consumer_fail_flag() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        consumer.set_fail_consume(true);

        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");
        publisher
            .publish(&subject, &serde_json::json!({}), TraceContext::default())
            .await;

        let events = publisher.get_events_for_subject(&subject.subject).await;
        let result = consumer.consume(&events[0]).await;

        match result {
            ConsumeResult::Failed { reason } => {
                assert!(reason.contains("simulated"));
            }
            _ => panic!("Expected Failed result"),
        }
    }

    #[tokio::test]
    async fn test_publish_consume_cycle() {
        // Full cycle: publish -> consume -> verify
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(InMemoryEventConsumer::new());

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let payload = serde_json::json!({
            "from_version": 1,
            "to_version": 2,
            "decision_class": "B",
            "outcome": "auto_proceeded"
        });
        let trace_ctx = TraceContext::new(
            Some("0af7651916cd43dd8448eb211c80319c".to_string()),
            Some("b7ad6b7169203331".to_string()),
        );

        // Publish
        let publish_result = publisher
            .publish(&subject, &payload, trace_ctx.clone())
            .await;
        assert!(matches!(publish_result, PublishResult::Published { .. }));

        // Consume
        let events = publisher.get_events_for_subject(&subject.subject).await;
        let consume_result = consumer.consume(&events[0]).await;
        assert!(matches!(consume_result, ConsumeResult::Consumed { .. }));

        // Verify the full cycle worked
        assert_eq!(publisher.total_count().await, 1);
        assert_eq!(consumer.consumed_count().await, 1);

        // Verify payload preserved through cycle
        let consumed = consumer.get_consumed().await;
        assert_eq!(consumed[0].payload, payload);
    }
}
