//! Query handlers for compensation action endpoints.
//!
//! Phase 3 Batch 1: Contains GET /intents/{intent_id}/compensation-actions
//! handler for read-only queries of compensation action records.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        BatchCandidatesSummary, ListBatchCandidatesQuery, ListBatchCandidatesResponse,
        ListCompensationActionsQuery, ListCompensationActionsResponse, ListDlqCandidatesQuery,
        ListDlqCandidatesResponse,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Compensation Action Handlers (Phase 3 Batch 1 bounded execution slice)
// ============================================================================

/// GET /intents/{intent_id}/compensation-actions - List compensation actions for an intent
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns all compensation actions
/// recorded for the given intent, scoped to the specified tenant. Actions are
/// ordered by generated_at descending (newest first).
///
/// **This endpoint is READ-ONLY** - it does not trigger compensation execution,
/// approval workflows, or any mutation. It only queries existing compensation
/// action records.
///
/// **Planner vs Executor distinction:**
/// - This endpoint returns actual compensation action records stored via the
///   compensation action service/repository
/// - The `compensation_planning` field in rebase-preview/apply responses shows
///   planner-generated skeleton/preview data (not stored records)
/// - Full compensation execution (executor trigger) is Batch 1+ scope
#[cfg(feature = "jwt-auth")]
pub async fn list_compensation_actions(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListCompensationActionsQuery>,
) -> Result<Json<ListCompensationActionsResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_compensation_actions: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = actions.len();

    Ok(Json(ListCompensationActionsResponse {
        compensation_actions: actions,
        total,
    }))
}

/// **This endpoint is READ-ONLY** - it does not trigger compensation execution,
/// approval workflows, or any mutation. It only queries existing compensation
/// action records.
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_compensation_actions(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListCompensationActionsQuery>,
) -> Result<Json<ListCompensationActionsResponse>, ApiErrorResponse> {
    let actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = actions.len();

    Ok(Json(ListCompensationActionsResponse {
        compensation_actions: actions,
        total,
    }))
}

// ============================================================================
// DLQ Candidate Handlers (Phase 3 Batch 1 bounded manual retry slice)
// ============================================================================

/// GET /compensation-actions/dlq - List DLQ (Dead Letter Queue) candidates
///
/// Phase 3 Batch 1 (bounded manual retry slice): Returns all compensation
/// actions that are DLQ candidates.
///
/// **Derived DLQ condition:** An action is a DLQ candidate when:
/// 1. Status is Failed AND
/// 2. Either:
///    a. attempt_count >= max_retries (exhausted retry budget), OR
///    b. The error code is non-retryable (permanent failure)
///
/// **No DLQ table:** This is a read-only derived query from existing data.
/// DLQ candidates cannot be reapproved - they represent failures that have
/// exhausted automated retry possibilities.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
/// **Manual intervention is the only path forward for DLQ candidates.**
#[cfg(feature = "jwt-auth")]
pub async fn list_dlq_candidates(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    axum::extract::Query(query): axum::extract::Query<ListDlqCandidatesQuery>,
) -> Result<Json<ListDlqCandidatesResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_dlq_candidates: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let dlq_candidates = state
        .compensation_action_service
        .list_dlq_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = dlq_candidates.len();

    Ok(Json(ListDlqCandidatesResponse {
        dlq_candidates,
        total,
    }))
}

/// **No DLQ table:** This is a read-only derived query from existing data.
/// DLQ candidates cannot be reapproved - they represent failures that have
/// exhausted automated retry possibilities.
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_dlq_candidates(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListDlqCandidatesQuery>,
) -> Result<Json<ListDlqCandidatesResponse>, ApiErrorResponse> {
    let dlq_candidates = state
        .compensation_action_service
        .list_dlq_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = dlq_candidates.len();

    Ok(Json(ListDlqCandidatesResponse {
        dlq_candidates,
        total,
    }))
}

// ============================================================================
// Batch Candidate Handlers (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
// ============================================================================

/// GET /compensation-actions/batch-candidates - List batch candidates across all categories
///
/// Phase 3 Batch 1 (bounded read-only batch candidate queue slice): Returns a
/// consolidated view of all actionable compensation categories for batch processing.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Four candidate categories:**
/// 1. `pending_approval_candidates` - Actions in Pending status awaiting approval
/// 2. `approved_service_executable_candidates` - Approved actions executable by the service
///    Phase 3 Batch 1 P7: Includes both Rollback+Automatic and CounterAction+SemiAutomatic
/// 3. `retryable_failed_candidates` - Failed actions that can be reapproved (retryable error + budget remains)
/// 4. `dlq_candidates` - Failed actions that exhausted retry budget or have non-retryable errors
///
/// **No execution, orchestration, or policy gate:**
/// This is a read-only query endpoint. It does not trigger any mutations,
/// execute any actions, or involve background workers.
///
/// **Tenant-scoped:** Results are filtered by the provided tenant_id.
#[cfg(feature = "jwt-auth")]
pub async fn list_batch_candidates(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    axum::extract::Query(query): axum::extract::Query<ListBatchCandidatesQuery>,
) -> Result<Json<ListBatchCandidatesResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_batch_candidates: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let batch = state
        .compensation_action_service
        .list_batch_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summary = BatchCandidatesSummary {
        pending_approval_count: batch.pending_approval_candidates.len(),
        approved_service_executable_count: batch.approved_service_executable_candidates.len(),
        retryable_failed_count: batch.retryable_failed_candidates.len(),
        dlq_count: batch.dlq_candidates.len(),
    };

    Ok(Json(ListBatchCandidatesResponse {
        pending_approval_candidates: batch.pending_approval_candidates,
        approved_service_executable_candidates: batch.approved_service_executable_candidates,
        retryable_failed_candidates: batch.retryable_failed_candidates,
        dlq_candidates: batch.dlq_candidates,
        summary,
    }))
}

/// GET /compensation-actions/batch-candidates - List batch candidates across all categories
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_batch_candidates(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListBatchCandidatesQuery>,
) -> Result<Json<ListBatchCandidatesResponse>, ApiErrorResponse> {
    let batch = state
        .compensation_action_service
        .list_batch_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summary = BatchCandidatesSummary {
        pending_approval_count: batch.pending_approval_candidates.len(),
        approved_service_executable_count: batch.approved_service_executable_candidates.len(),
        retryable_failed_count: batch.retryable_failed_candidates.len(),
        dlq_count: batch.dlq_candidates.len(),
    };

    Ok(Json(ListBatchCandidatesResponse {
        pending_approval_candidates: batch.pending_approval_candidates,
        approved_service_executable_candidates: batch.approved_service_executable_candidates,
        retryable_failed_candidates: batch.retryable_failed_candidates,
        dlq_candidates: batch.dlq_candidates,
        summary,
    }))
}
