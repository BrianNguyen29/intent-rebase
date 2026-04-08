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
