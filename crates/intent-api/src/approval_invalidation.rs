//! Approval invalidation and audit helper functions.
//!
//! This module contains the shared helpers for approval cancellation and audit event publishing
//! used by rebase_apply, trigger_reapproval, approval mutation handlers, and replay handlers.
//!
//! Phase 2b bounded slice: Encapsulates the cancellation+cancel-audit pattern for
//! existing Approved approvals when creating new pending approval requests.

use axum::http::StatusCode;
use intent_rebase_types::get_current_trace_context;
use rebase_orchestrator::{ApplyOutcome, CheckpointAlignmentOutcome, RuntimeExecutionStatus};
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Label Helper Functions
// ============================================================================

/// Maps ApplyOutcome to HTTP status code.
pub fn apply_status_code(outcome: &ApplyOutcome) -> StatusCode {
    match outcome {
        ApplyOutcome::BlockedManualReview => StatusCode::ACCEPTED,
        ApplyOutcome::NoOp
        | ApplyOutcome::AutoProceeded
        | ApplyOutcome::AutoProceededWithNotification => StatusCode::OK,
    }
}

/// Maps ApplyOutcome to a string label for audit events and responses.
pub fn apply_outcome_label(outcome: &ApplyOutcome) -> &'static str {
    match outcome {
        ApplyOutcome::NoOp => "no_op",
        ApplyOutcome::AutoProceeded => "auto_proceeded",
        ApplyOutcome::AutoProceededWithNotification => "auto_proceeded_with_notification",
        ApplyOutcome::BlockedManualReview => "blocked_manual_review",
    }
}

/// Maps CheckpointAlignmentOutcome to a string label for audit events.
pub fn checkpoint_alignment_label(outcome: &CheckpointAlignmentOutcome) -> &'static str {
    match outcome {
        CheckpointAlignmentOutcome::Aligned => "aligned",
        CheckpointAlignmentOutcome::ClosestMatch => "closest_match",
        CheckpointAlignmentOutcome::NoCheckpointRequired => "no_checkpoint_required",
        CheckpointAlignmentOutcome::NoCheckpointFound => "no_checkpoint_found",
        CheckpointAlignmentOutcome::MultipleCandidates => "multiple_candidates",
    }
}

/// Maps RuntimeExecutionStatus to a string label for audit events.
pub fn runtime_execution_status_label(status: &RuntimeExecutionStatus) -> &'static str {
    match status {
        RuntimeExecutionStatus::NotApplicable => "not_applicable",
        RuntimeExecutionStatus::SkippedNotReady => "skipped_not_ready",
        RuntimeExecutionStatus::Degraded => "degraded",
        RuntimeExecutionStatus::Succeeded => "succeeded",
        RuntimeExecutionStatus::SucceededNoReplay => "succeeded_no_replay",
    }
}

// ============================================================================
// Phase 2b: Approval Invalidation Helpers (bounded cancellation slice)
// ============================================================================

/// Context for targeted approval cancellation during rebase.
#[derive(Debug, Clone)]
pub struct CancelApprovalContext {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub actor_id: String,
    pub from_version: i32,
    pub to_version: i32,
    pub decision_class: String,
    pub new_approval_id: Uuid,
}

#[allow(clippy::too_many_arguments)]
/// Cancel existing Approved approvals for an intent and emit cancellation audit event.
///
/// Phase 2b bounded invalidation: When creating a new pending approval request,
/// any existing Approved approvals for the same tenant+intent are cancelled.
/// Only Approved approvals are cancelled — Pending/Rejected/Expired are not affected.
///
/// This helper encapsulates the cancellation+cancel-audit pattern used by both
/// trigger_reapproval and rebase_apply BlockedManualReview paths.
///
/// Returns the number of cancelled approvals (0 if none or on error).
///
/// Best-effort: errors are logged but do not fail the caller.
pub async fn cancel_existing_approved_and_audit(
    approval_repo: &Arc<dyn intent_service::ApprovalRequestRepository>,
    audit_service: &Arc<dyn intent_rebase_types::AuditRepository>,
    event_publisher: &Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    intent_id: Uuid,
    tenant_id: Uuid,
    actor_id: &str,
    from_version: i32,
    to_version: i32,
    decision_class: &str,
    new_approval_id: Uuid,
) -> usize {
    let cancellation_reason = format!(
        "Superseded by new approval request {} due to rebase apply",
        new_approval_id
    );

    let cancelled_count = match approval_repo
        .cancel_approved_by_intent(intent_id, tenant_id, actor_id, &cancellation_reason)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("Failed to cancel existing approved approvals: {:?}", e);
            return 0;
        }
    };

    if cancelled_count > 0 {
        let cancel_audit_payload = intent_rebase_types::ApprovalCancelledAuditPayload {
            intent_id,
            cancelled_version_from: from_version,
            cancelled_version_to: to_version,
            decision_class: decision_class.to_string(),
            cancelled_by: actor_id.to_string(),
            cancellation_reason,
            cancelled_count,
        };

        let audit_payload_for_publish = cancel_audit_payload.clone();

        if let Err(e) = audit_service
            .record_approval_cancelled(
                tenant_id,
                actor_id,
                intent_id,
                cancel_audit_payload,
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record ApprovalCancelled audit event: {:?}", e);
        } else {
            publish_audit_event(
                event_publisher,
                tenant_id,
                "ApprovalCancelled",
                &serde_json::to_value(audit_payload_for_publish)
                    .unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }
    }

    cancelled_count
}

/// Cancel specific Approved approvals by their IDs and emit cancellation audit event.
///
/// Slice 1 bounded targeted cancellation: Uses classifier-driven stale_ids to cancel
/// only the specific approvals that are affected by the rebase, rather than cancelling
/// all approved approvals for the intent.
///
/// Only cancels approvals that are BOTH in the provided IDs AND in Approved status.
/// Other statuses (pending, rejected, expired, cancelled) are not affected.
///
/// Returns the number of cancelled approvals (0 if none or on error).
///
/// Best-effort: errors are logged but do not fail the caller.
pub async fn cancel_specific_approved_and_audit(
    approval_repo: &Arc<dyn intent_service::ApprovalRequestRepository>,
    audit_service: &Arc<dyn intent_rebase_types::AuditRepository>,
    event_publisher: &Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    stale_ids: &[String],
    ctx: CancelApprovalContext,
) -> usize {
    if stale_ids.is_empty() {
        return 0;
    }

    // Parse the stale_ids from strings to Uuids
    // If any ID fails to parse, log and skip it
    let parsed_ids: Vec<Uuid> = stale_ids
        .iter()
        .filter_map(|id_str| {
            Uuid::parse_str(id_str)
                .map_err(|e| {
                    tracing::warn!("Failed to parse stale approval ID '{}': {}", id_str, e);
                    e
                })
                .ok()
        })
        .collect();

    if parsed_ids.is_empty() {
        tracing::warn!("No valid stale approval IDs to cancel");
        return 0;
    }

    let cancellation_reason = format!(
        "Superseded by new approval request {} due to rebase apply (targeted cancellation)",
        ctx.new_approval_id
    );

    let cancelled_count = match approval_repo
        .cancel_approved_by_ids(
            &parsed_ids,
            ctx.tenant_id,
            &ctx.actor_id,
            &cancellation_reason,
        )
        .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("Failed to cancel specific approved approvals: {:?}", e);
            return 0;
        }
    };

    if cancelled_count > 0 {
        let cancel_audit_payload = intent_rebase_types::ApprovalCancelledAuditPayload {
            intent_id: ctx.intent_id,
            cancelled_version_from: ctx.from_version,
            cancelled_version_to: ctx.to_version,
            decision_class: ctx.decision_class.clone(),
            cancelled_by: ctx.actor_id.clone(),
            cancellation_reason,
            cancelled_count,
        };

        let audit_payload_for_publish = cancel_audit_payload.clone();

        if let Err(e) = audit_service
            .record_approval_cancelled(
                ctx.tenant_id,
                &ctx.actor_id,
                ctx.intent_id,
                cancel_audit_payload,
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record ApprovalCancelled audit event: {:?}", e);
        } else {
            publish_audit_event(
                event_publisher,
                ctx.tenant_id,
                "ApprovalCancelled",
                &serde_json::to_value(audit_payload_for_publish)
                    .unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }
    }

    cancelled_count
}

// ============================================================================
// Phase 2b: Event Publishing Helpers (bounded event-streaming slice)
// ============================================================================

/// Phase 2b: Publish an audit event to the event stream (best-effort, fail-open).
///
/// This function is used after successful audit persistence to also publish
/// the event to the configured event stream.
///
/// **Bounded slice behavior**:
/// - Audit persistence is the source of truth (already completed successfully)
/// - Event publishing is best-effort: failures are logged but don't fail the operation
/// - When `event_publisher` is None, this is a no-op
/// - When event_publisher fails, the overall operation continues
///
/// **Phase 3 items** (not implemented in Phase 2b):
/// - Consumers (checkpoint-creator, snapshot-creator, notifier) — JetStream pull consumer now available
/// - Dead-letter queue (DLQ) for failed event processing
/// - Consumer startup wiring and lifecycle management
pub async fn publish_audit_event(
    event_publisher: &Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    tenant_id: Uuid,
    event_type: &str,
    payload: &serde_json::Value,
) {
    let publisher = match event_publisher {
        Some(p) => p.as_ref(),
        None => return, // No publisher configured - silently skip
    };

    let subject = intent_rebase_types::EventSubject::from_audit_event(tenant_id, event_type);
    match publisher
        .publish(&subject, payload, get_current_trace_context())
        .await
    {
        intent_rebase_types::PublishResult::Published {
            subject: s,
            sequence,
        } => {
            tracing::debug!("Published audit event to '{}' (seq={})", s, sequence);
        }
        intent_rebase_types::PublishResult::Skipped { reason } => {
            tracing::warn!(
                "Skipped publishing audit event to '{}': {}",
                subject.subject,
                reason
            );
        }
    }
}
