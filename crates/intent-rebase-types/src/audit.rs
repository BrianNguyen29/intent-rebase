//! Audit event types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An immutable audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: AuditEventType,
    pub actor_id: String,
    pub intent_id: Option<Uuid>,
    pub artifact_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// Audit event type taxonomy for Phase 2b bounded slice
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    IntentCreated,
    IntentUpdated,
    IntentArchived,
    RebaseDetected,
    RebasePreviewGenerated,
    RebaseApplied,
    RebaseApplyBlocked,
    ApprovalRequired,
    ApprovalGranted,
    ApprovalRevoked,
    ApprovalCancelled,
    /// Phase 2b bounded expiry slice: approval request expired manually
    ApprovalExpired,
    /// Phase 2b bounded replay slice: replay initiated via public endpoint
    ReplayInitiated,
    ArtifactProduced,
    ArtifactInvalidated,
}

/// Payload for RebaseApplied audit events (Phase 2b bounded slice)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseApplyAuditPayload {
    pub from_version: i32,
    pub to_version: i32,
    pub decision_class: String,
    pub risk_level: u8,
    pub outcome: String,
    pub manual_review_required: bool,
    pub rationale: String,
    pub aligned_checkpoint_id: Option<Uuid>,
    pub checkpoint_alignment_outcome: Option<String>,
    pub runtime_execution_status: String,
    pub signal_sent: bool,
    pub replay_attempted: bool,
    pub replay_completed: bool,
    pub graph_updates_applied: usize,
    pub graph_updates_failed: usize,
}

/// Payload for RebaseApplyBlocked audit events (Phase 2b bounded slice)
/// This is emitted when external apply hits blocked D/E path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseApplyBlockedAuditPayload {
    pub from_version: i32,
    pub to_version: i32,
    pub decision_class: String,
    pub risk_level: u8,
    pub rationale: String,
    pub requestor_id: String,
    pub requestor_type: String,
}

/// Payload for ApprovalGranted audit events (Phase 2b bounded slice)
/// This is emitted when an approval request is approved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGrantedAuditPayload {
    pub approval_request_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub decision_class: String,
    pub resolved_by: String,
    pub resolution_notes: Option<String>,
}

/// Payload for ApprovalRevoked audit events (Phase 2b bounded slice)
/// This is emitted when an approval request is rejected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRevokedAuditPayload {
    pub approval_request_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub decision_class: String,
    pub resolved_by: String,
    pub resolution_notes: Option<String>,
}

/// Payload for ApprovalCancelled audit events (Phase 2b bounded slice)
/// This is emitted when pending approval requests are cancelled due to intent version change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCancelledAuditPayload {
    pub intent_id: Uuid,
    pub cancelled_version_from: i32,
    pub cancelled_version_to: i32,
    pub decision_class: String,
    pub cancelled_by: String,
    pub cancellation_reason: String,
    pub cancelled_count: usize,
}

/// Payload for ApprovalExpired audit events (Phase 2b bounded expiry slice)
/// This is emitted when an approval request is manually marked as expired
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalExpiredAuditPayload {
    pub approval_request_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub decision_class: String,
    pub expired_by: String,
    pub expiry_reason: String,
}

/// Payload for ReplayInitiated audit events (Phase 2b bounded replay slice)
/// This is emitted when a replay operation is initiated via the public replay endpoint.
/// Note: This is bounded cooperative signal-based replay, NOT native Temporal reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayAuditPayload {
    pub from_version: Option<i32>,
    pub to_version: Option<i32>,
    pub checkpoint_id: Option<Uuid>,
    pub checkpoint_selection_outcome: String,
    pub replay_initiated_via: String,
    pub rationale: String,
}

/// Phase 2b: Payload for ArtifactInvalidated audit events.
///
/// Bounded artifact invalidation: only metadata/status is updated.
/// Real S3 quarantine move is Phase 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInvalidatedAuditPayload {
    pub artifact_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    /// Reason for invalidation
    pub reason: String,
    /// Actor who triggered the invalidation
    pub initiated_by: String,
    /// Quarantine status at time of audit
    pub quarantine_status: String,
}

// =============================================================================
// Notification Types (Phase 2b bounded notifier consumer slice)
// =============================================================================

/// Phase 2b: Kinds of notification intents recorded by the notifier consumer.
///
/// This enum represents the different types of notifications that can be
/// recorded when consuming approval-related events.
///
/// **Bounded to in-memory notification recording only (Phase 2b)**:
/// - Notifications are recorded as intents in memory
/// - Actual external notification delivery (email, webhook, NATS) is Phase 3
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationKind {
    /// Notify that an approval request was granted
    ApprovalGranted,
    /// Notify that an approval request was revoked/rejected
    ApprovalRevoked,
    /// Notify that approval requests were cancelled due to intent version change
    ApprovalCancelled,
}

/// Phase 2b: A recorded notification intent from the notifier consumer.
///
/// This represents a notification that SHOULD be sent but is currently
/// just recorded in memory. Actual delivery is Phase 3.
///
/// **Bounded to in-memory recording only (Phase 2b)**:
/// - No external email/webhook/NATS delivery
/// - No retry logic or DLQ
/// - Full notification delivery infrastructure is Phase 3
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// Unique ID for this notification record
    pub id: Uuid,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Kind of notification
    pub kind: NotificationKind,
    /// Intent ID this notification is about
    pub intent_id: Uuid,
    /// Approval request ID (if applicable)
    pub approval_request_id: Option<Uuid>,
    /// Human-readable notification message
    pub message: String,
    /// When this notification was recorded
    pub recorded_at: DateTime<Utc>,
    /// Event sequence number this notification was triggered from
    pub source_sequence: u64,
}

impl NotificationRecord {
    /// Create a new notification record for approval granted.
    pub fn approval_granted(
        tenant_id: Uuid,
        intent_id: Uuid,
        approval_request_id: Uuid,
        decision_class: &str,
        source_sequence: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            kind: NotificationKind::ApprovalGranted,
            intent_id,
            approval_request_id: Some(approval_request_id),
            message: format!(
                "Approval granted for intent {} (decision class: {})",
                intent_id, decision_class
            ),
            recorded_at: Utc::now(),
            source_sequence,
        }
    }

    /// Create a new notification record for approval revoked.
    pub fn approval_revoked(
        tenant_id: Uuid,
        intent_id: Uuid,
        approval_request_id: Uuid,
        decision_class: &str,
        source_sequence: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            kind: NotificationKind::ApprovalRevoked,
            intent_id,
            approval_request_id: Some(approval_request_id),
            message: format!(
                "Approval revoked for intent {} (decision class: {})",
                intent_id, decision_class
            ),
            recorded_at: Utc::now(),
            source_sequence,
        }
    }

    /// Create a new notification record for approval cancelled.
    pub fn approval_cancelled(
        tenant_id: Uuid,
        intent_id: Uuid,
        cancelled_count: usize,
        cancellation_reason: &str,
        source_sequence: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            kind: NotificationKind::ApprovalCancelled,
            intent_id,
            approval_request_id: None,
            message: format!(
                "Approval cancelled for intent {} ({} requests): {}",
                intent_id, cancelled_count, cancellation_reason
            ),
            recorded_at: Utc::now(),
            source_sequence,
        }
    }
}
