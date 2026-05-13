//! Query handlers for side-effect and orchestration dashboard endpoints.
//!
//! Phase 3 Batch 1: Contains GET /intents/{intent_id}/side-effects and
//! GET /intents/{intent_id}/orchestration-dashboard handlers for read-only queries.

use axum::{
    extract::{Path, State},
    Json,
};
#[allow(unused_imports)]
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        CompensationActionStatusCounts, CompensationActionSummary, DownstreamSystemStatus,
        ImpactCompensation, ImpactInvalidation, ImpactProvenance, ImpactReportQuery,
        ImpactReportResponse, ImpactScope, ImpactTrigger, IngestPropagationSignalRequest,
        IngestPropagationSignalResponse, ListSideEffectsQuery, ListSideEffectsResponse,
        OrchestrationDashboardQuery, OrchestrationDashboardResponse, PropagationStatusQuery,
        PropagationStatusResponse, PropagationSummary, SafetyGateSummary, SideEffectSummary,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Side Effect Handlers (Phase 3 Batch 1 groundwork)
// ============================================================================

/// GET /intents/{intent_id}/side-effects - List side effects for an intent
///
/// Phase 3 Batch 1 (groundwork): Returns all side effects recorded for the given
/// intent, scoped to the specified tenant. Side effects are ordered by
/// occurred_at descending (newest first).
///
/// This endpoint provides the query API for compensation planning input.
/// The actual compensation planning/execution is not included in this slice.
#[cfg(feature = "jwt-auth")]
pub async fn list_side_effects(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListSideEffectsQuery>,
) -> Result<Json<ListSideEffectsResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_side_effects: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = side_effects.len();

    Ok(Json(ListSideEffectsResponse {
        side_effects,
        total,
    }))
}

/// GET /intents/{intent_id}/side-effects - List side effects for an intent (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_side_effects(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListSideEffectsQuery>,
) -> Result<Json<ListSideEffectsResponse>, ApiErrorResponse> {
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = side_effects.len();

    Ok(Json(ListSideEffectsResponse {
        side_effects,
        total,
    }))
}

// ============================================================================
// Intent Orchestration Dashboard (Phase 3 Batch 1 bounded read-only slice)
// ============================================================================

/// GET /intents/{intent_id}/orchestration-dashboard - Get orchestration dashboard for an intent
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns a consolidated view
/// of side effects and compensation actions for a single intent within a tenant.
///
/// **This endpoint is READ-ONLY** - it does not trigger compensation execution,
/// approval workflows, or any mutation. It only queries existing compensation
/// action records and side effects, then computes summary statistics.
///
/// **Truthful summary fields:**
/// - `side_effect_summary.total`: count of all side effects for this intent
/// - `side_effect_summary.irreversible_count`: count of S4Irreversible side effects
/// - `side_effect_summary.auto_compensatable_count`: count of S0/S1 side effects
/// - `compensation_action_summary.status_counts.*`: count by CompensationStatus
/// - `compensation_action_summary.retryable_failed_count`: Failed actions with retryable errors
/// - `compensation_action_summary.dlq_candidate_count`: Failed + exhausted budget OR non-retryable error
/// - `compensation_action_summary.reapprovable_count`: Failed + retryable error + remaining budget
/// - `compensation_action_summary.auto_executable_count`: Approved + Automatic feasibility
///
/// **No batch execution or orchestration engine claims:**
/// This endpoint only aggregates existing persisted data. It does not execute
/// any compensation actions, trigger workflows, or involve background processing.
#[cfg(feature = "jwt-auth")]
pub async fn get_orchestration_dashboard(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationDashboardQuery>,
) -> Result<Json<OrchestrationDashboardResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("get_orchestration_dashboard: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Fetch side effects for this intent
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Fetch compensation actions for this intent
    let compensation_actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Compute side effect summary
    let side_effect_summary = {
        let total = side_effects.len();
        let irreversible_count = side_effects
            .iter()
            .filter(|se| se.effect_class == compensation_service::SideEffectClass::S4Irreversible)
            .count();
        let auto_compensatable_count = side_effects
            .iter()
            .filter(|se| se.is_auto_compensatable())
            .count();
        SideEffectSummary {
            total,
            irreversible_count,
            auto_compensatable_count,
        }
    };

    // Compute compensation action summary
    let compensation_action_summary = {
        let total = compensation_actions.len();

        // Count by status
        let mut status_counts = CompensationActionStatusCounts::default();
        for action in &compensation_actions {
            match action.status {
                compensation_service::CompensationStatus::Pending => status_counts.pending += 1,
                compensation_service::CompensationStatus::Approved => status_counts.approved += 1,
                compensation_service::CompensationStatus::Executed => status_counts.executed += 1,
                compensation_service::CompensationStatus::Failed => status_counts.failed += 1,
                compensation_service::CompensationStatus::Waived => status_counts.waived += 1,
            }
        }

        // Count retryable failed (Failed + retryable error code)
        let retryable_failed_count = compensation_actions
            .iter()
            .filter(|action| {
                if action.status != compensation_service::CompensationStatus::Failed {
                    return false;
                }
                // Check if error is retryable
                if let Some(ref result) = action.execution_result_payload {
                    if let Some(ref error_code) = result.error_code {
                        let classification =
                            compensation_service::CompensationAction::classify_error_code(
                                error_code,
                            );
                        return classification.retryable
                            == compensation_service::RetryableErrorClass::Retryable;
                    }
                }
                false
            })
            .count();

        // Count DLQ candidates (Failed + exhausted OR non-retryable)
        let dlq_candidate_count = compensation_actions
            .iter()
            .filter(|action| action.is_dlq_candidate())
            .count();

        // Count reapprovable (Failed + retryable error + remaining budget)
        let reapprovable_count = compensation_actions
            .iter()
            .filter(|action| action.can_be_reapproved())
            .count();

        // Count service-executable (Approved + service-executable: Rollback+Automatic or CounterAction+SemiAutomatic)
        let auto_executable_count = compensation_actions
            .iter()
            .filter(|action| {
                action.status == compensation_service::CompensationStatus::Approved
                    && action.is_service_executable()
            })
            .count();

        CompensationActionSummary {
            total,
            status_counts,
            retryable_failed_count,
            dlq_candidate_count,
            reapprovable_count,
            auto_executable_count,
        }
    };

    Ok(Json(OrchestrationDashboardResponse {
        intent_id,
        tenant_id: query.tenant_id,
        side_effects,
        side_effect_summary,
        compensation_actions,
        compensation_action_summary,
    }))
}

/// GET /intents/{intent_id}/orchestration-dashboard - Get orchestration dashboard for an intent
#[cfg(not(feature = "jwt-auth"))]
pub async fn get_orchestration_dashboard(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationDashboardQuery>,
) -> Result<Json<OrchestrationDashboardResponse>, ApiErrorResponse> {
    // Fetch side effects for this intent
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Fetch compensation actions for this intent
    let compensation_actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Compute side effect summary
    let side_effect_summary = {
        let total = side_effects.len();
        let irreversible_count = side_effects
            .iter()
            .filter(|se| se.effect_class == compensation_service::SideEffectClass::S4Irreversible)
            .count();
        let auto_compensatable_count = side_effects
            .iter()
            .filter(|se| se.is_auto_compensatable())
            .count();
        SideEffectSummary {
            total,
            irreversible_count,
            auto_compensatable_count,
        }
    };

    // Compute compensation action summary
    let compensation_action_summary = {
        let total = compensation_actions.len();

        // Count by status
        let mut status_counts = CompensationActionStatusCounts::default();
        for action in &compensation_actions {
            match action.status {
                compensation_service::CompensationStatus::Pending => status_counts.pending += 1,
                compensation_service::CompensationStatus::Approved => status_counts.approved += 1,
                compensation_service::CompensationStatus::Executed => status_counts.executed += 1,
                compensation_service::CompensationStatus::Failed => status_counts.failed += 1,
                compensation_service::CompensationStatus::Waived => status_counts.waived += 1,
            }
        }

        // Count retryable failed (Failed + retryable error code)
        let retryable_failed_count = compensation_actions
            .iter()
            .filter(|action| {
                if action.status != compensation_service::CompensationStatus::Failed {
                    return false;
                }
                // Check if error is retryable
                if let Some(ref result) = action.execution_result_payload {
                    if let Some(ref error_code) = result.error_code {
                        let classification =
                            compensation_service::CompensationAction::classify_error_code(
                                error_code,
                            );
                        return classification.retryable
                            == compensation_service::RetryableErrorClass::Retryable;
                    }
                }
                false
            })
            .count();

        // Count DLQ candidates (Failed + exhausted OR non-retryable)
        let dlq_candidate_count = compensation_actions
            .iter()
            .filter(|action| action.is_dlq_candidate())
            .count();

        // Count reapprovable (Failed + retryable error + remaining budget)
        let reapprovable_count = compensation_actions
            .iter()
            .filter(|action| action.can_be_reapproved())
            .count();

        // Count service-executable (Approved + service-executable: Rollback+Automatic or CounterAction+SemiAutomatic)
        let auto_executable_count = compensation_actions
            .iter()
            .filter(|action| {
                action.status == compensation_service::CompensationStatus::Approved
                    && action.is_service_executable()
            })
            .count();

        CompensationActionSummary {
            total,
            status_counts,
            retryable_failed_count,
            dlq_candidate_count,
            reapprovable_count,
            auto_executable_count,
        }
    };

    Ok(Json(OrchestrationDashboardResponse {
        intent_id,
        tenant_id: query.tenant_id,
        side_effects,
        side_effect_summary,
        compensation_actions,
        compensation_action_summary,
    }))
}

// ============================================================================
// ImpactReport Handler (Phase 2 bounded MVP — on-demand read-only projection)
// ============================================================================

/// Build an ImpactReportResponse from existing services.
///
/// Shared helper used by both JWT and non-JWT `get_impact_report` handlers,
/// and by `get_policy_snapshot_impact_report`.
pub(crate) async fn build_impact_report_response(
    state: &AppState,
    intent_id: Uuid,
    tenant_id: Uuid,
    from_version: i32,
    to_version: i32,
) -> Result<ImpactReportResponse, ApiErrorResponse> {
    let preview = state
        .service
        .compute_rebase_preview_with_graph(intent_id, from_version, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    let compensation_actions = state
        .compensation_action_service
        .list_by_intent(intent_id, tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let policy_gate = state
        .compensation_action_service
        .evaluate_policy_gates_for_intent(intent_id, tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let affected = &preview.affected_items;

    let trigger = ImpactTrigger {
        change_summary: preview.rationale.clone(),
        risk_tier: format!("{:?}", preview.risk_tier),
        decision_class: format!("{:?}", preview.decision_class),
    };

    let scope = ImpactScope {
        affected_artifacts_count: affected.affected_artifacts.len(),
        affected_approvals_count: affected.affected_approvals.len(),
        affected_side_effects_count: affected.side_effects.len(),
    };

    let invalidation = ImpactInvalidation {
        invalidated_artifacts_count: affected
            .affected_artifacts
            .iter()
            .filter(|a| {
                a.impact == intent_rebase_types::ClassificationImpact::Direct
                    || a.impact == intent_rebase_types::ClassificationImpact::Transitive
            })
            .count(),
        invalidated_approvals_count: affected
            .affected_approvals
            .iter()
            .filter(|a| {
                a.impact == intent_rebase_types::ClassificationImpact::Direct
                    || a.impact == intent_rebase_types::ClassificationImpact::Transitive
            })
            .count(),
    };

    let compensation = ImpactCompensation {
        total_actions: compensation_actions.len(),
        eligible_count: policy_gate.summary.eligible_count,
        blocked_count: policy_gate.summary.blocked_count,
        manual_review_required_count: policy_gate.summary.manual_review_required_count,
        dlq_candidate_count: policy_gate.summary.dlq_candidate_count,
    };

    let safety_gates = SafetyGateSummary {
        open_gates: policy_gate.summary.eligible_count,
        blocked_gates: policy_gate.summary.blocked_count,
        manual_review_gates: policy_gate.summary.manual_review_required_count,
    };

    let provenance = ImpactProvenance {
        generated_at: chrono::Utc::now(),
        from_version,
        to_version,
    };

    let unsupported_items = vec![
        "propagation-status downstream tracking".to_string(),
        "cross-workflow lineage impact".to_string(),
        "checkpoint alignment recommendations".to_string(),
    ];

    Ok(ImpactReportResponse {
        intent_id,
        tenant_id,
        trigger,
        scope,
        invalidation,
        compensation,
        safety_gates,
        provenance,
        unsupported_items,
    })
}

/// GET /intents/{intent_id}/impact-report - On-demand read-only impact projection
///
/// Bounded MVP: Aggregates existing primitives (intent diff, graph affected items,
/// side effects, compensation actions, policy gate evaluation) into a single
/// transient snapshot. No persistence, no migration, no mutation.
#[cfg(feature = "jwt-auth")]
pub async fn get_impact_report(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ImpactReportQuery>,
) -> Result<Json<ImpactReportResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("get_impact_report: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Verify intent exists and fetch head for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_impact_report: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    let response = build_impact_report_response(
        &state,
        intent_id,
        query.tenant_id,
        query.from_version,
        query.to_version,
    )
    .await?;

    Ok(Json(response))
}

/// GET /intents/{intent_id}/impact-report - On-demand read-only impact projection (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn get_impact_report(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ImpactReportQuery>,
) -> Result<Json<ImpactReportResponse>, ApiErrorResponse> {
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_impact_report: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    let response = build_impact_report_response(
        &state,
        intent_id,
        query.tenant_id,
        query.from_version,
        query.to_version,
    )
    .await?;

    Ok(Json(response))
}

// ============================================================================
// Propagation Status Handler (Phase 4+ design-only; bounded stub endpoint)
// ============================================================================

/// GET /intents/{intent_id}/propagation-status — Bounded stub endpoint.
///
/// Returns a contract-shaped response with empty downstream_systems and zeroed
/// summary. Full implementation (webhook delivery, event streaming, cross-workflow
/// lineage) is Phase 4+ deferred scope.
#[cfg(feature = "jwt-auth")]
pub async fn get_propagation_status(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<PropagationStatusQuery>,
) -> Result<Json<PropagationStatusResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("get_propagation_status: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_propagation_status: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Slice 1: If propagation record repo is available, query real records;
    // otherwise fall back to empty stub (preserves backward compatibility)
    let response = if let Some(ref repo) = state.propagation_record_repo {
        let records = repo
            .list_by_intent(intent_id, query.tenant_id)
            .await
            .map_err(ApiErrorResponse)?;

        let downstream_systems: Vec<DownstreamSystemStatus> = records
            .iter()
            .map(|r| DownstreamSystemStatus {
                system_id: r.downstream_system_id.clone(),
                acknowledged_at: r.acknowledged_at,
                status: format!("{:?}", r.status).to_lowercase(),
                last_seen_version: r.last_seen_version,
            })
            .collect();

        let total = downstream_systems.len();
        let acknowledged = downstream_systems
            .iter()
            .filter(|s| s.status == "acknowledged")
            .count();
        let pending = downstream_systems
            .iter()
            .filter(|s| s.status == "pending")
            .count();
        let failed = downstream_systems
            .iter()
            .filter(|s| s.status == "failed")
            .count();

        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems,
            propagation_summary: PropagationSummary {
                total,
                acknowledged,
                pending,
                failed,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    } else {
        // Bounded stub fallback when repository is not configured
        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems: vec![],
            propagation_summary: PropagationSummary {
                total: 0,
                acknowledged: 0,
                pending: 0,
                failed: 0,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    };

    Ok(Json(response))
}

/// GET /intents/{intent_id}/propagation-status — Bounded stub endpoint (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn get_propagation_status(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<PropagationStatusQuery>,
) -> Result<Json<PropagationStatusResponse>, ApiErrorResponse> {
    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_propagation_status: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Slice 1: If propagation record repo is available, query real records;
    // otherwise fall back to empty stub (preserves backward compatibility)
    let response = if let Some(ref repo) = state.propagation_record_repo {
        let records = repo
            .list_by_intent(intent_id, query.tenant_id)
            .await
            .map_err(ApiErrorResponse)?;

        let downstream_systems: Vec<DownstreamSystemStatus> = records
            .iter()
            .map(|r| DownstreamSystemStatus {
                system_id: r.downstream_system_id.clone(),
                acknowledged_at: r.acknowledged_at,
                status: format!("{:?}", r.status).to_lowercase(),
                last_seen_version: r.last_seen_version,
            })
            .collect();

        let total = downstream_systems.len();
        let acknowledged = downstream_systems
            .iter()
            .filter(|s| s.status == "acknowledged")
            .count();
        let pending = downstream_systems
            .iter()
            .filter(|s| s.status == "pending")
            .count();
        let failed = downstream_systems
            .iter()
            .filter(|s| s.status == "failed")
            .count();

        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems,
            propagation_summary: PropagationSummary {
                total,
                acknowledged,
                pending,
                failed,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    } else {
        // Bounded stub fallback when repository is not configured
        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems: vec![],
            propagation_summary: PropagationSummary {
                total: 0,
                acknowledged: 0,
                pending: 0,
                failed: 0,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    };

    Ok(Json(response))
}

// ============================================================================
// Propagation Signal Ingestion Handler (Slice 2 bounded)
// ============================================================================

/// POST /intents/{intent_id}/propagation-signals — Bounded signal ingestion.
///
/// Records that a downstream system has been signaled for an intent change.
/// This is a bounded internal API — no actual webhook delivery or event
/// streaming occurs. The record is created with status `pending`.
#[cfg(feature = "jwt-auth")]
pub async fn ingest_propagation_signal(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(body): Json<IngestPropagationSignalRequest>,
) -> Result<Json<IngestPropagationSignalResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if body.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match body tenant_id ({})",
                rls_claims.tenant_id, body.tenant_id
            );
            tracing::warn!("ingest_propagation_signal: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != body.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match body tenant_id ({})",
            intent_head.intent.tenant_id, body.tenant_id
        );
        tracing::warn!("ingest_propagation_signal: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Require propagation_record_repo to be configured
    let repo = state.propagation_record_repo.ok_or_else(|| {
        ApiErrorResponse(IntentRebaseError::Internal(
            "Propagation record repository not configured".to_string(),
        ))
    })?;

    let record = intent_rebase_types::PropagationRecord::new(
        body.tenant_id,
        intent_id,
        body.downstream_system_id.clone(),
    );
    let record = repo.create_record(record).await.map_err(ApiErrorResponse)?;

    Ok(Json(IngestPropagationSignalResponse {
        record_id: record.id,
        intent_id,
        tenant_id: body.tenant_id,
        downstream_system_id: body.downstream_system_id,
        status: format!("{:?}", record.status).to_lowercase(),
    }))
}

/// POST /intents/{intent_id}/propagation-signals — Bounded signal ingestion (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn ingest_propagation_signal(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(body): Json<IngestPropagationSignalRequest>,
) -> Result<Json<IngestPropagationSignalResponse>, ApiErrorResponse> {
    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != body.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match body tenant_id ({})",
            intent_head.intent.tenant_id, body.tenant_id
        );
        tracing::warn!("ingest_propagation_signal: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Require propagation_record_repo to be configured
    let repo = state.propagation_record_repo.ok_or_else(|| {
        ApiErrorResponse(IntentRebaseError::Internal(
            "Propagation record repository not configured".to_string(),
        ))
    })?;

    let record = intent_rebase_types::PropagationRecord::new(
        body.tenant_id,
        intent_id,
        body.downstream_system_id.clone(),
    );
    let record = repo.create_record(record).await.map_err(ApiErrorResponse)?;

    Ok(Json(IngestPropagationSignalResponse {
        record_id: record.id,
        intent_id,
        tenant_id: body.tenant_id,
        downstream_system_id: body.downstream_system_id,
        status: format!("{:?}", record.status).to_lowercase(),
    }))
}
