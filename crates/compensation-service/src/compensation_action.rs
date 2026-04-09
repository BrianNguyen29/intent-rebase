//! Compensation action model
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::side_effect::{SideEffect, SideEffectClass};

/// Rebase context that triggered compensation planning.
///
/// Carries minimal bounded-context information about the intent rebase
/// that caused side effects to need compensation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseContext {
    /// Intent ID that was rebased
    pub intent_id: Uuid,
    /// Source intent version before rebase
    pub from_version: i32,
    /// Target intent version after rebase
    pub to_version: i32,
    /// Workflow that initiated the rebase
    pub workflow_id: Uuid,
}

impl RebaseContext {
    /// Create a new rebase context.
    pub fn new(intent_id: Uuid, from_version: i32, to_version: i32, workflow_id: Uuid) -> Self {
        Self {
            intent_id,
            from_version,
            to_version,
            workflow_id,
        }
    }
}

/// Execution result payload for a compensation action.
///
/// Carries bounded context about what happened during execution
/// (success, failure, or waiver) so planner/retry can reason about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Human-readable summary of what happened
    pub summary: String,
    /// Error code if execution failed
    pub error_code: Option<String>,
    /// Error detail if execution failed
    pub error_detail: Option<String>,
    /// Timestamp when execution completed
    pub completed_at: DateTime<Utc>,
}

impl ExecutionResult {
    /// Create a successful execution result.
    pub fn success(summary: &str) -> Self {
        Self {
            success: true,
            summary: summary.to_string(),
            error_code: None,
            error_detail: None,
            completed_at: Utc::now(),
        }
    }

    /// Create a failed execution result.
    pub fn failure(summary: &str, error_code: &str, error_detail: Option<String>) -> Self {
        Self {
            success: false,
            summary: summary.to_string(),
            error_code: Some(error_code.to_string()),
            error_detail,
            completed_at: Utc::now(),
        }
    }
}

/// Compensation feasibility level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompensationFeasibility {
    Automatic,
    SemiAutomatic,
    ManualOnly,
    NotPossible,
}

/// Compensation strategy type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    Rollback,
    CounterAction,
    FollowupNotice,
    Quarantine,
    Escalation,
}

/// Compensation action status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompensationStatus {
    Pending,
    Approved,
    Executed,
    Failed,
    Waived,
}

/// Transition validation result with context for error reporting.
#[derive(Debug, Clone)]
pub struct TransitionValidation {
    /// Whether the transition is allowed
    pub allowed: bool,
    /// Human-readable reason if denied
    pub reason: Option<String>,
}

impl CompensationStatus {
    /// Valid status transitions for compensation actions (bounded Phase 3 Batch 1 slice).
    ///
    /// Transition matrix:
    /// - Pending → Approved (via approve_action)
    /// - Pending → Waived (via waive_action)
    /// - Pending → Failed (via record_result with failure result)
    /// - Approved → Executed (via execute_action, after executor runs)
    /// - Approved → Failed (via record_result with failure result)
    ///
    /// Illegal transitions that fail closed:
    /// - Executed → * (immutable, no retries in this slice)
    /// - Failed → * (retry/reapproval not in this slice)
    /// - Waived → * (immutable, no reactivation)
    /// - Approved → Pending (no undo of approval)
    /// - Pending → Executed (must be approved first)
    ///
    /// **Batch 1+ scope (not implemented):**
    /// - Failed → Pending (reapproval path)
    /// - Automatic status transitions based on feasibility
    pub fn can_transition_to(&self, target: CompensationStatus) -> TransitionValidation {
        match (self, target) {
            // Pending can transition to Approved or Waived
            (CompensationStatus::Pending, CompensationStatus::Approved) => TransitionValidation {
                allowed: true,
                reason: None,
            },
            (CompensationStatus::Pending, CompensationStatus::Waived) => TransitionValidation {
                allowed: true,
                reason: None,
            },
            // Note: Pending -> Failed happens via record_result, not a direct status update
            // The service layer routes to record_result which handles this transition

            // Approved can transition to Executed (via executor) or Failed (via record_result)
            (CompensationStatus::Approved, CompensationStatus::Executed) => TransitionValidation {
                allowed: true,
                reason: None,
            },
            // Note: Approved -> Failed happens via record_result

            // All terminal states cannot transition to any other state
            (CompensationStatus::Executed, _) => TransitionValidation {
                allowed: false,
                reason: Some("Executed is a terminal state; no transitions allowed".into()),
            },
            (CompensationStatus::Failed, _) => TransitionValidation {
                allowed: false,
                reason: Some(
                    "Failed is a terminal state in this slice; retry/reapproval not implemented"
                        .into(),
                ),
            },
            (CompensationStatus::Waived, _) => TransitionValidation {
                allowed: false,
                reason: Some("Waived is a terminal state; no transitions allowed".into()),
            },

            // General cases
            (_, CompensationStatus::Pending) => TransitionValidation {
                allowed: false,
                reason: Some("Cannot transition back to Pending".into()),
            },
            // Same status transition not allowed
            _ if *self == target => TransitionValidation {
                allowed: false,
                reason: Some("Action is already in target status".into()),
            },
            // Catch-all invalid transition
            (from, to) => TransitionValidation {
                allowed: false,
                reason: Some(format!("Invalid transition from {:?} to {:?}", from, to)),
            },
        }
    }

    /// Returns true if this status is a terminal state (no transitions allowed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CompensationStatus::Executed | CompensationStatus::Failed | CompensationStatus::Waived
        )
    }
}

/// A compensation action generated from a side effect.
///
/// **Batch 1 scope (this slice):** type scaffold with minimal persistent fields
/// and construction helpers. Planner and executor logic are Batch 1+ scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationAction {
    /// Unique identifier for this compensation action
    pub id: Uuid,
    /// Tenant this compensation action belongs to
    pub tenant_id: Uuid,
    /// Reference to the side effect this action compensates
    pub side_effect_id: Uuid,
    /// Reference to the intent this compensation action is scoped to
    /// (allows direct intent-scoped queries without joining through side_effects)
    pub intent_id: Uuid,
    /// Minimal rebase context that triggered compensation planning (JSONB-serialized RebaseContext)
    /// Used by planner/executor to reason about what caused the side effects needing compensation
    pub trigger_context: RebaseContext,
    /// Execution result context captured after executor runs (JSONB-serialized ExecutionResult)
    /// Used for retry/audit reasoning about what happened during execution
    pub execution_result_payload: Option<ExecutionResult>,
    /// Feasibility of compensating this effect
    pub feasibility: CompensationFeasibility,
    /// Chosen compensation strategy
    pub strategy_type: StrategyType,
    /// Human-readable rationale for the chosen strategy
    pub rationale: String,
    /// Current status of the compensation action
    pub status: CompensationStatus,
    /// Execution attempt counter for idempotency/retry tracking
    pub attempt_count: i32,
    /// Lock version for optimistic concurrency during status transitions
    pub lock_version: i32,
    /// When this action was generated
    pub generated_at: DateTime<Utc>,
    /// When compensation was approved
    pub approved_at: Option<DateTime<Utc>>,
    /// Who approved this compensation action
    pub approved_by: Option<String>,
    /// When compensation was executed
    pub executed_at: Option<DateTime<Utc>>,
    /// Who executed this compensation action
    pub executed_by: Option<String>,
    /// When compensation failed
    pub failed_at: Option<DateTime<Utc>>,
}

impl CompensationAction {
    /// Create a new compensation action (for testing and manual construction).
    pub fn new(
        tenant_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        trigger_context: RebaseContext,
        feasibility: CompensationFeasibility,
        strategy_type: StrategyType,
        rationale: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            side_effect_id,
            intent_id,
            trigger_context,
            execution_result_payload: None,
            feasibility,
            strategy_type,
            rationale: rationale.to_string(),
            status: CompensationStatus::Pending,
            attempt_count: 0,
            lock_version: 0,
            generated_at: Utc::now(),
            approved_at: None,
            approved_by: None,
            executed_at: None,
            executed_by: None,
            failed_at: None,
        }
    }

    /// Generate a compensation action from a side effect.
    ///
    /// This is a scaffold — the actual planner logic is Batch 1+.
    pub fn from_side_effect(
        tenant_id: Uuid,
        side_effect: &SideEffect,
        trigger_context: &RebaseContext,
        strategy: StrategyType,
        rationale: &str,
    ) -> Self {
        let feasibility = match side_effect.effect_class {
            SideEffectClass::S0PureRead => CompensationFeasibility::NotPossible,
            SideEffectClass::S1InternalReversible => CompensationFeasibility::Automatic,
            SideEffectClass::S2ExternalReversible => CompensationFeasibility::SemiAutomatic,
            SideEffectClass::S3ExternalPartiallyReversible => CompensationFeasibility::ManualOnly,
            SideEffectClass::S4Irreversible => CompensationFeasibility::NotPossible,
        };

        Self {
            id: Uuid::new_v4(),
            tenant_id,
            side_effect_id: side_effect.id,
            intent_id: side_effect.intent_id,
            trigger_context: trigger_context.clone(),
            execution_result_payload: None,
            feasibility,
            strategy_type: strategy,
            rationale: rationale.to_string(),
            status: CompensationStatus::Pending,
            attempt_count: 0,
            lock_version: 0,
            generated_at: Utc::now(),
            approved_at: None,
            approved_by: None,
            executed_at: None,
            executed_by: None,
            failed_at: None,
        }
    }

    /// Returns true if this action can be executed automatically.
    pub fn is_auto_executable(&self) -> bool {
        self.feasibility == CompensationFeasibility::Automatic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compensation_action_from_side_effect_auto() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record",
        );
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::from_side_effect(
            tenant_id,
            &effect,
            &rebase_context,
            StrategyType::Rollback,
            "Auto rollback internal metadata",
        );

        assert_eq!(action.tenant_id, tenant_id);
        assert_eq!(action.side_effect_id, effect.id);
        assert_eq!(action.intent_id, intent_id);
        assert_eq!(action.feasibility, CompensationFeasibility::Automatic);
        assert!(action.is_auto_executable());
        assert_eq!(action.status, CompensationStatus::Pending);
        assert_eq!(action.attempt_count, 0);
        assert_eq!(action.lock_version, 0);
    }

    #[test]
    fn test_compensation_action_from_side_effect_manual() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S3ExternalPartiallyReversible,
            "email_sent",
            "user@example.com",
        );
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::from_side_effect(
            tenant_id,
            &effect,
            &rebase_context,
            StrategyType::FollowupNotice,
            "Send correction email",
        );

        assert_eq!(action.feasibility, CompensationFeasibility::ManualOnly);
        assert!(!action.is_auto_executable());
    }

    #[test]
    fn test_compensation_action_not_possible_s0() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S0PureRead,
            "read",
            "noop",
        );
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let action = CompensationAction::from_side_effect(
            tenant_id,
            &effect,
            &rebase_context,
            StrategyType::Quarantine,
            "N/A",
        );

        assert_eq!(action.feasibility, CompensationFeasibility::NotPossible);
    }

    #[test]
    fn test_compensation_action_serialization_round_trip() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "pr_opened",
            "https://github.com/pulls/123",
        );
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = CompensationAction::from_side_effect(
            tenant_id,
            &effect,
            &rebase_context,
            StrategyType::CounterAction,
            "Close PR",
        );

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: CompensationAction = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, action.id);
        assert_eq!(deserialized.tenant_id, tenant_id);
        assert_eq!(deserialized.side_effect_id, effect.id);
        assert_eq!(deserialized.intent_id, intent_id);
        assert_eq!(
            deserialized.feasibility,
            CompensationFeasibility::SemiAutomatic
        );
        assert_eq!(deserialized.strategy_type, StrategyType::CounterAction);
    }
}
