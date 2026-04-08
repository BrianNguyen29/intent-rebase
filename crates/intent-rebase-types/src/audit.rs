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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    IntentCreated,
    IntentUpdated,
    IntentArchived,
    RebaseDetected,
    RebasePreviewGenerated,
    RebaseApplied,
    ApprovalRequired,
    ApprovalGranted,
    ApprovalRevoked,
    ArtifactProduced,
    ArtifactInvalidated,
}
