//! Read-only approval request handlers.
//!
//! Phase 2b: Contains GET handlers for listing pending approval requests
//! and revalidating approval request validity.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        ApprovalRequestSummary, ApprovalRevalidationResponse, ListPendingApprovalRequestsQuery,
        ListPendingApprovalRequestsResponse,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Read-Only Approval Request Handlers (Phase 2b bounded read-only slice)
// ============================================================================

/// GET /approval-requests/pending - List pending approval requests for a tenant
///
/// Phase 1 P1-S5f bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler validates that the query tenant_id matches the JWT tenant.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
#[cfg(feature = "jwt-auth")]
pub async fn list_pending_approval_requests(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    axum::extract::Query(query): axum::extract::Query<ListPendingApprovalRequestsQuery>,
) -> Result<Json<ListPendingApprovalRequestsResponse>, ApiErrorResponse> {
    // Phase 1 P1-S5f: Check if RLS path is available (pool exists AND JWT claims present)
    // Also performs tenant mismatch check
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: query tenant_id must match JWT tenant
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_pending_approval_requests: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        tracing::debug!(
            "list_pending_approval_requests: RLS path validated for tenant_id={}",
            rls_claims.tenant_id
        );

        let _ = rls_pool; // Used implicitly via RLS when repo supports SQL
    }

    let pending = state
        .approval_request_repo
        .list_pending_by_tenant(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summaries: Vec<ApprovalRequestSummary> = pending
        .into_iter()
        .map(ApprovalRequestSummary::from)
        .collect();

    let total = summaries.len();

    Ok(Json(ListPendingApprovalRequestsResponse {
        approval_requests: summaries,
        total,
    }))
}

/// GET /approval-requests/pending - List pending approval requests for a tenant (non-JWT fallback)
///
/// Phase 2b bounded slice: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_pending_approval_requests(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListPendingApprovalRequestsQuery>,
) -> Result<Json<ListPendingApprovalRequestsResponse>, ApiErrorResponse> {
    let pending = state
        .approval_request_repo
        .list_pending_by_tenant(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summaries: Vec<ApprovalRequestSummary> = pending
        .into_iter()
        .map(ApprovalRequestSummary::from)
        .collect();

    let total = summaries.len();

    Ok(Json(ListPendingApprovalRequestsResponse {
        approval_requests: summaries,
        total,
    }))
}

/// GET /approval-requests/{id}/revalidate - Check if an approval request is still valid
///
/// Phase 1 P1-S5g bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler validates that the approval request tenant matches the JWT tenant.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// Returns 404 if:
/// - Approval request not found
/// - Approval basis snapshot not found (should exist if approval exists)
///
/// Returns 200 with valid=false if latest snapshot is missing (policy not yet computed
/// for current intent version) - this is NOT a 404, as the approval still exists
/// but we cannot determine current validity without a latest snapshot.
#[cfg(feature = "jwt-auth")]
pub async fn revalidate_approval_request(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(approval_request_id): Path<Uuid>,
) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
    // Step 1: Fetch the approval request
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 1 P1-S5g: Check if RLS path is available (pool exists AND JWT claims present)
    // Also performs tenant mismatch check
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: approval request tenant must match JWT tenant
        if approval_request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match approval request tenant_id ({})",
                rls_claims.tenant_id, approval_request.tenant_id
            );
            tracing::warn!("revalidate_approval_request: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        tracing::debug!(
            "revalidate_approval_request: RLS path validated for tenant_id={}",
            rls_claims.tenant_id
        );

        let _ = rls_pool; // Used implicitly via RLS when repo supports SQL
    }

    // Step 2: Fetch the approval-basis policy snapshot (snapshot for intent_version_from)
    let approval_basis_snapshot = state
        .policy_snapshot_repo
        .get_by_intent_version(
            approval_request.intent_id,
            approval_request.intent_version_from,
            approval_request.tenant_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    let approval_basis_scope_hash = match approval_basis_snapshot {
        Some(snapshot) => snapshot.scope_hash,
        None => {
            // Approval basis snapshot missing - this is unexpected but return 404
            return Err(ApiErrorResponse(IntentRebaseError::PolicySnapshotNotFound(
                approval_request.intent_id,
            )));
        }
    };

    // Step 3: Fetch the latest policy snapshot for this intent
    let latest_snapshot = state
        .policy_snapshot_repo
        .get_latest_by_intent(approval_request.intent_id, approval_request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 4: Compare scope_hash values
    let (valid, reason) = match &latest_snapshot {
        Some(latest) if latest.scope_hash == approval_basis_scope_hash => {
            // Scope unchanged - approval remains valid
            (
                true,
                "Scope unchanged since approval was granted".to_string(),
            )
        }
        Some(latest) if latest.scope_hash != approval_basis_scope_hash => {
            // Scope changed - approval no longer valid
            (
                false,
                "Scope has changed since approval was granted".to_string(),
            )
        }
        None => {
            // No latest snapshot available - cannot determine validity
            // Return valid=false but with a clear reason (not a 404)
            (
                false,
                "No latest policy snapshot available for comparison".to_string(),
            )
        }
        // Should not reach here, but handle defensively
        _ => (false, "Unable to determine approval validity".to_string()),
    };

    let current_scope_hash = latest_snapshot.map(|s| s.scope_hash);

    Ok(Json(ApprovalRevalidationResponse {
        approval_id: approval_request_id,
        valid,
        reason,
        approval_basis_scope_hash,
        current_scope_hash,
        revalidation_required: !valid,
        intent_id: approval_request.intent_id,
        approval_basis_version: approval_request.intent_version_from,
    }))
}

/// GET /approval-requests/{id}/revalidate - Check if an approval request is still valid (non-JWT fallback)
///
/// Phase 2b bounded slice: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn revalidate_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
    // Step 1: Fetch the approval request
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2: Fetch the approval-basis policy snapshot (snapshot for intent_version_from)
    let approval_basis_snapshot = state
        .policy_snapshot_repo
        .get_by_intent_version(
            approval_request.intent_id,
            approval_request.intent_version_from,
            approval_request.tenant_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    let approval_basis_scope_hash = match approval_basis_snapshot {
        Some(snapshot) => snapshot.scope_hash,
        None => {
            // Approval basis snapshot missing - this is unexpected but return 404
            return Err(ApiErrorResponse(IntentRebaseError::PolicySnapshotNotFound(
                approval_request.intent_id,
            )));
        }
    };

    // Step 3: Fetch the latest policy snapshot for this intent
    let latest_snapshot = state
        .policy_snapshot_repo
        .get_latest_by_intent(approval_request.intent_id, approval_request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 4: Compare scope_hash values
    let (valid, reason) = match &latest_snapshot {
        Some(latest) if latest.scope_hash == approval_basis_scope_hash => {
            // Scope unchanged - approval remains valid
            (
                true,
                "Scope unchanged since approval was granted".to_string(),
            )
        }
        Some(latest) if latest.scope_hash != approval_basis_scope_hash => {
            // Scope changed - approval no longer valid
            (
                false,
                "Scope has changed since approval was granted".to_string(),
            )
        }
        None => {
            // No latest snapshot available - cannot determine validity
            // Return valid=false but with a clear reason (not a 404)
            (
                false,
                "No latest policy snapshot available for comparison".to_string(),
            )
        }
        // Should not reach here, but handle defensively
        _ => (false, "Unable to determine approval validity".to_string()),
    };

    let current_scope_hash = latest_snapshot.map(|s| s.scope_hash);

    Ok(Json(ApprovalRevalidationResponse {
        approval_id: approval_request_id,
        valid,
        reason,
        approval_basis_scope_hash,
        current_scope_hash,
        revalidation_required: !valid,
        intent_id: approval_request.intent_id,
        approval_basis_version: approval_request.intent_version_from,
    }))
}
