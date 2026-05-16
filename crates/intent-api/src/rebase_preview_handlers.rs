//! Rebase preview handlers.
//!
//! Bounded handler decomposition slice: Contains the rebase_preview handler
//! for generating rebase preview plans.
//!
//! This is a bounded non-production slice; preview logic is validated but
//! full production readiness requires external gates.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::{AffectedItemsStatus, DiffRequest};
use rebase_engine::planner::CompensationPlanningSummary;
use uuid::Uuid;

use crate::{ApiErrorResponse, AppState, RebasePreviewResponse};

// ============================================================================
// Metric Helper Functions
// ============================================================================

/// Record rebase preview request outcome
pub(crate) fn record_rebase_preview_request(status: &'static str) {
    metrics::counter!("intent_api_rebase_preview_requests_total", "status" => status).increment(1);
}

/// Record rebase preview duration
pub(crate) fn record_rebase_preview_duration(duration_secs: f64, graph_size: &'static str) {
    metrics::histogram!("intent_api_rebase_preview_duration_seconds", "graph_size" => graph_size)
        .record(duration_secs);
}

// ============================================================================
// Rebase Preview Handler (JWT-auth variant)
// ============================================================================

/// POST /intents/{intent_id}/rebase-preview - Generate rebase preview plan
///
/// Request body: { from_version, to_version }
/// Response: rebase preview with decision class, rationale, section decisions,
/// and graph-integrated affected items when available.
///
/// Phase 1 PR #16: Includes graph-integrated affected items when graph service
/// is available. The `affected_items.status` field indicates whether classification
/// succeeded. When `status` is `Unavailable`, the endpoint remains functional but
/// the affected items arrays may be incomplete.
#[cfg(feature = "jwt-auth")]
pub async fn rebase_preview(
    State(state): State<AppState>,
    crate::auth::OptionalRlsTenantClaims(optional_rls_claims): crate::auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    let start = std::time::Instant::now();

    // Phase 5.1: Fetch intent head to get tenant_id for JWT validation
    let intent_head = match state.service.get_intent_head(intent_id).await {
        Ok(h) => h,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("rebase_preview: tenant mismatch rejection");
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    // Always use graph-integrated preview - the service handles unavailability gracefully
    let plan_result = state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await;

    let plan = match plan_result {
        Ok(p) => p,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Get version info for response context
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let to_version = match state
        .service
        .get_version(intent_id, request.to_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with graph_size label (use "unknown" if affected_items unavailable)
    let graph_size = match &plan.affected_items.status {
        AffectedItemsStatus::Available => {
            let total = plan.affected_items.affected_artifacts.len()
                + plan.affected_items.affected_approvals.len()
                + plan.affected_items.side_effects.len();
            if total < 10 {
                "small"
            } else if total < 100 {
                "medium"
            } else {
                "large"
            }
        }
        _ => "unknown",
    };

    let duration = start.elapsed().as_secs_f64();
    record_rebase_preview_duration(duration, graph_size);
    record_rebase_preview_request("success");

    Ok(Json(RebasePreviewResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        rationale: plan.rationale,
        section_decisions: plan.section_decisions,
        affected_items: plan.affected_items,
        manual_review_recommended: plan.manual_review_recommended,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    }))
}

// ============================================================================
// Rebase Preview Handler (non-JWT variant)
// ============================================================================

#[cfg(not(feature = "jwt-auth"))]
pub async fn rebase_preview(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    let start = std::time::Instant::now();

    // Always use graph-integrated preview - the service handles unavailability gracefully
    let plan_result = state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await;

    let plan = match plan_result {
        Ok(p) => p,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Get version info for response context
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let to_version = match state
        .service
        .get_version(intent_id, request.to_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_preview_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with graph_size label (use "unknown" if affected_items unavailable)
    let graph_size = match &plan.affected_items.status {
        AffectedItemsStatus::Available => {
            let total = plan.affected_items.affected_artifacts.len()
                + plan.affected_items.affected_approvals.len()
                + plan.affected_items.side_effects.len();
            if total < 10 {
                "small"
            } else if total < 100 {
                "medium"
            } else {
                "large"
            }
        }
        _ => "unknown",
    };

    let duration = start.elapsed().as_secs_f64();
    record_rebase_preview_duration(duration, graph_size);
    record_rebase_preview_request("success");

    Ok(Json(RebasePreviewResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        rationale: plan.rationale,
        section_decisions: plan.section_decisions,
        affected_items: plan.affected_items,
        manual_review_recommended: plan.manual_review_recommended,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    }))
}
