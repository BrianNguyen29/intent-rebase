//! Compensation action DTOs and types.
//!
//! Phase 3 Batch 1: Extracted from compensation_action_service.rs to separate
//! data types from service logic.

use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, CompensationStatus, RetryableErrorClass,
    StrategyType,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Batch candidates response containing all four candidate categories.
#[derive(Debug, Clone)]
pub struct BatchCandidates {
    /// Actions in Pending status awaiting approval
    pub pending_approval_candidates: Vec<CompensationAction>,
    /// Approved actions that can be executed by the compensation service
    /// Phase 3 Batch 1 P7: Includes both Rollback+Automatic (S1) and CounterAction+SemiAutomatic (S2)
    pub approved_service_executable_candidates: Vec<CompensationAction>,
    /// Failed actions that can be reapproved (retryable error + budget remains)
    pub retryable_failed_candidates: Vec<CompensationAction>,
    /// Failed actions that exhausted retry budget or have non-retryable errors
    pub dlq_candidates: Vec<CompensationAction>,
}

// ============================================================================
// Manual Orchestration & Dry-Run Planner (Phase 3 Batch 1 bounded slice)
// ============================================================================

/// Proposed orchestration action for a compensation action.
///
/// Phase 3 Batch 1: This enum represents the action that would be taken
/// by the manual orchestration surface for a given compensation action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationAction {
    /// Action can be approved (Pending → Approved transition)
    Approve,
    /// Action can be reapproved (Failed → Pending transition)
    Reapprove,
    /// Action can be executed (Approved → Executed/Failed transition)
    Execute,
    /// No action can be taken; see reason for details
    NoAction,
}

impl OrchestrationAction {
    /// Returns the action name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            OrchestrationAction::Approve => "approve",
            OrchestrationAction::Reapprove => "reapprove",
            OrchestrationAction::Execute => "execute",
            OrchestrationAction::NoAction => "no_action",
        }
    }
}

/// A single item result from the dry-run planner.
///
/// Phase 3 Batch 1: Contains the proposed action and the reason for that proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationActionProposal {
    /// The compensation action ID
    pub action_id: Uuid,
    /// The proposed action (approve | reapprove | execute | no_action)
    pub proposed_action: OrchestrationAction,
    /// Human-readable reason for the proposal
    pub reason: String,
    /// Current status of the action (for informational purposes)
    pub current_status: CompensationStatus,
}

/// Result from the orchestration dry-run planner.
///
/// Phase 3 Batch 1: Returns per-item proposed actions for explicit compensation_action_ids.
/// This is a READ-ONLY dry-run - it does not execute any actions.
#[derive(Debug, Clone)]
pub struct OrchestrationDryRunResult {
    /// Per-item proposals
    pub proposals: Vec<OrchestrationActionProposal>,
    /// Actions that were found but not processed (empty list if all found)
    pub not_found: Vec<Uuid>,
    /// Summary counts
    pub summary: OrchestrationDryRunSummary,
}

/// Summary counts for dry-run results
#[derive(Debug, Clone, Default)]
pub struct OrchestrationDryRunSummary {
    pub total: usize,
    pub can_approve: usize,
    pub can_reapprove: usize,
    pub can_execute: usize,
    pub no_action: usize,
    pub not_found: usize,
}

/// Result of a batched orchestration command.
///
/// Phase 3 Batch 1: Contains per-item outcomes with partial-success semantics.
/// Even if some items fail, the batch continues processing and reports individual results.
#[derive(Debug, Clone)]
pub struct BatchOrchestrationResult {
    /// Per-item outcomes
    pub outcomes: Vec<BatchItemOutcome>,
    /// Actions that were not found
    pub not_found: Vec<Uuid>,
    /// Summary counts
    pub summary: BatchOrchestrationSummary,
}

/// A single item outcome from a batched orchestration command.
#[derive(Debug, Clone)]
pub struct BatchItemOutcome {
    /// The compensation action ID
    pub action_id: Uuid,
    /// Whether this item succeeded
    pub success: bool,
    /// The resulting action (if successful), or the error that occurred
    pub result: Result<CompensationAction, String>,
}

/// Summary counts for batched orchestration results
#[derive(Debug, Clone, Default)]
pub struct BatchOrchestrationSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub not_found: usize,
}

// ============================================================================
// Policy Gate Evaluation (Phase 3 Batch 1 bounded read-only slice)
// ============================================================================

/// Canonical policy gate statuses for compensation action evaluation.
///
/// **Bounded Phase 3 Batch 1 read-only slice:** Gate evaluation derives from
/// existing persisted fields and risk/policy surfaces. No new policy engine is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyGateStatus {
    /// Action can proceed: Approved + Automatic feasibility + no blocking conditions
    Eligible,
    /// Action cannot proceed: DLQ, non-retryable error, exhausted budget, terminal status
    Blocked,
    /// Action requires human intervention: Pending approval, SemiAutomatic/ManualOnly feasibility
    ManualReviewRequired,
}

// ============================================================================
// Coordination Status (Phase 3 Batch 1 bounded read-only orchestration view)
// ============================================================================

/// Canonical coordination statuses for the orchestration coordination read model.
///
/// These statuses represent the higher-level coordination state of compensation
/// actions within the intent rebase workflow.
///
/// **Bounded Phase 3 Batch 1 read-only slice:** Coordination status is derived
/// from existing CompensationAction fields at query time. No new orchestration
/// engine is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationStatus {
    /// Action is ready to proceed: Approved + Automatic feasibility + no blocking conditions
    Ready,
    /// Action is awaiting policy evaluation before it can proceed
    AwaitingPolicy,
    /// Action requires human intervention/manaul review
    AwaitingManualReview,
    /// Action is blocked and cannot proceed (DLQ, non-retryable error, exhausted budget)
    Blocked,
    /// Action has reached a terminal state (Executed, Waived)
    Terminal,
}

impl CoordinationStatus {
    /// Derive the coordination status for a compensation action.
    ///
    /// This function maps the compensation action's state to one of the five
    /// canonical coordination statuses.
    ///
    /// **Derivation logic:**
    /// - `Ready`: Approved + Automatic feasibility + not blocked + not terminal
    /// - `AwaitingPolicy`: Pending status (waiting for policy approval)
    /// - `AwaitingManualReview`: Approved but not Automatic (SemiAutomatic/ManualOnly feasibility)
    /// - `Blocked`: Failed + DLQ candidate OR non-retryable error OR exhausted budget
    /// - `Terminal`: Executed or Waived status
    pub fn from_compensation_action(action: &CompensationAction) -> Self {
        use CompensationStatus::*;

        // Terminal statuses are always Terminal
        if action.status.is_terminal() {
            return CoordinationStatus::Terminal;
        }

        // DLQ candidates are Blocked
        if action.is_dlq_candidate() {
            return CoordinationStatus::Blocked;
        }

        match action.status {
            // Pending → AwaitingPolicy (waiting for approval/policy decision)
            Pending => CoordinationStatus::AwaitingPolicy,

            // Failed → depends on whether it can be reapproved
            Failed => {
                if action.can_be_reapproved() {
                    // Has retry budget and retryable error → AwaitingManualReview
                    CoordinationStatus::AwaitingManualReview
                } else {
                    // Non-retryable error or exhausted budget → Blocked
                    CoordinationStatus::Blocked
                }
            }

            // Approved → check feasibility
            Approved => {
                if action.is_service_executable() {
                    // Rollback+Automatic or CounterAction+SemiAutomatic → Ready
                    CoordinationStatus::Ready
                } else {
                    // ManualOnly or other non-service-executable → AwaitingManualReview
                    CoordinationStatus::AwaitingManualReview
                }
            }

            // Executed/Waived handled above via is_terminal()
            Executed | Waived => CoordinationStatus::Terminal,
        }
    }
}

/// Policy gate evaluation for a single compensation action.
///
/// Contains the gate outcome plus policy/risk metadata useful for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateEvaluation {
    /// The compensation action being evaluated
    pub action: CompensationAction,
    /// Canonical gate status for this action
    pub gate_status: PolicyGateStatus,
    /// Human-readable reason for the gate status
    pub gate_reason: String,
    /// Supporting policy metadata
    pub policy_metadata: PolicyGateMetadata,
    /// Risk metadata derived from action state, retry history, and error classification
    pub risk_metadata: RiskMetadata,
}

/// Supporting policy metadata for gate evaluation.
///
/// Phase 3 Batch 1 (bounded read-only slice): Derived from existing persisted fields.
/// Contains action-state fields useful for understanding the action's configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateMetadata {
    /// Whether this action is auto-executable (Approved + Automatic feasibility)
    pub auto_executable: bool,
    /// Whether this action is a DLQ candidate
    pub is_dlq_candidate: bool,
    /// Whether this action can be reapproved (Failed + retryable + budget remains)
    pub can_reapprove: bool,
    /// Whether the action has exhausted its retry budget
    pub retry_budget_exhausted: bool,
    /// Whether the action has a non-retryable error
    pub has_non_retryable_error: bool,
    /// Feasibility level of the action
    pub feasibility: CompensationFeasibility,
    /// Strategy type of the action
    pub strategy_type: StrategyType,
    /// Current status of the action
    pub status: CompensationStatus,
    /// Number of attempts made
    pub attempt_count: i32,
    /// Maximum retries allowed
    pub max_retries: i32,
}

/// Risk metadata for gate evaluation.
///
/// Phase 3 Batch 1 (bounded read-only slice): Derived query-time from existing
/// persisted fields. Provides risk-relevant signals derived from action state,
/// retry history, and error classification.
///
/// **No new policy engine** - all fields derive from: status, attempt_count,
/// max_retries, error_code, feasibility, strategy_type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetadata {
    /// Severity of the strategy type (higher = more severe on failure)
    pub strategy_severity: StrategySeverity,
    /// Retry exhaustion risk: how close to DLQ state
    pub retry_exhaustion_risk: RetryExhaustionRisk,
    /// Feasibility risk: whether action can be automatically executed
    pub feasibility_risk: FeasibilityRisk,
    /// Error severity if the action failed
    pub error_severity: ErrorSeverity,
    /// Number of retry attempts remaining before DLQ
    pub retry_budget_remaining: i32,
    /// Error code classification if action has failed
    pub error_classification: Option<ErrorClassification>,
    /// Whether the action is in a terminal state
    pub is_terminal: bool,
    /// Whether manual intervention is required
    pub requires_manual_intervention: bool,
}

/// Severity tier for compensation strategy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategySeverity {
    /// Lowest severity - informational only
    Low,
    /// Medium severity - reversible with rollback
    Medium,
    /// High severity - external or counter-action required
    High,
    /// Highest severity - quarantine or escalation needed
    Critical,
}

impl StrategySeverity {
    /// Derive strategy severity from strategy type.
    pub fn from_strategy_type(strategy: StrategyType) -> Self {
        match strategy {
            StrategyType::FollowupNotice => StrategySeverity::Low,
            StrategyType::Rollback => StrategySeverity::Medium,
            StrategyType::CounterAction => StrategySeverity::High,
            StrategyType::Quarantine | StrategyType::Escalation => StrategySeverity::Critical,
        }
    }
}

/// Risk level for retry exhaustion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryExhaustionRisk {
    /// No attempts yet or plenty of budget remaining
    Low,
    /// Some attempts made, some budget consumed
    Medium,
    /// Most budget consumed, high risk of DLQ on next failure
    High,
    /// No budget remaining or DLQ already reached
    Critical,
}

impl RetryExhaustionRisk {
    /// Derive retry exhaustion risk from attempt_count and max_retries.
    pub fn from_attempts(attempt_count: i32, max_retries: i32) -> Self {
        if attempt_count >= max_retries {
            return RetryExhaustionRisk::Critical;
        }
        let ratio = attempt_count as f32 / max_retries as f32;
        if ratio >= 0.66 {
            RetryExhaustionRisk::High
        } else if ratio >= 0.33 {
            RetryExhaustionRisk::Medium
        } else {
            RetryExhaustionRisk::Low
        }
    }
}

/// Risk level based on feasibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeasibilityRisk {
    /// Automatic - can execute without human intervention
    Low,
    /// SemiAutomatic - may need manual trigger
    Medium,
    /// ManualOnly - requires human intervention
    High,
    /// NotPossible - cannot be executed
    Critical,
}

impl FeasibilityRisk {
    /// Derive feasibility risk from feasibility level.
    pub fn from_feasibility(feasibility: CompensationFeasibility) -> Self {
        match feasibility {
            CompensationFeasibility::Automatic => FeasibilityRisk::Low,
            CompensationFeasibility::SemiAutomatic => FeasibilityRisk::Medium,
            CompensationFeasibility::ManualOnly => FeasibilityRisk::High,
            CompensationFeasibility::NotPossible => FeasibilityRisk::Critical,
        }
    }
}

/// Error severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    /// No error - action succeeded or not yet executed
    None,
    /// Transient error - retry may succeed
    Low,
    /// Persistent error - requires investigation
    Medium,
    /// Permanent error - retry will not succeed
    High,
}

impl ErrorSeverity {
    /// Derive from error code classification.
    pub fn from_retryable_class(retryable: RetryableErrorClass) -> Self {
        match retryable {
            RetryableErrorClass::Retryable => ErrorSeverity::Low,
            RetryableErrorClass::NonRetryable => ErrorSeverity::High,
        }
    }
}

/// Classification of an error code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorClassification {
    /// The error code string
    pub error_code: String,
    /// Whether the error is retryable
    pub retryable: bool,
    /// Human-readable reason
    pub reason: String,
}

/// Summary of policy gate evaluations for a set of compensation actions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyGateSummary {
    /// Total number of actions evaluated
    pub total_actions: usize,
    /// Number of eligible actions
    pub eligible_count: usize,
    /// Number of blocked actions
    pub blocked_count: usize,
    /// Number of actions requiring manual review
    pub manual_review_required_count: usize,
    /// Number of DLQ candidates (subset of blocked)
    pub dlq_candidate_count: usize,
    /// Number of actions awaiting approval (subset of manual_review_required)
    pub pending_approval_count: usize,
    /// Number of auto-executable actions (subset of eligible)
    pub auto_executable_count: usize,
}

/// Result of policy gate evaluation for a set of compensation actions.
#[derive(Debug, Clone)]
pub struct PolicyGateEvaluationResult {
    /// Individual action evaluations
    pub evaluations: Vec<PolicyGateEvaluation>,
    /// Summary counts
    pub summary: PolicyGateSummary,
}

// ============================================================================
// Coordination Record (Phase 3 Batch 1 bounded read-only orchestration view)
// ============================================================================

/// A per-item coordination record for the orchestration coordination read model.
///
/// Represents the coordination state of a single compensation action within
/// the intent rebase workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationRecord {
    /// The compensation action this record is for
    pub action: CompensationAction,
    /// Canonical coordination status for this action
    pub coordination_status: CoordinationStatus,
    /// Human-readable reason for the coordination status
    pub coordination_reason: String,
    /// Whether the action is auto-executable
    pub auto_executable: bool,
    /// Whether the action is a DLQ candidate
    pub is_dlq_candidate: bool,
    /// Whether the action can be reapproved after failure
    pub can_reapprove: bool,
    /// Whether the action has exhausted its retry budget
    pub retry_budget_exhausted: bool,
    /// Feasibility level of the action
    pub feasibility: CompensationFeasibility,
    /// Strategy type of the action
    pub strategy_type: StrategyType,
    /// Current status of the action
    pub status: CompensationStatus,
    /// Number of attempts made
    pub attempt_count: i32,
    /// Maximum retries allowed
    pub max_retries: i32,
}

impl CoordinationRecord {
    /// Create a coordination record from a compensation action.
    pub fn from_action(action: &CompensationAction) -> Self {
        let coordination_status = CoordinationStatus::from_compensation_action(action);
        let coordination_reason = Self::compute_coordination_reason(action, &coordination_status);

        let _has_non_retryable_error = action
            .execution_result_payload
            .as_ref()
            .and_then(|r| r.error_code.as_ref())
            .map(|code| {
                let classification = CompensationAction::classify_error_code(code);
                classification.retryable == RetryableErrorClass::NonRetryable
            })
            .unwrap_or(false);

        Self {
            action: action.clone(),
            coordination_status,
            coordination_reason,
            auto_executable: action.is_service_executable(),
            is_dlq_candidate: action.is_dlq_candidate(),
            can_reapprove: action.can_be_reapproved(),
            retry_budget_exhausted: action.attempt_count >= action.max_retries,
            feasibility: action.feasibility,
            strategy_type: action.strategy_type,
            status: action.status,
            attempt_count: action.attempt_count,
            max_retries: action.max_retries,
        }
    }

    /// Compute the human-readable reason for the coordination status.
    fn compute_coordination_reason(
        action: &CompensationAction,
        status: &CoordinationStatus,
    ) -> String {
        use CompensationStatus::*;

        match status {
            CoordinationStatus::Ready => {
                format!(
                    "Action is ready: approved with {} feasibility and no blocking conditions",
                    format_feasibility(action.feasibility)
                )
            }
            CoordinationStatus::AwaitingPolicy => {
                format!(
                    "Action is awaiting policy approval ({} feasibility)",
                    format_feasibility(action.feasibility)
                )
            }
            CoordinationStatus::AwaitingManualReview => {
                if action.status == Failed {
                    if action.can_be_reapproved() {
                        return format!(
                            "Action failed but can be reapproved ({} retry attempts remaining, {} feasibility)",
                            action.max_retries - action.attempt_count,
                            format_feasibility(action.feasibility)
                        );
                    }
                    return format!(
                        "Action failed and requires manual review ({} feasibility)",
                        format_feasibility(action.feasibility)
                    );
                }
                if action.status == Approved {
                    return format!(
                        "Action requires manual execution ({})",
                        format_feasibility(action.feasibility)
                    );
                }
                format!(
                    "Action requires manual review ({})",
                    format_feasibility(action.feasibility)
                )
            }
            CoordinationStatus::Blocked => {
                if action.status.is_terminal() {
                    return format!("Action is in terminal status ({:?})", action.status);
                }
                if action.is_dlq_candidate() {
                    if action.attempt_count >= action.max_retries {
                        return format!(
                            "Action is blocked: retry budget exhausted ({}/{} attempts)",
                            action.attempt_count, action.max_retries
                        );
                    }
                    if let Some(ref result) = action.execution_result_payload {
                        if let Some(ref error_code) = result.error_code {
                            return format!(
                                "Action is blocked: non-retryable error ({})",
                                error_code
                            );
                        }
                    }
                    return "Action is blocked: DLQ candidate".to_string();
                }
                if let Some(ref reason) = action.reapproval_denial_reason() {
                    return reason.clone();
                }
                format!("Action is blocked due to {:?}", action.status)
            }
            CoordinationStatus::Terminal => {
                format!("Action has reached terminal status ({:?})", action.status)
            }
        }
    }
}

/// Summary of coordination records for a set of compensation actions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoordinationSummary {
    /// Total number of actions
    pub total_actions: usize,
    /// Number of ready actions
    pub ready_count: usize,
    /// Number of actions awaiting policy
    pub awaiting_policy_count: usize,
    /// Number of actions awaiting manual review
    pub awaiting_manual_review_count: usize,
    /// Number of blocked actions
    pub blocked_count: usize,
    /// Number of actions in terminal state
    pub terminal_count: usize,
    /// Number of DLQ candidates (subset of blocked)
    pub dlq_candidate_count: usize,
    /// Number of auto-executable actions (subset of ready)
    pub auto_executable_count: usize,
}

/// Result of coordination status query for a set of compensation actions.
#[derive(Debug, Clone)]
pub struct CoordinationResult {
    /// Per-item coordination records
    pub records: Vec<CoordinationRecord>,
    /// Summary counts
    pub summary: CoordinationSummary,
}

/// Format feasibility for display.
pub(crate) fn format_feasibility(f: CompensationFeasibility) -> &'static str {
    match f {
        CompensationFeasibility::Automatic => "Automatic",
        CompensationFeasibility::SemiAutomatic => "SemiAutomatic",
        CompensationFeasibility::ManualOnly => "ManualOnly",
        CompensationFeasibility::NotPossible => "NotPossible",
    }
}
