use intent_rebase_types::{AffectedItemsPreview, IntentVersion};
use rebase_engine::planner::CompensationPlanningSummary;
use rebase_engine::{
    DecisionClass, DiffRiskAnalysis, IntentVersionDiff, RiskTier, SectionDecision,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
