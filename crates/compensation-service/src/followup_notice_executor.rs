use std::sync::Arc;

use crate::compensation_action::{CompensationAction, ExecutionResult};
use crate::compensation_executor::CompensationExecutor;
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::IntentRebaseError;

/// Bounded real executor for FollowupNotice + ManualOnly compensation actions.
///
/// **Phase 3 Batch 1 P7 bounded slice:**
/// - Validates action is FollowupNotice strategy + ManualOnly feasibility
/// - Validates side effect class is S3ExternalPartiallyReversible
/// - Fetches side effect context from repository to validate target existence
/// - Returns truthful acknowledgment (not external resolution claim)
/// - All other strategy types or feasibility levels fail closed
///
/// **Summary semantics:** FollowupNotice is "acknowledged" — the current data
/// model cannot guarantee external resolution of partially reversible effects.
#[derive(Clone)]
pub struct FollowupNoticeExecutor {
    side_effect_repo: Arc<dyn SideEffectRepository>,
}

impl FollowupNoticeExecutor {
    /// Create a new FollowupNoticeExecutor with the given side effect repository.
    pub fn new(side_effect_repo: Arc<dyn SideEffectRepository>) -> Self {
        Self { side_effect_repo }
    }

    /// Execute a compensation action with FollowupNotice + ManualOnly semantics.
    ///
    /// **Bounded supported path:** Only FollowupNotice strategy + ManualOnly feasibility succeeds.
    /// All other combinations fail closed.
    async fn execute_impl(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        use crate::compensation_action::{CompensationFeasibility, StrategyType};

        // Strategy gate: only FollowupNotice is supported in this slice
        if action.strategy_type != StrategyType::FollowupNotice {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported strategy type: {:?}. Only FollowupNotice is supported in this slice.",
                    action.strategy_type
                ),
                "UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "Strategy {:?} requires Batch 1+ executor implementation",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only ManualOnly is supported in this slice
        if action.feasibility != CompensationFeasibility::ManualOnly {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Unsupported feasibility: {:?}. Only ManualOnly is supported in this slice.",
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
                        "Side effect {} not found: cannot validate followup notice target",
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

        // Validate side effect class is S3ExternalPartiallyReversible
        if side_effect.effect_class
            != crate::side_effect::SideEffectClass::S3ExternalPartiallyReversible
        {
            return Ok(ExecutionResult::failure(
                &format!(
                    "Invalid side effect class for followup notice: {:?}. Expected S3ExternalPartiallyReversible.",
                    side_effect.effect_class
                ),
                "INVALID_SIDE_EFFECT_CLASS",
                Some(format!(
                    "FollowupNotice is only valid for S3ExternalPartiallyReversible effects, got {:?}",
                    side_effect.effect_class
                )),
            ));
        }

        // FollowupNotice acknowledgment: validated against side effect ledger.
        // Does NOT claim external resolution — current data model does not support
        // actual artifact resolution. Summary is truthful.
        let summary = format!(
            "FollowupNotice for {} targeting {} acknowledged (side_effect_id={}, effect_class={:?})",
            side_effect.effect_type, side_effect.target, side_effect.id, side_effect.effect_class
        );

        Ok(ExecutionResult::success(&summary))
    }
}

impl CompensationExecutor for FollowupNoticeExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.execute_impl(action).await
    }
}
