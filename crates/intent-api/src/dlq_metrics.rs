// =============================================================================
// DLQ Metric Helper Functions (Phase 3 DLQ design — G3 evidence)
// =============================================================================
// Counter helpers (record_dlq_replay, record_dlq_replay_failure, record_dlq_message)
// ARE wired and called from DlqHelper in nats_jetstream.rs.
//
// Gauge/depth/age helpers (record_dlq_messages_current, record_dlq_message_age_seconds)
// remain as stubs — their runtime emission awaits lifecycle worker wiring (Phase 4/G3).

/// Record current DLQ depth (number of messages in dead-letter queue)
#[allow(dead_code)]
pub fn record_dlq_messages_current(count: f64) {
    metrics::gauge!("intent_api_dlq_messages_current").set(count);
}

/// Record age of oldest message in DLQ (seconds)
#[allow(dead_code)]
pub fn record_dlq_message_age_seconds(age_secs: f64) {
    metrics::gauge!("intent_api_dlq_message_age_seconds").set(age_secs);
}

/// Record DLQ replay operation
pub fn record_dlq_replay(status: &'static str) {
    metrics::counter!("intent_api_dlq_replay_total", "status" => status).increment(1);
}

/// Record failed DLQ replay attempt
pub fn record_dlq_replay_failure() {
    metrics::counter!("intent_api_dlq_replay_failures_total").increment(1);
}

/// Record message sent to DLQ
pub fn record_dlq_message() {
    metrics::counter!("intent_api_dlq_messages_total").increment(1);
}
