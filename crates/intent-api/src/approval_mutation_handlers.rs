//! Approval request mutation handlers.
//!
//! Phase 2b bounded slice: Contains POST handlers for approve, reject, and
//! expire approval requests with tenant-scoped validation and audit event
//! publishing.
//!
//! This is a bounded non-production slice.

use axum::{
    extract::{Path, State},
    Json,
};
#[allow(unused_imports)]
use intent_rebase_types::{get_current_trace_context, IntentRebaseError};
use uuid::Uuid;

use crate::{
    publish_audit_event,
    types::{
        ApprovalRequestResponse, ApproveApprovalRequestBody, ExpireApprovalRequestBody,
        RejectApprovalRequestBody,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Approval Mutation Handlers (Phase 2b bounded mutation slice)
// ============================================================================

/// POST /approval-requests/{id}/approve - Approve a pending approval request
///
/// Phase 1 P1-S5b/c bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// Does NOT resume or re-trigger apply.
#[cfg(feature = "jwt-auth")]
pub async fn approve_approval_request(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ApproveApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/approver)
    let actor_id = "external-api/approver";

    // Phase 1 P1-S5b/S5c: Check if RLS path is available (pool exists AND JWT claims present)
    // Also performs tenant mismatch check
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: JWT tenant must match approval request tenant
        if approval_request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match approval request tenant_id ({})",
                rls_claims.tenant_id, approval_request.tenant_id
            );
            tracing::warn!("approve_approval_request: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and update status within the transaction
        if let Some(sql_repo) = state.approval_request_repo.as_sqlx_approval_repo() {
            let update_result = sql_repo
                .update_status_with_tx(
                    &mut tx,
                    approval_request_id,
                    intent_service::ApprovalRequestStatus::Approved,
                    actor_id,
                    body.resolution_notes.as_deref(),
                )
                .await;

            let updated = match update_result {
                Ok(u) => u,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS approval status update failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "approve_approval_request: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );

            // Emit ApprovalGranted audit event (best-effort)
            let audit_payload = intent_rebase_types::ApprovalGrantedAuditPayload {
                approval_request_id,
                intent_id: approval_request.intent_id,
                intent_version_from: approval_request.intent_version_from,
                intent_version_to: approval_request.intent_version_to,
                decision_class: approval_request.decision_class.clone(),
                resolved_by: actor_id.to_string(),
                resolution_notes: body.resolution_notes.clone(),
            };

            if let Err(e) = state
                .audit_service
                .record_approval_granted(
                    approval_request.tenant_id,
                    actor_id,
                    approval_request.intent_id,
                    audit_payload.clone(),
                    get_current_trace_context(),
                )
                .await
            {
                tracing::warn!("Failed to record ApprovalGranted audit event: {:?}", e);
            } else {
                publish_audit_event(
                    &state.event_publisher,
                    approval_request.tenant_id,
                    "ApprovalGranted",
                    &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
                )
                .await;
            }

            return Ok(Json(ApprovalRequestResponse {
                id: updated.id,
                intent_id: updated.intent_id,
                status: format!("{:?}", updated.status),
                resolved_by: updated.resolved_by.unwrap_or_default(),
                resolved_at: updated.resolved_at,
                resolution_notes: updated.resolution_notes,
            }));
        } else {
            tracing::warn!(
                "approve_approval_request: rls_pool set but repo doesn't support SQL, falling back"
            );
        }
    }

    // Non-RLS path (no JWT claims or rls_pool is None) or repo doesn't support SQL
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            intent_service::ApprovalRequestStatus::Approved,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalGranted audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalGrantedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_granted(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalGranted audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalGranted",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/approve - Approve a pending approval request (non-JWT fallback)
///
/// Phase 2b bounded slice: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn approve_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ApproveApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/approver)
    let actor_id = "external-api/approver";

    // Update status to Approved
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            intent_service::ApprovalRequestStatus::Approved,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalGranted audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalGrantedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_granted(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalGranted audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalGranted",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/reject - Reject a pending approval request
///
/// Phase 1 P1-S5b/c bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// Does NOT resume or re-trigger apply.
#[cfg(feature = "jwt-auth")]
pub async fn reject_approval_request(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<RejectApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/rejector)
    let actor_id = "external-api/rejector";

    // Phase 1 P1-S5b/S5c: Check if RLS path is available (pool exists AND JWT claims present)
    // Also performs tenant mismatch check
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: JWT tenant must match approval request tenant
        if approval_request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match approval request tenant_id ({})",
                rls_claims.tenant_id, approval_request.tenant_id
            );
            tracing::warn!("reject_approval_request: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and update status within the transaction
        if let Some(sql_repo) = state.approval_request_repo.as_sqlx_approval_repo() {
            let update_result = sql_repo
                .update_status_with_tx(
                    &mut tx,
                    approval_request_id,
                    intent_service::ApprovalRequestStatus::Rejected,
                    actor_id,
                    body.resolution_notes.as_deref(),
                )
                .await;

            let updated = match update_result {
                Ok(u) => u,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS rejection status update failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "reject_approval_request: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );

            // Emit ApprovalRevoked audit event (best-effort)
            let audit_payload = intent_rebase_types::ApprovalRevokedAuditPayload {
                approval_request_id,
                intent_id: approval_request.intent_id,
                intent_version_from: approval_request.intent_version_from,
                intent_version_to: approval_request.intent_version_to,
                decision_class: approval_request.decision_class.clone(),
                resolved_by: actor_id.to_string(),
                resolution_notes: body.resolution_notes.clone(),
            };

            if let Err(e) = state
                .audit_service
                .record_approval_revoked(
                    approval_request.tenant_id,
                    actor_id,
                    approval_request.intent_id,
                    audit_payload.clone(),
                    get_current_trace_context(),
                )
                .await
            {
                tracing::warn!("Failed to record ApprovalRevoked audit event: {:?}", e);
            } else {
                publish_audit_event(
                    &state.event_publisher,
                    approval_request.tenant_id,
                    "ApprovalRevoked",
                    &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
                )
                .await;
            }

            return Ok(Json(ApprovalRequestResponse {
                id: updated.id,
                intent_id: updated.intent_id,
                status: format!("{:?}", updated.status),
                resolved_by: updated.resolved_by.unwrap_or_default(),
                resolved_at: updated.resolved_at,
                resolution_notes: updated.resolution_notes,
            }));
        } else {
            tracing::warn!(
                "reject_approval_request: rls_pool set but repo doesn't support SQL, falling back"
            );
        }
    }

    // Non-RLS path (no JWT claims or rls_pool is None) or repo doesn't support SQL
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            intent_service::ApprovalRequestStatus::Rejected,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalRevoked audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalRevokedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_revoked(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRevoked audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalRevoked",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/reject - Reject a pending approval request (non-JWT fallback)
///
/// Phase 2b bounded slice: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn reject_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<RejectApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/rejector)
    let actor_id = "external-api/rejector";

    // Update status to Rejected
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            intent_service::ApprovalRequestStatus::Rejected,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalRevoked audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalRevokedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_revoked(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRevoked audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalRevoked",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/expire - Mark a pending approval request as expired
///
/// Phase 2b bounded slice: Manual expiry transition for pending approval requests.
/// Only updates status to expired and emits audit event.
///
/// **No automatic expiry in Phase 2b** - this is a manual transition only.
/// No background worker or automatic expiry machinery exists.
///
/// Does NOT trigger re-approval workflow or resume/re-trigger apply.
#[cfg(feature = "jwt-auth")]
pub async fn expire_approval_request(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ExpireApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to system/expire)
    let actor_id = "system/expire";

    // Use provided reason or default
    let reason = body
        .reason
        .unwrap_or_else(|| "Approval time limit exceeded".to_string());

    // Phase 1 P1-S5e: Check if RLS path is available (pool exists AND JWT claims present)
    // Also performs tenant mismatch check
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: JWT tenant must match approval request tenant
        if approval_request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match approval request tenant_id ({})",
                rls_claims.tenant_id, approval_request.tenant_id
            );
            tracing::warn!("expire_approval_request: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and expire within the transaction
        if let Some(sql_repo) = state.approval_request_repo.as_sqlx_approval_repo() {
            let expire_result = sql_repo
                .mark_expired_with_tx(&mut tx, approval_request_id, actor_id, &reason)
                .await;

            let updated = match expire_result {
                Ok(u) => u,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS expiry status update failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "expire_approval_request: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );

            // Emit ApprovalExpired audit event (best-effort)
            let audit_payload = intent_rebase_types::ApprovalExpiredAuditPayload {
                approval_request_id,
                intent_id: approval_request.intent_id,
                intent_version_from: approval_request.intent_version_from,
                intent_version_to: approval_request.intent_version_to,
                decision_class: approval_request.decision_class.clone(),
                expired_by: actor_id.to_string(),
                expiry_reason: reason.clone(),
            };

            if let Err(e) = state
                .audit_service
                .record_approval_expired(
                    approval_request.tenant_id,
                    actor_id,
                    approval_request.intent_id,
                    audit_payload.clone(),
                    get_current_trace_context(),
                )
                .await
            {
                tracing::warn!("Failed to record ApprovalExpired audit event: {:?}", e);
            } else {
                publish_audit_event(
                    &state.event_publisher,
                    approval_request.tenant_id,
                    "ApprovalExpired",
                    &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
                )
                .await;
            }

            return Ok(Json(ApprovalRequestResponse {
                id: updated.id,
                intent_id: updated.intent_id,
                status: format!("{:?}", updated.status),
                resolved_by: updated.resolved_by.unwrap_or_default(),
                resolved_at: updated.resolved_at,
                resolution_notes: updated.resolution_notes,
            }));
        } else {
            tracing::warn!(
                "expire_approval_request: rls_pool set but repo doesn't support SQL, falling back"
            );
        }
    }

    // Non-RLS path (no JWT claims or rls_pool is None) or repo doesn't support SQL
    // Use the mark_expired repository method for atomic transition
    let updated = state
        .approval_request_repo
        .mark_expired(approval_request_id, actor_id, &reason)
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalExpired audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalExpiredAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        expired_by: actor_id.to_string(),
        expiry_reason: reason.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_expired(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalExpired audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalExpired",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/expire - Mark a pending approval request as expired (non-JWT fallback)
///
/// Phase 2b bounded slice: Manual expiry transition for pending approval requests.
/// Non-JWT fallback path when jwt-auth feature is disabled.
///
/// Does NOT trigger re-approval workflow or resume/re-trigger apply.
#[cfg(not(feature = "jwt-auth"))]
pub async fn expire_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ExpireApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to system/expire)
    let actor_id = "system/expire";

    // Use provided reason or default
    let reason = body
        .reason
        .unwrap_or_else(|| "Approval time limit exceeded".to_string());

    // Use the mark_expired repository method for atomic transition
    let updated = state
        .approval_request_repo
        .mark_expired(approval_request_id, actor_id, &reason)
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalExpired audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalExpiredAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        expired_by: actor_id.to_string(),
        expiry_reason: reason.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_expired(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalExpired audit event: {:?}", e);
    } else {
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalExpired",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}
