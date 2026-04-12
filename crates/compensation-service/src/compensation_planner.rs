//! Compensation planner — skeleton contract and stub implementation.
//!
//! Phase 3 Batch 1 (bounded persistence slice): skeleton planner contract only.
//!
//! **This slice scope:** Trait definition + in-memory stub + bounded side-effect-aware planner.
//! **Batch 1+ scope:** Full planner logic (risk scoring, strategy selection, etc.)
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use uuid::Uuid;

use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, RebaseContext, StrategyType,
};
use crate::side_effect::{SideEffect, SideEffectClass};
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

/// Bounded side-effect-aware compensation planner.
///
/// Phase 3 (this slice): Implements S0-S4 classification logic using actual
/// SideEffect data to generate appropriate compensation actions.
///
/// **Classification rules:**
/// | Class | Strategy | Feasibility | Rationale |
/// |-------|----------|-------------|-----------|
/// | S0PureRead | Quarantine | NotPossible | Skip - no action needed |
/// | S1InternalReversible | Rollback | Automatic | Auto rollback internal changes |
/// | S2ExternalReversible | CounterAction | SemiAutomatic | Counter-action for external effect |
/// | S3ExternalPartiallyReversible | FollowupNotice | ManualOnly | Manual followup required |
/// | S4Irreversible | Escalation | NotPossible | Escalation required - cannot compensate |
pub struct BoundedCompensationPlanner {
    /// Reserved field for future extension (e.g., custom strategy per class).
    _reserved: (),
}

impl BoundedCompensationPlanner {
    /// Create a new BoundedCompensationPlanner with default settings.
    pub fn new() -> Self {
        Self { _reserved: () }
    }

    /// Plan compensation actions from actual side effects.
    ///
    /// Uses S0-S4 classification to determine strategy and feasibility.
    /// S0 (pure read) produces no action - returns empty Vec.
    pub fn plan_from_side_effects(
        &self,
        rebase_context: &RebaseContext,
        side_effects: &[SideEffect],
        tenant_id: Uuid,
    ) -> Vec<CompensationAction> {
        side_effects
            .iter()
            .filter_map(|side_effect| {
                self.classify_and_plan_action(tenant_id, side_effect, rebase_context)
            })
            .collect()
    }

    /// Classify a side effect and generate a compensation action.
    ///
    /// Returns None for S0 (pure read) - no compensation needed.
    fn classify_and_plan_action(
        &self,
        tenant_id: Uuid,
        side_effect: &SideEffect,
        rebase_context: &RebaseContext,
    ) -> Option<CompensationAction> {
        match side_effect.effect_class {
            SideEffectClass::S0PureRead => {
                // S0: Pure read - no side effect, no compensation needed
                tracing::debug!(
                    "S0 pure read for side effect {} on intent {} - no action needed",
                    side_effect.id,
                    side_effect.intent_id
                );
                None
            }
            SideEffectClass::S1InternalReversible => {
                // S1: Internal reversible - automatic rollback possible
                Some(self.create_action(
                    tenant_id,
                    side_effect,
                    rebase_context,
                    StrategyType::Rollback,
                    CompensationFeasibility::Automatic,
                    &format!(
                        "Auto rollback internal reversible effect '{}' for rebase v{} -> v{}",
                        side_effect.effect_type,
                        rebase_context.from_version,
                        rebase_context.to_version
                    ),
                ))
            }
            SideEffectClass::S2ExternalReversible => {
                // S2: External reversible - counter-action compensation
                Some(self.create_action(
                    tenant_id,
                    side_effect,
                    rebase_context,
                    StrategyType::CounterAction,
                    CompensationFeasibility::SemiAutomatic,
                    &format!(
                        "Counter-action for external reversible effect '{}' targeting {} for rebase v{} -> v{}",
                        side_effect.effect_type,
                        side_effect.target,
                        rebase_context.from_version,
                        rebase_context.to_version
                    ),
                ))
            }
            SideEffectClass::S3ExternalPartiallyReversible => {
                // S3: External partially reversible - manual followup notice
                Some(self.create_action(
                    tenant_id,
                    side_effect,
                    rebase_context,
                    StrategyType::FollowupNotice,
                    CompensationFeasibility::ManualOnly,
                    &format!(
                        "Send followup notice for partially reversible effect '{}' targeting {} - manual intervention required for rebase v{} -> v{}",
                        side_effect.effect_type,
                        side_effect.target,
                        rebase_context.from_version,
                        rebase_context.to_version
                    ),
                ))
            }
            SideEffectClass::S4Irreversible => {
                // S4: Irreversible - escalation required, cannot compensate
                Some(self.create_action(
                    tenant_id,
                    side_effect,
                    rebase_context,
                    StrategyType::Escalation,
                    CompensationFeasibility::NotPossible,
                    &format!(
                        "ESCALATION: Irreversible effect '{}' targeting {} cannot be compensated for rebase v{} -> v{} - manual investigation required",
                        side_effect.effect_type,
                        side_effect.target,
                        rebase_context.from_version,
                        rebase_context.to_version
                    ),
                ))
            }
        }
    }

    /// Create a compensation action from a side effect.
    fn create_action(
        &self,
        tenant_id: Uuid,
        side_effect: &SideEffect,
        rebase_context: &RebaseContext,
        strategy: StrategyType,
        _feasibility: CompensationFeasibility,
        rationale: &str,
    ) -> CompensationAction {
        CompensationAction::from_side_effect(
            tenant_id,
            side_effect,
            rebase_context,
            strategy,
            rationale,
        )
    }
}

impl Default for BoundedCompensationPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod bounded_planner_tests {
    use super::*;

    fn create_test_rebase_context(intent_id: Uuid) -> RebaseContext {
        RebaseContext::new(intent_id, 5, 6, Uuid::new_v4())
    }

    #[test]
    fn test_s0_pure_read_no_action() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S0PureRead,
            "read",
            "noop",
        );

        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert!(
            actions.is_empty(),
            "S0 should produce no compensation action"
        );
    }

    #[test]
    fn test_s1_internal_reversible_auto_rollback() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record-123",
        );

        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        assert_eq!(action.strategy_type, StrategyType::Rollback);
        assert_eq!(action.feasibility, CompensationFeasibility::Automatic);
        assert!(action.rationale.contains("internal reversible"));
    }

    #[test]
    fn test_s2_external_reversible_semi_auto() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S2ExternalReversible,
            "pr_opened",
            "https://github.com/org/repo/pull/456",
        );

        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        assert_eq!(action.strategy_type, StrategyType::CounterAction);
        assert_eq!(action.feasibility, CompensationFeasibility::SemiAutomatic);
    }

    #[test]
    fn test_s3_external_partially_reversible_manual() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S3ExternalPartiallyReversible,
            "email_sent",
            "user@example.com",
        );

        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        assert_eq!(action.strategy_type, StrategyType::FollowupNotice);
        assert_eq!(action.feasibility, CompensationFeasibility::ManualOnly);
    }

    #[test]
    fn test_s4_irreversible_escalation() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S4Irreversible,
            "money_transfer",
            "account-xyz-amount-1000",
        );

        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        assert_eq!(action.strategy_type, StrategyType::Escalation);
        assert_eq!(action.feasibility, CompensationFeasibility::NotPossible);
        assert!(action.rationale.contains("ESCALATION"));
    }

    #[test]
    fn test_mixed_side_effects() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effects = vec![
            SideEffect::new(
                tenant_id,
                intent_id,
                5,
                SideEffectClass::S0PureRead,
                "read",
                "noop",
            ),
            SideEffect::new(
                tenant_id,
                intent_id,
                5,
                SideEffectClass::S1InternalReversible,
                "metadata_write",
                "db-record",
            ),
            SideEffect::new(
                tenant_id,
                intent_id,
                5,
                SideEffectClass::S2ExternalReversible,
                "pr_opened",
                "https://github.com/pull/123",
            ),
            SideEffect::new(
                tenant_id,
                intent_id,
                5,
                SideEffectClass::S3ExternalPartiallyReversible,
                "email_sent",
                "user@example.com",
            ),
            SideEffect::new(
                tenant_id,
                intent_id,
                5,
                SideEffectClass::S4Irreversible,
                "money_transfer",
                "account-xyz",
            ),
        ];

        let actions = planner.plan_from_side_effects(&rebase_context, &side_effects, tenant_id);

        // S0 produces no action, so 4 actions instead of 5
        assert_eq!(actions.len(), 4);

        // Verify action types are as expected
        let strategies: Vec<StrategyType> = actions.iter().map(|a| a.strategy_type).collect();
        assert!(strategies.contains(&StrategyType::Rollback)); // S1
        assert!(strategies.contains(&StrategyType::FollowupNotice)); // S3
        assert!(strategies.contains(&StrategyType::Escalation)); // S4
    }

    #[test]
    fn test_empty_side_effects() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let actions = planner.plan_from_side_effects(&rebase_context, &[], tenant_id);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_action_has_correct_metadata() {
        let planner = BoundedCompensationPlanner::new();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = create_test_rebase_context(intent_id);

        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            5,
            SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record-123",
        );

        let side_effect_id = side_effect.id;
        let actions = planner.plan_from_side_effects(&rebase_context, &[side_effect], tenant_id);
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        assert_eq!(action.tenant_id, tenant_id);
        assert_eq!(action.side_effect_id, side_effect_id);
        assert_eq!(action.intent_id, intent_id);
        assert_eq!(action.trigger_context.from_version, 5);
        assert_eq!(action.trigger_context.to_version, 6);
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
