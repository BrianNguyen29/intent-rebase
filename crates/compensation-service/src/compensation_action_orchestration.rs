use uuid::Uuid;

use crate::compensation_action::{CompensationAction, CompensationStatus};
use crate::compensation_action_types::*;
use crate::CompensationActionService;
use intent_rebase_types::IntentRebaseError;

impl CompensationActionService {
    /// Plan orchestration actions for explicit compensation action IDs (dry-run).
    ///
    /// Phase 3 Batch 1 (bounded dry-run slice): For each provided compensation_action_id,
    /// determines the proposed action (approve | reapprove | execute | no_action) based
    /// on the action's current state.
    ///
    /// **This is READ-ONLY** - it does not execute any actions.
    ///
    /// **Action determination logic:**
    /// - `approve`: Action is Pending (can transition to Approved)
    /// - `reapprove`: Action is Failed AND can_be_reapproved() (retryable error + budget remains)
    /// - `execute`: Action is Approved AND is_service_executable() (Automatic or SemiAutomatic feasibility)
    /// - `no_action`: Action is in a terminal state or cannot perform any valid transition
    ///
    /// **Bounded partial-success semantics:**
    /// - If an action_id is not found, it's added to `not_found` and does not cause failure
    /// - All found actions are processed, even if some have no_action
    ///
    /// **No background worker or queue claiming:**
    /// This is a direct query-based planner that reads current state and proposes actions.
    pub async fn plan_orchestration_actions(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
    ) -> Result<OrchestrationDryRunResult, IntentRebaseError> {
        let mut proposals = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = OrchestrationDryRunSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Skip actions that don't belong to this tenant (security check)
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.no_action += 1;
                        continue;
                    }

                    let proposal = self.compute_action_proposal(&action);
                    match proposal.proposed_action {
                        OrchestrationAction::Approve => summary.can_approve += 1,
                        OrchestrationAction::Reapprove => summary.can_reapprove += 1,
                        OrchestrationAction::Execute => summary.can_execute += 1,
                        OrchestrationAction::NoAction => summary.no_action += 1,
                    }
                    proposals.push(proposal);
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.no_action += 1;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(OrchestrationDryRunResult {
            proposals,
            not_found,
            summary,
        })
    }

    /// Compute the proposed action for a single compensation action.
    fn compute_action_proposal(&self, action: &CompensationAction) -> OrchestrationActionProposal {
        use CompensationStatus::*;

        // Determine the best action for this action based on its state
        let (proposed_action, reason) = match action.status {
            // Pending actions can be approved
            Pending => {
                // Validate the transition is still allowed
                if action
                    .status
                    .can_transition_to(CompensationStatus::Approved)
                    .allowed
                {
                    (
                        OrchestrationAction::Approve,
                        format!(
                            "Action is pending approval with {} feasibility",
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else {
                    (
                        OrchestrationAction::NoAction,
                        "Action is pending but cannot transition to approved".to_string(),
                    )
                }
            }

            // Approved actions can be executed (if service-executable)
            Approved => {
                // Phase 3 Batch 1 P7: Uses is_service_executable() which includes both
                // Rollback+Automatic (S1) and CounterAction+SemiAutomatic (S2) combos
                if action.is_service_executable() {
                    (
                        OrchestrationAction::Execute,
                        format!(
                            "Action is approved with {} feasibility and no blocking conditions",
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else {
                    (
                        OrchestrationAction::NoAction,
                        format!(
                            "Action is approved but requires manual execution ({})",
                            format_feasibility(action.feasibility)
                        ),
                    )
                }
            }

            // Failed actions can potentially be reapproved
            Failed => {
                if action.can_be_reapproved() {
                    (
                        OrchestrationAction::Reapprove,
                        format!(
                            "Action failed but can be reapproved ({} retry attempts remaining, {} feasibility)",
                            action.max_retries - action.attempt_count,
                            format_feasibility(action.feasibility)
                        ),
                    )
                } else if let Some(ref reason) = action.reapproval_denial_reason() {
                    (OrchestrationAction::NoAction, reason.clone())
                } else {
                    (
                        OrchestrationAction::NoAction,
                        format!(
                            "Action failed with non-retryable error or exhausted budget ({} feasibility)",
                            format_feasibility(action.feasibility)
                        ),
                    )
                }
            }

            // Terminal states - no action possible
            Executed => (
                OrchestrationAction::NoAction,
                "Action has already been executed (terminal state)".to_string(),
            ),
            Waived => (
                OrchestrationAction::NoAction,
                "Action has been waived (terminal state)".to_string(),
            ),
        };

        OrchestrationActionProposal {
            action_id: action.id,
            proposed_action,
            reason,
            current_status: action.status,
        }
    }

    /// Execute batch approve for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Approves multiple
    /// Pending compensation actions atomically where possible.
    ///
    /// **Bounded partial-success semantics:**
    /// - If an action_id is not found, it's recorded as `not_found` and continues
    /// - If an action fails validation, it's recorded as `failed` with error reason
    /// - Successful approvals are recorded as `succeeded`
    /// - Does NOT fail-fast on first error - all items are processed
    ///
    /// **Transition rules:**
    /// - Only Pending actions can be approved
    /// - Uses optimistic locking via lock_version to prevent concurrent updates
    /// - The lock_version is fetched from the action itself (no client-provided lock_version)
    ///
    /// **No background worker or queue claiming:**
    /// This is a direct service method that processes actions sequentially.
    pub async fn batch_approve(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        approved_by: Option<&str>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt approval using current lock_version
                    match self
                        .approve_action(action_id, action.lock_version, approved_by)
                        .await
                    {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err::<CompensationAction, _>(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }

    /// Execute batch reapprove for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Reapproves multiple
    /// Failed compensation actions that are eligible for retry.
    ///
    /// **Bounded partial-success semantics:** Same as batch_approve.
    ///
    /// **Policy gates (fail closed):**
    /// - Action must be in Failed status
    /// - Action must have remaining retry budget
    /// - Error code must be retryable
    ///
    /// **No background worker or queue claiming:** Same as batch_approve.
    pub async fn batch_reapprove(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt reapproval using current lock_version
                    match self.reapprove_action(action_id, action.lock_version).await {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err::<CompensationAction, _>(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }

    /// Execute batch execute for explicit compensation action IDs.
    ///
    /// Phase 3 Batch 1 (bounded manual orchestration slice): Executes multiple
    /// Approved compensation actions that are service-executable.
    ///
    /// **Bounded partial-success semantics:** Same as batch_approve.
    ///
    /// **Executor gate:** Only Approved + Service-executable actions can execute.
    ///
    /// **No background worker or queue claiming:** Same as batch_approve.
    pub async fn batch_execute(
        &self,
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        executed_by: Option<&str>,
    ) -> Result<BatchOrchestrationResult, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(action_ids.len());
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummary {
            total: action_ids.len(),
            ..Default::default()
        };

        for action_id in action_ids {
            match self.get_action(action_id).await {
                Ok(action) => {
                    // Security check: verify tenant ownership
                    if action.tenant_id != tenant_id {
                        not_found.push(action_id);
                        summary.not_found += 1;
                        summary.failed += 1;
                        outcomes.push(BatchItemOutcome {
                            action_id,
                            success: false,
                            result: Err("Action not found or access denied".to_string()),
                        });
                        continue;
                    }

                    // Attempt execution
                    match self.execute_action(action_id, executed_by).await {
                        Ok(updated) => {
                            summary.succeeded += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: true,
                                result: Ok(updated),
                            });
                        }
                        Err(e) => {
                            summary.failed += 1;
                            outcomes.push(BatchItemOutcome {
                                action_id,
                                success: false,
                                result: Err::<CompensationAction, _>(e.to_string()),
                            });
                        }
                    }
                }
                Err(IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    summary.not_found += 1;
                    summary.failed += 1;
                    outcomes.push(BatchItemOutcome {
                        action_id,
                        success: false,
                        result: Err("Compensation action not found".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BatchOrchestrationResult {
            outcomes,
            not_found,
            summary,
        })
    }
}
