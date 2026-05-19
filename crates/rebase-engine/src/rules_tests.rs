use crate::diff::{
    AcceptanceCriteriaDiff, AcceptanceCriterionDiff, ActionRefDiff, ApprovalRuleDiff,
    AuthorityDiff, ChangeType, ConstraintDiff, ConstraintsDiff, IntentVersionDiff, ScopeDiff,
    ScopeItemsDiff,
};
use crate::risk::{ManualReviewReason, RiskConfig, Severity};
use crate::rules::{analyze_diff_risk, analyze_diff_risk_with_config, MatchingStats};
use intent_rebase_types::*;
use uuid::Uuid;

fn make_test_constraint(
    clause_id: Option<Uuid>,
    key: &str,
    priority: ClausePriority,
) -> Constraint {
    Constraint {
        clause_id,
        constraint_type: ClauseType::Functional,
        key: key.to_string(),
        operator: ConstraintOperator::Eq,
        value: serde_json::json!("test"),
        rationale: None,
        priority,
    }
}

fn empty_scope_diff() -> ScopeDiff {
    ScopeDiff {
        in_scope: ScopeItemsDiff {
            added: vec![],
            removed: vec![],
        },
        out_of_scope: ScopeItemsDiff {
            added: vec![],
            removed: vec![],
        },
    }
}

fn empty_constraints_diff() -> ConstraintsDiff {
    ConstraintsDiff {
        functional: vec![],
        non_functional: vec![],
        policy: vec![],
        budget: vec![],
        time: vec![],
    }
}

fn empty_ac_diff() -> AcceptanceCriteriaDiff {
    AcceptanceCriteriaDiff {
        required: vec![],
        optional: vec![],
    }
}

fn empty_authority_diff() -> AuthorityDiff {
    AuthorityDiff {
        allowed_actions: vec![],
        forbidden_actions: vec![],
        approval_requirements: vec![],
    }
}

fn empty_intent_version_diff() -> IntentVersionDiff {
    IntentVersionDiff {
        scope: empty_scope_diff(),
        constraints: empty_constraints_diff(),
        acceptance_criteria: empty_ac_diff(),
        authority: empty_authority_diff(),
    }
}

#[test]
fn test_no_changes_low_severity() {
    let diff = empty_intent_version_diff();
    let analysis = analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );

    assert_eq!(analysis.severity, Severity::Low);
    assert_eq!(analysis.confidence, 1.0);
    assert!(!analysis.manual_review);
    assert!(analysis.rationale.unwrap().contains("No semantic changes"));
}

#[test]
fn test_scope_add_medium_severity() {
    let mut scope_diff = empty_scope_diff();
    scope_diff.in_scope.added.push("new item".to_string());

    let analysis = analyze_diff_risk(
        &scope_diff,
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    assert_eq!(analysis.severity, Severity::Medium);
    assert!(analysis.confidence < 1.0); // Scope items don't have clause_ids
}

#[test]
fn test_out_of_scope_only_not_low_severity() {
    // out_of_scope-only changes should NOT produce Low severity
    // This was a bug where out_of_scope additions/removals were ignored
    let mut scope_diff = empty_scope_diff();
    scope_diff
        .out_of_scope
        .added
        .push("removed item".to_string());

    let analysis = analyze_diff_risk(
        &scope_diff,
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    // out_of_scope change should be Medium, not Low
    assert_eq!(analysis.severity, Severity::Medium);
    assert!(analysis.confidence < 1.0); // Scope items don't have clause_ids
}

#[test]
fn test_must_priority_constraint_high_severity() {
    let id = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "must_have_auth".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id),
            "must_have_auth",
            ClausePriority::Must,
        ))),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    assert_eq!(analysis.severity, Severity::High);
}

#[test]
fn test_policy_constraint_critical_severity() {
    let id = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.policy.push(ConstraintDiff {
        clause_id: Some(id),
        change_type: ChangeType::Modified,
        constraint_type: ClauseType::Policy,
        key: "security_policy".to_string(),
        before: Some(Box::new(make_test_constraint(
            Some(id),
            "security_policy",
            ClausePriority::Must,
        ))),
        after: Some(Box::new(make_test_constraint(
            Some(id),
            "security_policy",
            ClausePriority::Must,
        ))),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    assert_eq!(analysis.severity, Severity::Critical);
    assert!(analysis.manual_review);
}

#[test]
fn test_authority_changes_high_severity() {
    let mut authority_diff = empty_authority_diff();
    authority_diff.allowed_actions.push(ActionRefDiff {
        change_type: ChangeType::Added,
        action: "deploy".to_string(),
        target: None,
        before: None,
        after: Some(Box::new(ActionRef {
            action: "deploy".to_string(),
            target: None,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );

    assert_eq!(analysis.severity, Severity::High);
}

#[test]
fn test_forbidden_action_removal_critical() {
    let mut authority_diff = empty_authority_diff();
    authority_diff.forbidden_actions.push(ActionRefDiff {
        change_type: ChangeType::Removed,
        action: "delete_production".to_string(),
        target: None,
        before: Some(Box::new(ActionRef {
            action: "delete_production".to_string(),
            target: None,
        })),
        after: None,
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );

    assert_eq!(analysis.severity, Severity::Critical);
    assert!(analysis.manual_review);
}

#[test]
fn test_approval_requirement_removal_critical() {
    let mut authority_diff = empty_authority_diff();
    authority_diff.approval_requirements.push(ApprovalRuleDiff {
        change_type: ChangeType::Removed,
        rule_id: "security_review".to_string(),
        description: "Requires security team approval".to_string(),
        before: Some(Box::new(ApprovalRuleRef {
            rule_id: "security_review".to_string(),
            description: "Requires security team approval".to_string(),
        })),
        after: None,
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );

    assert_eq!(analysis.severity, Severity::Critical);
    assert!(analysis.manual_review);
    // Verify the specific reason for approval removal is present
    assert!(analysis
        .manual_review_reasons
        .contains(&ManualReviewReason::ApprovalRequirementRemoved));
}

#[test]
fn test_approval_requirement_removal_specific_reason() {
    // This test explicitly verifies that approval removal produces the specific
    // ManualReviewReason::ApprovalRequirementRemoved reason (not just CriticalSeverity)
    let mut authority_diff = empty_authority_diff();
    authority_diff.approval_requirements.push(ApprovalRuleDiff {
        change_type: ChangeType::Removed,
        rule_id: "security_review".to_string(),
        description: "Requires security team approval".to_string(),
        before: Some(Box::new(ApprovalRuleRef {
            rule_id: "security_review".to_string(),
            description: "Requires security team approval".to_string(),
        })),
        after: None,
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );

    // The specific ApprovalRequirementRemoved reason must be present
    assert!(analysis
        .manual_review_reasons
        .iter()
        .any(|r| matches!(r, ManualReviewReason::ApprovalRequirementRemoved)));

    // CriticalSeverity should also be present (since severity is Critical)
    assert!(analysis
        .manual_review_reasons
        .iter()
        .any(|r| matches!(r, ManualReviewReason::CriticalSeverity)));

    // Both reasons should be present
    assert_eq!(analysis.manual_review_reasons.len(), 2);
}

#[test]
fn test_approval_addition_not_critical() {
    // Adding approval requirements should NOT be Critical
    // Critical severity is reserved for REMOVING oversight, not adding it
    // Note: Authority changes in general are escalated to High by policy,
    // but additions should NOT reach Critical (which was the bug with no_match > 0)
    let mut authority_diff = empty_authority_diff();
    authority_diff.approval_requirements.push(ApprovalRuleDiff {
        change_type: ChangeType::Added,
        rule_id: "security_review".to_string(),
        description: "Requires security team approval".to_string(),
        before: None,
        after: Some(Box::new(ApprovalRuleRef {
            rule_id: "security_review".to_string(),
            description: "Requires security team approval".to_string(),
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );

    // Approval addition should be High (authority change escalation) but NOT Critical
    // The bug was that no_match > 0 incorrectly escalated additions to Critical
    assert_eq!(analysis.severity, Severity::High);
    assert!(!analysis.manual_review);
}

#[test]
fn test_confidence_with_clause_id() {
    let id = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    // Modified constraint with clause_id = unique match
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id),
        change_type: ChangeType::Modified,
        constraint_type: ClauseType::Functional,
        key: "test".to_string(),
        before: Some(Box::new(make_test_constraint(
            Some(id),
            "test",
            ClausePriority::Should,
        ))),
        after: Some(Box::new(make_test_constraint(
            Some(id),
            "test",
            ClausePriority::Should,
        ))),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    // With unique clause_id match, confidence should be high
    assert!(analysis.confidence >= 0.9);
}

#[test]
fn test_confidence_without_clause_id() {
    let mut constraints_diff = empty_constraints_diff();
    // Added constraint without clause_id = ambiguous
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: None,
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "test".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            None,
            "test",
            ClausePriority::Should,
        ))),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    // Without clause_id, confidence is reduced
    assert!(analysis.confidence < 0.9);
}

#[test]
fn test_confidence_pure_add_remove_not_zero() {
    // Pure add/remove changes (no_match) should NOT collapse to zero confidence
    // Even without clause_id matching, clear intent is indicated
    let mut constraints_diff = empty_constraints_diff();
    let id = Uuid::new_v4();
    // Pure add with a clause_id - this is a no_match because it's Added (not Modified)
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "test".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id),
            "test",
            ClausePriority::Should,
        ))),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    // Pure add with unique clause_id should have high confidence (~0.8), NOT zero
    // The formula: unique_ratio=0, ambiguous_ratio=0, no_match_ratio=0.8
    // confidence = 0 + 0 + 0.8 = 0.8
    assert!(
        analysis.confidence > 0.0,
        "confidence should not be zero for pure add/remove"
    );
    assert!(
        analysis.confidence >= 0.7,
        "confidence should be >= 0.7 for pure add/remove"
    );
}

#[test]
fn test_manual_review_triggers_low_confidence() {
    let mut constraints_diff = empty_constraints_diff();
    // Add many constraints without clause_ids to reduce confidence
    for i in 0..10 {
        constraints_diff.functional.push(ConstraintDiff {
            clause_id: None,
            change_type: ChangeType::Added,
            constraint_type: ClauseType::Functional,
            key: format!("test_{}", i),
            before: None,
            after: Some(Box::new(make_test_constraint(
                None,
                &format!("test_{}", i),
                ClausePriority::Should,
            ))),
        });
    }

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    // With 10 ambiguous matches out of 10, confidence should be below 0.7 threshold
    assert!(analysis.manual_review);
    assert!(analysis
        .manual_review_reasons
        .iter()
        .any(|r| matches!(r, ManualReviewReason::LowConfidence { .. })));
}

#[test]
fn test_matching_stats_add_assign() {
    let mut stats1 = MatchingStats {
        total_changes: 5,
        unique_clause_id_match: 3,
        ambiguous_match: 1,
        no_match: 1,
        removed: 0,
    };

    let stats2 = MatchingStats {
        total_changes: 3,
        unique_clause_id_match: 2,
        ambiguous_match: 1,
        no_match: 0,
        removed: 0,
    };

    stats1 += stats2;

    assert_eq!(stats1.total_changes, 8);
    assert_eq!(stats1.unique_clause_id_match, 5);
    assert_eq!(stats1.ambiguous_match, 2);
    assert_eq!(stats1.no_match, 1);
    assert_eq!(stats1.removed, 0);
}

#[test]
fn test_deterministic_output() {
    // Running the same diff through analysis twice should produce identical results
    let id = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "test".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id),
            "test",
            ClausePriority::Must,
        ))),
    });

    let analysis1 = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );
    let analysis2 = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &empty_authority_diff(),
    );

    assert_eq!(analysis1.severity, analysis2.severity);
    assert_eq!(analysis1.confidence, analysis2.confidence);
    assert_eq!(analysis1.manual_review, analysis2.manual_review);
}

#[test]
fn test_rationale_generation() {
    // No changes
    let diff = empty_intent_version_diff();
    let analysis = analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );
    assert!(analysis.rationale.unwrap().contains("No semantic changes"));

    // High severity changes
    let mut authority_diff = empty_authority_diff();
    authority_diff.allowed_actions.push(ActionRefDiff {
        change_type: ChangeType::Added,
        action: "deploy".to_string(),
        target: None,
        before: None,
        after: Some(Box::new(ActionRef {
            action: "deploy".to_string(),
            target: None,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &empty_ac_diff(),
        &authority_diff,
    );
    assert!(analysis.rationale.unwrap().contains("high severity"));
}

// === Acceptance Criteria Risk Tests ===

#[test]
fn test_acceptance_criterion_should_to_must_modified_high_severity() {
    // Transition from Should to Must via modification should be High severity
    // The AFTER state is Must (now a mandatory requirement)
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Modified,
        priority: "Should".to_string(), // BEFORE state
        before: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Should monitor".to_string(),
            priority: ClausePriority::Should,
        })),
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must monitor".to_string(),
            priority: ClausePriority::Must, // AFTER state is Must
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Should->Must upgrade should be High severity (the new requirement is Must)
    assert_eq!(analysis.severity, Severity::High);
    // High severity alone does NOT trigger manual review - need multiple high severity
    // sections, low confidence, or other triggers. Single High section = no manual review.
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_must_to_should_modified_medium_severity() {
    // Transition from Must to Should via modification should be Medium (not High)
    // The AFTER state is Should (no longer a mandatory requirement)
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Modified,
        priority: "Must".to_string(), // BEFORE state
        before: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must monitor".to_string(),
            priority: ClausePriority::Must, // BEFORE state
        })),
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Should monitor".to_string(),
            priority: ClausePriority::Should, // AFTER state is Should
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Must->Should downgrade should be Medium (the new requirement is Should)
    assert_eq!(analysis.severity, Severity::Medium);
    // Medium severity should not trigger manual review by itself
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_added_as_must_high_severity() {
    // Adding a Must criterion should be High severity
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must have monitoring".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Adding Must criterion should be High
    assert_eq!(analysis.severity, Severity::High);
    // High severity alone does NOT trigger manual review - need multiple high severity
    // sections, low confidence, or other triggers. Single High section = no manual review.
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_added_as_should_medium_severity() {
    // Adding a Should criterion should be Medium severity
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        priority: "Should".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Should have monitoring".to_string(),
            priority: ClausePriority::Should,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Adding Should criterion should be Medium
    assert_eq!(analysis.severity, Severity::Medium);
    // Medium alone should not trigger manual review
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_removed_must_medium_severity() {
    // Removing a Must criterion should be Medium (not high, since we're not adding a Must)
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Removed,
        priority: "Must".to_string(),
        before: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must have monitoring".to_string(),
            priority: ClausePriority::Must,
        })),
        after: None,
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Removing Must criterion is Medium severity
    assert_eq!(analysis.severity, Severity::Medium);
}

#[test]
fn test_acceptance_criterion_optional_must_added_high_severity() {
    // Adding a Must criterion to optional section should still be High
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.optional.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must track metrics".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Adding Must criterion (even to optional) should be High
    assert_eq!(analysis.severity, Severity::High);
    // Single High section does NOT trigger manual review by itself
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_multiple_changes_high_manual_review() {
    // Multiple high-severity AC changes should trigger manual review
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();

    // First Must AC added
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id1),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id1),
            description: "Must have auth".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    // Second Must AC added
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id2),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id2),
            description: "Must have logging".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Multiple Must additions should be High severity
    // But 2 high-severity sections doesn't trigger manual_review (need >= 3)
    assert_eq!(analysis.severity, Severity::High);
    assert!(!analysis.manual_review);
}

#[test]
fn test_acceptance_criterion_section_risk_tracked() {
    // Verify that acceptance_criteria section risk is properly tracked in section_risks
    let id = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id),
            description: "Must verify".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &empty_constraints_diff(),
        &ac_diff,
        &empty_authority_diff(),
    );

    // Check that acceptance_criteria is in section_risks with correct severity
    let ac_section = analysis
        .section_risks
        .iter()
        .find(|r| r.section == "acceptance_criteria");
    assert!(ac_section.is_some());
    let ac_section = ac_section.unwrap();
    assert_eq!(ac_section.severity, Severity::High);
    assert_eq!(ac_section.change_count, 1);
}

// === MultipleHighSeverityChanges Boundary Tests ===

#[test]
fn test_multiple_high_severity_changes_at_threshold_triggers_review() {
    // At exactly 3 high-severity sections (threshold = 3), MultipleHighSeverityChanges should trigger.
    // Create High severity in: authority (any change), constraints (Must), acceptance_criteria (Must)

    // 1. Authority change -> High severity
    let mut authority_diff = empty_authority_diff();
    authority_diff.allowed_actions.push(ActionRefDiff {
        change_type: ChangeType::Added,
        action: "deploy".to_string(),
        target: None,
        before: None,
        after: Some(Box::new(ActionRef {
            action: "deploy".to_string(),
            target: None,
        })),
    });

    // 2. Must constraint -> High severity
    let id1 = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id1),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "must_have_auth".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id1),
            "must_have_auth",
            ClausePriority::Must,
        ))),
    });

    // 3. Must acceptance criterion -> High severity
    let id2 = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id2),
        change_type: ChangeType::Added,
        priority: "Must".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id2),
            description: "Must have auth".to_string(),
            priority: ClausePriority::Must,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &ac_diff,
        &authority_diff,
    );

    // Verify we have 3 high-severity sections
    let high_section_count = analysis
        .section_risks
        .iter()
        .filter(|r| r.severity == Severity::High || r.severity == Severity::Critical)
        .count();
    assert_eq!(
        high_section_count, 3,
        "Should have exactly 3 high-severity sections"
    );

    // At threshold (3), MultipleHighSeverityChanges SHOULD be triggered (>= threshold)
    assert!(analysis.manual_review);
    assert!(analysis.manual_review_reasons.iter().any(
        |r| matches!(r, ManualReviewReason::MultipleHighSeverityChanges { count } if *count == 3)
    ));
}

#[test]
fn test_multiple_high_severity_changes_below_threshold_no_review() {
    // Below threshold (2 high-severity sections, threshold = 3), MultipleHighSeverityChanges should NOT trigger.
    // Create High severity in only 2 sections: authority and constraints (but not acceptance_criteria)

    // 1. Authority change -> High severity
    let mut authority_diff = empty_authority_diff();
    authority_diff.allowed_actions.push(ActionRefDiff {
        change_type: ChangeType::Added,
        action: "deploy".to_string(),
        target: None,
        before: None,
        after: Some(Box::new(ActionRef {
            action: "deploy".to_string(),
            target: None,
        })),
    });

    // 2. Must constraint -> High severity (but no Must AC, so acceptance_criteria is Medium/Low)
    let id1 = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id1),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "must_have_auth".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id1),
            "must_have_auth",
            ClausePriority::Must,
        ))),
    });

    // 3. Only Should AC -> Medium severity, not High
    let id2 = Uuid::new_v4();
    let mut ac_diff = empty_ac_diff();
    ac_diff.required.push(AcceptanceCriterionDiff {
        clause_id: Some(id2),
        change_type: ChangeType::Added,
        priority: "Should".to_string(),
        before: None,
        after: Some(Box::new(AcceptanceCriterion {
            clause_id: Some(id2),
            description: "Should have auth".to_string(),
            priority: ClausePriority::Should,
        })),
    });

    let analysis = analyze_diff_risk(
        &empty_scope_diff(),
        &constraints_diff,
        &ac_diff,
        &authority_diff,
    );

    // Verify we have exactly 2 high-severity sections
    let high_section_count = analysis
        .section_risks
        .iter()
        .filter(|r| r.severity == Severity::High || r.severity == Severity::Critical)
        .count();
    assert_eq!(
        high_section_count, 2,
        "Should have exactly 2 high-severity sections"
    );

    // Below threshold (2 < 3), MultipleHighSeverityChanges should NOT trigger
    // Note: manual_review might still be true if other conditions apply (e.g., High severity alone doesn't trigger)
    // But MultipleHighSeverityChanges should NOT be in the reasons
    assert!(!analysis
        .manual_review_reasons
        .iter()
        .any(|r| matches!(r, ManualReviewReason::MultipleHighSeverityChanges { .. })));
}

#[test]
fn test_multiple_high_severity_changes_with_custom_threshold() {
    // Test that the threshold is configurable via RiskConfig
    // With threshold = 2, 2 high-severity sections SHOULD trigger MultipleHighSeverityChanges

    let config = RiskConfig {
        confidence_threshold: 0.7,
        max_high_severity_before_manual_review: 2, // Lower threshold
    };

    // Create exactly 2 high-severity sections: authority and constraints
    let mut authority_diff = empty_authority_diff();
    authority_diff.allowed_actions.push(ActionRefDiff {
        change_type: ChangeType::Added,
        action: "deploy".to_string(),
        target: None,
        before: None,
        after: Some(Box::new(ActionRef {
            action: "deploy".to_string(),
            target: None,
        })),
    });

    let id1 = Uuid::new_v4();
    let mut constraints_diff = empty_constraints_diff();
    constraints_diff.functional.push(ConstraintDiff {
        clause_id: Some(id1),
        change_type: ChangeType::Added,
        constraint_type: ClauseType::Functional,
        key: "must_have_auth".to_string(),
        before: None,
        after: Some(Box::new(make_test_constraint(
            Some(id1),
            "must_have_auth",
            ClausePriority::Must,
        ))),
    });

    let analysis = analyze_diff_risk_with_config(
        &empty_scope_diff(),
        &constraints_diff,
        &empty_ac_diff(),
        &authority_diff,
        &config,
    );

    // With threshold = 2, 2 high-severity sections >= 2 should trigger
    assert!(analysis.manual_review_reasons.iter().any(
        |r| matches!(r, ManualReviewReason::MultipleHighSeverityChanges { count } if *count == 2)
    ));
}
