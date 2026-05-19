use chrono::{DateTime, Utc};
use compensation_service::{
    CompensationAction, OrchestrationActionDecision, OrchestrationRun, RunStatus, SideEffect,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Compensation Action Types
// =============================================================================

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
