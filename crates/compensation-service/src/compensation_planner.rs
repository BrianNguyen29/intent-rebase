//! Compensation planner — skeleton contract and stub implementation.
//!
//! Phase 3 Batch 1 (bounded persistence slice): skeleton planner contract only.
//!
//! **This slice scope:** Trait definition + in-memory stub + basic test.
//! **Batch 1+ scope:** Full planner logic (risk scoring, strategy selection, etc.)
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use uuid::Uuid;

use crate::compensation_action::{CompensationAction, RebaseContext};
use intent_rebase_types::IntentRebaseError;

/// Skeleton planner: produces compensation actions from rebase context and side effects.
///
/// The planner analyzes side effects needing compensation and generates
/// CompensationAction records. Actual execution is deferred to the executor.
///
/// **This slice:** Stub implementation that generates actions from side effect IDs
/// using basic feasibility heuristics. Full planner logic is Batch 1+ scope.
pub trait CompensationPlanner: Send + Sync {
    /// Given rebase context and a list of side effect IDs, generate compensation actions.
    ///
    /// Returns the list of compensation actions to execute (may be empty).
    /// The caller is responsible for providing the side_effects via the repository
    /// if needed for detailed planning.
    fn plan(
        &self,
        rebase_context: &RebaseContext,
        side_effect_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError>;
}

/// In-memory stub planner for testing and Phase 3 Batch 1.
///
/// This is a minimal stub — actual planner logic (risk scoring, strategy selection,
/// multi-side-effect coordination) is Batch 1+ scope.
pub struct InMemoryCompensationPlanner {
    /// Default strategy type to use when creating actions
    default_strategy: crate::compensation_action::StrategyType,
}

impl InMemoryCompensationPlanner {
    /// Create a new InMemoryCompensationPlanner with the given default strategy.
    pub fn new(default_strategy: crate::compensation_action::StrategyType) -> Self {
        Self { default_strategy }
    }

    /// Create a new InMemoryCompensationPlanner with Rollback as default strategy.
    pub fn with_rollback_default() -> Self {
        Self {
            default_strategy: crate::compensation_action::StrategyType::Rollback,
        }
    }
}

impl Default for InMemoryCompensationPlanner {
    fn default() -> Self {
        Self::with_rollback_default()
    }
}

impl CompensationPlanner for InMemoryCompensationPlanner {
    fn plan(
        &self,
        rebase_context: &RebaseContext,
        side_effect_ids: &[Uuid],
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        // Stub: generate one action per side effect ID using default strategy.
        // Full planner would inspect each side effect's details to determine
        // the appropriate strategy and feasibility.
        let actions: Vec<CompensationAction> = side_effect_ids
            .iter()
            .map(|side_effect_id| {
                CompensationAction::new(
                    tenant_id,
                    *side_effect_id,
                    rebase_context.intent_id,
                    rebase_context.clone(),
                    crate::compensation_action::CompensationFeasibility::ManualOnly,
                    self.default_strategy,
                    &format!(
                        "Compensation for rebase {} -> {} on intent {}",
                        rebase_context.from_version,
                        rebase_context.to_version,
                        rebase_context.intent_id
                    ),
                )
            })
            .collect();

        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::StrategyType;

    #[test]
    fn test_in_memory_planner_creates_actions() {
        let planner = InMemoryCompensationPlanner::with_rollback_default();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let from_version = 5;
        let to_version = 6;

        let rebase_context = RebaseContext::new(intent_id, from_version, to_version, workflow_id);
        let side_effect_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

        let result = planner
            .plan(&rebase_context, &side_effect_ids, tenant_id)
            .unwrap();

        assert_eq!(result.len(), 2);
        for (i, action) in result.iter().enumerate() {
            assert_eq!(action.tenant_id, tenant_id);
            assert_eq!(action.side_effect_id, side_effect_ids[i]);
            assert_eq!(action.strategy_type, StrategyType::Rollback);
        }
    }

    #[test]
    fn test_in_memory_planner_with_custom_strategy() {
        let planner = InMemoryCompensationPlanner::new(StrategyType::CounterAction);
        let tenant_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(Uuid::new_v4(), 1, 2, Uuid::new_v4());
        let side_effect_ids = vec![Uuid::new_v4()];

        let result = planner
            .plan(&rebase_context, &side_effect_ids, tenant_id)
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].strategy_type, StrategyType::CounterAction);
    }

    #[test]
    fn test_in_memory_planner_empty_side_effects() {
        let planner = InMemoryCompensationPlanner::default();
        let tenant_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(Uuid::new_v4(), 1, 2, Uuid::new_v4());
        let side_effect_ids: Vec<Uuid> = vec![];

        let result = planner
            .plan(&rebase_context, &side_effect_ids, tenant_id)
            .unwrap();

        assert!(result.is_empty());
    }
}
