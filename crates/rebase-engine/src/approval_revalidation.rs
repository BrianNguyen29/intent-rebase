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
#[allow(unused_imports)]
use crate::planner::{
    ApprovalNeedingRevalidation, DecisionClass, RebasePlan, RevalidationStrategy, RiskTier,
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
/// Returns a set of correlation IDs for approvals that are directly or transitively affected.
/// Uses `external_ref.ref_id` as the correlation key when available (this maps to
/// `ApprovalRequest.id`), falling back to `node_id` (graph node UUID) when external_ref
/// is not populated.
///
/// This ensures targeted cancellation can correctly correlate affected graph items
/// with actual ApprovalRequest records.
fn compute_affected_approval_ids(plan: &RebasePlan) -> Vec<String> {
    plan.affected_items
        .affected_approvals
        .iter()
        .filter(|item| item.impact != ClassificationImpact::Unchanged)
        .map(|item| {
            // Prefer external_ref.ref_id as the correlation key when available.
            // This maps to ApprovalRequest.id for targeted cancellation.
            // Fall back to node_id (graph node UUID) when external_ref is absent.
            if let Some(ref ext) = item.external_ref {
                ext.ref_id.to_string()
            } else {
                item.node_id.to_string()
            }
        })
        .collect()
}

/// Classify approvals based on the rebase plan
///
/// This is a pure function that takes a `RebasePlan` and the current list of
/// approvals, and returns an `ApprovalRevalidationResult` describing which
/// approvals need revalidation and with what strategy.
///
/// # Risk Tier Rules (ADR-07)
///
/// - **Critical**: Full invalidation of all current approvals (unless DecisionClass::E)
/// - **High**: Partial invalidation using affected approval IDs/external_ref correlation
/// - **Medium**: Log + notify; no approval cancellation
/// - **Low**: No approval impact
///
/// # Decision Class Override
///
/// DecisionClass::E always takes precedence over risk tier and results in
/// Deferred strategy with manual review required.
///
/// # Arguments
/// * `plan` - The rebase plan containing decision class, risk tier, and affected items
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
    // DecisionClass::E overrides risk tier - always Deferred with manual review
    if plan.decision_class == DecisionClass::E {
        return ApprovalRevalidationResult {
            strategy: RevalidationStrategy::Deferred,
            stale_ids: vec![],
            revalidation_required: vec![],
            requires_manual_review: true,
        };
    }

    let affected_ids = compute_affected_approval_ids(plan);
    // stale_ids is the intersection of affected approvals and current approvals
    // Only approvals that BOTH exist (current_approvals) AND are affected should be marked stale
    let current_approval_set: std::collections::HashSet<_> = current_approvals.iter().collect();
    let stale_ids: Vec<String> = affected_ids
        .iter()
        .filter(|id| current_approval_set.contains(id))
        .cloned()
        .collect();

    // ADR-07 risk tier rules applied before decision class logic
    match plan.risk_tier {
        // Critical: Full invalidation of all current approvals
        RiskTier::Critical => {
            return ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Full,
                stale_ids: current_approvals.to_vec(),
                revalidation_required: vec![],
                requires_manual_review: true,
            };
        }

        // Medium: Log + notify; no approval cancellation
        RiskTier::Medium => {
            return ApprovalRevalidationResult {
                strategy: RevalidationStrategy::LogNotify,
                stale_ids: vec![],
                revalidation_required: vec![],
                requires_manual_review: false,
            };
        }

        // Low: No approval impact
        RiskTier::Low => {
            return ApprovalRevalidationResult {
                strategy: RevalidationStrategy::Incremental,
                stale_ids: vec![],
                revalidation_required: vec![],
                requires_manual_review: false,
            };
        }

        // High: Partial invalidation using affected approval IDs (proceed to decision class)
        RiskTier::High => {}
    }

    // For High risk tier with decision class A, we still want incremental/partial behavior
    // not full Drop. High risk means partial invalidation of affected items only.
    let use_incremental_for_high_a =
        plan.risk_tier == RiskTier::High && plan.decision_class == DecisionClass::A;

    match plan.decision_class {
        // Class A: No semantic changes - all approvals are stale, strategy=Drop
        // Exception: High risk tier uses incremental/partial invalidation instead
        DecisionClass::A => {
            let stale_ids: Vec<String> = if use_incremental_for_high_a {
                // High risk: partial invalidation of affected items only
                affected_ids
                    .iter()
                    .filter(|id| current_approval_set.contains(id))
                    .cloned()
                    .collect()
            } else {
                current_approvals.to_vec()
            };
            ApprovalRevalidationResult {
                strategy: if use_incremental_for_high_a {
                    RevalidationStrategy::Incremental
                } else {
                    RevalidationStrategy::Drop
                },
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

    fn create_test_plan_with_risk(
        decision_class: DecisionClass,
        risk_tier: RiskTier,
    ) -> RebasePlan {
        let risk_level = match risk_tier {
            RiskTier::Low => 1,
            RiskTier::Medium => 2,
            RiskTier::High => 3,
            RiskTier::Critical => 4,
        };
        RebasePlan {
            decision_class,
            rationale: "Test plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: DeferredFields::default(),
            manual_review_recommended: false,
            risk_tier,
            risk_level,
        }
    }

    fn create_plan_with_affected_approvals(
        decision_class: DecisionClass,
        affected_approvals: Vec<AffectedItem>,
    ) -> RebasePlan {
        // Default to High risk for standard decision class tests.
        // See create_test_plan for rationale.
        create_plan_with_affected_approvals_and_risk(
            decision_class,
            affected_approvals,
            RiskTier::High,
        )
    }

    fn create_plan_with_affected_approvals_and_risk(
        decision_class: DecisionClass,
        affected_approvals: Vec<AffectedItem>,
        risk_tier: RiskTier,
    ) -> RebasePlan {
        let risk_level = match risk_tier {
            RiskTier::Low => 1,
            RiskTier::Medium => 2,
            RiskTier::High => 3,
            RiskTier::Critical => 4,
        };
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
            risk_tier,
            risk_level,
        }
    }

    // === Decision Class A Tests ===

    #[test]
    fn test_class_a_all_approvals_stale() {
        // With ADR-07 risk semantics, only Critical risk marks ALL approvals stale.
        // High risk + Class A uses incremental (partial) invalidation.
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Critical);
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec!["approval-1", "approval-2"]);
        assert!(result.revalidation_required.is_empty());
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_class_a_empty_approvals() {
        // With Critical risk and empty approvals, should return Full with empty stale_ids
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Critical);
        let current_approvals: Vec<String> = vec![];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert!(result.stale_ids.is_empty());
        assert!(result.revalidation_required.is_empty());
        assert!(result.requires_manual_review);
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

    // === External Ref Correlation Tests ===

    #[test]
    fn test_external_ref_ref_id_used_when_present() {
        // When external_ref is present, external_ref.ref_id should be used as correlation ID
        // (not node_id), because external_ref.ref_id maps to ApprovalRequest.id
        let graph_node_id = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4(); // This is the actual ApprovalRequest.id

        let affected_approvals = vec![AffectedItem {
            node_id: graph_node_id,
            label: "Approval With External Ref".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Has external reference".to_string(),
            external_ref: Some(intent_rebase_types::ExternalRef {
                ref_type: intent_rebase_types::ExternalRefType::Approval,
                ref_id: approval_request_id,
            }),
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);

        // The current_approvals contains the actual ApprovalRequest.id values
        let current_approvals = vec![approval_request_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // stale_ids should contain the approval_request_id (from external_ref.ref_id),
        // NOT the graph_node_id
        assert_eq!(result.stale_ids.len(), 1);
        assert_eq!(result.stale_ids[0], approval_request_id.to_string());
    }

    #[test]
    fn test_node_id_used_as_fallback_when_no_external_ref() {
        // When external_ref is absent, node_id (graph node UUID) should be used
        let graph_node_id = Uuid::new_v4();

        let affected_approvals = vec![AffectedItem {
            node_id: graph_node_id,
            label: "Approval Without External Ref".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "No external reference".to_string(),
            external_ref: None,
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);

        // When there's no external_ref, the current_approvals would need to match node_id
        let current_approvals = vec![graph_node_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // stale_ids should contain the graph_node_id (since no external_ref exists)
        assert_eq!(result.stale_ids.len(), 1);
        assert_eq!(result.stale_ids[0], graph_node_id.to_string());
    }

    #[test]
    fn test_mixed_external_ref_and_node_id_fallback() {
        // Test scenario where some affected items have external_ref and some don't
        let graph_node_id_1 = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4();
        let graph_node_id_2 = Uuid::new_v4();

        let affected_approvals = vec![
            AffectedItem {
                node_id: graph_node_id_1,
                label: "Approval With External Ref".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Has external reference".to_string(),
                external_ref: Some(intent_rebase_types::ExternalRef {
                    ref_type: intent_rebase_types::ExternalRefType::Approval,
                    ref_id: approval_request_id,
                }),
            },
            AffectedItem {
                node_id: graph_node_id_2,
                label: "Approval Without External Ref".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "No external reference".to_string(),
                external_ref: None,
            },
        ];

        let plan = create_plan_with_affected_approvals(DecisionClass::B, affected_approvals);

        // current_approvals contains both the approval_request_id and graph_node_id_2
        let current_approvals = vec![approval_request_id.to_string(), graph_node_id_2.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Both should be marked stale: approval_request_id from external_ref,
        // and graph_node_id_2 as fallback
        assert_eq!(result.stale_ids.len(), 2);
        assert!(result.stale_ids.contains(&approval_request_id.to_string()));
        assert!(result.stale_ids.contains(&graph_node_id_2.to_string()));
    }

    #[test]
    fn test_external_ref_with_different_node_id() {
        // Verify that when external_ref.ref_id exists, it's used even if node_id differs
        let graph_node_id = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4();

        let affected_approvals = vec![AffectedItem {
            node_id: graph_node_id,
            label: "Approval With Different Node ID".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Node ID differs from approval request ID".to_string(),
            external_ref: Some(intent_rebase_types::ExternalRef {
                ref_type: intent_rebase_types::ExternalRefType::Approval,
                ref_id: approval_request_id,
            }),
        }];

        let plan = create_plan_with_affected_approvals(DecisionClass::C, affected_approvals);

        // Only the approval_request_id is in current_approvals (graph_node_id is NOT)
        let current_approvals = vec![approval_request_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // The stale_ids should be approval_request_id (from external_ref.ref_id),
        // NOT graph_node_id. This means the approval gets correctly identified as stale.
        assert_eq!(result.stale_ids.len(), 1);
        assert_eq!(result.stale_ids[0], approval_request_id.to_string());
    }

    // === Risk Tier Tests (ADR-07) ===

    #[test]
    fn test_critical_risk_marks_all_approvals_stale() {
        // Critical risk should invalidate ALL current approvals regardless of decision class
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Critical);
        let current_approvals = vec![
            "approval-1".to_string(),
            "approval-2".to_string(),
            "approval-3".to_string(),
        ];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(
            result.stale_ids,
            vec!["approval-1", "approval-2", "approval-3"]
        );
        assert!(result.requires_manual_review);
        assert!(result.revalidation_required.is_empty());
    }

    #[test]
    fn test_critical_risk_overrides_class_b() {
        // Critical risk should override decision class B
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::B,
            vec![AffectedItem {
                node_id: Uuid::new_v4(),
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Critical,
        );
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Critical should mark ALL approvals stale, not just affected ones
        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec!["approval-1", "approval-2"]);
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_critical_risk_overrides_class_c() {
        // Critical risk should override decision class C
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::C,
            vec![AffectedItem {
                node_id: Uuid::new_v4(),
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Critical,
        );
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec!["approval-1", "approval-2"]);
    }

    #[test]
    fn test_class_e_overrides_critical_risk() {
        // DecisionClass::E should override Critical risk tier
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::E,
            vec![AffectedItem {
                node_id: Uuid::new_v4(),
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Critical,
        );
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // E class should always be Deferred regardless of risk tier
        assert_eq!(result.strategy, RevalidationStrategy::Deferred);
        assert!(result.stale_ids.is_empty());
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_class_e_overrides_high_risk() {
        // DecisionClass::E should override High risk tier
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::E,
            vec![AffectedItem {
                node_id: Uuid::new_v4(),
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::High,
        );
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Deferred);
        assert!(result.stale_ids.is_empty());
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_class_e_overrides_medium_risk() {
        // DecisionClass::E should override Medium risk tier
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::E,
            vec![],
            RiskTier::Medium,
        );
        let current_approvals = vec!["approval-1".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Deferred);
        assert!(result.stale_ids.is_empty());
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_class_e_overrides_low_risk() {
        // DecisionClass::E should override Low risk tier
        let plan =
            create_plan_with_affected_approvals_and_risk(DecisionClass::E, vec![], RiskTier::Low);
        let current_approvals = vec!["approval-1".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Deferred);
        assert!(result.stale_ids.is_empty());
        assert!(result.requires_manual_review);
    }

    #[test]
    fn test_high_risk_partial_invalidation_class_a() {
        // High risk with Class A should use incremental (partial) invalidation, not full Drop
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::A,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::High,
        );
        let current_approvals = vec![
            affected_id.to_string(),
            "unaffected-1".to_string(),
            "unaffected-2".to_string(),
        ];

        let result = classify_approvals(&plan, &current_approvals);

        // High risk + Class A should be incremental, not Drop
        // Only affected approvals should be stale, not all
        assert_eq!(result.strategy, RevalidationStrategy::Incremental);
        assert_eq!(result.stale_ids, vec![affected_id.to_string()]);
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_high_risk_with_class_b() {
        // High risk with Class B should use incremental (partial) invalidation
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::B,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::High,
        );
        let current_approvals = vec![affected_id.to_string(), "unaffected".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Incremental);
        assert_eq!(result.stale_ids, vec![affected_id.to_string()]);
    }

    #[test]
    fn test_high_risk_with_class_c() {
        // High risk with Class C should use full (but only of affected)
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::C,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::High,
        );
        let current_approvals = vec![affected_id.to_string(), "unaffected".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec![affected_id.to_string()]);
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_medium_risk_no_cancellation() {
        // Medium risk should LogNotify and NOT cancel any approvals
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::B,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Medium,
        );
        let current_approvals = vec![affected_id.to_string(), "other".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Medium risk should NOT cancel any approvals
        assert_eq!(result.strategy, RevalidationStrategy::LogNotify);
        assert!(result.stale_ids.is_empty());
        assert!(!result.requires_manual_review);
        assert!(result.revalidation_required.is_empty());
    }

    #[test]
    fn test_medium_risk_with_class_a() {
        // Medium risk with Class A should still LogNotify, not Drop
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Medium);
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Medium should LogNotify regardless of decision class
        assert_eq!(result.strategy, RevalidationStrategy::LogNotify);
        assert!(result.stale_ids.is_empty());
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_medium_risk_with_class_d() {
        // Medium risk with Class D should still LogNotify (no cancellation)
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::D,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Medium,
        );
        let current_approvals = vec![affected_id.to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Even Class D + Medium should LogNotify
        assert_eq!(result.strategy, RevalidationStrategy::LogNotify);
        assert!(result.stale_ids.is_empty());
        // Class D normally requires manual review, but Medium takes precedence
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_low_risk_no_approval_impact() {
        // Low risk should have no approval impact
        let affected_id = Uuid::new_v4();
        let plan = create_plan_with_affected_approvals_and_risk(
            DecisionClass::B,
            vec![AffectedItem {
                node_id: affected_id,
                label: "Affected".to_string(),
                impact: ClassificationImpact::Direct,
                reason: "Directly affected".to_string(),
                external_ref: None,
            }],
            RiskTier::Low,
        );
        let current_approvals = vec![affected_id.to_string(), "other".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Low risk should have no impact on approvals
        assert_eq!(result.strategy, RevalidationStrategy::Incremental);
        assert!(result.stale_ids.is_empty());
        assert!(!result.requires_manual_review);
        assert!(result.revalidation_required.is_empty());
    }

    #[test]
    fn test_low_risk_with_class_a() {
        // Low risk with Class A should have no approval impact
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Low);
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Low risk should not drop approvals even for Class A
        assert!(result.stale_ids.is_empty());
        assert!(!result.requires_manual_review);
    }

    #[test]
    fn test_risk_tier_is_respected_before_decision_class() {
        // Verify risk tier takes precedence over decision class for critical scenarios
        // Critical + A should not behave like Class A (Drop) but like Critical (Full)
        let plan = create_test_plan_with_risk(DecisionClass::A, RiskTier::Critical);
        let current_approvals = vec!["approval-1".to_string(), "approval-2".to_string()];

        let result = classify_approvals(&plan, &current_approvals);

        // Should be Critical behavior (Full), not Class A behavior (Drop)
        assert_eq!(result.strategy, RevalidationStrategy::Full);
        assert_eq!(result.stale_ids, vec!["approval-1", "approval-2"]);
    }

    #[test]
    fn test_all_risk_tiers_deterministic() {
        // Same input should always produce same output
        let affected_id = Uuid::new_v4();
        let affected_approvals = vec![AffectedItem {
            node_id: affected_id,
            label: "Test".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Test".to_string(),
            external_ref: None,
        }];

        for risk_tier in [
            RiskTier::Low,
            RiskTier::Medium,
            RiskTier::High,
            RiskTier::Critical,
        ] {
            for decision_class in [
                DecisionClass::A,
                DecisionClass::B,
                DecisionClass::C,
                DecisionClass::D,
            ] {
                // Skip E since it always returns the same result
                let plan = create_plan_with_affected_approvals_and_risk(
                    decision_class,
                    affected_approvals.clone(),
                    risk_tier.clone(),
                );
                let current_approvals = vec![affected_id.to_string()];

                let result1 = classify_approvals(&plan, &current_approvals);
                let result2 = classify_approvals(&plan, &current_approvals);

                assert_eq!(
                    result1.strategy, result2.strategy,
                    "Strategy mismatch for {:?} + {:?}",
                    risk_tier, decision_class
                );
                assert_eq!(result1.stale_ids, result2.stale_ids);
                assert_eq!(
                    result1.requires_manual_review,
                    result2.requires_manual_review
                );
            }
        }
    }
}
