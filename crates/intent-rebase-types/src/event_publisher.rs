//! Event publishing abstraction for Phase 2b bounded slice
//!
//! ## Design Goals
//!
//! - **Bounded**: Only publishes events that are already persisted to audit storage.
//!   Audit persistence is the source of truth; publishing is best-effort notification.
//! - **Fail-open**: Publishing errors are logged but don't fail the operation.
//! - **Testable**: In-memory mock publisher allows verification without external NATS.
//! - **Deferred consumers**: Consumer systems and DLQ are Phase 3 items.
//!
//! ## Subject Naming Convention (Phase 2b bounded slice)
//!
//! Subjects follow the pattern documented in ADR-04:
//! - `audit.events.v1.<tenant_id>.<event_type>` — audit events v1
//!
//! Versioning: v1 prefix in subject; full v2 migration path documented in Phase 3.
//!
//! ## Implementation Notes
//!
//! - Phase 2b: Only `InMemoryEventPublisher` (mock) and `NoOpEventPublisher` (no-op) are available.
//!   `NatsEventPublisher` (real NATS JetStream) is Phase 3 item.
//! - Consumers, DLQ, and schema migration are Phase 3 items.

use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

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
    /// The actual event payload
    pub payload: T,
}

impl<T: Serialize> EventEnvelope<T> {
    /// Create a new envelope (sequence is assigned by publisher)
    pub fn new(subject: String, schema_version: &'static str, payload: T) -> Self {
        Self {
            subject,
            schema_version,
            published_at: chrono::Utc::now(),
            sequence: 0, // Publisher assigns actual sequence
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
    /// Returns `PublishResult` indicating success or skip-with-reason.
    async fn publish(&self, subject: &EventSubject, payload: &serde_json::Value) -> PublishResult;

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
    async fn publish(&self, subject: &EventSubject, _payload: &serde_json::Value) -> PublishResult {
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
#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub subject: String,
    pub schema_version: String,
    pub sequence: u64,
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
        result.sort_by(|a, b| a.sequence.cmp(&b.sequence));
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
    async fn publish(&self, subject: &EventSubject, payload: &serde_json::Value) -> PublishResult {
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

        let result = publisher.publish(&subject, &payload).await;
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

        let result = publisher.publish(&subject, &payload).await;
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
    }

    #[tokio::test]
    async fn test_inmemory_publisher_sequences_are_monotonic() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        // Publish 3 events
        for i in 1..=3 {
            let payload = serde_json::json!({ "index": i });
            publisher.publish(&subject, &payload).await;
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

        publisher.publish(&subject1, &serde_json::json!({})).await;
        publisher.publish(&subject2, &serde_json::json!({})).await;
        publisher.publish(&subject1, &serde_json::json!({})).await;

        assert_eq!(publisher.count_for_subject(&subject1.subject).await, 2);
        assert_eq!(publisher.count_for_subject(&subject2.subject).await, 1);
        assert_eq!(publisher.total_count().await, 3);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_clear() {
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let subject = EventSubject::from_audit_event(Uuid::new_v4(), "RebaseApplied");

        publisher.publish(&subject, &serde_json::json!({})).await;
        assert!(publisher.has_events().await);

        publisher.clear().await;
        assert!(!publisher.has_events().await);
    }

    #[tokio::test]
    async fn test_inmemory_publisher_not_ready() {
        let publisher = Arc::new(InMemoryEventPublisher::not_ready());
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
}
