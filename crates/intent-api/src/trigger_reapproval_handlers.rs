//! Trigger reapproval handlers.
//!
//! Phase 2b ADR-07: Contains POST handler for triggering re-approval when scope changes.

use axum::{extract::State, Json};
use intent_rebase_types::get_current_trace_context;

use crate::{publish_audit_event, types::TriggerReapprovalResponse, ApiErrorResponse, AppState};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Trigger Reapproval Handlers (Phase 2b ADR-07 bounded slice)
// ============================================================================

/// POST /approval-requests/trigger-reapproval - Trigger re-approval for scope change
///
/// **ADR-07 bounded slice**: Creates a pending approval request when scope hashes differ.
///
/// **Behavior**:
/// - If `original_scope_hash != current_scope_hash`: Creates new pending approval request
/// - If `original_scope_hash == current_scope_hash`: Returns 400 Bad Request (no scope drift)
/// - If intent not found: Returns 404
///
/// **Phase 3 P3-S5 bounded RLS slice**: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler validates tenant ownership before creating the approval request.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Scope limitations**:
/// - Does NOT send notifications (Phase 3 external notification system)
/// - Cancels existing Approved approvals for same tenant+intent (non-Approved statuses unaffected)
/// - Does NOT trigger rebase or orchestration
/// - Does NOT claim production readiness
///
/// **Use case**: Called by external systems that detect scope drift and need to
/// trigger a new approval cycle while preserving audit trail.
#[cfg(feature = "jwt-auth")]
pub async fn trigger_reapproval(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<crate::types::TriggerReapprovalRequest>,
) -> Result<(axum::http::StatusCode, Json<TriggerReapprovalResponse>), ApiErrorResponse> {
    // Step 1: Check if scope hashes match — if so, return 400 (no reapproval needed)
    if request.original_scope_hash == request.current_scope_hash {
        return Err(ApiErrorResponse(
            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                "Scope hashes match — no re-approval required".into(),
            ),
        ));
    }

    // Step 2: Verify intent exists to get workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2b: Phase 3 P3-S5 tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(ref rls_claims) = optional_rls_claims {
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("trigger_reapproval: tenant mismatch rejection");
            return Err(ApiErrorResponse(
                intent_rebase_types::IntentRebaseError::Unauthorized(msg),
            ));
        }
    }

    // Actor attribution: external-api/trigger-reapproval
    let actor_id = "external-api/trigger-reapproval";

    // Step 3: Create new pending approval request using existing primitives
    let approval_request = intent_service::ApprovalRequest::new_pending(
        request.intent_id,
        request.original_version_from,
        request.current_version_to,
        intent_head.intent.workflow_id,
        intent_head.intent.tenant_id,
        actor_id,
        "external-api",
        "ScopeChange",
        &request.reapproval_reason,
    );

    // Step 3b: P1-S5f/P1-S5i RLS transaction wrapping for create+cancel
    // Check if RLS path is available (pool exists AND JWT claims present AND SQL repo)
    let created_approval;
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        if let Some(sql_repo) = state.approval_request_repo.as_sqlx_approval_repo() {
            // Use RLS-aware transaction for create+cancel
            let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
            let mut tx = match tx_result {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::Internal(format!(
                            "trigger_reapproval: failed to begin RLS transaction: {}",
                            e
                        )),
                    ));
                }
            };

            // Create approval request within transaction
            match sql_repo
                .create_approval_request_with_tx(&mut tx, &approval_request)
                .await
            {
                Ok(created) => created_approval = created,
                Err(e) => {
                    tracing::warn!("trigger_reapproval: RLS create failed, rolling back: {}", e);
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::StorageError(format!(
                            "trigger_reapproval: RLS approval creation failed: {}",
                            e
                        )),
                    ));
                }
            };

            // Cancel existing Approved approvals within the same transaction
            let cancellation_reason = format!(
                "Superseded by new approval request {} due to scope change",
                created_approval.id
            );
            match sql_repo
                .cancel_approved_by_intent_with_tx(
                    &mut tx,
                    request.intent_id,
                    intent_head.intent.tenant_id,
                    actor_id,
                    &cancellation_reason,
                )
                .await
            {
                Ok(_cancelled_count) => {
                    tracing::debug!(
                        "trigger_reapproval: cancelled {} existing approved approvals within RLS tx",
                        _cancelled_count
                    );
                }
                Err(e) => {
                    tracing::warn!("trigger_reapproval: RLS cancel failed, rolling back: {}", e);
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::StorageError(format!(
                            "trigger_reapproval: RLS cancellation failed: {}",
                            e
                        )),
                    ));
                }
            }

            // Commit the RLS transaction
            if let Err(e) = tx.commit().await {
                return Err(ApiErrorResponse(
                    intent_rebase_types::IntentRebaseError::StorageError(format!(
                        "trigger_reapproval: failed to commit RLS transaction: {}",
                        e
                    )),
                ));
            }

            tracing::debug!(
                "trigger_reapproval: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );
        } else {
            // Fallback: non-SQL repo, use bare pool create+cancel
            tracing::debug!(
                "trigger_reapproval: rls_pool set but repo doesn't support SQL, falling back to bare pool"
            );
            created_approval = state
                .approval_request_repo
                .create_approval_request(approval_request)
                .await
                .map_err(ApiErrorResponse)?;

            // Cancel any existing Approved approvals for this intent+tenant
            let _cancelled_count = crate::cancel_existing_approved_and_audit(
                &state.approval_request_repo,
                &state.audit_service,
                &state.event_publisher,
                request.intent_id,
                intent_head.intent.tenant_id,
                actor_id,
                request.original_version_from,
                request.current_version_to,
                "ScopeChange",
                created_approval.id,
            )
            .await;
        }
    } else {
        // Non-RLS path: use bare pool operations
        created_approval = state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
            .map_err(ApiErrorResponse)?;

        // Cancel any existing Approved approvals for this intent+tenant
        let _cancelled_count = crate::cancel_existing_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            request.intent_id,
            intent_head.intent.tenant_id,
            actor_id,
            request.original_version_from,
            request.current_version_to,
            "ScopeChange",
            created_approval.id,
        )
        .await;
    }

    // Step 4: Emit audit event (best-effort, post-commit)
    let audit_payload = intent_rebase_types::ApprovalRequestedAuditPayload {
        approval_request_id: created_approval.id,
        intent_id: request.intent_id,
        intent_version_from: request.original_version_from,
        intent_version_to: request.current_version_to,
        decision_class: "ScopeChange".to_string(),
        reapproval_reason: request.reapproval_reason.clone(),
        original_scope_hash: request.original_scope_hash.clone(),
        current_scope_hash: request.current_scope_hash.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_requested(
            intent_head.intent.tenant_id,
            actor_id,
            request.intent_id,
            audit_payload,
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRequested audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ApprovalRequested",
            &serde_json::to_value(serde_json::json!({
                "approval_request_id": created_approval.id,
                "intent_id": request.intent_id,
                "intent_version_from": request.original_version_from,
                "intent_version_to": request.current_version_to,
                "reason": request.reapproval_reason
            }))
            .unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Step 5: Return response
    Ok((
        axum::http::StatusCode::CREATED,
        Json(TriggerReapprovalResponse {
            approval_request_id: created_approval.id,
            intent_id: request.intent_id,
            intent_version_from: request.original_version_from,
            intent_version_to: request.current_version_to,
            status: format!("{:?}", created_approval.status),
            notification_intent: true, // Advisory only — Phase 3 handles actual delivery
            reason: request.reapproval_reason,
        }),
    ))
}

/// POST /approval-requests/trigger-reapproval - Trigger re-approval for scope change (non-JWT fallback)
///
/// **ADR-07 bounded slice**: Creates a pending approval request when scope hashes differ.
/// Non-JWT path for backward compatibility when jwt-auth feature is disabled.
///
/// **Behavior**:
/// - If `original_scope_hash != current_scope_hash`: Creates new pending approval request
/// - If `original_scope_hash == current_scope_hash`: Returns 400 Bad Request (no scope drift)
/// - If intent not found: Returns 404
///
/// **Scope limitations**:
/// - Does NOT send notifications (Phase 3 external notification system)
/// - Cancels existing Approved approvals for same tenant+intent (non-Approved statuses unaffected)
/// - Does NOT trigger rebase or orchestration
/// - Does NOT claim production readiness
#[cfg(not(feature = "jwt-auth"))]
pub async fn trigger_reapproval(
    State(state): State<AppState>,
    Json(request): Json<crate::types::TriggerReapprovalRequest>,
) -> Result<(axum::http::StatusCode, Json<TriggerReapprovalResponse>), ApiErrorResponse> {
    // Step 1: Check if scope hashes match — if so, return 400 (no reapproval needed)
    if request.original_scope_hash == request.current_scope_hash {
        return Err(ApiErrorResponse(
            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                "Scope hashes match — no re-approval required".into(),
            ),
        ));
    }

    // Step 2: Verify intent exists to get workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 3: Create new pending approval request using existing primitives
    // Actor attribution: external-api/trigger-reapproval
    let actor_id = "external-api/trigger-reapproval";

    let approval_request = intent_service::ApprovalRequest::new_pending(
        request.intent_id,
        request.original_version_from,
        request.current_version_to,
        intent_head.intent.workflow_id,
        intent_head.intent.tenant_id,
        actor_id,
        "external-api",
        "ScopeChange",
        &request.reapproval_reason,
    );

    // Step 4: Persist the approval request
    let created = state
        .approval_request_repo
        .create_approval_request(approval_request)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 4b: Cancel any existing Approved approvals for this intent+tenant
    // Uses cancel_existing_approved_and_audit helper to handle both cancellation and audit.
    // Only Approved approvals are cancelled; Pending/Rejected/Expired are not affected.
    let _cancelled_count = crate::cancel_existing_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        request.intent_id,
        intent_head.intent.tenant_id,
        actor_id,
        request.original_version_from,
        request.current_version_to,
        "ScopeChange",
        created.id,
    )
    .await;

    // Step 5: Emit audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalRequestedAuditPayload {
        approval_request_id: created.id,
        intent_id: request.intent_id,
        intent_version_from: request.original_version_from,
        intent_version_to: request.current_version_to,
        decision_class: "ScopeChange".to_string(),
        reapproval_reason: request.reapproval_reason.clone(),
        original_scope_hash: request.original_scope_hash.clone(),
        current_scope_hash: request.current_scope_hash.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_requested(
            intent_head.intent.tenant_id,
            actor_id,
            request.intent_id,
            audit_payload,
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRequested audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ApprovalRequested",
            &serde_json::to_value(serde_json::json!({
                "approval_request_id": created.id,
                "intent_id": request.intent_id,
                "intent_version_from": request.original_version_from,
                "intent_version_to": request.current_version_to,
                "reason": request.reapproval_reason
            }))
            .unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Step 6: Return response
    Ok((
        axum::http::StatusCode::CREATED,
        Json(TriggerReapprovalResponse {
            approval_request_id: created.id,
            intent_id: request.intent_id,
            intent_version_from: request.original_version_from,
            intent_version_to: request.current_version_to,
            status: format!("{:?}", created.status),
            notification_intent: true, // Advisory only — Phase 3 handles actual delivery
            reason: request.reapproval_reason,
        }),
    ))
}
