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

/// Classification of whether an error code is retryable.
///
/// **Bounded Phase 3 Batch 1 retry slice:** Simple explicit classification.
/// Retryable errors are transient failures that may succeed on retry
/// (e.g., network timeouts, temporary unavailability).
/// Non-retryable errors are permanent failures that will not succeed on retry
/// (e.g., invalid configuration, permission denied, resource not found).
///
/// This is a simple allowlist approach - only explicitly listed codes are retryable.
/// All other errors are treated as non-retryable (fail closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryableErrorClass {
    /// Error is retryable - transient failure that may succeed on retry
    Retryable,
    /// Error is not retryable - permanent failure that will not succeed on retry
    NonRetryable,
}

/// Classification result for a specific error code.
#[derive(Debug, Clone)]
pub struct ErrorCodeClassification {
    /// The error code that was classified
    pub error_code: String,
    /// Whether this error is retryable
    pub retryable: RetryableErrorClass,
    /// Human-readable reason for classification
    pub reason: &'static str,
}

impl CompensationAction {
    /// Default maximum retry attempts for compensation actions.
    ///
    /// After exhausting this budget, the action cannot be reapproved and becomes
    /// a derived DLQ candidate.
    pub const DEFAULT_MAX_RETRIES: i32 = 3;

    /// Classify whether an error code is retryable.
    ///
    /// **Bounded Phase 3 Batch 1 retry slice:** Simple explicit allowlist approach.
    /// Only error codes in the `RETRYABLE_ERROR_CODES` list are considered retryable.
    /// All other error codes are non-retryable (fail closed).
    ///
    /// This is intentionally restrictive to prevent infinite retry loops on
    /// permanent failures.
    pub fn classify_error_code(error_code: &str) -> ErrorCodeClassification {
        // Simple allowlist of retryable error codes
        // These represent transient failures that may succeed on retry
        const RETRYABLE_ERROR_CODES: &[&str] = &[
            // Network/connectivity issues
            "CONNECTION_TIMEOUT",
            "CONNECTION_REFUSED",
            "NETWORK_UNREACHABLE",
            "READ_TIMEOUT",
            "WRITE_TIMEOUT",
            // Temporary service unavailability
            "SERVICE_UNAVAILABLE",
            "TEMPORARILY_OVERLOADED",
            "BACKEND_ERROR",
            // Resource contention
            "RESOURCE_BUSY",
            "LOCK_ACQUISITION_FAILED",
            // Rate limiting (transient)
            "RATE_LIMIT_EXCEEDED",
            "QUOTA_EXCEEDED",
        ];

        // Case-sensitive match for explicit error codes
        let retryable = RETRYABLE_ERROR_CODES.contains(&error_code);

        ErrorCodeClassification {
            error_code: error_code.to_string(),
            retryable: if retryable {
                RetryableErrorClass::Retryable
            } else {
                RetryableErrorClass::NonRetryable
            },
            reason: if retryable {
                "Transient failure - retry may succeed"
            } else {
                "Permanent failure - retry will not succeed"
            },
        }
    }

    /// Check if this action is a DLQ (Dead Letter Queue) candidate.
    ///
    /// **Derived DLQ condition:** An action is a DLQ candidate when:
    /// 1. Status is Failed AND
    /// 2. Either:
    ///    a. attempt_count >= max_retries (exhausted retry budget), OR
    ///    b. The error code is non-retryable (permanent failure)
    ///
    /// **No DLQ table:** This is a read-only derived condition from existing data.
    /// DLQ candidates are identified by querying Failed actions and filtering
    /// based on these conditions.
    pub fn is_dlq_candidate(&self) -> bool {
        if self.status != CompensationStatus::Failed {
            return false;
        }

        // Check if retry budget is exhausted
        if self.attempt_count >= self.max_retries {
            return true;
        }

        // Check if error is non-retryable
        if let Some(ref result) = self.execution_result_payload {
            if let Some(ref error_code) = result.error_code {
                let classification = Self::classify_error_code(error_code);
                return classification.retryable == RetryableErrorClass::NonRetryable;
            }
        }

        false
    }

    /// Check if this action can be manually reapproved after failure.
    ///
    /// Returns true only if ALL of:
    /// 1. Status is Failed
    /// 2. attempt_count < max_retries (has remaining retry budget)
    /// 3. The error code is retryable (not a permanent failure)
    ///
    /// When this returns false, the action is either:
    /// - A DLQ candidate (exhausted budget or non-retryable error), OR
    /// - Not in Failed status
    pub fn can_be_reapproved(&self) -> bool {
        if self.status != CompensationStatus::Failed {
            return false;
        }

        // Must have remaining retry budget
        if self.attempt_count >= self.max_retries {
            return false;
        }

        // Error must be retryable
        if let Some(ref result) = self.execution_result_payload {
            if let Some(ref error_code) = result.error_code {
                let classification = Self::classify_error_code(error_code);
                return classification.retryable == RetryableErrorClass::Retryable;
            }
        }

        // If no error code present, allow reapproval (assume transient)
        true
    }

    /// Returns the reason why this action cannot be reapproved.
    ///
    /// Returns None if can_be_reapproved() would return true.
    /// Returns Some with a detailed reason otherwise.
    pub fn reapproval_denial_reason(&self) -> Option<String> {
        if self.status != CompensationStatus::Failed {
            return Some(format!("Action is in {:?} status, not Failed", self.status));
        }

        // Check retry budget
        if self.attempt_count >= self.max_retries {
            return Some(format!(
                "Retry budget exhausted: {} attempts made (max={})",
                self.attempt_count, self.max_retries
            ));
        }

        // Check error code
        if let Some(ref result) = self.execution_result_payload {
            if let Some(ref error_code) = result.error_code {
                let classification = Self::classify_error_code(error_code);
                if classification.retryable == RetryableErrorClass::NonRetryable {
                    return Some(format!(
                        "Non-retryable error code '{}': {}",
                        error_code, classification.reason
                    ));
                }
            }
        }

        None // can_be_reapproved would return true
    }
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
    /// - **Failed → Pending (via reapprove_action - manual retry path)**
    ///
    /// Illegal transitions that fail closed:
    /// - Executed → * (immutable, no retries in this slice)
    /// - Waived → * (immutable, no reactivation)
    /// - Approved → Pending (no undo of approval)
    /// - Pending → Executed (must be approved first)
    ///
    /// **Manual retry policy (Phase 3 Batch 1):**
    /// - Failed → Pending is allowed only when can_be_reapproved() returns true
    /// - This requires: retryable error code AND remaining retry budget
    /// - Non-retryable errors and exhausted budgets cannot be reapproved
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

            // **Batch 1 scope: Manual retry path**
            // Failed → Pending allowed only when can_be_reapproved() returns true
            // (retryable error AND remaining retry budget)
            (CompensationStatus::Failed, CompensationStatus::Pending) => TransitionValidation {
                allowed: true,
                reason: Some(
                    "Manual retry allowed when retryable error and budget remains".to_string(),
                ),
            },

            // All terminal states cannot transition to any other state
            (CompensationStatus::Executed, _) => TransitionValidation {
                allowed: false,
                reason: Some("Executed is a terminal state; no transitions allowed".into()),
            },
            (CompensationStatus::Failed, _) => TransitionValidation {
                allowed: false,
                reason: Some(
                    "Failed can only transition to Pending (manual retry) or is terminal otherwise"
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
    ///
    /// **Phase 3 Batch 1:** Failed is NOT terminal because manual retry allows
    /// Failed → Pending transition when policy conditions are met.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CompensationStatus::Executed | CompensationStatus::Waived
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
    /// Maximum retry attempts allowed before the action becomes a DLQ candidate.
    /// Defaults to DEFAULT_MAX_RETRIES (3) if not explicitly set.
    /// When attempt_count >= max_retries, the action cannot be reapproved.
    pub max_retries: i32,
    /// Lock version for optimistic concurrency during status transitions
    pub lock_version: i32,
    /// When this action was generated
    pub generated_at: DateTime<Utc>,
    /// When compensation was approved
    pub approved_at: Option<DateTime<Utc>>,
    /// Who approved this compensation action
    pub approved_by: Option<String>,
    /// When compensation was waived
    pub waived_at: Option<DateTime<Utc>>,
    /// Who waived this compensation action
    pub waived_by: Option<String>,
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
            max_retries: Self::DEFAULT_MAX_RETRIES,
            lock_version: 0,
            generated_at: Utc::now(),
            approved_at: None,
            approved_by: None,
            waived_at: None,
            waived_by: None,
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
            max_retries: Self::DEFAULT_MAX_RETRIES,
            lock_version: 0,
            generated_at: Utc::now(),
            approved_at: None,
            approved_by: None,
            waived_at: None,
            waived_by: None,
            executed_at: None,
            executed_by: None,
            failed_at: None,
        }
    }

    /// Returns true if this action can be executed automatically.
    pub fn is_auto_executable(&self) -> bool {
        self.feasibility == CompensationFeasibility::Automatic
    }

    /// Returns true if this action can be executed by the compensation service.
    ///
    /// **Phase 3 Batch 1 P7 bounded slice:** In addition to Automatic-only actions,
    /// this includes:
    /// - CounterAction + SemiAutomatic combos (S2ExternalReversible effects)
    /// - FollowupNotice + ManualOnly combos (S3ExternalPartiallyReversible effects)
    /// - Escalation + NotPossible combos (S4Irreversible effects)
    ///
    /// All other strategy/feasibility combos require human intervention or are
    /// not executable in this slice.
    pub fn is_service_executable(&self) -> bool {
        matches!(
            (self.strategy_type, self.feasibility),
            (StrategyType::Rollback, CompensationFeasibility::Automatic)
                | (
                    StrategyType::CounterAction,
                    CompensationFeasibility::SemiAutomatic,
                )
                | (
                    StrategyType::FollowupNotice,
                    CompensationFeasibility::ManualOnly,
                )
                | (
                    StrategyType::Escalation,
                    CompensationFeasibility::NotPossible
                )
        )
    }
}
