use std::sync::Arc;

use crate::compensation_action::{CompensationAction, ExecutionResult};
use crate::compensation_executor::CompensationExecutor;
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::IntentRebaseError;

/// Bounded real executor for Escalation + NotPossible compensation actions.
///
/// **Phase 3 Batch 1 P7 bounded slice:**
/// - Validates action is Escalation strategy + NotPossible feasibility
/// - Validates side effect class is S4Irreversible
/// - Fetches side effect context from repository to validate target existence
/// - Returns truthful acknowledgment (not external resolution claim)
/// - All other strategy types or feasibility levels fail closed
///
/// **Summary semantics:** Escalation is "acknowledged" — the current data
/// model cannot guarantee external resolution of irreversible effects.
#[derive(Clone)]
pub struct EscalationExecutor {
    side_effect_repo: Arc<dyn SideEffectRepository>,
}

impl EscalationExecutor {
    /// Create a new EscalationExecutor with the given side effect repository.
    pub fn new(side_effect_repo: Arc<dyn SideEffectRepository>) -> Self {
        Self { side_effect_repo }
    }

    /// Execute a compensation action with Escalation + NotPossible semantics.
    ///
    /// **Bounded supported path:** Only Escalation strategy + NotPossible feasibility succeeds.
    /// All other combinations fail closed.
    async fn execute_impl(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        use crate::compensation_action::{CompensationFeasibility, StrategyType};

        // Strategy gate: only Escalation is supported in this slice
        if action.strategy_type != StrategyType::Escalation {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported strategy type: {:?}. Only Escalation is supported in this slice.",
                    action.strategy_type
                ),
                "UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "Strategy {:?} requires Batch 1+ executor implementation",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only NotPossible is supported in this slice
        if action.feasibility != CompensationFeasibility::NotPossible {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported feasibility: {:?}. Only NotPossible is supported in this slice.",
                    action.feasibility
                ),
                "UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "Feasibility {:?} requires human intervention or is not executable",
                    action.feasibility
                )),
            ));
        }

        // Side effect validation: fetch context to ensure target exists
        let side_effect = match self.side_effect_repo.get(action.side_effect_id).await {
            Ok(se) => se,
            Err(e) => {
                return Ok(ExecutionResult::failure(
                    &format!(
                        "Side effect {} not found: cannot validate escalation target",
                        action.side_effect_id
                    ),
                    "SIDE_EFFECT_NOT_FOUND",
                    Some(format!("{:?}", e)),
                ));
            }
        };

        // Validate side effect belongs to same tenant/intent as action
        if side_effect.tenant_id != action.tenant_id {
            return Ok(ExecutionResult::failure(
                "Side effect tenant_id mismatch",
                "TENANT_MISMATCH",
                Some(format!(
                    "Action tenant={}, side effect tenant={}",
                    action.tenant_id, side_effect.tenant_id
                )),
            ));
        }

        if side_effect.intent_id != action.intent_id {
            return Ok(ExecutionResult::failure(
                "Side effect intent_id mismatch",
                "INTENT_MISMATCH",
                Some(format!(
                    "Action intent={}, side effect intent={}",
                    action.intent_id, side_effect.intent_id
                )),
            ));
        }

        // Validate side effect class is S4Irreversible
        if side_effect.effect_class != crate::side_effect::SideEffectClass::S4Irreversible {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Invalid side effect class for escalation: {:?}. Expected S4Irreversible.",
                    side_effect.effect_class
                ),
                "INVALID_SIDE_EFFECT_CLASS",
                Some(format!(
                    "Escalation is only valid for S4Irreversible effects, got {:?}",
                    side_effect.effect_class
                )),
            ));
        }

        // Escalation acknowledgment: validated against side effect ledger.
        // Does NOT claim external resolution — current data model does not support
        // actual artifact resolution. Summary is truthful.
        let summary = format!(
            "Escalation for {} targeting {} acknowledged (side_effect_id={}, effect_class={:?})",
            side_effect.effect_type, side_effect.target, side_effect.id, side_effect.effect_class
        );

        Ok(ExecutionResult::success(&summary))
    }
}

impl CompensationExecutor for EscalationExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.execute_impl(action).await
    }
}
