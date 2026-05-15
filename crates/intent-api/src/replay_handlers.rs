//! Replay handlers (Phase 2b bounded replay slice)
//!
//! This module contains the replay_intent handler(s) for initiating bounded
//! replay operations on intent versions.
//!
//! Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
//! seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.

use axum::{
    extract::{Path, State},
    Json,
};
#[allow(unused_imports)]
use intent_rebase_types::{get_current_trace_context, IntentRebaseError, ReplayAuditPayload};
use uuid::Uuid;

use crate::types::{ReplayRequest, ReplayResponse};
use crate::{runtime_execution_status_label, ApiErrorResponse, AppState};

/// POST /intents/{intent_id}/replay - Initiate a bounded replay operation
///
/// Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
/// seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided in request, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Returns bounded replay outcome with checkpoint alignment details.
///
/// Phase 3 P1-S5i: When valid JWT claims are present, this handler validates
/// tenant ownership before initiating replay. Fails closed on tenant mismatch;
/// fails open when JWT is absent (backward compatible).
#[cfg(feature = "jwt-auth")]
pub(crate) async fn replay_intent(
    State(state): State<AppState>,
    crate::auth::OptionalRlsTenantClaims(optional_rls_claims): crate::auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    // Phase 3 P1-S5i: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        // Get intent head to find workflow_id and tenant_id
        let intent_head = state
            .service
            .get_intent_head(intent_id)
            .await
            .map_err(ApiErrorResponse)?;

        // Tenant mismatch rejection: JWT tenant must match the intent's tenant
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("replay_intent: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        let from_version = request
            .from_version
            .unwrap_or(intent_head.version.version_number);
        let to_version = request.to_version;

        // Execute bounded replay via orchestrator
        let replay_result = state
            .orchestrator
            .replay(
                intent_id,
                intent_head.intent.tenant_id,
                intent_head.intent.workflow_id,
                from_version,
                to_version,
                request.checkpoint_id,
            )
            .await
            .map_err(ApiErrorResponse)?;

        // Record ReplayInitiated audit event (best-effort)
        let actor_id = "external-api/replay";
        let audit_payload = ReplayAuditPayload {
            from_version: Some(from_version),
            to_version: Some(to_version),
            checkpoint_id: replay_result.aligned_checkpoint_id,
            checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
            replay_initiated_via: "post-intents-intent-id-replay".to_string(),
            rationale: format!(
                "Bounded replay initiated from v{} to v{} via public replay endpoint",
                from_version, to_version
            ),
        };

        if let Err(e) = state
            .audit_service
            .record_replay_initiated(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                audit_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            crate::publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "ReplayInitiated",
                &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        return Ok(Json(ReplayResponse {
            intent_id,
            from_version,
            to_version,
            aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
            checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
            runtime_execution_status: runtime_execution_status_label(
                &replay_result.runtime_execution_result.status,
            )
            .to_string(),
            signal_sent: replay_result.runtime_execution_result.signal_sent,
            replay_attempted: replay_result.runtime_execution_result.replay_attempted,
            replay_completed: replay_result.runtime_execution_result.replay_completed,
        }));
    }

    // Non-JWT path (no JWT claims) - proceed without tenant validation
    // Get intent head to find workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    let from_version = request
        .from_version
        .unwrap_or(intent_head.version.version_number);
    let to_version = request.to_version;

    // Phase 2b: Validate target version exists before attempting replay
    state
        .service
        .get_version(intent_id, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b: Validate source version exists if explicitly specified
    if request.from_version.is_some() {
        state
            .service
            .get_version(intent_id, from_version)
            .await
            .map_err(ApiErrorResponse)?;
    }

    // Execute bounded replay via orchestrator
    let replay_result = state
        .orchestrator
        .replay(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            from_version,
            to_version,
            request.checkpoint_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Record ReplayInitiated audit event (best-effort)
    let actor_id = "external-api/replay";
    let audit_payload = ReplayAuditPayload {
        from_version: Some(from_version),
        to_version: Some(to_version),
        checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
        replay_initiated_via: "post-intents-intent-id-replay".to_string(),
        rationale: format!(
            "Bounded replay initiated from v{} to v{} via public replay endpoint",
            from_version, to_version
        ),
    };

    if let Err(e) = state
        .audit_service
        .record_replay_initiated(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        crate::publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ReplayInitiated",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ReplayResponse {
        intent_id,
        from_version,
        to_version,
        aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
        runtime_execution_status: runtime_execution_status_label(
            &replay_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: replay_result.runtime_execution_result.signal_sent,
        replay_attempted: replay_result.runtime_execution_result.replay_attempted,
        replay_completed: replay_result.runtime_execution_result.replay_completed,
    }))
}

/// POST /intents/{intent_id}/replay - Initiate a bounded replay operation (non-JWT fallback)
///
/// Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
/// seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided in request, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Returns bounded replay outcome with checkpoint alignment details.
#[cfg(not(feature = "jwt-auth"))]
pub(crate) async fn replay_intent(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    // Non-JWT path - proceed without tenant validation
    // Get intent head to find workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    let from_version = request
        .from_version
        .unwrap_or(intent_head.version.version_number);
    let to_version = request.to_version;

    // Phase 2b: Validate target version exists before attempting replay
    state
        .service
        .get_version(intent_id, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b: Validate source version exists if explicitly specified
    if request.from_version.is_some() {
        state
            .service
            .get_version(intent_id, from_version)
            .await
            .map_err(ApiErrorResponse)?;
    }

    // Execute bounded replay via orchestrator
    let replay_result = state
        .orchestrator
        .replay(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            from_version,
            to_version,
            request.checkpoint_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Record ReplayInitiated audit event (best-effort)
    let actor_id = "external-api/replay";
    let audit_payload = ReplayAuditPayload {
        from_version: Some(from_version),
        to_version: Some(to_version),
        checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
        replay_initiated_via: "post-intents-intent-id-replay".to_string(),
        rationale: format!(
            "Bounded replay initiated from v{} to v{} via public replay endpoint",
            from_version, to_version
        ),
    };

    if let Err(e) = state
        .audit_service
        .record_replay_initiated(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        crate::publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ReplayInitiated",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ReplayResponse {
        intent_id,
        from_version,
        to_version,
        aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
        runtime_execution_status: runtime_execution_status_label(
            &replay_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: replay_result.runtime_execution_result.signal_sent,
        replay_attempted: replay_result.runtime_execution_result.replay_attempted,
        replay_completed: replay_result.runtime_execution_result.replay_completed,
    }))
}
