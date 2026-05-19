//! Webhook outbox DLQ handlers (Slice 5b — bounded local-dev failed-status DLQ)
//!
//! Provides HTTP handlers for viewing and replaying failed webhook outbox records:
//! - GET /webhooks/outbox/dlq?tenant_id=<uuid>[&limit=<i64>]
//! - POST /webhooks/outbox/dlq/:id/replay?tenant_id=<uuid>
//!
//! Bounded scope: no separate DLQ table; uses existing `WebhookOutboxStatus::Failed`
//! records. Replay resets attempt_count and clears error/lock state.
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 5b)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::types::{
    BulkReplayWebhookOutboxDlqRequest, BulkReplayWebhookOutboxDlqResponse,
    ListWebhookOutboxDlqQuery, ListWebhookOutboxDlqResponse, ListWebhookOutboxReplayedQuery,
    ListWebhookOutboxReplayedResponse, ReplayWebhookOutboxDlqQuery, ReplayWebhookOutboxDlqResponse,
    WebhookOutboxDlqStatsQuery, WebhookOutboxDlqStatsResponse,
};
use crate::{ApiErrorResponse, AppState};

// =============================================================================
// GET /webhooks/outbox/dlq
// =============================================================================

/// List failed webhook outbox records for a tenant.
///
/// Returns 200 OK with failed records ordered by `updated_at` desc.
/// If the outbox repository is not configured, returns an empty list.
pub async fn list_dlq(
    State(state): State<AppState>,
    Query(query): Query<ListWebhookOutboxDlqQuery>,
) -> Result<Json<ListWebhookOutboxDlqResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Ok(Json(ListWebhookOutboxDlqResponse {
                records: vec![],
                total: 0,
            }));
        }
    };

    let limit = query.limit.unwrap_or(100);
    match repo.list_failed(query.tenant_id, limit).await {
        Ok(records) => {
            let total = records.len();
            Ok(Json(ListWebhookOutboxDlqResponse { records, total }))
        }
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// POST /webhooks/outbox/dlq/:id/replay
// =============================================================================

/// Replay a failed webhook outbox record.
///
/// Transitions the record from `Failed` to `Pending`, resets `attempt_count`
/// to 0, clears `last_error`, `locked_at`, and `locked_by`, and increments
/// `lock_version`.
///
/// Returns 200 OK with the replayed record, or 400/404/500 on error.
/// Idempotency-bounded: only `Failed` records can be replayed; a second replay
/// of the same record will fail because the status is no longer `Failed`.
pub async fn replay_dlq(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReplayWebhookOutboxDlqQuery>,
) -> Result<(StatusCode, Json<ReplayWebhookOutboxDlqResponse>), ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(
                "Webhook outbox repository is not configured".to_string(),
            )));
        }
    };

    match repo
        .replay_failed(id, query.tenant_id, query.replayed_by)
        .await
    {
        Ok(record) => Ok((
            StatusCode::OK,
            Json(ReplayWebhookOutboxDlqResponse { record }),
        )),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// GET /webhooks/outbox/dlq/replayed
// =============================================================================

/// List replayed webhook outbox records for a tenant.
///
/// Returns 200 OK with records that have `replay_count > 0` and `replayed_at`
/// present, ordered by `replayed_at` desc.
/// If the outbox repository is not configured, returns an empty list.
///
/// Phase 1.3: bounded local-dev replay audit query — no production audit trail claim.
pub async fn list_replayed(
    State(state): State<AppState>,
    Query(query): Query<ListWebhookOutboxReplayedQuery>,
) -> Result<Json<ListWebhookOutboxReplayedResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Ok(Json(ListWebhookOutboxReplayedResponse {
                records: vec![],
                total: 0,
            }));
        }
    };

    let limit = query.limit.unwrap_or(100);
    match repo
        .list_replayed(query.tenant_id, query.since, limit)
        .await
    {
        Ok(records) => {
            let total = records.len();
            Ok(Json(ListWebhookOutboxReplayedResponse { records, total }))
        }
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// POST /webhooks/outbox/dlq/bulk-replay
// =============================================================================

/// Hard cap on the number of records that can be replayed in a single bulk replay call.
const BULK_REPLAY_HARD_CAP: i64 = 100;

/// Bulk replay failed webhook outbox records for a tenant.
///
/// Phase 2.2: bounded local-dev bulk replay. Lists failed records for the tenant
/// (up to `max_records` or a hard cap of 100), then replays each one individually
/// using the existing `replay_failed` repository method. Only `Failed` records are
/// replayed; records that are no longer `Failed` when the replay is attempted are
/// counted as skipped (race condition).
///
/// Returns 200 OK with replayed/skipped/error counts and the list of successfully
/// replayed records. If the outbox repository is not configured, returns an error.
pub async fn bulk_replay_dlq(
    State(state): State<AppState>,
    Json(body): Json<BulkReplayWebhookOutboxDlqRequest>,
) -> Result<(StatusCode, Json<BulkReplayWebhookOutboxDlqResponse>), ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(
                "Webhook outbox repository is not configured".to_string(),
            )));
        }
    };

    let limit = body.max_records.unwrap_or(50).min(BULK_REPLAY_HARD_CAP);
    let failed = match repo.list_failed(body.tenant_id, limit).await {
        Ok(records) => records,
        Err(IntentRebaseError::StorageError(msg)) => {
            return Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)));
        }
        Err(e) => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string())));
        }
    };

    let mut replayed_count = 0usize;
    let mut skipped_count = 0usize;
    let mut error_count = 0usize;
    let mut replayed_records = Vec::new();

    for record in failed {
        match repo
            .replay_failed(record.id, body.tenant_id, body.replayed_by.clone())
            .await
        {
            Ok(replayed) => {
                replayed_count += 1;
                replayed_records.push(replayed);
            }
            Err(IntentRebaseError::StorageError(ref msg)) if msg.contains("is not failed") => {
                skipped_count += 1;
            }
            Err(_) => {
                error_count += 1;
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(BulkReplayWebhookOutboxDlqResponse {
            replayed: replayed_count,
            skipped: skipped_count,
            errors: error_count,
            records: replayed_records,
        }),
    ))
}

// =============================================================================
// GET /webhooks/outbox/dlq/stats
// =============================================================================

/// Get DLQ stats for a tenant.
///
/// Phase 2.3: bounded local-dev stats query. Returns counts and age summary
/// for failed and replayed records, plus a grouped error summary. No production
/// dashboard or automated remediation claim.
pub async fn dlq_stats(
    State(state): State<AppState>,
    Query(query): Query<WebhookOutboxDlqStatsQuery>,
) -> Result<Json<WebhookOutboxDlqStatsResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(
                "Webhook outbox repository is not configured".to_string(),
            )));
        }
    };

    match repo.dlq_stats(query.tenant_id).await {
        Ok(stats) => Ok(Json(WebhookOutboxDlqStatsResponse { stats })),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}
