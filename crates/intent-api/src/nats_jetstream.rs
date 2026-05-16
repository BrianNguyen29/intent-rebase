//! NATS JetStream integration for Phase 3 bounded slice
//!
//! ## Design Goals
//!
//! - **Idempotent stream creation**: Stream is created once and reused across restarts.
//! - **Fail-safe startup**: NATS unavailability at startup does not crash the service.
//! - **Bounded consumer**: Pull-consumer adapter dispatches to existing `EventConsumer` trait.
//! - **Native traceparent extraction**: Parses W3C traceparent from NATS message headers.
//! - **Bounded ack behavior**: Ack on success, no infinite retry loop.
//!
//! ## Bounded App-Level DLQ First Slice (Phase 3 DLQ Design)
//!
//! **IMPLEMENTED (bounded first slice):**
//! - `DlqHelper` struct with explicit DLQ subject derivation
//! - `publish_to_dlq()` for routing failed messages to DLQ subject
//! - `replay_from_dlq()` and `replay_to_subject()` for replay primitives
//! - DLQ metadata headers (`Nats-Orig-Subject`, `Nats-Deliver-Count`, `Nats-DLQ-Reason`, `Nats-DLQ-Timestamp`)
//! - `DlqMetricsWorker` for depth/age metric emission (behind `INTENT_API_NATS_DLQ_WORKER` gate)
//!
//! **NOT YET IMPLEMENTED (gates pending — see docs/10-delivery/14-dlq-retry-design.md):**
//! - G1: Design approval
//! - G2: JetStream consumer `dead_letter` config (CLI/server-side)
//! - G3: Full monitoring/lifecycle wiring
//! - G4: RB11 runbook update for app-level DLQ
//! - G5: Integration test coverage
//!
//! **Production Readiness:** This is a BOUNDED FIRST SLICE. Not production-ready until:
//! - All gates (G1–G5) pass
//! - See `docs/10-delivery/14-dlq-retry-design.md` for full status
//!
//! ## Stream Configuration (Phase 3 bounded slice)
//!
//! Single stream `audit_events` for subject `audit.events.v1.>`:
//! - Subject: `audit.events.v1.>`
//! - No replication/cluster (single-node bounded scope)
//! - Default retention (messages kept until consumed/expired)
//!
//! ## Consumer Configuration (Phase 3 bounded slice)
//!
//! Single pull consumer per stream:
//! - Consumer name: `audit_events_consumer`
//! - Ack policy: explicit (ack after successful processing)
//! - Max deliver: 3 (bounded retry/advisory config; no infinite retry)
//! - Ack timeout: 30 seconds
//! - No dead letter subject (app-level DLQ helpers provided instead)

pub mod stream;
pub use stream::JetStreamInitializer;

pub mod consumer;
pub use consumer::{ConsumerRegistry, ConsumerRegistryError, ConsumerRegistryHandle};

// =============================================================================
// App-Level DLQ Worker (Bounded First Slice — Phase 3 DLQ Design)
// =============================================================================
//
// Bounded implementation for app-level DLQ handling:
// - Explicit DLQ subject derivation from original subject
// - Message routing/replay primitives
// - Runtime metric emissions via existing record_* helpers in lib.rs
//
// **Production Readiness Note:**
// This is a BOUNDED FIRST SLICE implementation. Not production-ready until:
// - G1: Design approved
// - G2: JetStream configured with DLQ subjects
// - G3: Monitoring/lifecycle wiring complete
// - G4: Runbook RB11 updated
// - G5: Test coverage passes
//
// async-nats 0.47 lacks Rust `dead_letter` config, so we use app-level explicit
// DLQ publishing instead of native JetStream dead-letter routing.

pub mod dlq;
pub mod dlq_metrics_worker;
pub mod dlq_replay_worker;
pub use dlq::{
    DlqHelper, DlqPublishError, DlqReplayError, DlqSubjectError, HEADER_DELIVERY_COUNT,
    HEADER_DLQ_REASON, HEADER_DLQ_TIMESTAMP, HEADER_ORIG_SUBJECT,
};
pub use dlq_metrics_worker::{
    DlqMetricsWorker, DlqMetricsWorkerBuilder, DlqMetricsWorkerConfig, DlqMetricsWorkerError,
    DlqMetricsWorkerHandle,
};
pub use dlq_replay_worker::{
    DlqReplayWorker, DlqReplayWorkerBuilder, DlqReplayWorkerConfig, DlqReplayWorkerError,
    DlqReplayWorkerHandle,
};

// =============================================================================
// Tests (Unit Tests for Traceparent Extraction)
// =============================================================================

#[cfg(test)]
mod tests_unit;

// =============================================================================
// Live Integration Tests (require docker-compose NATS with JetStream)
// Run with: cargo test -p intent-api --all-features --lib -- nats_jetstream::live_integration_tests --ignored
// =============================================================================

#[cfg(test)]
mod tests_live_integration;

// =============================================================================
// Phase 4 Lifecycle Tests (Bounded — No Live NATS Required)
// =============================================================================

#[cfg(test)]
mod tests_lifecycle;
