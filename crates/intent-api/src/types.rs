//! Intent API response and request types
//!
//! Phase 2: Bounded file decomposition slice. This module contains pure data types
//! for HTTP request/response handling. These types are re-exported from the crate root
//! to maintain API compatibility.
//!
//! **Bounded scope:** This is a first extraction slice. Not all types have been
//! moved here yet. Types remaining in lib.rs include AppState, handlers, middleware,
//! and complex composed types.

use chrono::{DateTime, Utc};
use intent_rebase_types::{AffectedItemsPreview, IntentVersion, PolicySnapshot, ScopeDefinition};
use intent_service::ApprovalRequest;
use rebase_engine::planner::CompensationPlanningSummary;
use rebase_engine::{
    DecisionClass, DiffRiskAnalysis, IntentVersionDiff, RiskTier, SectionDecision,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// API Error Types
// =============================================================================

/// API error response matching OpenAPI Error schema
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// =============================================================================
// Replay Types
// =============================================================================

/// Request body for replay endpoint (Phase 2b bounded replay slice).
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Note: This is cooperative signal-based replay, NOT native Temporal reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRequest {
    /// Source version for replay (optional, uses current head if not specified)
    #[serde(default)]
    pub from_version: Option<i32>,
    /// Target version for replay (required)
    pub to_version: i32,
    /// Optional specific checkpoint ID to use for replay
    #[serde(default)]
    pub checkpoint_id: Option<Uuid>,
}

/// Response for replay endpoint (Phase 2b bounded replay slice).
///
/// Reflects cooperative signal-based replay semantics using existing
/// runtime/checkpoint seams. This is NOT native Temporal reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResponse {
    pub intent_id: Uuid,
    pub from_version: i32,
    pub to_version: i32,
    pub aligned_checkpoint_id: Option<Uuid>,
    pub checkpoint_selection_outcome: String,
    pub runtime_execution_status: String,
    pub signal_sent: bool,
    pub replay_attempted: bool,
    pub replay_completed: bool,
}

// =============================================================================
// Diff/Rebase Types
// =============================================================================

/// Response for diff computation including version context, diff, and risk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub diff: IntentVersionDiff,
    pub risk: DiffRiskAnalysis,
}

/// Response for rebase preview (Phase 1 PR #16 - graph-integrated affected items)
///
/// Exposes semantically reliable planner summary fields plus graph-integrated
/// affected items when graph data is available. The `affected_items.status` field
/// indicates whether graph classification succeeded.
///
/// When `status` is `Unavailable`, the graph service was not available or the
/// IntentVersion node was not found in the graph. The endpoint remains functional
/// even without graph coverage - this is NOT an error condition.
///
/// Phase 2b: `risk_tier` is the canonical public risk enum field (Low/Medium/High/Critical).
/// `risk_level` (u8 1-5) and `decision_class` remain as supporting fields.
///
/// Phase 3 Batch 1 (bounded slice): `compensation_planning` exposes read-only
/// compensation planning summary from the rebase planner. This is a skeleton/preview
/// only — does not indicate execution capability or actual compensation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebasePreviewResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub decision_class: DecisionClass,
    pub rationale: String,
    pub section_decisions: Vec<SectionDecision>,
    pub affected_items: AffectedItemsPreview,
    pub manual_review_recommended: bool,
    /// Phase 2b: Canonical public risk tier (primary public field)
    pub risk_tier: RiskTier,
    /// Supporting risk level (1=lowest, 5=highest)
    pub risk_level: u8,
    /// Phase 3 Batch 1: Read-only compensation planning summary.
    /// This is planner-generated preview data, NOT executed compensation actions.
    /// The `ready` field indicates whether full compensation planning is available;
    /// when `false`, the action list is empty and execution is not supported.
    pub compensation_planning: CompensationPlanningSummary,
}

/// Response for rebase apply.
///
/// Phase 2b: `risk_tier` is the canonical public risk enum field (Low/Medium/High/Critical).
/// `risk_level` (u8 1-5) and `decision_class` remain as supporting fields.
///
/// Phase 3 Batch 1 (bounded slice): `compensation_planning` exposes read-only
/// compensation planning summary from the rebase planner. This is a skeleton/preview
/// only — does not indicate execution capability or actual compensation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseApplyResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub decision_class: DecisionClass,
    /// Phase 2b: Canonical public risk tier (primary public field)
    pub risk_tier: RiskTier,
    /// Supporting risk level (1=lowest, 5=highest)
    pub risk_level: u8,
    pub outcome: String,
    pub manual_review_required: bool,
    pub notification_required: bool,
    pub rationale: String,
    pub aligned_checkpoint_id: Option<Uuid>,
    pub checkpoint_alignment_outcome: Option<String>,
    pub runtime_execution_status: String,
    pub signal_sent: bool,
    pub replay_attempted: bool,
    pub replay_completed: bool,
    pub graph_updates_applied: usize,
    pub graph_updates_failed: usize,
    /// Phase 3 Batch 1: Read-only compensation planning summary.
    /// This is planner-generated preview data, NOT executed compensation actions.
    /// The `ready` field indicates whether full compensation planning is available;
    /// when `false`, the action list is empty and execution is not supported.
    pub compensation_planning: CompensationPlanningSummary,
}

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

// =============================================================================
// Policy Snapshot Types
// =============================================================================

/// Query parameters for getting policy snapshot by ID
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting latest policy snapshot by intent
#[derive(Debug, Deserialize)]
pub struct GetLatestPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting policy snapshot by intent version
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotByVersionQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for listing policy snapshots by intent
#[derive(Debug, Deserialize)]
pub struct ListPolicySnapshotsQuery {
    pub tenant_id: Uuid,
}

/// Response type for a single policy snapshot
#[derive(Debug, Serialize)]
pub struct PolicySnapshotResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version: i32,
    pub rule_pack_version: String,
    pub scope_definition: ScopeDefinition,
    pub scope_hash: String,
    pub snapshot_uri: String,
    pub created_at: DateTime<Utc>,
    pub canonicalized_at: DateTime<Utc>,
}

impl From<PolicySnapshot> for PolicySnapshotResponse {
    fn from(s: PolicySnapshot) -> Self {
        Self {
            id: s.id,
            tenant_id: s.tenant_id,
            intent_id: s.intent_id,
            intent_version: s.intent_version,
            rule_pack_version: s.rule_pack_version,
            scope_definition: s.scope_definition,
            scope_hash: s.scope_hash,
            snapshot_uri: s.snapshot_uri,
            created_at: s.created_at,
            canonicalized_at: s.canonicalized_at,
        }
    }
}

/// Response for listing policy snapshots
#[derive(Debug, Serialize)]
pub struct ListPolicySnapshotsResponse {
    pub policy_snapshots: Vec<PolicySnapshotResponse>,
    pub total: usize,
}
