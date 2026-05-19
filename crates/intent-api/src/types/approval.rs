use chrono::{DateTime, Utc};
use intent_service::ApprovalRequest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Approval Request Types
// =============================================================================

/// Response for listing pending approval requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPendingApprovalRequestsResponse {
    pub approval_requests: Vec<ApprovalRequestSummary>,
    pub total: usize,
}

/// Summary of an approval request for list responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestSummary {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub decision_class: String,
    pub reason: String,
    pub requestor_id: String,
    pub requestor_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<ApprovalRequest> for ApprovalRequestSummary {
    fn from(req: ApprovalRequest) -> Self {
        Self {
            id: req.id,
            intent_id: req.intent_id,
            intent_version_from: req.intent_version_from,
            intent_version_to: req.intent_version_to,
            decision_class: req.decision_class,
            reason: req.reason,
            requestor_id: req.requestor_id,
            requestor_type: req.requestor_type,
            status: format!("{:?}", req.status),
            created_at: req.created_at,
            expires_at: req.expires_at,
        }
    }
}

/// Request body for approving an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveApprovalRequestBody {
    #[serde(default)]
    pub resolution_notes: Option<String>,
}

/// Request body for rejecting an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectApprovalRequestBody {
    #[serde(default)]
    pub resolution_notes: Option<String>,
}

/// Request body for expiring an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpireApprovalRequestBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response for approve/reject approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestResponse {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub status: String,
    pub resolved_by: String,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_notes: Option<String>,
}

/// Query parameters for listing pending approval requests
#[derive(Debug, Deserialize)]
pub struct ListPendingApprovalRequestsQuery {
    pub tenant_id: Uuid,
}

/// Response for approval revalidation (Phase 2b bounded slice)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRevalidationResponse {
    /// ID of the approval request being revalidated
    pub approval_id: Uuid,
    /// Whether the approval scope remains valid (scope_hash unchanged)
    pub valid: bool,
    /// Human-readable reason for invalidation status
    pub reason: String,
    /// The scope_hash at the time of original approval
    pub approval_basis_scope_hash: String,
    /// The current latest scope_hash for this intent (None if no latest snapshot exists)
    pub current_scope_hash: Option<String>,
    /// Whether re-approval would be required (always true when valid=false)
    pub revalidation_required: bool,
    /// Intent ID this approval is for
    pub intent_id: Uuid,
    /// Intent version when approval was originally granted
    pub approval_basis_version: i32,
}

/// Request body for POST /approval-requests/trigger-reapproval
///
/// **ADR-07 bounded slice**: Creates a pending approval request when scope hashes differ.
/// If scope hashes match, returns 400 Bad Request (no duplicate reapproval created).
///
/// **Scope**: Non-production bounded trigger — creates approval record and returns
/// queue intent. Does NOT send notifications, trigger orchestration, or modify
/// existing approval state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerReapprovalRequest {
    /// Intent ID to request re-approval for
    pub intent_id: Uuid,
    /// Original intent version that was previously approved
    pub original_version_from: i32,
    /// Current intent version that requires re-approval
    pub current_version_to: i32,
    /// Scope hash at the time of original approval
    pub original_scope_hash: String,
    /// Current scope hash (computed from latest intent state)
    pub current_scope_hash: String,
    /// Human-readable reason for re-approval requirement
    pub reapproval_reason: String,
}

/// Response for POST /approval-requests/trigger-reapproval
///
/// **ADR-07 bounded slice**: Returns created approval request metadata.
/// notification_intent=true is advisory only — actual notification delivery
/// is Phase 3 scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerReapprovalResponse {
    /// ID of the newly created approval request
    pub approval_request_id: Uuid,
    /// Intent ID this approval request is for
    pub intent_id: Uuid,
    /// Original version that was previously approved
    pub intent_version_from: i32,
    /// Current version requiring re-approval
    pub intent_version_to: i32,
    /// Approval status (always "Pending" for newly created requests)
    pub status: String,
    /// Advisory flag indicating notification SHOULD be sent
    /// Note: Actual notification delivery is Phase 3 scope
    pub notification_intent: bool,
    /// Human-readable reason for re-approval
    pub reason: String,
}
