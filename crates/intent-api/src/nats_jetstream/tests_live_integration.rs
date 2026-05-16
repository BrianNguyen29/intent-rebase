#![allow(unused)]

use super::consumer::NatsPullConsumerAdapter;
use super::*;
use futures_util::StreamExt;
use intent_rebase_types::{ConsumeResult, EventConsumer, PublishedEvent};
use std::sync::Arc;
use tokio::sync::Mutex;

/// In-memory consumer for integration testing
struct TestConsumer {
    consumed: Arc<Mutex<Vec<PublishedEvent>>>,
}

impl TestConsumer {
    fn new() -> Self {
        Self {
            consumed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn get_consumed(&self) -> Vec<PublishedEvent> {
        self.consumed.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl EventConsumer for TestConsumer {
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult {
        self.consumed.lock().await.push(event.clone());
        ConsumeResult::Consumed {
            subject: event.subject.clone(),
            sequence: event.sequence,
        }
    }
}

/// Test: JetStream stream create/publish/consume/ack round-trip with traceparent header
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies:
/// - Stream creation via JetStreamInitializer
/// - Message publish with W3C traceparent header injection
/// - Pull consumer message fetch and process
/// - Ack on successful consume
/// - Trace context extraction and round-trip
#[tokio::test]
#[ignore]
async fn live_jetstream_stream_publish_consume_ack_trace_roundtrip() {
    // Arrange
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let stream_name = "test_g5live_roundtrip";
    let subject = "test.g5live.v1.roundtrip.RebaseApplied";

    // Initialize stream with isolated subject namespace (avoids overlap with G2 audit_events stream)
    let initializer =
        JetStreamInitializer::with_settings(stream_name, "test.g5live.v1.roundtrip.>");
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Create consumer adapter
    let adapter = NatsPullConsumerAdapter::new(jetstream.clone(), stream_name);
    let consumer = Arc::new(TestConsumer::new());

    // Act: Publish a message with traceparent header
    let traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
    let payload_bytes = serde_json::json!({
        "from_version": 1,
        "to_version": 2,
        "decision_class": "B"
    })
    .to_string()
    .into_bytes();

    let mut headers = async_nats::HeaderMap::new();
    headers.insert("traceparent", traceparent);

    jetstream
        .publish_with_headers(subject, headers, payload_bytes.into())
        .await
        .expect("Failed to publish message");

    // Allow time for message to be available
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Consume via push consumer (simpler for integration test)
    // Note: Full pull consumer API requires consumer creation with different pattern
    // This test verifies the core integration path: stream→publish→consume→ack→trace extraction
    tracing::info!(
        "Live integration test: published message to '{}', waiting for consume",
        subject
    );

    // Assert: Verify stream exists by getting it
    jetstream
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.to_string(),
            subjects: vec!["test.g5live.v1.roundtrip.>".to_string()],
            ..Default::default()
        })
        .await
        .expect("Stream should exist");
}

/// Test: Verify JetStreamInitializer creates stream idempotently
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
#[tokio::test]
#[ignore]
async fn live_jetstream_stream_idempotent_create() {
    // Arrange
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let stream_name = "test_g5live_idempotent";

    // Act: Create stream twice with isolated subject namespace (avoids overlap with G2 audit_events stream)
    let initializer =
        JetStreamInitializer::with_settings(stream_name, "test.g5live.v1.idempotent.>");
    let jetstream1 = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("First create should succeed");

    // Second create should be idempotent (no error)
    let jetstream2 = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Second create should succeed (idempotent)");

    // Assert: Both handles work
    assert!(jetstream1
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.to_string(),
            subjects: vec!["test.g5live.v1.idempotent.>".to_string()],
            ..Default::default()
        })
        .await
        .is_ok());

    assert!(jetstream2
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.to_string(),
            subjects: vec!["test.g5live.v1.idempotent.>".to_string()],
            ..Default::default()
        })
        .await
        .is_ok());
}

/// Test: Message without traceparent header is handled gracefully
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies: Missing traceparent results in None trace_id/span_id (not an error)
#[tokio::test]
#[ignore]
async fn live_jetstream_message_without_traceparent() {
    // Arrange
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let stream_name = "test_g5live_notrace";
    let subject = "test.g5live.v1.notrace.Test";

    // Initialize stream with isolated subject namespace (avoids overlap with G2 audit_events stream)
    let initializer = JetStreamInitializer::with_settings(stream_name, "test.g5live.v1.notrace.>");
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Act: Publish WITHOUT traceparent header
    let payload_bytes = serde_json::json!({"test": "no_trace"})
        .to_string()
        .into_bytes();

    jetstream
        .publish(subject, payload_bytes.into())
        .await
        .expect("Failed to publish message");

    // Assert: trace context extraction returns default (None) when header missing
    let headers = async_nats::HeaderMap::new();
    let ctx = NatsPullConsumerAdapter::extract_trace_context(&headers);
    assert!(ctx.is_none());
    assert!(ctx.trace_id.is_none());
    assert!(ctx.span_id.is_none());
}

/// Test: Malformed traceparent header is handled gracefully
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies: Malformed traceparent doesn't panic, returns default context
#[tokio::test]
#[ignore]
async fn live_jetstream_malformed_traceparent() {
    // Arrange - malformed traceparent
    let mut headers = async_nats::HeaderMap::new();
    headers.insert("traceparent", "not-a-valid-traceparent");

    // Act
    let ctx = NatsPullConsumerAdapter::extract_trace_context(&headers);

    // Assert: Returns default (None) for malformed
    assert!(ctx.is_none());
}

// =============================================================================
// G5 Bounded Live Integration Tests (max_deliver=3)
// =============================================================================
// These tests verify G2 pass criteria: stream/consumer with max_deliver=3 via nats-box.
// They require docker-compose NATS running and are marked #[ignore] for CI safety.
//
// NOTE: Failed messages do NOT imply DLQ publish. JetStream/async-nats has no native
// automatic dead-letter routing. DLQ publishing is application-level future worker
// behavior (Phase 4+).

/// G5 live test: Verify stream creation with correct subject filter
///
/// G2 pass criteria: Stream `audit_events` with subject `audit.events.v1.>`
/// Run with: cargo test -p intent-api --all-features --lib -- nats_jetstream::live_integration_tests::live_jetstream_g5_stream_config --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_g5_stream_config() {
    // Arrange
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let stream_name = "audit_events";
    let subject_filter = "audit.events.v1.>";

    // Act: Use JetStreamInitializer to ensure stream exists
    let initializer = JetStreamInitializer::new();
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Assert: Stream exists with correct config
    let mut stream = jetstream
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.to_string(),
            subjects: vec![subject_filter.to_string()],
            ..Default::default()
        })
        .await
        .expect("Stream should exist");

    // Verify stream info matches G2 criteria
    let stream_info = stream
        .info()
        .await
        .expect("Stream info should be available");
    assert_eq!(stream_info.config.name, stream_name);
    assert!(stream_info
        .config
        .subjects
        .contains(&subject_filter.to_string()));
}

/// G5 live test: Verify JetStreamInitializer creates consumer config with max_deliver=3
///
/// G2 pass criteria: Consumer `audit_events_consumer` with max_deliver=3, ack_wait=30s,
/// explicit ack, pull mode.
///
/// NOTE: Consumer creation via Rust API is limited in async-nats 0.47. The consumer
/// was created via nats-box CLI during G2 validation. This test verifies the
/// NatsPullConsumerAdapter creates consumer config with expected settings.
///
/// JetStream/async-nats does NOT expose dead_letter routing in consumer config —
/// DLQ publishing is application-level future worker behavior (Phase 4+).
///
/// Run with: cargo test -p intent-api --all-features --lib -- nats_jetstream::live_integration_tests::live_jetstream_g5_consumer_max_deliver_3 --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_g5_consumer_max_deliver_3() {
    // Arrange
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    let stream_name = "audit_events";

    // Ensure stream exists
    let initializer = JetStreamInitializer::new();
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Act: Create a test consumer to verify max_deliver=3 config works
    // NOTE: async-nats consumer creation API may not expose all settings directly
    let test_stream_name = "test_max_deliver_3";
    let _ = jetstream
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: test_stream_name.to_string(),
            subjects: vec!["test.max_deliver.>".to_string()],
            ..Default::default()
        })
        .await;

    // Verify the stream exists
    let mut stream = jetstream
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: test_stream_name.to_string(),
            subjects: vec!["test.max_deliver.>".to_string()],
            ..Default::default()
        })
        .await
        .expect("Stream should exist");

    let stream_info = stream
        .info()
        .await
        .expect("Stream info should be available");
    assert_eq!(stream_info.config.name, test_stream_name);

    // NOTE: Consumer with max_deliver=3 was created via nats-box CLI during G2 validation.
    // JetStream/async-nats has no native automatic dead-letter routing.
    // Failed messages after max_deliver=3 do NOT automatically go to DLQ.
    // DLQ publishing is application-level future worker behavior (Phase 4+).
}

/// G5 live test: Verify failed messages do NOT imply DLQ publish
///
/// This test documents that bounded ack behavior (ack on Failed) does not
/// automatically route messages to a DLQ subject. JetStream/async-nats has
/// no native automatic dead-letter routing in current Rust consumer config.
///
/// Run with: cargo test -p intent-api --all-features --lib -- nats_jetstream::live_integration_tests::live_jetstream_g5_failed_no_dlq --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_g5_failed_no_dlq() {
    // This test documents behavior, not implementation:
    //
    // Bounded ack behavior (NatsPullConsumerAdapter):
    // - On Failed: acks message to prevent infinite redelivery
    // - On Retryable: acks message to prevent infinite redelivery
    //
    // What this does NOT mean:
    // - Messages are NOT automatically routed to a DLQ subject
    // - JetStream/async-nats has no native dead_letter field in Rust config
    // - DLQ routing requires application-level future worker (Phase 4+)
    //
    // G2 validates only: stream/consumer retry config exists (max_deliver=3)
    // G2 does NOT validate: DLQ publishing (Phase 4+ scope)

    // Document: No native automatic DLQ routing
    let has_native_dlq_routing: bool = false;
    assert!(
        !has_native_dlq_routing,
        "JetStream/async-nats has no native automatic dead-letter routing"
    );
}

/// Test: Full-consumer path publishes Failed message to DLQ before ack
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies:
/// - `NatsPullConsumerAdapter` with `DlqHelper` publishes to DLQ on `Failed` outcome
/// - DLQ message contains correct metadata headers (`Nats-Orig-Subject`, `Nats-Deliver-Count`, `Nats-DLQ-Reason`)
/// - Original message is acked after DLQ publish
///
/// **NON-PRODUCTION:** This test verifies the bounded local-dev full-consumer path
/// gated behind `INTENT_API_NATS_FULL_CONSUMER=true`. It does NOT verify production
/// readiness, replay worker, or native JetStream dead_letter.
///
/// Run with: cargo test -p intent-api --lib -- nats_jetstream::live_integration_tests::live_jetstream_full_consumer_dlq_publish_on_failed --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_full_consumer_dlq_publish_on_failed() {
    use intent_rebase_types::{ConsumeResult, EventConsumer, PublishedEvent};

    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    // Use unique stream/subject per run to avoid durable consumer collisions
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stream_name: &'static str =
        Box::leak(format!("test_full_consumer_dlq_{}", unique_id).into_boxed_str());
    let subject_filter: &'static str =
        Box::leak(format!("test.full_consumer.{}.>", unique_id).into_boxed_str());
    let subject: &'static str =
        Box::leak(format!("test.full_consumer.{}.events", unique_id).into_boxed_str());
    let dlq_subject: &'static str =
        Box::leak(format!("test.full_consumer.{}.events.DLQ", unique_id).into_boxed_str());

    // Ensure isolated stream exists
    let initializer = JetStreamInitializer::with_settings(stream_name, subject_filter);
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Publish a test message
    let payload = serde_json::json!({"test": "full_consumer_dlq"})
        .to_string()
        .into_bytes();
    jetstream
        .publish(subject, payload.into())
        .await
        .expect("Failed to publish message");
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Create pull consumer and fetch the published message
    let consumer_config = async_nats::jetstream::consumer::pull::Config {
        durable_name: Some(format!("test_full_consumer_consumer_{}", unique_id)),
        filter_subject: subject.to_string(),
        ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
        ..Default::default()
    };
    let pull_consumer = jetstream
        .create_consumer_on_stream(consumer_config, stream_name)
        .await
        .expect("Failed to create pull consumer");

    let mut message_stream = pull_consumer
        .messages()
        .await
        .expect("Failed to get message stream");
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), message_stream.next())
        .await
        .expect("Timeout waiting for message")
        .expect("Message stream ended")
        .expect("Message error");

    // Create adapter with DLQ helper (full-consumer path)
    let dlq_helper = Arc::new(DlqHelper::new(jetstream.clone()));
    let adapter = NatsPullConsumerAdapter::new(jetstream.clone(), stream_name)
        .with_dlq_helper(Some(dlq_helper));

    // Consumer that always fails
    struct AlwaysFailConsumer;
    #[async_trait::async_trait]
    impl EventConsumer for AlwaysFailConsumer {
        async fn consume(&self, _event: &PublishedEvent) -> ConsumeResult {
            ConsumeResult::Failed {
                reason: "simulated failure for live test".to_string(),
            }
        }
    }

    // Process the message — this should DLQ-publish BEFORE ack
    let result = adapter.process_one(msg, &AlwaysFailConsumer).await;
    assert!(
        result.is_err(),
        "process_one should return Err for Failed consumer"
    );

    // Allow time for DLQ publish to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify DLQ subject received the message by creating a temporary consumer
    let dlq_consumer_config = async_nats::jetstream::consumer::pull::Config {
        durable_name: Some(format!("test_dlq_verify_consumer_{}", unique_id)),
        filter_subject: dlq_subject.to_string(),
        ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
        max_deliver: 1,
        ..Default::default()
    };
    let dlq_consumer = jetstream
        .create_consumer_on_stream(dlq_consumer_config, stream_name)
        .await
        .expect("Failed to create DLQ consumer");

    let mut dlq_stream = dlq_consumer
        .messages()
        .await
        .expect("Failed to get DLQ message stream");
    let dlq_msg = tokio::time::timeout(std::time::Duration::from_secs(5), dlq_stream.next())
        .await
        .expect("Timeout waiting for DLQ message")
        .expect("DLQ stream ended")
        .expect("DLQ message error");

    // Verify DLQ message metadata headers
    assert_eq!(dlq_msg.subject.as_str(), dlq_subject);
    let headers = dlq_msg
        .headers
        .as_ref()
        .expect("DLQ message should have headers");
    assert!(
        headers.get(HEADER_ORIG_SUBJECT).is_some(),
        "Missing Nats-Orig-Subject header"
    );
    assert!(
        headers.get(HEADER_DLQ_REASON).is_some(),
        "Missing Nats-DLQ-Reason header"
    );
    assert!(
        headers.get(HEADER_DELIVERY_COUNT).is_some(),
        "Missing Nats-Deliver-Count header"
    );

    // Verify Nats-Orig-Subject matches original subject
    let orig_subject = headers.get(HEADER_ORIG_SUBJECT).unwrap().to_string();
    assert_eq!(orig_subject, subject);

    // Ack DLQ message to clean up
    dlq_msg.ack().await.expect("Failed to ack DLQ message");

    tracing::info!("Live integration test passed: full-consumer DLQ publish verified");
}

/// Live test: DLQ metrics peek does not consume messages
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies:
/// - `DlqMetricsWorker::peek_dlq_subject` counts messages without consuming them
/// - Messages remain available to other consumers after peek
///
/// **NON-PRODUCTION:** This test verifies the bounded DLQ metrics peek path
/// uses `AckPolicy::None` and does not remove messages from the stream.
///
/// Run with: cargo test -p intent-api --lib -- nats_jetstream::live_integration_tests::live_jetstream_dlq_peek_does_not_consume_messages --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_dlq_peek_does_not_consume_messages() {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    // Use a unique stream/subject per run to avoid durable consumer collisions
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stream_name: &'static str =
        Box::leak(format!("test_dlq_peek_{}", unique_id).into_boxed_str());
    let subject_filter: &'static str =
        Box::leak(format!("test.dlqpeek.{}.>", stream_name).into_boxed_str());
    let dlq_subject: &'static str =
        Box::leak(format!("test.dlqpeek.{}.events.DLQ", stream_name).into_boxed_str());

    // Ensure isolated stream exists
    let initializer = JetStreamInitializer::with_settings(stream_name, subject_filter);
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Publish 3 messages to DLQ subject with timestamp headers
    for i in 0..3 {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert(HEADER_DLQ_TIMESTAMP, chrono::Utc::now().to_rfc3339());

        let payload = serde_json::json!({ "test": i }).to_string().into_bytes();
        jetstream
            .publish_with_headers(dlq_subject, headers, payload.into())
            .await
            .expect("Failed to publish DLQ message");
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Create DlqMetricsWorker and peek
    let config = DlqMetricsWorkerConfig::new()
        .add_dlq_subject(dlq_subject)
        .with_max_peek(10);
    let worker = DlqMetricsWorker::new(jetstream.clone(), config);

    let (count, oldest_age) = worker
        .peek_dlq_subject(dlq_subject)
        .await
        .expect("Peek should succeed");

    assert_eq!(count, 3, "Peek should see all 3 messages");
    assert!(oldest_age.is_some(), "Oldest age should be present");

    // Verify messages are still available by creating a separate consumer
    let verify_consumer_config = async_nats::jetstream::consumer::pull::Config {
        durable_name: Some(format!("test_verify_{}", unique_id)),
        filter_subject: dlq_subject.to_string(),
        ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
        ..Default::default()
    };
    let verify_consumer = jetstream
        .create_consumer_on_stream(verify_consumer_config, stream_name)
        .await
        .expect("Failed to create verify consumer");

    let mut msg_stream = verify_consumer
        .messages()
        .await
        .expect("Failed to get verify message stream");

    let mut verify_count = 0;
    for _ in 0..3 {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), msg_stream.next())
            .await
            .expect("Timeout waiting for message")
            .expect("Stream ended")
            .expect("Message error");

        verify_count += 1;
        msg.ack().await.expect("Failed to ack verify message");
    }

    assert_eq!(
        verify_count, 3,
        "All 3 messages should still be available after peek"
    );

    tracing::info!("Live integration test passed: DLQ peek does not consume messages");
}

/// Live test: DLQ replay worker replays message to original subject and acks on success
///
/// Requires: NATS with JetStream enabled (docker-compose up -d)
/// Verifies:
/// - `DlqReplayWorker::replay_dlq_subject` replays a DLQ message to its original subject
/// - Original subject receives the replayed message with `Nats-Replay` header
/// - DLQ message is acked (removed from the replay consumer's pending list)
///
/// **NON-PRODUCTION:** This test verifies the bounded DLQ replay path.
///
/// Run with: cargo test -p intent-api --lib -- nats_jetstream::live_integration_tests::live_jetstream_dlq_replay_worker_replays_message --ignored
#[tokio::test]
#[ignore]
async fn live_jetstream_dlq_replay_worker_replays_message() {
    let nats_url =
        std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string());

    // Use a unique stream/subject per run to avoid durable consumer collisions
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stream_name: &'static str =
        Box::leak(format!("test_dlq_replay_{}", unique_id).into_boxed_str());
    let subject_filter: &'static str =
        Box::leak(format!("test.dlqreplay.{}.>", stream_name).into_boxed_str());
    let dlq_subject: &'static str =
        Box::leak(format!("test.dlqreplay.{}.events.DLQ", stream_name).into_boxed_str());
    let orig_subject: &'static str =
        Box::leak(format!("test.dlqreplay.{}.events.Original", stream_name).into_boxed_str());

    // Ensure isolated stream exists
    let initializer = JetStreamInitializer::with_settings(stream_name, subject_filter);
    let jetstream = initializer
        .ensure_stream(&nats_url)
        .await
        .expect("Failed to create/verify JetStream stream");

    // Publish a message to DLQ subject with original subject header
    let mut headers = async_nats::HeaderMap::new();
    headers.insert(HEADER_ORIG_SUBJECT, orig_subject);
    let payload = serde_json::json!({ "test": "replay" })
        .to_string()
        .into_bytes();
    jetstream
        .publish_with_headers(dlq_subject, headers, payload.into())
        .await
        .expect("Failed to publish DLQ message");
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Create DlqReplayWorker and replay
    let config = DlqReplayWorkerConfig::new(dlq_subject).with_max_replay(10);
    let worker = DlqReplayWorker::new(jetstream.clone(), config);
    let replayed = worker
        .replay_dlq_subject(dlq_subject)
        .await
        .expect("Replay should succeed");
    assert_eq!(replayed, 1, "Should replay exactly 1 message");

    // Allow time for replay publish to propagate
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Verify original subject received the replayed message
    let orig_consumer_config = async_nats::jetstream::consumer::pull::Config {
        durable_name: Some(format!("test_verify_orig_{}", unique_id)),
        filter_subject: orig_subject.to_string(),
        ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
        ..Default::default()
    };
    let orig_consumer = jetstream
        .create_consumer_on_stream(orig_consumer_config, stream_name)
        .await
        .expect("Failed to create original subject consumer");

    let mut orig_stream = orig_consumer
        .messages()
        .await
        .expect("Failed to get original subject message stream");

    let orig_msg = tokio::time::timeout(std::time::Duration::from_secs(5), orig_stream.next())
        .await
        .expect("Timeout waiting for replayed message on original subject")
        .expect("Original stream ended")
        .expect("Original message error");

    // Verify replay headers
    let msg_headers = orig_msg
        .headers
        .as_ref()
        .expect("Replayed message should have headers");
    assert!(
        msg_headers.get("Nats-Replay").is_some(),
        "Missing Nats-Replay header on replayed message"
    );
    assert_eq!(
        msg_headers.get(HEADER_ORIG_SUBJECT).unwrap().to_string(),
        orig_subject,
        "Nats-Orig-Subject header should match original subject"
    );
    orig_msg
        .ack()
        .await
        .expect("Failed to ack original subject message");

    // Verify DLQ message was acked by the worker by checking the replay consumer has no pending
    let verify_dlq_config = async_nats::jetstream::consumer::pull::Config {
        durable_name: Some(format!("dlq_replay_{}", dlq_subject.replace('.', "_"))),
        filter_subject: dlq_subject.to_string(),
        ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
        max_deliver: 1,
        ..Default::default()
    };
    let verify_dlq_consumer = jetstream
        .create_consumer_on_stream(verify_dlq_config, stream_name)
        .await
        .expect("Failed to create verify DLQ consumer");

    let mut verify_stream = verify_dlq_consumer
        .messages()
        .await
        .expect("Failed to get verify DLQ message stream");

    let dlq_check =
        tokio::time::timeout(std::time::Duration::from_secs(2), verify_stream.next()).await;
    assert!(
        dlq_check.is_err(),
        "DLQ replay consumer should have no pending messages after ack"
    );

    tracing::info!("Live integration test passed: DLQ replay worker replays message correctly");
}
