//! Rebase apply handlers.
//!
//! Bounded handler decomposition slice: Contains the rebase_apply handler
//! for applying rebase operations to intents.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use intent_rebase_types::{get_current_trace_context, AffectedItemsStatus, DiffRequest};
use rebase_engine::planner::CompensationPlanningSummary;
use rebase_engine::{classify_approvals, RiskTier};
use rebase_orchestrator::apply_pipeline::ApplyOutcome;
use uuid::Uuid;

use crate::{
    approval_invalidation::{
        apply_outcome_label, apply_status_code, cancel_existing_approved_and_audit,
        cancel_specific_approved_and_audit, checkpoint_alignment_label, publish_audit_event,
        runtime_execution_status_label, CancelApprovalContext,
    },
    ApiErrorResponse, AppState, RebaseApplyResponse,
};

// ============================================================================
// Metric Helper Functions
// ============================================================================

/// Record rebase apply request outcome
pub(crate) fn record_rebase_apply_request(status: &'static str) {
    metrics::counter!("intent_api_rebase_apply_requests_total", "status" => status).increment(1);
}

/// Record rebase apply duration
pub(crate) fn record_rebase_apply_duration(duration_secs: f64, risk_class: &'static str) {
    metrics::histogram!("intent_api_rebase_apply_duration_seconds", "risk_class" => risk_class)
        .record(duration_secs);
}

// ============================================================================
// Rebase Apply Handler (JWT-auth variant)
// ============================================================================

/// POST /intents/{intent_id}/rebase-apply - Apply a rebase to an intent
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before applying the rebase.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
#[cfg(feature = "jwt-auth")]
pub(crate) async fn rebase_apply(
    State(state): State<AppState>,
    crate::auth::OptionalRlsTenantClaims(optional_rls_claims): crate::auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<(StatusCode, Json<RebaseApplyResponse>), ApiErrorResponse> {
    let start = std::time::Instant::now();

    let intent_head = match state.service.get_intent_head(intent_id).await {
        Ok(h) => h,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("rebase_apply: tenant mismatch rejection");
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
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
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let plan = match state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let apply_result = match state
        .orchestrator
        .apply_rebase(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            &from_version,
            &to_version,
            &plan,
            &plan.affected_items,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with risk_class label
    let risk_class = match plan.risk_tier {
        RiskTier::Low => "low",
        RiskTier::Medium => "medium",
        RiskTier::High => "high",
        RiskTier::Critical => "critical",
    };
    let duration = start.elapsed().as_secs_f64();
    record_rebase_apply_duration(duration, risk_class);

    // Phase 2b bounded slice: Record audit event for all external apply outcomes
    // Best-effort actor attribution: fallback external-api/unknown
    let actor_id = "external-api/unknown";
    let audit_payload = intent_rebase_types::RebaseApplyAuditPayload {
        from_version: request.from_version,
        to_version: request.to_version,
        decision_class: format!("{:?}", plan.decision_class),
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
    };

    // Record audit event (best-effort, don't fail the response)
    if let Err(e) = state
        .audit_service
        .record_rebase_applied(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "RebaseApplied",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Phase 2b bounded slice: Create pending approval_request when blocked D/E
    if matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview) {
        let blocked_payload = intent_rebase_types::RebaseApplyBlockedAuditPayload {
            from_version: request.from_version,
            to_version: request.to_version,
            decision_class: format!("{:?}", plan.decision_class),
            risk_level: plan.risk_level,
            rationale: apply_result.rationale.clone(),
            requestor_id: actor_id.to_string(),
            requestor_type: "external-api".to_string(),
        };

        // Record blocked audit event (best-effort)
        if let Err(e) = state
            .audit_service
            .record_rebase_apply_blocked(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                blocked_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "RebaseApplyBlocked",
                &serde_json::to_value(blocked_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        // Create pending approval_request record
        let approval_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            request.from_version,
            request.to_version,
            intent_head.intent.workflow_id,
            intent_head.intent.tenant_id,
            actor_id,
            "external-api",
            &format!("{:?}", plan.decision_class),
            &apply_result.rationale,
        );

        // Only proceed with cancellation if creation succeeded
        match state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
        {
            Ok(created) => {
                // Slice 1 bounded targeted cancellation: Use classifier when graph data is available
                //
                // Check if graph data is available for targeted cancellation:
                // - affected_items.status == Available indicates graph classification succeeded
                // - Non-empty affected_approvals means we have specific approvals to target
                //
                // Fallback to flat cancellation when:
                // - Graph data is unavailable (status == Unavailable)
                // - No affected approvals identified
                // - Classifier returns empty stale_ids
                //
                // This ensures no approvals remain valid due to missing graph/classifier data.
                let use_classifier = plan.affected_items.status == AffectedItemsStatus::Available
                    && !plan.affected_items.affected_approvals.is_empty();

                if use_classifier {
                    // Get all current approval IDs for the intent to pass to classifier
                    match state
                        .approval_request_repo
                        .list_by_intent(intent_id, intent_head.intent.tenant_id)
                        .await
                    {
                        Ok(current_approvals) => {
                            // Extract approval IDs as strings for the classifier
                            let current_approval_ids: Vec<String> =
                                current_approvals.iter().map(|a| a.id.to_string()).collect();

                            // Classify approvals to determine which are stale
                            let classification = classify_approvals(&plan, &current_approval_ids);

                            if !classification.stale_ids.is_empty() {
                                // Use targeted cancellation with classifier-determined stale_ids
                                tracing::debug!(
                                    "Classifier identified {} stale approvals for targeted cancellation",
                                    classification.stale_ids.len()
                                );
                                let cancelled_count = cancel_specific_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    &classification.stale_ids,
                                    CancelApprovalContext {
                                        intent_id,
                                        tenant_id: intent_head.intent.tenant_id,
                                        actor_id: actor_id.to_string(),
                                        from_version: request.from_version,
                                        to_version: request.to_version,
                                        decision_class: format!("{:?}", plan.decision_class),
                                        new_approval_id: created.id,
                                    },
                                )
                                .await;

                                // Fall back to flat cancellation if targeted cancellation cancelled
                                // fewer approvals than expected. This handles the case where
                                // external_ref.ref_id didn't correlate correctly with ApprovalRequest.id
                                // (e.g., production graph not populated or ID mapping incomplete).
                                if cancelled_count < classification.stale_ids.len() {
                                    tracing::warn!(
                                        "Targeted cancellation cancelled {} of {} expected approvals, falling back to flat cancellation",
                                        cancelled_count,
                                        classification.stale_ids.len()
                                    );
                                    let _fallback_count = cancel_existing_approved_and_audit(
                                        &state.approval_request_repo,
                                        &state.audit_service,
                                        &state.event_publisher,
                                        intent_id,
                                        intent_head.intent.tenant_id,
                                        actor_id,
                                        request.from_version,
                                        request.to_version,
                                        &format!("{:?}", plan.decision_class),
                                        created.id,
                                    )
                                    .await;
                                }
                            } else {
                                // Classifier returned no stale_ids - fall back to flat cancellation
                                // to ensure no approvals remain valid due to missing data
                                tracing::debug!(
                                    "Classifier returned empty stale_ids, falling back to flat cancellation"
                                );
                                let _cancelled_count = cancel_existing_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    intent_id,
                                    intent_head.intent.tenant_id,
                                    actor_id,
                                    request.from_version,
                                    request.to_version,
                                    &format!("{:?}", plan.decision_class),
                                    created.id,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
                            // Failed to list approvals - fall back to flat cancellation
                            tracing::warn!(
                                "Failed to list approvals for classifier, falling back to flat cancellation: {:?}",
                                e
                            );
                            let _cancelled_count = cancel_existing_approved_and_audit(
                                &state.approval_request_repo,
                                &state.audit_service,
                                &state.event_publisher,
                                intent_id,
                                intent_head.intent.tenant_id,
                                actor_id,
                                request.from_version,
                                request.to_version,
                                &format!("{:?}", plan.decision_class),
                                created.id,
                            )
                            .await;
                        }
                    }
                } else {
                    // Graph data unavailable or no affected approvals - use flat cancellation fallback
                    // This preserves existing behavior when classifier input is missing/uncertain
                    tracing::debug!(
                        "Graph data unavailable for targeted cancellation, using flat cancellation fallback"
                    );
                    let _cancelled_count = cancel_existing_approved_and_audit(
                        &state.approval_request_repo,
                        &state.audit_service,
                        &state.event_publisher,
                        intent_id,
                        intent_head.intent.tenant_id,
                        actor_id,
                        request.from_version,
                        request.to_version,
                        &format!("{:?}", plan.decision_class),
                        created.id,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create approval_request record: {:?}", e);
            }
        }
    }

    let response = RebaseApplyResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        notification_required: apply_result.notification_required,
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    };

    record_rebase_apply_request("success");
    Ok((apply_status_code(&apply_result.outcome), Json(response)))
}

// ============================================================================
// Rebase Apply Handler (non-JWT variant)
// ============================================================================

/// POST /intents/{intent_id}/rebase-apply - Apply a rebase to an intent (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub(crate) async fn rebase_apply(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<(StatusCode, Json<RebaseApplyResponse>), ApiErrorResponse> {
    let start = std::time::Instant::now();

    let intent_head = match state.service.get_intent_head(intent_id).await {
        Ok(h) => h,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
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
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let plan = match state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let apply_result = match state
        .orchestrator
        .apply_rebase(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            &from_version,
            &to_version,
            &plan,
            &plan.affected_items,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with risk_class label
    let risk_class = match plan.risk_tier {
        RiskTier::Low => "low",
        RiskTier::Medium => "medium",
        RiskTier::High => "high",
        RiskTier::Critical => "critical",
    };
    let duration = start.elapsed().as_secs_f64();
    record_rebase_apply_duration(duration, risk_class);

    // Phase 2b bounded slice: Record audit event for all external apply outcomes
    // Best-effort actor attribution: fallback external-api/unknown
    let actor_id = "external-api/unknown";
    let audit_payload = intent_rebase_types::RebaseApplyAuditPayload {
        from_version: request.from_version,
        to_version: request.to_version,
        decision_class: format!("{:?}", plan.decision_class),
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
    };

    // Record audit event (best-effort, don't fail the response)
    if let Err(e) = state
        .audit_service
        .record_rebase_applied(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "RebaseApplied",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Phase 2b bounded slice: Create pending approval_request when blocked D/E
    if matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview) {
        let blocked_payload = intent_rebase_types::RebaseApplyBlockedAuditPayload {
            from_version: request.from_version,
            to_version: request.to_version,
            decision_class: format!("{:?}", plan.decision_class),
            risk_level: plan.risk_level,
            rationale: apply_result.rationale.clone(),
            requestor_id: actor_id.to_string(),
            requestor_type: "external-api".to_string(),
        };

        // Record blocked audit event (best-effort)
        if let Err(e) = state
            .audit_service
            .record_rebase_apply_blocked(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                blocked_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "RebaseApplyBlocked",
                &serde_json::to_value(blocked_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        // Create pending approval_request record
        let approval_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            request.from_version,
            request.to_version,
            intent_head.intent.workflow_id,
            intent_head.intent.tenant_id,
            actor_id,
            "external-api",
            &format!("{:?}", plan.decision_class),
            &apply_result.rationale,
        );

        // Only proceed with cancellation if creation succeeded
        match state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
        {
            Ok(created) => {
                // Slice 1 bounded targeted cancellation: Use classifier when graph data is available
                //
                // Check if graph data is available for targeted cancellation:
                // - affected_items.status == Available indicates graph classification succeeded
                // - Non-empty affected_approvals means we have specific approvals to target
                //
                // Fallback to flat cancellation when:
                // - Graph data is unavailable (status == Unavailable)
                // - No affected approvals identified
                // - Classifier returns empty stale_ids
                //
                // This ensures no approvals remain valid due to missing graph/classifier data.
                let use_classifier = plan.affected_items.status == AffectedItemsStatus::Available
                    && !plan.affected_items.affected_approvals.is_empty();

                if use_classifier {
                    // Get all current approval IDs for the intent to pass to classifier
                    match state
                        .approval_request_repo
                        .list_by_intent(intent_id, intent_head.intent.tenant_id)
                        .await
                    {
                        Ok(current_approvals) => {
                            // Extract approval IDs as strings for the classifier
                            let current_approval_ids: Vec<String> =
                                current_approvals.iter().map(|a| a.id.to_string()).collect();

                            // Classify approvals to determine which are stale
                            let classification = classify_approvals(&plan, &current_approval_ids);

                            if !classification.stale_ids.is_empty() {
                                // Use targeted cancellation with classifier-determined stale_ids
                                tracing::debug!(
                                    "Classifier identified {} stale approvals for targeted cancellation",
                                    classification.stale_ids.len()
                                );
                                let cancelled_count = cancel_specific_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    &classification.stale_ids,
                                    CancelApprovalContext {
                                        intent_id,
                                        tenant_id: intent_head.intent.tenant_id,
                                        actor_id: actor_id.to_string(),
                                        from_version: request.from_version,
                                        to_version: request.to_version,
                                        decision_class: format!("{:?}", plan.decision_class),
                                        new_approval_id: created.id,
                                    },
                                )
                                .await;

                                // Fall back to flat cancellation if targeted cancellation cancelled
                                // fewer approvals than expected. This handles the case where
                                // external_ref.ref_id didn't correlate correctly with ApprovalRequest.id
                                // (e.g., production graph not populated or ID mapping incomplete).
                                if cancelled_count < classification.stale_ids.len() {
                                    tracing::warn!(
                                        "Targeted cancellation cancelled {} of {} expected approvals, falling back to flat cancellation",
                                        cancelled_count,
                                        classification.stale_ids.len()
                                    );
                                    let _fallback_count = cancel_existing_approved_and_audit(
                                        &state.approval_request_repo,
                                        &state.audit_service,
                                        &state.event_publisher,
                                        intent_id,
                                        intent_head.intent.tenant_id,
                                        actor_id,
                                        request.from_version,
                                        request.to_version,
                                        &format!("{:?}", plan.decision_class),
                                        created.id,
                                    )
                                    .await;
                                }
                            } else {
                                // Classifier returned no stale_ids - fall back to flat cancellation
                                // to ensure no approvals remain valid due to missing data
                                tracing::debug!(
                                    "Classifier returned empty stale_ids, falling back to flat cancellation"
                                );
                                let _cancelled_count = cancel_existing_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    intent_id,
                                    intent_head.intent.tenant_id,
                                    actor_id,
                                    request.from_version,
                                    request.to_version,
                                    &format!("{:?}", plan.decision_class),
                                    created.id,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
                            // Failed to list approvals - fall back to flat cancellation
                            tracing::warn!(
                                "Failed to list approvals for classifier, falling back to flat cancellation: {:?}",
                                e
                            );
                            let _cancelled_count = cancel_existing_approved_and_audit(
                                &state.approval_request_repo,
                                &state.audit_service,
                                &state.event_publisher,
                                intent_id,
                                intent_head.intent.tenant_id,
                                actor_id,
                                request.from_version,
                                request.to_version,
                                &format!("{:?}", plan.decision_class),
                                created.id,
                            )
                            .await;
                        }
                    }
                } else {
                    // Graph data unavailable or no affected approvals - use flat cancellation fallback
                    // This preserves existing behavior when classifier input is missing/uncertain
                    tracing::debug!(
                        "Graph data unavailable for targeted cancellation, using flat cancellation fallback"
                    );
                    let _cancelled_count = cancel_existing_approved_and_audit(
                        &state.approval_request_repo,
                        &state.audit_service,
                        &state.event_publisher,
                        intent_id,
                        intent_head.intent.tenant_id,
                        actor_id,
                        request.from_version,
                        request.to_version,
                        &format!("{:?}", plan.decision_class),
                        created.id,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create approval_request record: {:?}", e);
            }
        }
    }

    let response = RebaseApplyResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        notification_required: apply_result.notification_required,
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    };

    record_rebase_apply_request("success");
    Ok((apply_status_code(&apply_result.outcome), Json(response)))
}
