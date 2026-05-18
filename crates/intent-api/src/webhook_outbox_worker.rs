//! Webhook outbox worker (Phase 4a Slice 2 + 3)
//!
//! Bounded local-dev worker foundation around `WebhookOutboxRepository`.
//! Slice 2: claim/list-pending flow.
//! Slice 3: dispatch boundary integration with HMAC signing.
//!
//! Does NOT run a background loop — provides deterministic single-batch
//! `process_once` for testability.
//!
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 2–3)

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

use crate::webhook_delivery::compute_backoff_delay;
use crate::webhook_dispatcher::{WebhookDispatchFailure, WebhookDispatcher};
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
    /// - Dispatches via `WebhookDispatcher`
    /// - Marks delivered on success, failed on dispatch failure
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

/// Concrete worker implementation backed by a `WebhookOutboxRepository`
/// and a `WebhookDispatcher`.
pub struct WebhookOutboxWorkerImpl<R: WebhookOutboxRepository> {
    repo: Arc<R>,
    dispatcher: Arc<dyn WebhookDispatcher>,
}

impl<R: WebhookOutboxRepository> WebhookOutboxWorkerImpl<R> {
    pub fn new(repo: Arc<R>, dispatcher: Arc<dyn WebhookDispatcher>) -> Self {
        Self { repo, dispatcher }
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

            match self.dispatcher.dispatch(&claimed).await {
                Ok(()) => {
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
                Err(WebhookDispatchFailure::Retryable { reason }) => {
                    tracing::warn!(
                        "Dispatch retryable failure for outbox record {} for tenant {}: {}",
                        claimed.id,
                        tenant_id,
                        reason
                    );
                    let exhausted = claimed.attempt_count + 1 >= claimed.max_attempts;
                    if exhausted {
                        if let Err(e) = self.repo.mark_failed(claimed.id, tenant_id, reason).await {
                            tracing::warn!(
                                "Failed to mark outbox record {} as failed for tenant {}: {}",
                                claimed.id,
                                tenant_id,
                                e
                            );
                        }
                        record_webhook_outbox_worker_item_failed();
                    } else {
                        let delay =
                            compute_backoff_delay((claimed.attempt_count + 1).max(0) as u32);
                        let scheduled_at = Utc::now()
                            + chrono::Duration::from_std(delay)
                                .unwrap_or_else(|_| chrono::Duration::seconds(0));
                        if let Err(e) = self
                            .repo
                            .reschedule_retry(claimed.id, tenant_id, reason, scheduled_at)
                            .await
                        {
                            tracing::warn!(
                                "Failed to reschedule outbox record {} for tenant {}: {}",
                                claimed.id,
                                tenant_id,
                                e
                            );
                            record_webhook_outbox_worker_item_failed();
                        }
                    }
                }
                Err(WebhookDispatchFailure::Terminal { reason }) => {
                    tracing::warn!(
                        "Dispatch terminal failure for outbox record {} for tenant {}: {}",
                        claimed.id,
                        tenant_id,
                        reason
                    );
                    if let Err(e) = self.repo.mark_failed(claimed.id, tenant_id, reason).await {
                        tracing::warn!(
                            "Failed to mark outbox record {} as failed for tenant {}: {}",
                            claimed.id,
                            tenant_id,
                            e
                        );
                    }
                    record_webhook_outbox_worker_item_failed();
                }
            }
        }

        record_webhook_outbox_worker_batch_processed(processed);
        Ok(processed)
    }
}

// =============================================================================
// Background Worker Handle
// =============================================================================

/// Handle to a running webhook outbox background worker.
///
/// Allows graceful shutdown of the worker loop.
#[derive(Debug)]
pub struct WebhookOutboxWorkerHandle {
    /// Task handle for the running worker
    handle: tokio::task::JoinHandle<()>,
    /// Shutdown signal sender
    shutdown_tx: watch::Sender<bool>,
}

impl WebhookOutboxWorkerHandle {
    /// Signal the worker to stop gracefully.
    pub fn shutdown(&self) {
        tracing::info!("WebhookOutboxWorkerHandle: sending shutdown signal");
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for the worker to finish.
    pub async fn wait_for_all(self) {
        tracing::info!("WebhookOutboxWorkerHandle: waiting for worker to finish");
        match self.handle.await {
            Ok(()) => {
                tracing::info!("WebhookOutboxWorkerHandle: worker finished normally");
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    crate::panic_hardening::format_join_error("WebhookOutboxWorkerHandle", e)
                );
            }
        }
    }
}

/// Default poll interval for the background worker.
///
/// Bounded local-dev value: 30 seconds between discovery/processing passes.
pub const WEBHOOK_OUTBOX_WORKER_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Default batch size per tenant per processing pass.
pub const WEBHOOK_OUTBOX_WORKER_BATCH_SIZE: i64 = 100;

/// Start the webhook outbox background worker if the env gate is enabled.
///
/// - Checks `INTENT_API_WEBHOOK_OUTBOX_WORKER` env var
/// - If enabled, spawns a tokio task that loops:
///   1. Discovers tenants with pending outbox records
///   2. Calls `process_once` for each tenant
///   3. Sleeps for `WEBHOOK_OUTBOX_WORKER_POLL_INTERVAL`
/// - If disabled, returns `None` and does not spawn a task
///
/// **Non-production:** This is a bounded local-dev background loop.
/// It does not implement backpressure, horizontal scaling, or production
/// worker lease semantics.
pub fn maybe_start_webhook_outbox_worker<R>(
    repo: Arc<R>,
    dispatcher: Arc<dyn WebhookDispatcher>,
) -> Option<WebhookOutboxWorkerHandle>
where
    R: WebhookOutboxRepository + 'static,
{
    if !is_webhook_outbox_worker_enabled() {
        tracing::info!(
            "INTENT_API_WEBHOOK_OUTBOX_WORKER not enabled — background worker not started"
        );
        return None;
    }

    tracing::info!(
        "INTENT_API_WEBHOOK_OUTBOX_WORKER=true — starting webhook outbox background worker"
    );

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let worker = WebhookOutboxWorkerImpl::new(repo, dispatcher);

    let handle = tokio::spawn(async move {
        loop {
            // Check shutdown signal before each pass
            if *shutdown_rx.borrow() {
                tracing::info!("Webhook outbox worker: shutdown signal received, exiting loop");
                break;
            }

            // Discover tenants with pending records
            let tenants = match worker.repo.list_distinct_pending_tenants().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(
                        "Webhook outbox worker: failed to list pending tenants: {}",
                        e
                    );
                    Vec::new()
                }
            };

            if tenants.is_empty() {
                tracing::debug!("Webhook outbox worker: no pending tenants");
            } else {
                tracing::debug!(
                    "Webhook outbox worker: processing {} tenant(s)",
                    tenants.len()
                );
            }

            for tenant_id in tenants {
                // Re-check shutdown between tenants for faster stop
                if *shutdown_rx.borrow() {
                    tracing::info!(
                        "Webhook outbox worker: shutdown signal received during tenant pass"
                    );
                    break;
                }

                match worker
                    .process_once(tenant_id, WEBHOOK_OUTBOX_WORKER_BATCH_SIZE)
                    .await
                {
                    Ok(processed) => {
                        if processed > 0 {
                            tracing::info!(
                                "Webhook outbox worker: processed {} record(s) for tenant {}",
                                processed,
                                tenant_id
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Webhook outbox worker: failed to process tenant {}: {}",
                            tenant_id,
                            e
                        );
                    }
                }
            }

            // Sleep with shutdown awareness
            let sleep = tokio::time::sleep(WEBHOOK_OUTBOX_WORKER_POLL_INTERVAL);
            tokio::pin!(sleep);

            tokio::select! {
                _ = &mut sleep => {},
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("Webhook outbox worker: shutdown signal received during sleep");
                        break;
                    }
                }
            }
        }

        tracing::info!("Webhook outbox worker: loop exited");
    });

    Some(WebhookOutboxWorkerHandle {
        handle,
        shutdown_tx,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhook_dispatcher::{WebhookDispatchFailure, WebhookDispatcher};
    use crate::webhook_outbox_repo::{
        InMemoryWebhookOutboxRepository, WebhookOutboxRecord, WebhookOutboxStatus,
    };

    struct MockDispatcher {
        result: Result<(), WebhookDispatchFailure>,
    }

    impl MockDispatcher {
        fn new(result: Result<(), WebhookDispatchFailure>) -> Self {
            Self { result }
        }
    }

    #[async_trait]
    impl WebhookDispatcher for MockDispatcher {
        async fn dispatch(
            &self,
            _record: &WebhookOutboxRecord,
        ) -> Result<(), WebhookDispatchFailure> {
            self.result.clone()
        }
    }

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
            None,
        )
    }

    #[test]
    fn test_worker_disabled_by_default() {
        temp_env::with_var_unset(WEBHOOK_OUTBOX_WORKER_ENV_VAR, || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let worker = WebhookOutboxWorkerImpl::new(repo, dispatcher);
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
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let worker = WebhookOutboxWorkerImpl::new(repo.clone(), dispatcher);
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
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let worker = WebhookOutboxWorkerImpl::new(repo, dispatcher);
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
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let worker = WebhookOutboxWorkerImpl::new(repo.clone(), dispatcher);
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

    #[test]
    fn test_worker_marks_failed_on_terminal_dispatch_error() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Err(WebhookDispatchFailure::Terminal {
                reason: "HTTP 404".to_string(),
            })));
            let worker = WebhookOutboxWorkerImpl::new(repo.clone(), dispatcher);
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Failed);
            assert_eq!(fetched.last_error, Some("HTTP 404".to_string()));
            assert_eq!(fetched.lock_version, 2); // claim + failed
        });
    }

    #[test]
    fn test_worker_reschedules_retryable_failure() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Err(
                WebhookDispatchFailure::Retryable {
                    reason: "network timeout".to_string(),
                },
            )));
            let worker = WebhookOutboxWorkerImpl::new(repo.clone(), dispatcher);
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Pending);
            assert_eq!(fetched.attempt_count, 1);
            assert_eq!(fetched.last_error, Some("network timeout".to_string()));
            assert!(fetched.scheduled_at > Utc::now());
            assert_eq!(fetched.locked_at, None);
            assert_eq!(fetched.locked_by, None);
            assert_eq!(fetched.lock_version, 2); // claim + reschedule
        });
    }

    #[test]
    fn test_worker_marks_failed_when_retries_exhausted() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Err(
                WebhookDispatchFailure::Retryable {
                    reason: "network timeout".to_string(),
                },
            )));
            let worker = WebhookOutboxWorkerImpl::new(repo.clone(), dispatcher);
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let mut record = sample_record(tenant, intent, sub);
            record.attempt_count = 2; // one away from max_attempts=3

            rt.block_on(repo.create(record.clone())).unwrap();

            let result = rt.block_on(worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Failed);
            assert_eq!(fetched.last_error, Some("network timeout".to_string()));
            assert_eq!(fetched.lock_version, 2); // claim + failed
        });
    }

    #[test]
    fn test_worker_reschedules_then_delivers_on_retry() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();

            // First pass: retryable failure → rescheduled
            let fail_dispatcher = Arc::new(MockDispatcher::new(Err(
                WebhookDispatchFailure::Retryable {
                    reason: "network timeout".to_string(),
                },
            )));
            let fail_worker = WebhookOutboxWorkerImpl::new(repo.clone(), fail_dispatcher);
            let result = rt.block_on(fail_worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 0);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Pending);
            assert_eq!(fetched.attempt_count, 1);

            // Second pass: success → delivered
            let ok_dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let ok_worker = WebhookOutboxWorkerImpl::new(repo.clone(), ok_dispatcher);
            let result = rt.block_on(ok_worker.process_once(tenant, 10));
            assert_eq!(result.unwrap(), 1);

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Delivered);
            assert_eq!(fetched.lock_version, 4); // claim + reschedule + claim + delivered
        });
    }

    #[test]
    fn test_maybe_start_webhook_outbox_worker_disabled_by_default() {
        temp_env::with_var_unset(WEBHOOK_OUTBOX_WORKER_ENV_VAR, || {
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let handle = maybe_start_webhook_outbox_worker(repo, dispatcher);
            assert!(handle.is_none());
        });
    }

    #[test]
    fn test_maybe_start_webhook_outbox_worker_enabled() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let handle = rt.block_on(async {
                // maybe_start_webhook_outbox_worker calls tokio::spawn, so it
                // must run inside a tokio runtime context.
                maybe_start_webhook_outbox_worker(repo, dispatcher)
            });
            assert!(handle.is_some());

            let handle = handle.unwrap();
            handle.shutdown();
            rt.block_on(handle.wait_for_all());
        });
    }

    #[test]
    fn test_webhook_outbox_worker_background_processes_pending() {
        temp_env::with_var(WEBHOOK_OUTBOX_WORKER_ENV_VAR, Some("true"), || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
            let dispatcher = Arc::new(MockDispatcher::new(Ok(())));
            let tenant = Uuid::new_v4();
            let intent = Uuid::new_v4();
            let sub = Uuid::new_v4();
            let record = sample_record(tenant, intent, sub);

            rt.block_on(repo.create(record.clone())).unwrap();

            let handle =
                rt.block_on(async { maybe_start_webhook_outbox_worker(repo.clone(), dispatcher) });
            assert!(handle.is_some());

            // Give the worker a moment to process
            std::thread::sleep(std::time::Duration::from_millis(200));

            let handle = handle.unwrap();
            handle.shutdown();
            rt.block_on(handle.wait_for_all());

            let fetched = rt.block_on(repo.get(record.id, tenant)).unwrap();
            assert_eq!(fetched.status, WebhookOutboxStatus::Delivered);
        });
    }
}
