//! Webhook outbox repository (Phase 4a Slice 1)
//!
//! Provides storage for webhook outbox records that track pending,
//! claimed, delivered, and failed webhook deliveries.
//!
//! Bounded Slice 1: schema + types + repository + in-memory implementation.
//! Background worker (Slice 2), HMAC signing (Slice 3), subscription CRUD
//! (Slice 4), and retry/DLQ full lifecycle (Slice 5) remain deferred.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 1)

pub mod memory;
pub mod sqlx;
pub mod types;

#[cfg(test)]
mod tests;

// Re-export all public items to preserve the existing API surface.
pub use types::{
    WebhookOutboxDlqErrorSummary, WebhookOutboxDlqStats, WebhookOutboxRecord,
    WebhookOutboxRepository, WebhookOutboxStatus,
};

pub use memory::InMemoryWebhookOutboxRepository;
pub use sqlx::SqlxWebhookOutboxRepository;
