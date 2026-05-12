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
use compensation_service::{OrchestrationActionDecision, OrchestrationRun, RunStatus};
use forensic_service::{
    BundlePurpose, BundleStatus, ExportPurpose, ExportStatus, ForensicBundle, VerificationPurpose,
    VerificationStatus,
};
use intent_rebase_types::{
    AffectedItemsPreview, EdgeType, ExternalRef, GraphEdge, GraphNode, IntentVersion, NodeType,
    PolicySnapshot, ScopeDefinition, SideEffectCaptureContext,
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

// =============================================================================
// Compensation Planner Types
// =============================================================================

/// Counts of actions by feasibility level.
#[derive(Debug, Clone, Serialize)]
pub struct FeasibilityCounts {
    pub automatic: usize,
    pub semi_automatic: usize,
    pub manual_only: usize,
    pub not_possible: usize,
}

/// Response for compensation action planning.
#[derive(Debug, Clone, Serialize)]
pub struct PlanCompensationActionsResponse {
    /// Generated compensation actions
    pub actions: Vec<CompensationActionResponse>,
    /// Total count of generated actions
    pub total: usize,
    /// Count by feasibility level
    pub feasibility_counts: FeasibilityCounts,
}

/// Request body for planning compensation actions from side effects.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanCompensationActionsRequest {
    /// Intent ID to plan compensation for
    pub intent_id: Uuid,
    /// Tenant ID for scoping
    pub tenant_id: Uuid,
    /// Source version before rebase
    pub from_version: i32,
    /// Target version after rebase
    pub to_version: i32,
    /// Workflow ID that initiated the rebase
    pub workflow_id: Uuid,
}

// =============================================================================
// Orchestration Run Types
// =============================================================================

/// Request body for creating an orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrchestrationRunRequest {
    /// List of compensation action IDs to process in this run.
    pub action_ids: Vec<Uuid>,
    /// Optional intent scope for this run.
    #[serde(default)]
    pub intent_id: Option<Uuid>,
    /// Optional actor who initiated this run (for audit purposes).
    #[serde(default)]
    pub initiated_by: Option<String>,
}

/// Query parameters for getting/listing orchestration runs.
#[derive(Debug, Deserialize)]
pub struct OrchestrationRunQuery {
    pub tenant_id: Uuid,
}

// =============================================================================
// Artifact Ingest Types
// =============================================================================

/// Request body for artifact ingest with optional side effect capture
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the artifact (e.g., from artifact service)
    pub external_ref: ExternalRef,
    /// Human-readable label for the artifact
    pub label: String,
    /// IntentVersion node IDs this artifact depends on
    pub depends_on_intent_versions: Vec<Uuid>,
    /// Optional properties to attach to the artifact node
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    /// Phase 3 Batch 1 (groundwork): Optional context for side effect capture.
    /// When provided with sufficient fields, enables capture-on-write to the
    /// compensation ledger after successful artifact ingest.
    #[serde(default)]
    pub side_effect_context: Option<SideEffectCaptureContext>,
}

/// Response for artifact ingest with side effect capture result
#[derive(Debug, Serialize)]
pub struct ArtifactIngestResponse {
    pub node: GraphNode,
    pub edges: Vec<GraphEdge>,
    /// Phase 3 Batch 1 (groundwork): Indicates whether a side effect was recorded
    pub side_effect_recorded: bool,
    pub side_effect_id: Option<Uuid>,
}

// =============================================================================
// Dry Run Types
// =============================================================================

/// Request body for dry-run orchestration action planning.
#[derive(Debug, Clone, Deserialize)]
pub struct OrchestrationDryRunRequest {
    /// List of compensation action IDs to plan for
    pub action_ids: Vec<Uuid>,
}

/// Response for dry-run orchestration action planning.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunResponse {
    /// Per-item proposals
    pub proposals: Vec<OrchestrationDryRunProposalResponse>,
    /// Actions that were not found
    pub not_found: Vec<Uuid>,
    /// Summary counts
    pub summary: OrchestrationDryRunSummaryResponse,
}

/// A single proposal from the dry-run planner.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunProposalResponse {
    /// The compensation action ID
    pub action_id: Uuid,
    /// The proposed action (approve | reapprove | execute | no_action)
    pub proposed_action: String,
    /// Human-readable reason for the proposal
    pub reason: String,
    /// Current status of the action
    pub current_status: String,
}

/// Summary for dry-run results.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunSummaryResponse {
    pub total: usize,
    pub can_approve: usize,
    pub can_reapprove: usize,
    pub can_execute: usize,
    pub no_action: usize,
    pub not_found: usize,
}

// =============================================================================
// Batch Orchestration Types
// =============================================================================

/// Query parameters for manual orchestration endpoints
#[derive(Debug, Deserialize)]
pub struct OrchestrationQuery {
    pub tenant_id: Uuid,
}

/// Request body for batch orchestration commands.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchOrchestrationRequest {
    /// List of compensation action IDs to process
    pub action_ids: Vec<Uuid>,
    /// Optional actor who initiated the batch (for audit purposes)
    #[serde(default)]
    pub initiated_by: Option<String>,
}

/// A single item outcome from a batched command.
#[derive(Debug, Clone, Serialize)]
pub struct BatchItemOutcomeResponse {
    /// The compensation action ID
    pub action_id: Uuid,
    /// Whether this item succeeded
    pub success: bool,
    /// The resulting action (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<CompensationActionResponse>,
    /// The error that occurred (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Summary for batched orchestration results.
#[derive(Debug, Clone, Serialize)]
pub struct BatchOrchestrationSummaryResponse {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub not_found: usize,
}

/// Response for batch orchestration commands.
#[derive(Debug, Clone, Serialize)]
pub struct BatchOrchestrationResponse {
    /// Per-item outcomes
    pub outcomes: Vec<BatchItemOutcomeResponse>,
    /// Actions that were not found
    pub not_found: Vec<Uuid>,
    /// Summary counts
    pub summary: BatchOrchestrationSummaryResponse,
}

// =============================================================================
// Forensic Bundle Types
// =============================================================================

/// Time range for forensic bundle request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Summary of contents in a forensic bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleContentsSummary {
    pub intent_versions: usize,
    pub artifacts: usize,
    pub approvals: usize,
    pub audit_events: usize,
    pub policy_snapshots: usize,
}

/// Integrity information for a forensic bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleIntegrityInfo {
    /// SHA-256 hash of the bundle manifest
    pub manifest_hash: String,
    /// Whether the hash chain was verified (always false for new bundles)
    pub chain_verified: bool,
    /// When integrity was computed
    pub verification_timestamp: DateTime<Utc>,
}

/// Request body for forensic bundle generation
///
/// **P4 bounded slice:** Collects real data, generates a bundle manifest,
/// persists it to S3/MinIO, and records the bundle in the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent IDs to include in the bundle
    pub intent_ids: Vec<Uuid>,
    /// Time range to collect data for
    pub time_range: ForensicBundleTimeRange,
    /// Purpose of the bundle
    pub purpose: BundlePurpose,
    /// Actor who triggered bundle generation
    #[serde(default = "default_actor")]
    pub created_by: String,
}

fn default_actor() -> String {
    "system".to_string()
}

/// Response for forensic bundle creation
///
/// **P4 bounded slice:** Returns the generated bundle manifest with
/// storage location and size. The bundle bytes are already persisted
/// to S3/MinIO when this response is returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleResponse {
    /// Unique identifier for the generated bundle
    pub bundle_id: Uuid,
    /// When the bundle was created
    pub created_at: DateTime<Utc>,
    /// Actor who triggered bundle generation
    pub created_by: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Time range covered by the bundle
    pub time_range: ForensicBundleTimeRange,
    /// Bundle generation status (always "ready" on success)
    pub status: BundleStatus,
    /// Purpose of the bundle
    pub purpose: BundlePurpose,
    /// Summary of bundle contents
    pub contents: ForensicBundleContentsSummary,
    /// Integrity information
    pub integrity: ForensicBundleIntegrityInfo,
    /// Storage location (S3/MinIO path)
    pub storage_location: String,
    /// Size of stored bundle in bytes
    pub bundle_size_bytes: usize,
    /// Human-readable message
    pub message: String,
}

// =============================================================================
// Forensic Bundle Replay Types
// =============================================================================

/// Request body for forensic bundle replay verification.
///
/// **Bounded replay evidence slice:** Provides content sections to verify against
/// the per-section hashes stored in the bundle manifest. This is read-only
/// integrity verification, not full runtime/state reconstruction replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleReplayRequest {
    /// Tenant ID for access validation
    pub tenant_id: Uuid,
    /// Intent version entries to verify against the bundle
    pub intent_versions: Vec<forensic_service::IntentVersionEntry>,
    /// Artifact entries to verify against the bundle
    pub artifacts: Vec<forensic_service::ArtifactEntry>,
    /// Approval entries to verify against the bundle
    pub approvals: Vec<forensic_service::ApprovalEntry>,
    /// Audit event entries to verify against the bundle
    pub audit_events: Vec<forensic_service::AuditEventEntry>,
    /// Policy snapshot entries to verify against the bundle
    pub policy_snapshots: Vec<forensic_service::PolicySnapshotEntry>,
}

/// Response for forensic bundle replay verification.
///
/// **Bounded replay evidence slice:** Returns the result of verifying provided
/// content sections against the stored per-section integrity hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleReplayResponse {
    /// Bundle ID that was verified
    pub bundle_id: Uuid,
    /// Whether all sections passed verification
    pub overall_verified: bool,
    /// Number of sections that passed
    pub sections_passed: usize,
    /// Number of sections that failed
    pub sections_failed: usize,
    /// Human-readable summary
    pub summary: String,
    /// Per-section verification results
    pub sections: Vec<forensic_service::ReplaySectionResult>,
}

// =============================================================================
// Forensic Export Types
// =============================================================================

/// Request for forensic archive export
///
/// **Phase 3 Batch 3b (bounded slice):** Triggers in-memory archive generation
/// from the given parameters. The archive contains scaffolded/fictional data
/// representing what a real bundle would contain.
///
/// **Truthful semantics:**
/// - Generated archive is entirely in-memory with scaffolded entries
/// - Does NOT query actual services for real intent versions, artifacts, etc.
/// - `item_count` reflects the configured generator counts, not actual data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to generate archive for
    pub intent_id: Uuid,
    /// Time range to include in archive
    pub time_range: ForensicExportTimeRange,
    /// Purpose of the archive
    #[serde(default)]
    pub purpose: ExportPurpose,
    /// Whether to include artifact entries
    #[serde(default = "default_export_include_artifacts")]
    pub include_artifacts: bool,
    /// Whether to include audit event entries
    #[serde(default = "default_export_include_audit_events")]
    pub include_audit_events: bool,
    /// Whether to include policy snapshot entries
    #[serde(default = "default_export_include_policy_snapshots")]
    pub include_policy_snapshots: bool,
}

fn default_export_include_artifacts() -> bool {
    true
}

fn default_export_include_audit_events() -> bool {
    true
}

fn default_export_include_policy_snapshots() -> bool {
    true
}

/// Time range for forensic export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Response for forensic archive export
///
/// **Phase 3 Batch 3b (bounded slice):** Returns archive metadata and size
/// for the generated in-memory archive. The actual archive content is NOT
/// embedded in this response — it is generated on-demand.
///
/// **Truthful semantics:**
/// - `archive_id` is a unique identifier for the generated archive
/// - `generated_at` timestamps when generation was triggered
/// - `item_count` is the count of scaffolded entries generated
/// - `archive_size_bytes` reflects the JSON-serialized size of the archive
///
/// **NOT claimed:**
/// - Actual bundle generation from real services
/// - Bundle storage (S3 or any persistence)
/// - Async job orchestration
/// - Real replay engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportResponse {
    /// Unique identifier for this archive
    pub archive_id: Uuid,
    /// When archive was generated
    pub generated_at: DateTime<Utc>,
    /// Export status
    pub status: ExportStatus,
    /// Human-readable status reason
    pub status_reason: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID
    pub intent_id: Uuid,
    /// Time range covered
    pub time_range: ForensicExportTimeRange,
    /// Purpose of archive
    pub purpose: ExportPurpose,
    /// Summary of archive contents
    pub contents: ForensicExportContentsSummary,
    /// Total item count
    pub item_count: usize,
    /// Content type (application/json)
    pub content_type: String,
    /// Archive size in bytes
    pub archive_size_bytes: usize,
}

/// Summary of contents in an export archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportContentsSummary {
    /// Number of intent version entries
    pub intent_versions: usize,
    /// Number of artifact entries
    pub artifacts: usize,
    /// Number of audit event entries
    pub audit_events: usize,
    /// Number of policy snapshot entries
    pub policy_snapshots: usize,
}

// =============================================================================
// List Forensic Bundles Types
// =============================================================================

/// Query parameters for listing forensic bundles
#[derive(Debug, Deserialize)]
pub struct ListForensicBundlesQuery {
    pub tenant_id: Uuid,
    /// Optional limit for the number of bundles to return
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response for listing forensic bundles
#[derive(Debug, Serialize)]
pub struct ListForensicBundlesResponse {
    pub bundles: Vec<ForensicBundleSummary>,
    pub total: usize,
}

/// Summary of a forensic bundle for list responses
#[derive(Debug, Serialize)]
pub struct ForensicBundleSummary {
    pub bundle_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub tenant_id: Uuid,
    pub time_range: ForensicBundleTimeRange,
    pub status: BundleStatus,
    pub purpose: BundlePurpose,
    pub contents: ForensicBundleContentsSummary,
    pub integrity: ForensicBundleIntegrityInfo,
}

impl From<ForensicBundle> for ForensicBundleSummary {
    fn from(bundle: ForensicBundle) -> Self {
        Self {
            bundle_id: bundle.bundle_id,
            created_at: bundle.created_at,
            created_by: bundle.created_by,
            tenant_id: bundle.tenant_id,
            time_range: ForensicBundleTimeRange {
                start: bundle.time_range.start,
                end: bundle.time_range.end,
            },
            status: bundle.status,
            purpose: bundle.purpose,
            contents: ForensicBundleContentsSummary {
                intent_versions: bundle.contents.intent_versions,
                artifacts: bundle.contents.artifacts,
                approvals: bundle.contents.approvals,
                audit_events: bundle.contents.audit_events,
                policy_snapshots: bundle.contents.policy_snapshots,
            },
            integrity: ForensicBundleIntegrityInfo {
                manifest_hash: bundle.integrity.manifest_hash,
                chain_verified: bundle.integrity.chain_verified,
                verification_timestamp: bundle.integrity.verification_timestamp,
            },
        }
    }
}

// =============================================================================
// Policy Gate Evaluation Types (Phase 3 Batch 1 bounded read-only slice)
// =============================================================================

use compensation_service::{
    CompensationFeasibility, CompensationStatus, ErrorClassification, ErrorSeverity,
    FeasibilityRisk, PolicyGateEvaluation as ServicePolicyGateEvaluation,
    PolicyGateEvaluationResult as ServicePolicyGateEvaluationResult,
    PolicyGateMetadata as ServicePolicyGateMetadata, PolicyGateStatus as ServicePolicyGateStatus,
    PolicyGateSummary as ServicePolicyGateSummary, RetryExhaustionRisk,
    RiskMetadata as ServiceRiskMetadata, StrategySeverity, StrategyType,
};

/// Query parameters for tenant-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct CompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct IntentCompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// API response for policy gate evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationPolicyGateResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub evaluations: Vec<PolicyGateEvaluationResponse>,
    pub summary: PolicyGateSummaryResponse,
}

/// Policy gate evaluation for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateEvaluationResponse {
    pub action: compensation_service::CompensationAction,
    pub gate_status: String,
    pub gate_reason: String,
    pub policy_metadata: PolicyGateMetadataResponse,
    pub risk_metadata: RiskMetadataResponse,
}

/// Policy gate metadata for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateMetadataResponse {
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub has_non_retryable_error: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Risk metadata for a single action (API version)
///
/// Phase 3 Batch 1 (bounded read-only slice): Derived from existing action state fields.
/// Provides risk-relevant signals: strategy severity, retry exhaustion risk, feasibility risk,
/// error severity, remaining retry budget, error classification, terminal state flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetadataResponse {
    pub strategy_severity: String,
    pub retry_exhaustion_risk: String,
    pub feasibility_risk: String,
    pub error_severity: String,
    pub retry_budget_remaining: i32,
    pub error_classification: Option<ErrorClassificationResponse>,
    pub is_terminal: bool,
    pub requires_manual_intervention: bool,
}

/// Error classification response (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorClassificationResponse {
    pub error_code: String,
    pub retryable: bool,
    pub reason: String,
}

/// Summary of policy gate evaluations (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateSummaryResponse {
    pub total_actions: usize,
    pub eligible_count: usize,
    pub blocked_count: usize,
    pub manual_review_required_count: usize,
    pub dlq_candidate_count: usize,
    pub pending_approval_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServicePolicyGateEvaluationResult> for CompensationPolicyGateResponse {
    fn from(result: ServicePolicyGateEvaluationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            evaluations: result
                .evaluations
                .into_iter()
                .map(PolicyGateEvaluationResponse::from)
                .collect(),
            summary: PolicyGateSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServicePolicyGateEvaluation> for PolicyGateEvaluationResponse {
    fn from(eval: ServicePolicyGateEvaluation) -> Self {
        Self {
            action: eval.action,
            gate_status: format_gate_status(&eval.gate_status),
            gate_reason: eval.gate_reason,
            policy_metadata: PolicyGateMetadataResponse::from(eval.policy_metadata),
            risk_metadata: RiskMetadataResponse::from(eval.risk_metadata),
        }
    }
}

impl From<ServicePolicyGateMetadata> for PolicyGateMetadataResponse {
    fn from(meta: ServicePolicyGateMetadata) -> Self {
        Self {
            auto_executable: meta.auto_executable,
            is_dlq_candidate: meta.is_dlq_candidate,
            can_reapprove: meta.can_reapprove,
            retry_budget_exhausted: meta.retry_budget_exhausted,
            has_non_retryable_error: meta.has_non_retryable_error,
            feasibility: format_feasibility(&meta.feasibility),
            strategy_type: format_strategy_type(&meta.strategy_type),
            status: format_compensation_status(&meta.status),
            attempt_count: meta.attempt_count,
            max_retries: meta.max_retries,
        }
    }
}

impl From<ServicePolicyGateSummary> for PolicyGateSummaryResponse {
    fn from(summary: ServicePolicyGateSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            eligible_count: summary.eligible_count,
            blocked_count: summary.blocked_count,
            manual_review_required_count: summary.manual_review_required_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            pending_approval_count: summary.pending_approval_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}

impl From<ServiceRiskMetadata> for RiskMetadataResponse {
    fn from(risk: ServiceRiskMetadata) -> Self {
        Self {
            strategy_severity: format_strategy_severity(&risk.strategy_severity),
            retry_exhaustion_risk: format_retry_exhaustion_risk(&risk.retry_exhaustion_risk),
            feasibility_risk: format_feasibility_risk(&risk.feasibility_risk),
            error_severity: format_error_severity(&risk.error_severity),
            retry_budget_remaining: risk.retry_budget_remaining,
            error_classification: risk
                .error_classification
                .map(ErrorClassificationResponse::from),
            is_terminal: risk.is_terminal,
            requires_manual_intervention: risk.requires_manual_intervention,
        }
    }
}

impl From<ErrorClassification> for ErrorClassificationResponse {
    fn from(ec: ErrorClassification) -> Self {
        Self {
            error_code: ec.error_code,
            retryable: ec.retryable,
            reason: ec.reason,
        }
    }
}

// Pure formatting helpers for Policy Gate types
pub(crate) fn format_gate_status(status: &ServicePolicyGateStatus) -> String {
    match status {
        ServicePolicyGateStatus::Eligible => "eligible".to_string(),
        ServicePolicyGateStatus::Blocked => "blocked".to_string(),
        ServicePolicyGateStatus::ManualReviewRequired => "manual_review_required".to_string(),
    }
}

pub(crate) fn format_feasibility(f: &CompensationFeasibility) -> String {
    match f {
        CompensationFeasibility::Automatic => "automatic".to_string(),
        CompensationFeasibility::SemiAutomatic => "semi_automatic".to_string(),
        CompensationFeasibility::ManualOnly => "manual_only".to_string(),
        CompensationFeasibility::NotPossible => "not_possible".to_string(),
    }
}

pub(crate) fn format_strategy_type(s: &StrategyType) -> String {
    match s {
        StrategyType::Rollback => "rollback".to_string(),
        StrategyType::CounterAction => "counter_action".to_string(),
        StrategyType::FollowupNotice => "followup_notice".to_string(),
        StrategyType::Quarantine => "quarantine".to_string(),
        StrategyType::Escalation => "escalation".to_string(),
    }
}

pub(crate) fn format_compensation_status(s: &CompensationStatus) -> String {
    match s {
        CompensationStatus::Pending => "pending".to_string(),
        CompensationStatus::Approved => "approved".to_string(),
        CompensationStatus::Executed => "executed".to_string(),
        CompensationStatus::Failed => "failed".to_string(),
        CompensationStatus::Waived => "waived".to_string(),
    }
}

pub(crate) fn format_strategy_severity(s: &StrategySeverity) -> String {
    match s {
        StrategySeverity::Low => "low".to_string(),
        StrategySeverity::Medium => "medium".to_string(),
        StrategySeverity::High => "high".to_string(),
        StrategySeverity::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_retry_exhaustion_risk(r: &RetryExhaustionRisk) -> String {
    match r {
        RetryExhaustionRisk::Low => "low".to_string(),
        RetryExhaustionRisk::Medium => "medium".to_string(),
        RetryExhaustionRisk::High => "high".to_string(),
        RetryExhaustionRisk::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_feasibility_risk(f: &FeasibilityRisk) -> String {
    match f {
        FeasibilityRisk::Low => "low".to_string(),
        FeasibilityRisk::Medium => "medium".to_string(),
        FeasibilityRisk::High => "high".to_string(),
        FeasibilityRisk::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_error_severity(e: &ErrorSeverity) -> String {
    match e {
        ErrorSeverity::None => "none".to_string(),
        ErrorSeverity::Low => "low".to_string(),
        ErrorSeverity::Medium => "medium".to_string(),
        ErrorSeverity::High => "high".to_string(),
    }
}

// =============================================================================
// Orchestration Coordination Types (Phase 3 Batch 1 bounded read-only view)
// =============================================================================

use compensation_service::{
    CoordinationRecord as ServiceCoordinationRecord,
    CoordinationResult as ServiceCoordinationResult,
    CoordinationStatus as ServiceCoordinationStatus,
    CoordinationSummary as ServiceCoordinationSummary,
};

/// Query parameters for tenant-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct OrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct IntentOrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// API response for orchestration coordination status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationCoordinationResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub records: Vec<CoordinationRecordResponse>,
    pub summary: CoordinationSummaryResponse,
}

/// Coordination record for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationRecordResponse {
    pub action: CompensationAction,
    pub coordination_status: String,
    pub coordination_reason: String,
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Summary of coordination records (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationSummaryResponse {
    pub total_actions: usize,
    pub ready_count: usize,
    pub awaiting_policy_count: usize,
    pub awaiting_manual_review_count: usize,
    pub blocked_count: usize,
    pub terminal_count: usize,
    pub dlq_candidate_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServiceCoordinationResult> for OrchestrationCoordinationResponse {
    fn from(result: ServiceCoordinationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            records: result
                .records
                .into_iter()
                .map(CoordinationRecordResponse::from)
                .collect(),
            summary: CoordinationSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServiceCoordinationRecord> for CoordinationRecordResponse {
    fn from(record: ServiceCoordinationRecord) -> Self {
        Self {
            action: record.action,
            coordination_status: format_coordination_status(&record.coordination_status),
            coordination_reason: record.coordination_reason,
            auto_executable: record.auto_executable,
            is_dlq_candidate: record.is_dlq_candidate,
            can_reapprove: record.can_reapprove,
            retry_budget_exhausted: record.retry_budget_exhausted,
            feasibility: format_feasibility(&record.feasibility),
            strategy_type: format_strategy_type(&record.strategy_type),
            status: format_compensation_status(&record.status),
            attempt_count: record.attempt_count,
            max_retries: record.max_retries,
        }
    }
}

impl From<ServiceCoordinationSummary> for CoordinationSummaryResponse {
    fn from(summary: ServiceCoordinationSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            ready_count: summary.ready_count,
            awaiting_policy_count: summary.awaiting_policy_count,
            awaiting_manual_review_count: summary.awaiting_manual_review_count,
            blocked_count: summary.blocked_count,
            terminal_count: summary.terminal_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}

pub(crate) fn format_coordination_status(status: &ServiceCoordinationStatus) -> String {
    match status {
        ServiceCoordinationStatus::Ready => "ready".to_string(),
        ServiceCoordinationStatus::AwaitingPolicy => "awaiting_policy".to_string(),
        ServiceCoordinationStatus::AwaitingManualReview => "awaiting_manual_review".to_string(),
        ServiceCoordinationStatus::Blocked => "blocked".to_string(),
        ServiceCoordinationStatus::Terminal => "terminal".to_string(),
    }
}

// =============================================================================
// Forensic Verification Types
// =============================================================================

/// Request body for forensic verification
///
/// **Phase 3 Batch 3b (bounded slice):** Verifies forensic bundle feasibility
/// for the given parameters WITHOUT generating actual bundles or storing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to verify forensic coverage for
    pub intent_id: Uuid,
    /// Time range to verify
    pub time_range: ForensicVerificationTimeRange,
    /// Purpose of the verification
    #[serde(default)]
    pub purpose: VerificationPurpose,
    /// Whether to verify artifact coverage
    #[serde(default = "default_include_artifacts")]
    pub include_artifacts: bool,
    /// Whether to verify audit event coverage
    #[serde(default = "default_include_audit_events")]
    pub include_audit_events: bool,
    /// Whether to verify policy snapshot coverage
    #[serde(default = "default_include_policy_snapshots")]
    pub include_policy_snapshots: bool,
}

fn default_include_artifacts() -> bool {
    true
}

fn default_include_audit_events() -> bool {
    true
}

fn default_include_policy_snapshots() -> bool {
    true
}

/// Time range for forensic verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Response for forensic verification
///
/// **Phase 3 Batch 3b (bounded slice):** Reports what a forensic bundle WOULD contain
/// if generated with the given parameters. This is verification/reporting ONLY.
///
/// **Truthful semantics:**
/// - `status: "ready"` means all referenced entities exist and are within time range
/// - `status: "incomplete"` means some entities are missing or time range has gaps
/// - `estimated_bundle_item_count` is an estimate, NOT actual bundle size
///
/// **NOT claimed:**
/// - Actual bundle generation (no data is collected)
/// - Bundle storage (no S3 or persistence writes)
/// - Bundle retrieval (no stored bundle download)
/// - Bundle replay (no state reproduction)
/// - Hash chain integrity (requires generated bundle)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationResponse {
    /// Unique identifier for this verification
    pub verification_id: Uuid,
    /// When verification was performed
    pub verified_at: DateTime<Utc>,
    /// Verification status
    pub status: VerificationStatus,
    /// Human-readable status reason
    pub status_reason: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID
    pub intent_id: Uuid,
    /// Time range that was verified
    pub time_range: ForensicVerificationTimeRange,
    /// Purpose of verification
    pub purpose: VerificationPurpose,
    /// Intent version coverage
    pub intent_version_coverage: ForensicIntentVersionCoverage,
    /// Artifact coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_coverage: Option<ForensicArtifactCoverage>,
    /// Audit event coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_event_coverage: Option<ForensicAuditEventCoverage>,
    /// Policy snapshot coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_coverage: Option<ForensicPolicySnapshotCoverage>,
    /// Estimated total items that would be in a full bundle
    pub estimated_bundle_item_count: usize,
}

/// Intent version coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicIntentVersionCoverage {
    /// Whether intent exists
    pub intent_exists: bool,
    /// Intent ID
    pub intent_id: Uuid,
    /// Number of versions within the time range
    pub version_count: usize,
    /// Earliest version timestamp within range
    pub earliest_version: Option<DateTime<Utc>>,
    /// Latest version timestamp within range
    pub latest_version: Option<DateTime<Utc>>,
    /// Whether all versions have artifact traceability
    pub has_artifact_traceability: bool,
}

/// Artifact coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicArtifactCoverage {
    /// Number of artifacts found for the intent
    pub artifact_count: usize,
    /// Number of artifacts with complete provenance chain
    pub artifacts_with_provenance: usize,
    /// Whether artifact coverage is complete
    pub coverage_complete: bool,
}

/// Audit event coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicAuditEventCoverage {
    /// Number of audit events found for the tenant in time range
    pub event_count: usize,
    /// Whether the time range has full coverage (no gaps)
    pub time_range_complete: bool,
    /// First event timestamp in range
    pub first_event: Option<DateTime<Utc>>,
    /// Last event timestamp in range
    pub last_event: Option<DateTime<Utc>>,
}

/// Policy snapshot coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicPolicySnapshotCoverage {
    /// Number of policy snapshots found for the intent
    pub snapshot_count: usize,
    /// Whether snapshots cover all versions
    pub coverage_complete: bool,
}

impl From<forensic_service::ForensicVerificationResponse> for ForensicVerificationResponse {
    fn from(resp: forensic_service::ForensicVerificationResponse) -> Self {
        Self {
            verification_id: resp.verification_id,
            verified_at: resp.verified_at,
            status: resp.status,
            status_reason: resp.status_reason,
            tenant_id: resp.tenant_id,
            intent_id: resp.intent_id,
            time_range: ForensicVerificationTimeRange {
                start: resp.time_range.start,
                end: resp.time_range.end,
            },
            purpose: resp.purpose,
            intent_version_coverage: ForensicIntentVersionCoverage {
                intent_exists: resp.intent_version_coverage.intent_exists,
                intent_id: resp.intent_version_coverage.intent_id,
                version_count: resp.intent_version_coverage.version_count,
                earliest_version: resp.intent_version_coverage.earliest_version,
                latest_version: resp.intent_version_coverage.latest_version,
                has_artifact_traceability: resp.intent_version_coverage.has_artifact_traceability,
            },
            artifact_coverage: resp.artifact_coverage.map(|ac| ForensicArtifactCoverage {
                artifact_count: ac.artifact_count,
                artifacts_with_provenance: ac.artifacts_with_provenance,
                coverage_complete: ac.coverage_complete,
            }),
            audit_event_coverage: resp
                .audit_event_coverage
                .map(|aec| ForensicAuditEventCoverage {
                    event_count: aec.event_count,
                    time_range_complete: aec.time_range_complete,
                    first_event: aec.first_event,
                    last_event: aec.last_event,
                }),
            policy_snapshot_coverage: resp.policy_snapshot_coverage.map(|psc| {
                ForensicPolicySnapshotCoverage {
                    snapshot_count: psc.snapshot_count,
                    coverage_complete: psc.coverage_complete,
                }
            }),
            estimated_bundle_item_count: resp.estimated_bundle_item_count,
        }
    }
}

// =============================================================================
// Health and Request ID Types (Phase 2 bounded extraction)
// =============================================================================

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// Request ID stored in request extensions by the request_id_middleware.
#[derive(Clone)]
pub struct RequestId(pub String);

// =============================================================================
// Orchestration Run Response Types (Phase 3 Batch 1 bounded extraction)
// =============================================================================

/// Response for an orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRunResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub action_ids: Vec<Uuid>,
    pub status: String,
    pub initiated_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub not_found_count: usize,
    pub total_count: usize,
    pub item_results: Vec<RunItemResultResponse>,
}

/// Per-item result within a run (API version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunItemResultResponse {
    pub action_id: Uuid,
    pub action_taken: String,
    pub success: bool,
    pub reason: String,
    pub resulting_status: String,
}

impl From<OrchestrationRun> for OrchestrationRunResponse {
    fn from(run: OrchestrationRun) -> Self {
        Self {
            id: run.id,
            tenant_id: run.tenant_id,
            intent_id: run.intent_id,
            action_ids: run.action_ids,
            status: format_run_status(&run.status),
            initiated_by: run.initiated_by,
            created_at: run.created_at,
            started_at: run.started_at,
            completed_at: run.completed_at,
            succeeded_count: run.succeeded_count,
            failed_count: run.failed_count,
            skipped_count: run.skipped_count,
            not_found_count: run.not_found_count,
            total_count: run.total_count,
            item_results: run
                .item_results
                .into_iter()
                .map(|r| RunItemResultResponse {
                    action_id: r.action_id,
                    action_taken: format_action_decision(&r.action_taken),
                    success: r.success,
                    reason: r.reason,
                    resulting_status: r.resulting_status,
                })
                .collect(),
        }
    }
}

fn format_run_status(s: &RunStatus) -> String {
    match s {
        RunStatus::Pending => "pending".to_string(),
        RunStatus::Running => "running".to_string(),
        RunStatus::Completed => "completed".to_string(),
        RunStatus::CompletedWithErrors => "completed_with_errors".to_string(),
        RunStatus::Failed => "failed".to_string(),
    }
}

fn format_action_decision(d: &OrchestrationActionDecision) -> String {
    match d {
        OrchestrationActionDecision::Approve => "approve".to_string(),
        OrchestrationActionDecision::Reapprove => "reapprove".to_string(),
        OrchestrationActionDecision::Execute => "execute".to_string(),
        OrchestrationActionDecision::Skip => "skip".to_string(),
        OrchestrationActionDecision::NotFound => "not_found".to_string(),
    }
}
