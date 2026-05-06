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
use intent_rebase_types::{
    AffectedItemsPreview, EdgeType, IntentVersion, NodeType, PolicySnapshot, ScopeDefinition,
};
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

// =============================================================================
// Graph Query Types
// =============================================================================

/// Query parameters for listing graph nodes
#[derive(Debug, Deserialize)]
pub struct ListGraphNodesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub node_type: Option<NodeType>,
}

/// Query parameters for listing graph edges
#[derive(Debug, Deserialize)]
pub struct ListGraphEdgesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub from_node_id: Option<Uuid>,
    pub edge_type: Option<EdgeType>,
}

// =============================================================================
// Compensation Action Types
// =============================================================================

use compensation_service::{CompensationAction, SideEffect};

/// Query parameters for listing compensation actions
#[derive(Debug, Deserialize)]
pub struct ListCompensationActionsQuery {
    pub tenant_id: Uuid,
}

/// Response for listing compensation actions
#[derive(Debug, Serialize)]
pub struct ListCompensationActionsResponse {
    pub compensation_actions: Vec<CompensationAction>,
    pub total: usize,
}

/// Request body for approve compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct ApproveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
    /// Optional actor who approved (for audit purposes)
    #[serde(default)]
    pub approved_by: Option<String>,
}

/// Request body for waive compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct WaiveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
    /// Optional actor who waived (for audit purposes)
    #[serde(default)]
    pub waived_by: Option<String>,
}

/// Request body for execute compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteCompensationActionBody {
    /// Optional actor who executed (for audit purposes)
    #[serde(default)]
    pub executed_by: Option<String>,
}

/// Request body for reapprove compensation action (manual retry)
#[derive(Debug, Clone, Deserialize)]
pub struct ReapproveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
}

/// Response for compensation action mutation (approve/waive/execute)
#[derive(Debug, Clone, Serialize)]
pub struct CompensationActionResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub status: String,
    pub strategy_type: String,
    pub feasibility: String,
    pub rationale: String,
    pub attempt_count: i32,
    pub lock_version: i32,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub waived_at: Option<DateTime<Utc>>,
    pub waived_by: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub executed_by: Option<String>,
    pub failed_at: Option<DateTime<Utc>>,
    pub execution_result_payload: Option<serde_json::Value>,
}

impl From<CompensationAction> for CompensationActionResponse {
    fn from(action: CompensationAction) -> Self {
        // Use serde_json to serialize enum fields to snake_case strings
        // instead of Debug formatting (which produces PascalCase).
        // serde_json::to_string returns the JSON representation including quotes,
        // so we trim the surrounding quotes.
        fn to_snake_case_string<T: serde::Serialize>(val: &T) -> String {
            serde_json::to_string(val)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default()
        }

        Self {
            id: action.id,
            tenant_id: action.tenant_id,
            intent_id: action.intent_id,
            status: to_snake_case_string(&action.status),
            strategy_type: to_snake_case_string(&action.strategy_type),
            feasibility: to_snake_case_string(&action.feasibility),
            rationale: action.rationale,
            attempt_count: action.attempt_count,
            lock_version: action.lock_version,
            approved_at: action.approved_at,
            approved_by: action.approved_by,
            waived_at: action.waived_at,
            waived_by: action.waived_by,
            executed_at: action.executed_at,
            executed_by: action.executed_by,
            failed_at: action.failed_at,
            execution_result_payload: action
                .execution_result_payload
                .map(|r| serde_json::to_value(&r).unwrap_or_else(|_| serde_json::json!({}))),
        }
    }
}

// =============================================================================
// Side Effect Types
// =============================================================================

/// Query parameters for listing side effects
#[derive(Debug, Deserialize)]
pub struct ListSideEffectsQuery {
    pub tenant_id: Uuid,
}

/// Response for listing side effects
#[derive(Debug, Serialize)]
pub struct ListSideEffectsResponse {
    pub side_effects: Vec<SideEffect>,
    pub total: usize,
}

// =============================================================================
// Simulation Types
// =============================================================================

/// Query parameters for rebase simulation endpoint.
///
/// **N4-4 scope:** Deterministic/stochastic mock simulation using CompensationSimulator.
/// This is READ-ONLY simulation - does not execute real compensation actions.
#[derive(Debug, Deserialize)]
pub struct RebaseSimulationQuery {
    /// Tenant ID to scope the query (required)
    pub tenant_id: Uuid,
    /// Source intent version before rebase (required)
    pub from_version: i32,
    /// Target intent version after rebase (required)
    pub to_version: i32,
    /// Simulation mode: "deterministic" (default) or "stochastic"
    #[serde(default)]
    pub mode: Option<String>,
    /// RNG seed for stochastic mode reproducibility (optional, only used when mode=stochastic)
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Request body for POST /compensation-simulation/run endpoint.
///
/// **N4-4 scope:** Bounded read-only compensation simulation using CompensationSimulator.
/// This is READ-ONLY simulation - does not execute real compensation actions or mutate state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationSimulationRequest {
    /// Intent ID to simulate compensation for (required)
    pub intent_id: Uuid,
    /// Tenant ID to scope the query (required)
    pub tenant_id: Uuid,
    /// Source intent version before rebase (required)
    pub from_version: i32,
    /// Target intent version after rebase (required)
    pub to_version: i32,
    /// Simulation mode: "deterministic" (default) or "stochastic"
    #[serde(default)]
    pub mode: Option<String>,
    /// RNG seed for stochastic mode reproducibility (optional, only used when mode=stochastic)
    #[serde(default)]
    pub seed: Option<u64>,
    /// Optional list of specific side effect IDs to simulate.
    /// If provided, only these side effects are included in the simulation.
    /// If not provided, all side effects for the intent are simulated.
    #[serde(default)]
    pub side_effect_ids: Option<Vec<Uuid>>,
}

// =============================================================================
// Orchestration Dashboard Types
// =============================================================================

/// Query parameters for orchestration dashboard
#[derive(Debug, Deserialize)]
pub struct OrchestrationDashboardQuery {
    pub tenant_id: Uuid,
}

/// Summary counts for compensation actions by status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompensationActionStatusCounts {
    pub pending: usize,
    pub approved: usize,
    pub executed: usize,
    pub failed: usize,
    pub waived: usize,
}

/// Summary of side effects for an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffectSummary {
    pub total: usize,
    pub irreversible_count: usize,
    pub auto_compensatable_count: usize,
}

/// Summary of compensation actions for an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationActionSummary {
    pub total: usize,
    pub status_counts: CompensationActionStatusCounts,
    pub retryable_failed_count: usize,
    pub dlq_candidate_count: usize,
    pub reapprovable_count: usize,
    pub auto_executable_count: usize,
}

/// Response for the intent orchestration dashboard endpoint
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns a consolidated view
/// of side effects and compensation actions for a single intent within a tenant.
///
/// **This endpoint is READ-ONLY** - it does not trigger any mutations.
/// It only queries existing data and computes summary statistics.
///
/// **Summary fields are truthful:**
/// - `side_effect_summary` counts are derived from persisted side effects
/// - `compensation_action_summary` counts are derived from persisted compensation actions
/// - No batch execution, orchestration engine, or background processing is claimed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationDashboardResponse {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub side_effects: Vec<SideEffect>,
    pub side_effect_summary: SideEffectSummary,
    pub compensation_actions: Vec<CompensationAction>,
    pub compensation_action_summary: CompensationActionSummary,
}

// =============================================================================
// DLQ Types
// =============================================================================

/// Query parameters for listing DLQ candidates
#[derive(Debug, Deserialize)]
pub struct ListDlqCandidatesQuery {
    pub tenant_id: Uuid,
}

/// Response for listing DLQ candidates
#[derive(Debug, Clone, Serialize)]
pub struct ListDlqCandidatesResponse {
    pub dlq_candidates: Vec<CompensationAction>,
    pub total: usize,
}

// =============================================================================
// Batch Candidates Types
// =============================================================================

/// Query parameters for listing batch candidates
#[derive(Debug, Deserialize)]
pub struct ListBatchCandidatesQuery {
    pub tenant_id: Uuid,
}

/// Summary counts for batch candidate categories
#[derive(Debug, Clone, Serialize)]
pub struct BatchCandidatesSummary {
    pub pending_approval_count: usize,
    pub approved_service_executable_count: usize,
    pub retryable_failed_count: usize,
    pub dlq_count: usize,
}

/// Response for listing batch candidates across all categories
#[derive(Debug, Clone, Serialize)]
pub struct ListBatchCandidatesResponse {
    /// Actions in Pending status awaiting approval
    pub pending_approval_candidates: Vec<CompensationAction>,
    /// Approved actions with Service-executable feasibility that can be service-executed
    pub approved_service_executable_candidates: Vec<CompensationAction>,
    /// Failed actions that can be reapproved (retryable error + budget remains)
    pub retryable_failed_candidates: Vec<CompensationAction>,
    /// Failed actions that exhausted retry budget or have non-retryable errors
    pub dlq_candidates: Vec<CompensationAction>,
    /// Summary counts for each category
    pub summary: BatchCandidatesSummary,
}
