//! Approval revalidation classifier
//!
//! Pure function that takes a `RebasePlan` and current approval state,
/// and returns a `RevalidationStrategy` with details about which approvals
/// need revalidation.
///
/// Classification rules per decision class:
/// - A: Drop all approvals (rebase is trivial, no semantic changes)
/// - B: Incremental revalidation (only approvals on changed sections)
/// - C: Full revalidation of all affected approvals
/// - D: Full revalidation + manual reviewer flag
/// - E: Deferred (cannot auto-revalidate, needs human review)
use crate::planner::{
    ApprovalNeedingRevalidation, DecisionClass, RebasePlan, RevalidationStrategy,
};
use intent_rebase_types::ClassificationImpact;

/// Result of approval revalidation classification
#[derive(Debug, Clone)]
pub struct ApprovalRevalidationResult {
    /// The revalidation strategy to apply
    pub strategy: RevalidationStrategy,
    /// IDs of approvals that are now stale and should be dropped
    pub stale_ids: Vec<String>,
    /// Approvals that need revalidation with details
    pub revalidation_required: Vec<ApprovalNeedingRevalidation>,
    /// Whether manual review is required
    pub requires_manual_review: bool,
}

/// Compute affected approval IDs from the plan's affected items preview
///
/// Returns a set of node_id strings for approvals that are directly or transitively affected.
fn compute_affected_approval_ids(plan: &RebasePlan) -> Vec<String> {
    plan.affected_items
        .affected_approvals
        .iter()
        .filter(|item| item.impact != ClassificationImpact::Unchanged)
        .map(|item| item.node_id.to_string())
        .collect()
}

/// Classify approvals based on the rebase plan
///
/// This is a pure function that takes a `RebasePlan` and the current list of
/// approvals, and returns an `ApprovalRevalidationResult` describing which
/// approvals need revalidation and with what strategy.
///
/// # Arguments
/// * `plan` - The rebase plan containing decision class and affected items
/// * `current_approvals` - Slice of approval node IDs (as strings) that are currently active
///
/// # Returns
/// `ApprovalRevalidationResult` with:
/// - `strategy`: The revalidation strategy to apply
/// - `stale_ids`: Approval IDs that are now stale
/// - `revalidation_required`: Details of approvals needing revalidation
/// - `requires_manual_review`: Whether manual review is needed
pub fn classify_approvals(
    plan: &RebasePlan,
    current_approvals: &[String],
) -> ApprovalRevalidationResult {
    let affected_ids = compute_affected_approval_ids(plan);
    // stale_ids is the intersection of affected approvals and current approvals
    // Only approvals that BOTH exist (current_approvals) AND are affected should be marked stale
    let current_approval_set: std::collections::HashSet<_> = current_approvals.iter().collect();
    let stale_ids: Vec<String> = affected_ids
        .iter()
        .filter(|id| current_approval_set.contains(id))
        .cloned()
        .collect();

    match plan.decision_class {
        // Class A: No semantic changes - all approvals are stale, strategy=Drop
        DecisionClass::A => {
            let stale_ids: Vec<String> = current_approvals.to_vec();
            ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Drop,
                stale_ids,
                revalidation_required: vec![],
                requires_manual_review: false,
            }
        }

        // Class B: Incremental revalidation - only approvals on changed sections
        DecisionClass::B => {
            // Build revalidation items from affected approvals
            let revalidation_required: Vec<ApprovalNeedingRevalidation> = plan
                .affected_items
                .affected_approvals
                .iter()
                .filter(|item| item.impact != ClassificationImpact::Unchanged)
                .map(|item| ApprovalNeedingRevalidation {
                    node_id: item.node_id.to_string(),
                    label: item.label.clone(),
                    original_rule_id: item
                        .external_ref
                        .as_ref()
                        .map(|r| format!("{:?}", r))
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: item.reason.clone(),
                })
                .collect();

            ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Incremental,
                stale_ids,
                revalidation_required,
                requires_manual_review: false,
            }
        }

        // Class C: Full revalidation of all affected approvals
        DecisionClass::C => {
            let revalidation_required: Vec<ApprovalNeedingRevalidation> = plan
                .affected_items
                .affected_approvals
                .iter()
                .filter(|item| item.impact != ClassificationImpact::Unchanged)
                .map(|item| ApprovalNeedingRevalidation {
                    node_id: item.node_id.to_string(),
                    label: item.label.clone(),
                    original_rule_id: item
                        .external_ref
                        .as_ref()
                        .map(|r| format!("{:?}", r))
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: item.reason.clone(),
                })
                .collect();

            ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Full,
                stale_ids,
                revalidation_required,
                requires_manual_review: false,
            }
        }

        // Class D: Full revalidation + manual reviewer flag
        DecisionClass::D => {
            let revalidation_required: Vec<ApprovalNeedingRevalidation> = plan
                .affected_items
                .affected_approvals
                .iter()
                .filter(|item| item.impact != ClassificationImpact::Unchanged)
                .map(|item| ApprovalNeedingRevalidation {
                    node_id: item.node_id.to_string(),
                    label: item.label.clone(),
                    original_rule_id: item
                        .external_ref
                        .as_ref()
                        .map(|r| format!("{:?}", r))
                        .unwrap_or_else(|| "unknown".to_string()),
                    reason: item.reason.clone(),
                })
                .collect();

            ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Full,
                stale_ids,
                revalidation_required,
                requires_manual_review: true,
            }
        }

        // Class E: Deferred - cannot auto-revalidate, needs human review
        DecisionClass::E => {
            // For E class, we don't mark anything as stale since we can't
            // auto-revalidate - human review is required first
            ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Deferred,
                stale_ids: vec![],
                revalidation_required: vec![],
                requires_manual_review: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::DeferredFields;
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, AffectedItemsStatus};
    use uuid::Uuid;

    fn create_test_plan(decision_class: DecisionClass) -> RebasePlan {
        RebasePlan {
            decision_class,
            rationale: "Test plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: DeferredFields::default(),
            manual_review_recommended: false,
            risk_level: 1,
        }
    }

    fn create_plan_with_affected_approvals(
        decision_class: DecisionClass,
        affected_approvals: Vec<AffectedItem>,
    ) -> RebasePlan {
        RebasePlan {
            decision_class,
            rationale: "Test plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview {
                status: AffectedItemsStatus::Available,
                affected_artifacts: vec![],
                affected_approvals,
                side_effects: vec![],
            },
            deferred: DeferredFields::default(),
            manual_review_recommended: false,
            risk_level: 1,
        }
    }

    // === Decision Class A Tests ===

    #[test]
    fn test_class_a_all_approvals_stale() {
        let plan = create_test_plan(DecisionClass::A);
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Drop);
        assert_eq!(result.stale_ids, vec!["approval-1", "approval-2"]);
        assert!(result.revalidation_required.is_empty());
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_class_a_empty_approvals() {
        let plan = create_test_plan(DecisionClass::A);
        let current_approvals: Vec<String> = vec![];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Drop);
        assert!(result.stale_ids.is_empty());
        assert!(result.revalidation_required.is_empty());
        assert!(!result.requires_manual_review);
    }

    // === Decision Class B Tests ===

    #[test]
    fn test_class_b_incremental_revalidation() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Security Review".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Directly affected by scope change".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);
        let current_approvals = vec![
            "approval-1".to_string(),
            affected_approval_id.to_string(),
            "approval-3".to_string(),
        ];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Incremental);
        assert_eq!(result.stale_ids, vec![affected_approval_id.to_string()]);
        assert_eq!(result.revalidation_required.len(), 1);
        assert_eq!(
            result.revalidation_required[0].node_id,
            affected_approval_id.to_string()
        );
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_class_b_no_affected_approvals() {
        let plan = create_plan_with_affected_approvals(DecisionClass::B, vec![]);
        let current_approvals = vec!["approval-1".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Incremental);
        assert!(result.stale_ids.is_empty());
        assert!(result.revalidation_required.is_empty());
        assert!(!result.requires_manual_review);
    }

    // === Decision Class C Tests ===

    #[test]
    fn test_class_c_full_revalidation() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Compliance Check".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Affected by constraint change".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::C, affected_approvals);
        let current_approvals = vec![affected_approval_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec![affected_approval_id.to_string()]);
        assert_eq!(result.revalidation_required.len(), 1);
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_class_c_transitive_impact() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Indirect Approval".to_string(),
            impact: ClassificationImpact::Transitive,
            reason: "Transitively affected through dependency chain".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::C, affected_approvals);
        let current_approvals = vec![affected_approval_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Both Direct and Transitive impacts require revalidation
        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids.len(), 1);
        assert_eq!(result.revalidation_required.len(), 1);
    }

    // === Decision Class D Tests ===

    #[test]
    fn test_class_d_full_revalidation_with_manual_review() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "High Risk Approval".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "High severity section affected".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::D, affected_approvals);
        let current_approvals = vec![affected_approval_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert!(result.requires_manual_review);
        assert_eq!(result.revalidation_required.len(), 1);
    }

    #[test]
    fn test_class_d_multiple_affected_approvals() {
        let approval1_id = Uuid::new_v4();
        let approval2_id = Uuid::new_v4();
        let affected_approvals = vec![
            AffectedItem {
                node_id: approval1_id,
                label: "Approval 1".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            },
            AffectedItem {
                node_id: approval2_id,
                label: "Approval 2".to_string(),
                impact: ClassificationImpact::Transitive,
                reason: "Transitively affected".to_string(),
                external_ref: None,
            },
        ];

        let plan = create_plan_with_affected_approvals(DecisionClass::D, affected_approvals);
        let current_approvals = vec![approval1_id.to_string(), approval2_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert!(result.requires_manual_review);
        assert_eq!(result.stale_ids.len(), 2);
        assert_eq!(result.revalidation_required.len(), 2);
    }

    // === Decision Class E Tests ===

    #[test]
    fn test_class_e_deferred_no_auto_revalidation() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Critical Approval".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Critical severity change".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::E, affected_approvals);
        let current_approvals = vec![affected_approval_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // E class defers - no automatic stale marking or revalidation
        assert_eq!(result.strategy, RevalidationStrategy::Deferred);
        assert!(result.stale_ids.is_empty());
        assert!(result.revalidation_required.is_empty());
        assert!(result.requires_manual_review);
    }

    // === Edge Case Tests ===

    #[test]
    fn test_unchanged_impact_not_in_stale() {
        let changed_id = Uuid::new_v4();
        let unchanged_id = Uuid::new_v4();
        let affected_approvals = vec![
            AffectedItem {
                node_id: changed_id,
                label: "Changed Approval".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            },
            AffectedItem {
                node_id: unchanged_id,
                label: "Unchanged Approval".to_string(),
                impact: ClassificationImpact::Unchanged,
                reason: "Not affected".to_string(),
                external_ref: None,
            },
        ];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);
        let current_approvals = vec![changed_id.to_string(), unchanged_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Only Direct impact should be in stale_ids
        assert_eq!(result.stale_ids, vec![changed_id.to_string()]);
        // Unchanged should not be in revalidation_required
        assert_eq!(result.revalidation_required.len(), 1);
        assert_eq!(
            result.revalidation_required[0].node_id,
            changed_id.to_string()
        );
    }

    #[test]
    fn test_empty_current_approvals() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Test Approval".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Directly affected".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::C, affected_approvals);
        let current_approvals: Vec<String> = vec![];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert!(result.stale_ids.is_empty());
        assert_eq!(result.revalidation_required.len(), 1);
    }

    #[test]
    fn test_all_decision_classes_deterministic() {
        // Same input should always produce same output
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Test".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Test".to_string(),
            external_ref: None,
        }];

        for decision_class in [
            DecisionClass::A,
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            let plan =
                create_plan_with_affected_approvals(decision_class, affected_approvals.clone());
            let current_approvals = vec![affected_approval_id.to_string()];

            let result1 = classify_approvals(&plan, &current_approvals);
            let result2 = classify_approvals(&plan, &current_approvals);

            assert_eq!(result1.strategy, result2.strategy);
            assert_eq!(result1.stale_ids, result2.stale_ids);
            assert_eq!(
                result1.revalidation_required.len(),
                result2.revalidation_required.len()
            );
            assert_eq!(
                result1.requires_manual_review,
                result2.requires_manual_review
            );
        }
    }

    #[test]
    fn test_unknown_external_ref_graceful_handling() {
        let affected_approval_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_approval_id,
            label: "Approval Without Ref".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "No external reference".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);
        let current_approvals = vec![affected_approval_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.revalidation_required.len(), 1);
        assert_eq!(result.revalidation_required[0].original_rule_id, "unknown");
    }
}
