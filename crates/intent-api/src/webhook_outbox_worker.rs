//! Webhook outbox worker (Phase 4a Slice 2)
//!
//! Bounded local-dev worker foundation around `WebhookOutboxRepository`.
//! Does NOT perform actual HTTP dispatch — that is Slice 3+ scope.
//! Provides a deterministic single-batch processing method for testability.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 2)

use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::webhook_outbox_repo::WebhookOutboxRepository;
use intent_rebase_types::IntentRebaseError;

// =============================================================================
// Env Gate
// =============================================================================

/// Environment variable name for the webhook outbox worker enablement gate.
pub const WEBHOOK_OUTBOX_WORKER_ENV_VAR: &str = "INTENT_API_WEBHOOK_OUTBOX_WORKER";

/// Parse the webhook outbox worker env gate.
///
/// - Explicit "true" / "1" / "yes" → enabled
/// - Unset, empty, or any other value → disabled (conservative default)
pub fn is_webhook_outbox_worker_enabled() -> bool {
    matches!(
        std::env::var(WEBHOOK_OUTBOX_WORKER_ENV_VAR),
        Ok(v) if v.eq_ignore_ascii_case("true") || v == "1" || v.eq_ignore_ascii_case("yes")
    )
}

// =============================================================================
// Metrics Placeholders (consistent with existing webhook_delivery.rs style)
// =============================================================================

/// Record a batch of outbox records processed by the worker.
pub(crate) fn record_webhook_outbox_worker_batch_processed(count: usize) {
    metrics::counter!("intent_api_webhook_outbox_worker_batch_processed_total")
        .increment(count as u64);
}

/// Record an outbox record that the worker failed to claim or process.
pub(crate) fn record_webhook_outbox_worker_item_failed() {
    metrics::counter!("intent_api_webhook_outbox_worker_item_failed_total").increment(1);
}

// =============================================================================
// Worker Trait
// =============================================================================

#[async_trait]
pub trait WebhookOutboxWorker: Send + Sync {
    /// Process a single batch of pending outbox records for a tenant.
    ///
    /// - Lists pending records up to `batch_size`
    /// - Claims each record (optimistic concurrency)
    /// - Marks each claimed record as delivered (bounded Slice 2 — no real HTTP dispatch)
    ///
    /// Returns the number of records successfully processed.
    async fn process_once(
        &self,
        tenant_id: Uuid,
        batch_size: i64,
    ) -> Result<usize, IntentRebaseError>;
}

// =============================================================================
// Worker Implementation
// =============================================================================

/// Concrete worker implementation backed by a `WebhookOutboxRepository`.
pub struct WebhookOutboxWorkerImpl<R: WebhookOutboxRepository> {
    repo: Arc<R>,
}

impl<R: WebhookOutboxRepository> WebhookOutboxWorkerImpl<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl<R: WebhookOutboxRepository> WebhookOutboxWorker for WebhookOutboxWorkerImpl<R> {
    async fn process_once(
        &self,
        tenant_id: Uuid,
        batch_size: i64,
    ) -> Result<usize, IntentRebaseError> {
        if !is_webhook_outbox_worker_enabled() {
            return Ok(0);
        }

        let pending = self.repo.list_pending(tenant_id, batch_size).await?;
        let mut processed = 0;

        for record in pending {
            let claimed = match self
                .repo
                .claim(record.id, tenant_id, "local-worker".to_string())
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Failed to claim outbox record {} for tenant {}: {}",
                        record.id,
                        tenant_id,
                        e
                    );
                    record_webhook_outbox_worker_item_failed();
                    continue;
                }
            };

            // Bounded Slice 2: no actual HTTP dispatch.
            // Mark as delivered for local-dev testability.
            // Future slices will replace this with real dispatch + retry logic.
            if let Err(e) = self.repo.mark_delivered(claimed.id, tenant_id).await {
                tracing::warn!(
                    "Failed to mark outbox record {} as delivered for tenant {}: {}",
                    claimed.id,
                    tenant_id,
                    e
                );
                record_webhook_outbox_worker_item_failed();
                continue;
            }

            processed += 1;
        }

        record_webhook_outbox_worker_batch_processed(processed);
        Ok(processed)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhook_outbox_repo::{
        InMemoryWebhookOutboxRepository, WebhookOutboxRecord, WebhookOutboxStatus,
    };

    fn sample_record(
        tenant_id: Uuid,
        intent_id: Uuid,
        subscription_id: Uuid,
    ) -> WebhookOutboxRecord {
        WebhookOutboxRecord::new(
            tenant_id,
            intent_id,
            subscription_id,
            "intent_changed".to_string(),
            serde_json::json!({"foo": "bar"}),
        )
    }

    #[test]
    fn test_worker_disabled_by_default() {
        temp_env::with_var_unset(WEBHOOK_OUTBOX_WORKER_ENV_VAR, || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let worker = WebhookOutboxWorkerImpl::new(repo);
            let tenant = Uuid::new_v4();
            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);
        });
    }

    #[test]
    fn test_worker_claims_and_processes_pending() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let worker = WebhookOutboxWorkerImpl::new(repo.clone());
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 1);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Delivered);
            assert_eq!(fetched.lock_version, 2); // claim + delivered
        });
    }

    #[test]
    fn test_worker_no_pending_rows() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let worker = WebhookOutboxWorkerImpl::new(repo);
            let tenant = Uuid::new_v4();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);
        });
    }

    #[test]
    fn test_worker_skips_already_claimed() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let worker = WebhookOutboxWorkerImpl::new(repo.clone());
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();
            rt.block_on(repo.claim(record.id, tenant, "other-worker".to_string()))
                .unwrap();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Claimed);
            assert_eq!(fetched.locked_by, Some("other-worker".to_string()));
        });
    }
}
