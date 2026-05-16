//! Propagation signal helpers.
//!
//! Bounded helper decomposition slice: Contains best-effort propagation signal
//! creation after successful rebase apply. Queries existing propagation records
//! as a de facto downstream registry and updates each to status `pending` with
//! the new version. Failures are logged as warnings and never fail the apply
//! response.

use crate::AppState;
use intent_rebase_types::PropagationStatus;
use uuid::Uuid;

/// Record propagation signal creation attempt.
pub(crate) fn record_propagation_signal_attempted() {
    metrics::counter!("intent_api_propagation_signals_attempted_total").increment(1);
}

/// Record propagation signal creation success.
pub(crate) fn record_propagation_signal_succeeded() {
    metrics::counter!("intent_api_propagation_signals_succeeded_total").increment(1);
}

/// Record propagation signal creation failure.
pub(crate) fn record_propagation_signal_failed() {
    metrics::counter!("intent_api_propagation_signals_failed_total").increment(1);
}

/// Record propagation signal creation with no downstream records found.
pub(crate) fn record_propagation_signal_no_downstream() {
    metrics::counter!("intent_api_propagation_signals_no_downstream_total").increment(1);
}

/// Best-effort propagation signal creation after successful rebase apply.
///
/// Queries existing propagation records for the intent (de facto downstream
/// registry) and updates each to status `pending` with the new version.
/// Failures are logged as warnings and never fail the apply response.
pub(crate) async fn create_propagation_signals_after_apply(
    state: &AppState,
    intent_id: Uuid,
    tenant_id: Uuid,
    to_version: i32,
) {
    let resolver: Box<dyn crate::webhook_delivery::WebhookSubscriptionResolver> = match &state
        .rls_pool
    {
        Some(rls_pool) => Box::new(
            crate::webhook_delivery::SqlxWebhookSubscriptionResolver::new(rls_pool.pool().clone()),
        ),
        None => Box::new(crate::webhook_delivery::EmptyWebhookSubscriptionResolver),
    };
    create_propagation_signals_after_apply_inner(
        state,
        intent_id,
        tenant_id,
        to_version,
        resolver.as_ref(),
    )
    .await;
}

/// Test-only seam for injecting a custom `WebhookSubscriptionResolver` into the
/// apply-path webhook dispatch flow. Production must continue using the normal
/// resolver derived from `AppState` / `rls_pool`.
#[cfg(test)]
pub(crate) async fn create_propagation_signals_after_apply_with_resolver(
    state: &AppState,
    intent_id: Uuid,
    tenant_id: Uuid,
    to_version: i32,
    resolver: &dyn crate::webhook_delivery::WebhookSubscriptionResolver,
) {
    create_propagation_signals_after_apply_inner(state, intent_id, tenant_id, to_version, resolver)
        .await;
}

async fn create_propagation_signals_after_apply_inner(
    state: &AppState,
    intent_id: Uuid,
    tenant_id: Uuid,
    to_version: i32,
    resolver: &dyn crate::webhook_delivery::WebhookSubscriptionResolver,
) {
    let repo = match &state.propagation_record_repo {
        Some(repo) => repo,
        None => return,
    };

    record_propagation_signal_attempted();

    let records = match repo.list_by_intent(intent_id, tenant_id).await {
        Ok(recs) => recs,
        Err(e) => {
            record_propagation_signal_failed();
            tracing::warn!(
                "Failed to list propagation records for signal creation: {}",
                e
            );
            return;
        }
    };

    if records.is_empty() {
        record_propagation_signal_no_downstream();
        tracing::info!(
            "No downstream propagation records found for intent {}, skipping signal creation",
            intent_id
        );
        return;
    }

    let mut success_count = 0;
    let mut fail_count = 0;

    for record in records {
        if let Err(e) = repo
            .update_status(record.id, tenant_id, PropagationStatus::Pending, to_version)
            .await
        {
            record_propagation_signal_failed();
            fail_count += 1;
            tracing::warn!(
                "Failed to update propagation signal for system {}: {}",
                record.downstream_system_id,
                e
            );
        } else {
            record_propagation_signal_succeeded();
            success_count += 1;
            tracing::info!(
                "Propagation signal updated for intent {} system {} to version {}",
                intent_id,
                record.downstream_system_id,
                to_version
            );
        }
    }

    if fail_count > 0 {
        tracing::warn!(
            "Propagation signal creation partial failure: {} succeeded, {} failed for intent {}",
            success_count,
            fail_count,
            intent_id
        );
    }

    // B5/B6 bounded: env-gated webhook dispatch.
    // When enabled, records delivery attempt, sends webhook, and records outcome.
    // When disabled (default), this block is skipped and no delivery attempts are recorded.
    if crate::webhook_delivery::is_webhook_delivery_enabled() {
        let client = crate::webhook_delivery::build_webhook_client();
        crate::webhook_delivery::dispatch_webhooks_for_intent(
            repo,
            &client,
            resolver,
            tenant_id,
            intent_id,
            to_version,
            &crate::webhook_delivery::TokioSleeper,
        )
        .await;
    }
}
