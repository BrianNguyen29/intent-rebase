use uuid::Uuid;

use crate::compensation_action::{CompensationAction, CompensationStatus, RetryableErrorClass};
use crate::compensation_action_types::*;
use crate::CompensationActionService;
use intent_rebase_types::IntentRebaseError;

impl CompensationActionService {
    /// Evaluate policy gates for all compensation actions of a tenant.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
    /// for all compensation actions belonging to the specified tenant.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Gate evaluation logic:**
    /// - `eligible`: Approved + Automatic feasibility + not blocked + not DLQ
    /// - `blocked`: DLQ candidate OR exhausted retry budget OR non-retryable error OR terminal status
    /// - `manual_review_required`: Pending status OR SemiAutomatic/ManualOnly feasibility
    ///
    /// **Derivation from existing surfaces:**
    /// - Gate status is derived from existing CompensationAction fields (status, feasibility,
    ///   attempt_count, max_retries, execution_result_payload.error_code)
    /// - No new policy engine or external risk surface is queried
    pub async fn evaluate_policy_gates(
        &self,
        tenant_id: Uuid,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let actions = self.list_by_tenant(tenant_id, None).await?;
        self.evaluate_policy_gates_from_actions(actions)
    }

    /// Evaluate policy gates for all compensation actions of an intent.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
    /// for all compensation actions belonging to the specified intent.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Gate evaluation logic:** Same as `evaluate_policy_gates`.
    pub async fn evaluate_policy_gates_for_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let actions = self.list_by_intent(intent_id, tenant_id).await?;
        self.evaluate_policy_gates_from_actions(actions)
    }

    /// Evaluate policy gates from a collection of compensation actions.
    ///
    /// Internal helper that computes gate evaluations and summary.
    fn evaluate_policy_gates_from_actions(
        &self,
        actions: Vec<CompensationAction>,
    ) -> Result<PolicyGateEvaluationResult, IntentRebaseError> {
        let total_actions = actions.len();
        let mut evaluations = Vec::with_capacity(total_actions);
        let mut summary = PolicyGateSummary::default();

        for action in actions {
            let evaluation = self.evaluate_single_action(&action);
            match evaluation.gate_status {
                PolicyGateStatus::Eligible => summary.eligible_count += 1,
                PolicyGateStatus::Blocked => summary.blocked_count += 1,
                PolicyGateStatus::ManualReviewRequired => summary.manual_review_required_count += 1,
            }

            if evaluation.policy_metadata.is_dlq_candidate {
                summary.dlq_candidate_count += 1;
            }
            if evaluation.policy_metadata.auto_executable {
                summary.auto_executable_count += 1;
            }
            if matches!(action.status, CompensationStatus::Pending) {
                summary.pending_approval_count += 1;
            }

            evaluations.push(evaluation);
        }

        summary.total_actions = total_actions;

        Ok(PolicyGateEvaluationResult {
            evaluations,
            summary,
        })
    }

    /// Evaluate policy gate for a single compensation action.
    fn evaluate_single_action(&self, action: &CompensationAction) -> PolicyGateEvaluation {
        let gate_status = self.compute_gate_status(action);
        let gate_reason = self.compute_gate_reason(action, &gate_status);
        let policy_metadata = self.compute_policy_metadata(action);
        let risk_metadata = self.compute_risk_metadata(action);

        PolicyGateEvaluation {
            action: action.clone(),
            gate_status,
            gate_reason,
            policy_metadata,
            risk_metadata,
        }
    }

    /// Compute the gate status for a compensation action.
    fn compute_gate_status(&self, action: &CompensationAction) -> PolicyGateStatus {
        use CompensationStatus::*;

        // Terminal statuses (Executed, Waived) are always blocked
        if action.status.is_terminal() {
            return PolicyGateStatus::Blocked;
        }

        // DLQ candidates are blocked
        if action.is_dlq_candidate() {
            return PolicyGateStatus::Blocked;
        }

        // Pending status requires manual review
        if action.status == Pending {
            return PolicyGateStatus::ManualReviewRequired;
        }

        // Failed status with remaining budget and retryable error - manual review
        if action.status == Failed {
            if action.can_be_reapproved() {
                return PolicyGateStatus::ManualReviewRequired;
            }
            // Otherwise it's blocked (non-retryable or exhausted budget)
            return PolicyGateStatus::Blocked;
        }

        // Approved status - check service executability
        if action.status == Approved {
            // Rollback+Automatic or CounterAction+SemiAutomatic = service-executable = eligible
            if action.is_service_executable() {
                return PolicyGateStatus::Eligible;
            }
            // ManualOnly = manual review required
            return PolicyGateStatus::ManualReviewRequired;
        }

        // Default to blocked for any unexpected state
        PolicyGateStatus::Blocked
    }

    /// Compute the human-readable reason for the gate status.
    fn compute_gate_reason(
        &self,
        action: &CompensationAction,
        gate_status: &PolicyGateStatus,
    ) -> String {
        use CompensationStatus::*;

        match gate_status {
            PolicyGateStatus::Eligible => {
                format!(
                    "Action is approved with {} feasibility and no blocking conditions",
                    format_feasibility(action.feasibility)
                )
            }
            PolicyGateStatus::Blocked => {
                if action.status.is_terminal() {
                    return format!("Action is in terminal status ({:?})", action.status);
                }
                if action.is_dlq_candidate() {
                    if action.attempt_count >= action.max_retries {
                        return format!(
                            "Action is DLQ candidate: retry budget exhausted ({}/{} attempts)",
                            action.attempt_count, action.max_retries
                        );
                    }
                    if let Some(ref result) = action.execution_result_payload {
                        if let Some(ref error_code) = result.error_code {
                            return format!(
                                "Action is DLQ candidate: non-retryable error ({})",
                                error_code
                            );
                        }
                    }
                    return "Action is DLQ candidate".to_string();
                }
                if let Some(ref reason) = action.reapproval_denial_reason() {
                    return reason.clone();
                }
                format!("Action is blocked due to {:?}", action.status)
            }
            PolicyGateStatus::ManualReviewRequired => match action.status {
                Pending => {
                    format!(
                        "Action awaits approval ({} feasibility)",
                        format_feasibility(action.feasibility)
                    )
                }
                Failed => {
                    if action.can_be_reapproved() {
                        return format!(
                                "Action failed but can be reapproved ({} retry attempts remaining, {} feasibility)",
                                action.max_retries - action.attempt_count,
                                format_feasibility(action.feasibility)
                            );
                    }
                    if let Some(ref reason) = action.reapproval_denial_reason() {
                        return reason.clone();
                    }
                    format!(
                        "Action failed and requires manual review ({} feasibility)",
                        format_feasibility(action.feasibility)
                    )
                }
                Approved => {
                    format!(
                        "Action requires manual execution ({})",
                        format_feasibility(action.feasibility)
                    )
                }
                _ => format!(
                    "Action requires manual review ({})",
                    format_feasibility(action.feasibility)
                ),
            },
        }
    }

    /// Compute policy metadata for a compensation action.
    fn compute_policy_metadata(&self, action: &CompensationAction) -> PolicyGateMetadata {
        let has_non_retryable_error = action
            .execution_result_payload
            .as_ref()
            .and_then(|r| r.error_code.as_ref())
            .map(|code| {
                let classification = CompensationAction::classify_error_code(code);
                classification.retryable == RetryableErrorClass::NonRetryable
            })
            .unwrap_or(false);

        PolicyGateMetadata {
            auto_executable: action.is_auto_executable(),
            is_dlq_candidate: action.is_dlq_candidate(),
            can_reapprove: action.can_be_reapproved(),
            retry_budget_exhausted: action.attempt_count >= action.max_retries,
            has_non_retryable_error,
            feasibility: action.feasibility,
            strategy_type: action.strategy_type,
            status: action.status,
            attempt_count: action.attempt_count,
            max_retries: action.max_retries,
        }
    }

    /// Compute risk metadata for a compensation action.
    ///
    /// Phase 3 Batch 1 (bounded read-only slice): Derives risk signals from
    /// existing action state fields. No new policy engine - all fields derive
    /// from status, attempt_count, max_retries, error_code, feasibility, strategy_type.
    fn compute_risk_metadata(&self, action: &CompensationAction) -> RiskMetadata {
        let error_classification = action.execution_result_payload.as_ref().and_then(|r| {
            r.error_code.as_ref().map(|code| {
                let classification = CompensationAction::classify_error_code(code);
                ErrorClassification {
                    error_code: code.clone(),
                    retryable: classification.retryable == RetryableErrorClass::Retryable,
                    reason: classification.reason.to_string(),
                }
            })
        });

        let error_severity = action
            .execution_result_payload
            .as_ref()
            .and_then(|r| {
                r.error_code.as_ref().map(|code| {
                    let classification = CompensationAction::classify_error_code(code);
                    ErrorSeverity::from_retryable_class(classification.retryable)
                })
            })
            .unwrap_or(ErrorSeverity::None);

        RiskMetadata {
            strategy_severity: StrategySeverity::from_strategy_type(action.strategy_type),
            retry_exhaustion_risk: RetryExhaustionRisk::from_attempts(
                action.attempt_count,
                action.max_retries,
            ),
            feasibility_risk: FeasibilityRisk::from_feasibility(action.feasibility),
            error_severity,
            retry_budget_remaining: (action.max_retries - action.attempt_count).max(0),
            error_classification,
            is_terminal: action.status.is_terminal(),
            requires_manual_intervention: matches!(
                action.status,
                CompensationStatus::Pending | CompensationStatus::Failed
            ) || !action.is_auto_executable(),
        }
    }

    // ============================================================================
    // Coordination Status Evaluation (Phase 3 Batch 1 bounded read-only orchestration view)
    // ============================================================================

    /// Evaluate coordination status for all compensation actions of a tenant.
    ///
    /// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
    /// coordination status for all compensation actions belonging to the specified tenant.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Canonical coordination statuses:**
    /// - `ready`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
    /// - `awaiting_policy`: Action awaits policy approval (Pending status)
    /// - `awaiting_manual_review`: Action requires human intervention (Failed + can reapprove, or Approved + non-Automatic)
    /// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget)
    /// - `terminal`: Action has reached terminal state (Executed, Waived)
    ///
    /// **Derivation from existing surfaces:**
    /// - Coordination status is derived from existing CompensationAction fields at query time
    /// - No new orchestration engine or external policy surface is queried
    pub async fn evaluate_coordination_status(
        &self,
        tenant_id: Uuid,
    ) -> Result<CoordinationResult, IntentRebaseError> {
        let actions = self.list_by_tenant(tenant_id, None).await?;
        Ok(self.evaluate_coordination_from_actions(actions))
    }

    /// Evaluate coordination status for all compensation actions of an intent.
    ///
    /// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
    /// coordination status for all compensation actions belonging to the specified intent.
    ///
    /// **This endpoint is READ-ONLY** - it only queries existing data.
    ///
    /// **Canonical coordination statuses:** Same as `evaluate_coordination_status`.
    pub async fn evaluate_coordination_status_for_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<CoordinationResult, IntentRebaseError> {
        let actions = self.list_by_intent(intent_id, tenant_id).await?;
        Ok(self.evaluate_coordination_from_actions(actions))
    }

    /// Evaluate coordination status from a collection of compensation actions.
    ///
    /// Internal helper that computes coordination records and summary.
    fn evaluate_coordination_from_actions(
        &self,
        actions: Vec<CompensationAction>,
    ) -> CoordinationResult {
        let total_actions = actions.len();
        let mut records = Vec::with_capacity(total_actions);
        let mut summary = CoordinationSummary::default();

        for action in actions {
            let record = CoordinationRecord::from_action(&action);
            match record.coordination_status {
                CoordinationStatus::Ready => summary.ready_count += 1,
                CoordinationStatus::AwaitingPolicy => summary.awaiting_policy_count += 1,
                CoordinationStatus::AwaitingManualReview => {
                    summary.awaiting_manual_review_count += 1
                }
                CoordinationStatus::Blocked => summary.blocked_count += 1,
                CoordinationStatus::Terminal => summary.terminal_count += 1,
            }

            if record.is_dlq_candidate {
                summary.dlq_candidate_count += 1;
            }
            if record.auto_executable {
                summary.auto_executable_count += 1;
            }

            records.push(record);
        }

        summary.total_actions = total_actions;

        CoordinationResult { records, summary }
    }
}
