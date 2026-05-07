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
    types::{ListCompensationActionsQuery, ListCompensationActionsResponse},
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
