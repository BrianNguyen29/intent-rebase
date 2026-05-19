use crate::diff::{
    AcceptanceCriteriaDiff, AuthorityDiff, ConstraintsDiff, IntentVersionDiff, ScopeDiff,
    ScopeItemsDiff,
};
use crate::planner::*;
use crate::risk::{DiffRiskAnalysis, SectionRisk, Severity};

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

// === Decision Class A Tests ===

#[test]
fn test_class_a_no_changes() {
    let diff = empty_intent_version_diff();
    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 1.0,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![],
        rationale: Some("No semantic changes detected".to_string()),
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::A);
    assert_eq!(plan.risk_level, 1);
    assert_eq!(plan.risk_tier, RiskTier::Low);
    assert!(!plan.manual_review_recommended);
}

#[test]
fn test_class_a_description() {
    assert_eq!(
        DecisionClass::A.description(),
        "No semantic changes — no rebase needed"
    );
}

// === Decision Class B Tests ===

#[test]
fn test_class_b_low_severity_no_review() {
    let mut diff = empty_intent_version_diff();
    diff.scope
        .in_scope
        .added
        .push("clarification_item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Low,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: Some("Minor clarification changes".to_string()),
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::B);
    assert!(plan.manual_review_recommended); // Low severity but no critical/high
}

#[test]
fn test_class_b_low_confidence() {
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("ambiguous_item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 0.4, // Below 0.7 threshold
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::LowConfidence {
            confidence: 0.4,
            threshold: 0.7,
        }],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Low,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::B);
    assert!(plan.manual_review_recommended);
}

// === Decision Class C Tests ===

#[test]
fn test_class_c_high_severity_single_section_no_manual_review() {
    let diff = empty_intent_version_diff();
    let mut diff = diff;
    diff.scope.in_scope.added.push("new_item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: Some("Scope addition".to_string()),
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::C);
    assert!(plan.manual_review_recommended); // High severity always recommends review
}

#[test]
fn test_class_c_medium_severity_two_sections() {
    let diff = empty_intent_version_diff();
    let mut diff = diff;
    diff.scope.in_scope.added.push("item".to_string());
    diff.acceptance_criteria
        .required
        .push(crate::diff::AcceptanceCriterionDiff {
            clause_id: None,
            change_type: crate::diff::ChangeType::Added,
            priority: "Should".to_string(),
            before: None,
            after: None,
        });

    let risk = DiffRiskAnalysis {
        severity: Severity::Medium,
        confidence: 0.8,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![
            SectionRisk {
                section: "scope".to_string(),
                severity: Severity::Medium,
                change_count: 1,
                high_priority_changes: 0,
            },
            SectionRisk {
                section: "acceptance_criteria".to_string(),
                severity: Severity::Medium,
                change_count: 1,
                high_priority_changes: 0,
            },
        ],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::C);
}

// === Decision Class D Tests ===

#[test]
fn test_class_d_high_severity_with_manual_review() {
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("high_risk_item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.8,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        }],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: Some("Authority changes detected".to_string()),
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::D);
    assert!(plan.manual_review_recommended);
    assert_eq!(plan.risk_level, 4);
}

#[test]
fn test_class_d_high_severity_two_high_sections_no_manual_review() {
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item1".to_string());
    diff.scope.in_scope.added.push("item2".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![
            SectionRisk {
                section: "scope".to_string(),
                severity: Severity::High,
                change_count: 2,
                high_priority_changes: 0,
            },
            SectionRisk {
                section: "constraints".to_string(),
                severity: Severity::High,
                change_count: 1,
                high_priority_changes: 1,
            },
        ],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::D);
}

// === Decision Class E Tests ===

#[test]
fn test_class_e_critical_severity() {
    let mut diff = empty_intent_version_diff();
    diff.authority
        .forbidden_actions
        .push(crate::diff::ActionRefDiff {
            change_type: crate::diff::ChangeType::Removed,
            action: "delete_production".to_string(),
            target: None,
            before: None,
            after: None,
        });

    let risk = DiffRiskAnalysis {
        severity: Severity::Critical,
        confidence: 0.9,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::CriticalSeverity],
        section_risks: vec![SectionRisk {
            section: "authority".to_string(),
            severity: Severity::Critical,
            change_count: 1,
            high_priority_changes: 1,
        }],
        rationale: Some("Critical authority change".to_string()),
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::E);
    assert_eq!(plan.risk_level, 5);
}

#[test]
fn test_class_e_three_high_severity_sections() {
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item1".to_string());
    diff.scope.out_of_scope.added.push("item2".to_string());
    diff.acceptance_criteria
        .required
        .push(crate::diff::AcceptanceCriterionDiff {
            clause_id: None,
            change_type: crate::diff::ChangeType::Added,
            priority: "Must".to_string(),
            before: None,
            after: None,
        });

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.7,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![
            SectionRisk {
                section: "scope".to_string(),
                severity: Severity::High,
                change_count: 2,
                high_priority_changes: 0,
            },
            SectionRisk {
                section: "constraints".to_string(),
                severity: Severity::High,
                change_count: 1,
                high_priority_changes: 1,
            },
            SectionRisk {
                section: "acceptance_criteria".to_string(),
                severity: Severity::High,
                change_count: 1,
                high_priority_changes: 0,
            },
        ],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::E);
}

// === Determinism Tests ===

#[test]
fn test_deterministic_class_mapping() {
    // Same input must produce same output
    let diff = empty_intent_version_diff();
    let mut diff = diff;
    diff.scope.in_scope.added.push("item1".to_string());
    diff.scope.in_scope.added.push("item2".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::Medium,
        confidence: 0.8,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Medium,
            change_count: 2,
            high_priority_changes: 0,
        }],
        rationale: None,
    };

    let plan1 = RebasePlan::from_diff_and_risk(&diff, &risk);
    let plan2 = RebasePlan::from_diff_and_risk(&diff, &risk);

    assert_eq!(plan1.decision_class, plan2.decision_class);
    assert_eq!(plan1.risk_level, plan2.risk_level);
    assert_eq!(plan1.rationale, plan2.rationale);
}

#[test]
fn test_section_decisions_populated() {
    let diff = empty_intent_version_diff();
    let mut diff = diff;
    diff.scope.in_scope.added.push("new_scope".to_string());
    diff.constraints
        .functional
        .push(crate::diff::ConstraintDiff {
            clause_id: None,
            change_type: crate::diff::ChangeType::Added,
            constraint_type: intent_rebase_types::ClauseType::Functional,
            key: "new_constraint".to_string(),
            before: None,
            after: None,
        });

    let risk = DiffRiskAnalysis {
        severity: Severity::Medium,
        confidence: 0.8,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![
            SectionRisk {
                section: "scope".to_string(),
                severity: Severity::Medium,
                change_count: 1,
                high_priority_changes: 0,
            },
            SectionRisk {
                section: "constraints".to_string(),
                severity: Severity::Medium,
                change_count: 1,
                high_priority_changes: 0,
            },
        ],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

    assert_eq!(plan.section_decisions.len(), 2);
    let sections: Vec<&str> = plan
        .section_decisions
        .iter()
        .map(|s| s.section.as_str())
        .collect();
    assert!(sections.contains(&"scope"));
    assert!(sections.contains(&"constraints"));
}

#[test]
fn test_deferred_fields_typed_groundwork() {
    let diff = empty_intent_version_diff();
    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 1.0,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

    // Phase 1: ready=false for all deferred fields (PR #18: heuristic populates
    // candidates/selected/rationale for checkpoint_selection but ready stays false)
    assert!(
        !plan.deferred.checkpoint_selection.ready,
        "Checkpoint selection should be deferred in Phase 1"
    );
    assert!(
        !plan.deferred.approval_revalidation.ready,
        "Approval revalidation should be deferred in Phase 1"
    );
    assert!(
        !plan.deferred.compensation.ready,
        "Compensation planning should be deferred in Phase 1"
    );

    // candidates/approvals_needing_revalidation/potential_actions should be empty
    // (PR #18: for Class A, candidates is still empty because no checkpoint needed)
    assert!(plan.deferred.checkpoint_selection.candidates.is_empty());
    assert!(plan
        .deferred
        .approval_revalidation
        .approvals_needing_revalidation
        .is_empty());
    assert!(plan.deferred.compensation.potential_actions.is_empty());

    // Selected should be None for Class A (no checkpoint needed)
    assert!(plan.deferred.checkpoint_selection.selected.is_none());
    // Rationale is now populated even for empty state (explains no items affected)
    // PR #18: heuristic provides rationale for approval/compensation too
    assert!(plan.deferred.checkpoint_selection.rationale.is_some());
    // Approval/compensation rationales are now populated (explain empty state)
    assert!(plan.deferred.approval_revalidation.rationale.is_some());
    assert!(plan.deferred.compensation.rationale.is_some());
}

#[test]
fn test_checkpoint_selection_deferred() {
    let cs = CheckpointSelection::deferred();
    assert!(!cs.ready);
    assert!(cs.candidates.is_empty());
    assert!(cs.selected.is_none());
    assert!(cs.rationale.is_none());
}

#[test]
fn test_approval_revalidation_deferred() {
    let ar = ApprovalRevalidation::deferred();
    assert!(!ar.ready);
    assert!(ar.approvals_needing_revalidation.is_empty());
    assert_eq!(ar.strategy, RevalidationStrategy::Deferred);
    assert!(ar.rationale.is_none());
}

#[test]
fn test_compensation_readiness_deferred() {
    let cr = CompensationReadiness::deferred();
    assert!(!cr.ready);
    assert!(cr.potential_actions.is_empty());
    assert!(!cr.has_irreversible_effects);
    assert!(cr.rationale.is_none());
}

// === Risk Level Tests ===

#[test]
fn test_risk_levels_increase_with_class() {
    // Class A → risk level 1 (empty diff = no-op)
    let diff_a = empty_intent_version_diff();
    let risk_a = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 1.0,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![],
        rationale: None,
    };
    let plan_a = RebasePlan::from_diff_and_risk(&diff_a, &risk_a);
    assert_eq!(plan_a.decision_class, DecisionClass::A);
    assert_eq!(plan_a.risk_level, 1);

    // Class B → risk level 2 (low severity, single section, good confidence)
    let mut diff_b = empty_intent_version_diff();
    diff_b.scope.in_scope.added.push("item".to_string());
    let risk_b = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Low,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_b = RebasePlan::from_diff_and_risk(&diff_b, &risk_b);
    assert_eq!(plan_b.decision_class, DecisionClass::B);
    assert_eq!(plan_b.risk_level, 2);

    // Class C → risk level 3 (high severity, single section, no manual review)
    let mut diff_c = empty_intent_version_diff();
    diff_c.scope.in_scope.added.push("new_item".to_string());
    let risk_c = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_c = RebasePlan::from_diff_and_risk(&diff_c, &risk_c);
    assert_eq!(plan_c.decision_class, DecisionClass::C);
    assert_eq!(plan_c.risk_level, 3);

    // Class D → risk level 4
    let mut diff_d = empty_intent_version_diff();
    diff_d
        .scope
        .in_scope
        .added
        .push("high_risk_item".to_string());
    let risk_d = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.8,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        }],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_d = RebasePlan::from_diff_and_risk(&diff_d, &risk_d);
    assert_eq!(plan_d.decision_class, DecisionClass::D);
    assert_eq!(plan_d.risk_level, 4);

    // Class E → risk level 5
    let mut diff_e = empty_intent_version_diff();
    diff_e
        .authority
        .forbidden_actions
        .push(crate::diff::ActionRefDiff {
            change_type: crate::diff::ChangeType::Removed,
            action: "delete_production".to_string(),
            target: None,
            before: None,
            after: None,
        });
    let risk_e = DiffRiskAnalysis {
        severity: Severity::Critical,
        confidence: 0.9,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::CriticalSeverity],
        section_risks: vec![SectionRisk {
            section: "authority".to_string(),
            severity: Severity::Critical,
            change_count: 1,
            high_priority_changes: 1,
        }],
        rationale: None,
    };
    let plan_e = RebasePlan::from_diff_and_risk(&diff_e, &risk_e);
    assert_eq!(plan_e.decision_class, DecisionClass::E);
    assert_eq!(plan_e.risk_level, 5);
}

// === Integration Tests (real engine/risk path) ===

#[test]
fn test_integration_medium_severity_through_real_risk_path() {
    // This test goes through the real analyze_diff_risk function
    // to ensure the planner correctly handles risk analysis output.
    // Previously there was a bug where Medium severity paths used
    // all section_risks entries instead of only changed sections.

    let mut diff = empty_intent_version_diff();
    // Add scope change (ambiguous - no clause_id, contributes 0.5 confidence)
    diff.scope.in_scope.added.push("new_scope_item".to_string());
    // Add constraint with clause_id (unique match, contributes 0.8 confidence)
    // 3 such constraints push overall confidence above 0.7 threshold
    // to avoid manual_review triggering (which would independently cause D)
    use uuid::Uuid;
    for i in 0..3 {
        diff.constraints
            .functional
            .push(crate::diff::ConstraintDiff {
                clause_id: Some(Uuid::new_v4()),
                change_type: crate::diff::ChangeType::Added,
                constraint_type: intent_rebase_types::ClauseType::Functional,
                key: format!("constraint_{}", i),
                before: None,
                after: Some(Box::new(intent_rebase_types::Constraint {
                    clause_id: Some(Uuid::new_v4()),
                    constraint_type: intent_rebase_types::ClauseType::Functional,
                    key: format!("constraint_{}", i),
                    operator: intent_rebase_types::ConstraintOperator::Eq,
                    value: serde_json::json!("value"),
                    rationale: None,
                    priority: intent_rebase_types::ClausePriority::Should,
                })),
            });
    }

    // Use the REAL analyze_diff_risk function from rules module
    let risk = crate::rules::analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );

    // With 3 high-confidence constraint changes (unique matches),
    // overall confidence should be high enough (0.8) to avoid manual_review
    assert_eq!(risk.severity, Severity::Medium);
    assert!(
        !risk.manual_review,
        "Expected high confidence to avoid manual_review"
    );

    // Verify we have exactly 2 changed sections (scope + constraints)
    // Note: acceptance_criteria and authority have no changes
    let actual_changed_sections = risk
        .section_risks
        .iter()
        .filter(|s| s.change_count > 0)
        .count();
    assert_eq!(
        actual_changed_sections, 2,
        "Should only have scope and constraints with changes"
    );

    // Now create the rebase plan through the planner
    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

    // With 2 changed sections, Medium severity, and NO manual_review trigger,
    // should be Class C (not Class D)
    // Class D would be incorrectly triggered if we counted section_risks entries
    // with change_count == 0 (original bug)
    assert_eq!(plan.decision_class, DecisionClass::C);
    assert_eq!(plan.risk_level, 3);
}

#[test]
fn test_integration_class_d_with_three_changed_sections() {
    // Test that 3+ changed sections with Medium severity triggers Class D

    let mut diff = empty_intent_version_diff();
    // Change 1: scope
    diff.scope.in_scope.added.push("scope_item".to_string());
    // Change 2: constraints (functional)
    diff.constraints
        .functional
        .push(crate::diff::ConstraintDiff {
            clause_id: None,
            change_type: crate::diff::ChangeType::Added,
            constraint_type: intent_rebase_types::ClauseType::Functional,
            key: "new_constraint".to_string(),
            before: None,
            after: None,
        });
    // Change 3: acceptance criteria
    diff.acceptance_criteria
        .required
        .push(crate::diff::AcceptanceCriterionDiff {
            clause_id: None,
            change_type: crate::diff::ChangeType::Added,
            priority: "Should".to_string(),
            before: None,
            after: None,
        });

    // Use real risk analysis
    let risk = crate::rules::analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );

    // Verify Medium severity with 3 changed sections
    assert_eq!(risk.severity, Severity::Medium);
    let actual_changed_sections = risk
        .section_risks
        .iter()
        .filter(|s| s.change_count > 0)
        .count();
    assert_eq!(actual_changed_sections, 3);

    // Create plan
    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

    // With 3 changed sections and Medium severity, should be Class D
    assert_eq!(plan.decision_class, DecisionClass::D);
    assert_eq!(plan.risk_level, 4);
}

// === Checkpoint Selection Heuristic Tests (PR #18) ===

#[test]
fn test_checkpoint_heuristic_class_a_no_candidates() {
    // Class A: no semantic changes → no checkpoint candidates
    let diff = empty_intent_version_diff();
    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 1.0,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![],
        rationale: None,
    };
    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::A);

    // Class A should have empty candidates and None selected
    assert!(plan.deferred.checkpoint_selection.candidates.is_empty());
    assert!(plan.deferred.checkpoint_selection.selected.is_none());
    // Rationale is still populated (even for Class A, it explains why no checkpoint)
    assert!(plan.deferred.checkpoint_selection.rationale.is_some());
    // But ready remains false
    assert!(!plan.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_class_b_selects_most_recent() {
    // Class B: should select "most recent" checkpoint
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 0.8,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Low,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::B);

    // Class B should have candidates
    assert!(!plan.deferred.checkpoint_selection.candidates.is_empty());
    // Should select most recent
    assert!(plan.deferred.checkpoint_selection.selected.is_some());
    let selected = plan
        .deferred
        .checkpoint_selection
        .selected
        .as_ref()
        .unwrap();
    assert_eq!(selected.id, "checkpoint-most-recent");
    // Rationale should be populated
    assert!(plan.deferred.checkpoint_selection.rationale.is_some());
    // But ready remains false (Phase 2 feature)
    assert!(!plan.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_class_c_selects_minimal() {
    // Class C: should select "minimal" checkpoint if available
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::C);

    // Class C should have candidates including "minimal"
    assert!(!plan.deferred.checkpoint_selection.candidates.is_empty());
    let has_minimal = plan
        .deferred
        .checkpoint_selection
        .candidates
        .iter()
        .any(|c| c.id == "checkpoint-minimal");
    assert!(
        has_minimal,
        "Class C should have minimal checkpoint candidate"
    );

    // Should select minimal
    assert!(plan.deferred.checkpoint_selection.selected.is_some());
    let selected = plan
        .deferred
        .checkpoint_selection
        .selected
        .as_ref()
        .unwrap();
    assert_eq!(selected.id, "checkpoint-minimal");
    // ready remains false
    assert!(!plan.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_class_d_selects_before_side_effects() {
    // Class D: should select before-side-effects checkpoint
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item".to_string());

    let risk = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.8,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        }],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::D);

    // Class D should have candidates including "before-side-effects"
    assert!(!plan.deferred.checkpoint_selection.candidates.is_empty());
    let has_side_effects = plan
        .deferred
        .checkpoint_selection
        .candidates
        .iter()
        .any(|c| c.id == "checkpoint-before-side-effects");
    assert!(
        has_side_effects,
        "Class D should have before-side-effects candidate"
    );

    // Should select before-side-effects
    assert!(plan.deferred.checkpoint_selection.selected.is_some());
    let selected = plan
        .deferred
        .checkpoint_selection
        .selected
        .as_ref()
        .unwrap();
    assert_eq!(selected.id, "checkpoint-before-side-effects");
    // ready remains false
    assert!(!plan.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_class_e_selects_last_known_good() {
    // Class E: should select "last-known-good" checkpoint
    let mut diff = empty_intent_version_diff();
    diff.authority
        .forbidden_actions
        .push(crate::diff::ActionRefDiff {
            change_type: crate::diff::ChangeType::Removed,
            action: "delete_production".to_string(),
            target: None,
            before: None,
            after: None,
        });

    let risk = DiffRiskAnalysis {
        severity: Severity::Critical,
        confidence: 0.9,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::CriticalSeverity],
        section_risks: vec![SectionRisk {
            section: "authority".to_string(),
            severity: Severity::Critical,
            change_count: 1,
            high_priority_changes: 1,
        }],
        rationale: None,
    };

    let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
    assert_eq!(plan.decision_class, DecisionClass::E);

    // Class E should have candidates including "last-known-good"
    assert!(!plan.deferred.checkpoint_selection.candidates.is_empty());
    let has_last_good = plan
        .deferred
        .checkpoint_selection
        .candidates
        .iter()
        .any(|c| c.id == "checkpoint-last-known-good");
    assert!(
        has_last_good,
        "Class E should have last-known-good candidate"
    );

    // Should select last-known-good
    assert!(plan.deferred.checkpoint_selection.selected.is_some());
    let selected = plan
        .deferred
        .checkpoint_selection
        .selected
        .as_ref()
        .unwrap();
    assert_eq!(selected.id, "checkpoint-last-known-good");
    // ready remains false
    assert!(!plan.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_ready_always_false() {
    // Verify that ready is always false regardless of decision class
    // This is a key invariant: checkpoint selection is heuristic-only in Phase 1
    let mut diff = empty_intent_version_diff();
    diff.scope.in_scope.added.push("item".to_string());

    // Test Class B
    let risk_b = DiffRiskAnalysis {
        severity: Severity::Low,
        confidence: 0.8,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::Low,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_b = RebasePlan::from_diff_and_risk(&diff, &risk_b);
    assert!(!plan_b.deferred.checkpoint_selection.ready);

    // Test Class C
    let risk_c = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.9,
        manual_review: false,
        manual_review_reasons: vec![],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_c = RebasePlan::from_diff_and_risk(&diff, &risk_c);
    assert!(!plan_c.deferred.checkpoint_selection.ready);

    // Test Class D
    let risk_d = DiffRiskAnalysis {
        severity: Severity::High,
        confidence: 0.8,
        manual_review: true,
        manual_review_reasons: vec![crate::risk::ManualReviewReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        }],
        section_risks: vec![SectionRisk {
            section: "scope".to_string(),
            severity: Severity::High,
            change_count: 1,
            high_priority_changes: 0,
        }],
        rationale: None,
    };
    let plan_d = RebasePlan::from_diff_and_risk(&diff, &risk_d);
    assert!(!plan_d.deferred.checkpoint_selection.ready);
}

#[test]
fn test_checkpoint_heuristic_with_affected_items() {
    // Test that affected items influence candidate generation
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let artifacts = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Artifact".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(artifacts, vec![], vec![]);

    // Class B with affected items should have more candidates
    let selection = CheckpointSelection::with_heuristic(DecisionClass::B, &affected_items);

    assert!(!selection.candidates.is_empty());
    // Should have "before-invalidation" candidate when affected items exist
    let has_before_invalidation = selection
        .candidates
        .iter()
        .any(|c| c.id == "checkpoint-before-invalidation");
    assert!(has_before_invalidation);
    // Should have selected something
    assert!(selection.selected.is_some());
    // Rationale should mention affected items
    let rationale = selection.rationale.as_ref().unwrap();
    assert!(rationale.contains("1 affected item"));
}

// === ApprovalRevalidation Heuristic Tests ===

#[test]
fn test_approval_revalidation_heuristic_empty_approvals() {
    // No affected approvals should result in empty list and Drop strategy
    let affected_items = AffectedItemsPreview::unavailable();

    let revalidation =
        ApprovalRevalidation::with_affected_approvals(DecisionClass::A, &affected_items);

    assert!(!revalidation.ready);
    assert!(revalidation.approvals_needing_revalidation.is_empty());
    assert_eq!(revalidation.strategy, RevalidationStrategy::Drop);
    assert!(revalidation.rationale.is_some());
    assert!(revalidation
        .rationale
        .as_ref()
        .unwrap()
        .contains("No affected approvals"));
}

#[test]
fn test_approval_revalidation_heuristic_with_affected_approvals() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let approvals = vec![
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "Review Approval".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Directly affected by scope change".to_string(),
            external_ref: None,
        },
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "QA Approval".to_string(),
            impact: ClassificationImpact::Transitive,
            reason: "Transitively affected".to_string(),
            external_ref: None,
        },
    ];

    let affected_items = AffectedItemsPreview::from_classification(vec![], approvals, vec![]);

    let revalidation =
        ApprovalRevalidation::with_affected_approvals(DecisionClass::D, &affected_items);

    assert!(!revalidation.ready);
    assert_eq!(revalidation.approvals_needing_revalidation.len(), 2);
    assert_eq!(revalidation.strategy, RevalidationStrategy::Full);
    assert!(revalidation.rationale.is_some());
    let rationale = revalidation.rationale.as_ref().unwrap();
    assert!(rationale.contains("2 approval(s) need revalidation"));
    assert!(rationale.contains("class D"));
}

#[test]
fn test_approval_revalidation_heuristic_class_b_drops() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let approvals = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Approval".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Affected".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(vec![], approvals, vec![]);

    let revalidation =
        ApprovalRevalidation::with_affected_approvals(DecisionClass::B, &affected_items);

    assert!(!revalidation.ready);
    assert_eq!(revalidation.strategy, RevalidationStrategy::Drop);
}

#[test]
fn test_approval_revalidation_heuristic_class_c_incremental() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let approvals = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Approval".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Affected".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(vec![], approvals, vec![]);

    let revalidation =
        ApprovalRevalidation::with_affected_approvals(DecisionClass::C, &affected_items);

    assert!(!revalidation.ready);
    assert_eq!(revalidation.strategy, RevalidationStrategy::Incremental);
}

#[test]
fn test_approval_revalidation_heuristic_ready_always_false() {
    // Key invariant: ready is always false regardless of decision class or affected approvals
    let affected_items = AffectedItemsPreview::unavailable();

    for class in &[
        DecisionClass::A,
        DecisionClass::B,
        DecisionClass::C,
        DecisionClass::D,
        DecisionClass::E,
    ] {
        let revalidation = ApprovalRevalidation::with_affected_approvals(*class, &affected_items);
        assert!(
            !revalidation.ready,
            "ApprovalRevalidation::ready should be false for class {:?}",
            class
        );
    }
}

// === CompensationReadiness Heuristic Tests ===

#[test]
fn test_compensation_readiness_heuristic_empty_side_effects() {
    // No side effects should result in empty actions and has_irreversible_effects=false
    let affected_items = AffectedItemsPreview::unavailable();

    let compensation = CompensationReadiness::with_side_effects(DecisionClass::A, &affected_items);

    assert!(!compensation.ready);
    assert!(compensation.potential_actions.is_empty());
    assert!(!compensation.has_irreversible_effects);
    assert!(compensation.rationale.is_some());
    assert!(compensation
        .rationale
        .as_ref()
        .unwrap()
        .contains("No side effects"));
}

#[test]
fn test_compensation_readiness_heuristic_with_side_effects() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let side_effects = vec![
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "Database Migration".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Direct side effect".to_string(),
            external_ref: None,
        },
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "Config Update".to_string(),
            impact: ClassificationImpact::Transitive,
            reason: "Transitive side effect".to_string(),
            external_ref: None,
        },
    ];

    let affected_items = AffectedItemsPreview::from_classification(vec![], vec![], side_effects);

    let compensation = CompensationReadiness::with_side_effects(DecisionClass::D, &affected_items);

    assert!(!compensation.ready);
    assert_eq!(compensation.potential_actions.len(), 2);
    assert!(compensation.has_irreversible_effects);
    assert!(compensation.rationale.is_some());
    let rationale = compensation.rationale.as_ref().unwrap();
    assert!(rationale.contains("2 potential compensation action(s)"));
    assert!(rationale.contains("irreversible: true"));
}

#[test]
fn test_compensation_readiness_heuristic_transitive_only_no_irreversible() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let side_effects = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Log Propagation".to_string(),
        impact: ClassificationImpact::Transitive,
        reason: "Transitive only".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(vec![], vec![], side_effects);

    let compensation = CompensationReadiness::with_side_effects(DecisionClass::B, &affected_items);

    assert!(!compensation.ready);
    assert_eq!(compensation.potential_actions.len(), 1);
    assert!(compensation.has_irreversible_effects); // Transitive still counts as irreversible
    let action = &compensation.potential_actions[0];
    assert!(!action.reversible); // Transitive effects are NOT reversible (only Unchanged is)
    assert_eq!(action.priority, 2); // Transitive = priority 2
}

#[test]
fn test_compensation_readiness_heuristic_direct_effects_reversible_false() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let side_effects = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Data Deletion".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Direct side effect".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(vec![], vec![], side_effects);

    let compensation = CompensationReadiness::with_side_effects(DecisionClass::E, &affected_items);

    assert!(!compensation.ready);
    let action = &compensation.potential_actions[0];
    assert!(!action.reversible); // Direct effects are not reversible
    assert_eq!(action.priority, 1); // Direct = priority 1
}

#[test]
fn test_compensation_readiness_heuristic_ready_always_false() {
    // Key invariant: ready is always false regardless of decision class or side effects
    let affected_items = AffectedItemsPreview::unavailable();

    for class in &[
        DecisionClass::A,
        DecisionClass::B,
        DecisionClass::C,
        DecisionClass::D,
        DecisionClass::E,
    ] {
        let compensation = CompensationReadiness::with_side_effects(*class, &affected_items);
        assert!(
            !compensation.ready,
            "CompensationReadiness::ready should be false for class {:?}",
            class
        );
    }
}

// === DeferredFields::phase1_baseline Integration Tests ===

#[test]
fn test_phase1_baseline_calls_all_three_heuristics() {
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let artifacts = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Artifact".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];
    let approvals = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Approval".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];
    let side_effects = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Side Effect".to_string(),
        impact: ClassificationImpact::Transitive,
        reason: "test".to_string(),
        external_ref: None,
    }];

    let affected_items =
        AffectedItemsPreview::from_classification(artifacts, approvals, side_effects);

    let deferred = DeferredFields::phase1_baseline(DecisionClass::D, &affected_items);

    // CheckpointSelection heuristic was already tested, verify it's populated
    assert!(!deferred.checkpoint_selection.ready);
    assert!(!deferred.checkpoint_selection.candidates.is_empty());

    // ApprovalRevalidation heuristic should be populated
    assert!(!deferred.approval_revalidation.ready);
    assert_eq!(
        deferred
            .approval_revalidation
            .approvals_needing_revalidation
            .len(),
        1
    );
    assert_eq!(
        deferred.approval_revalidation.strategy,
        RevalidationStrategy::Full
    );

    // CompensationReadiness heuristic should be populated
    assert!(!deferred.compensation.ready);
    assert_eq!(deferred.compensation.potential_actions.len(), 1);
    assert!(deferred.compensation.has_irreversible_effects);
}

#[test]
fn test_phase1_baseline_invariant_all_three_ready_false() {
    // Verify that all three deferred fields have ready=false
    // regardless of decision class - this is the key Phase 1 invariant
    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use uuid::Uuid;

    let artifacts = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Artifact".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];
    let approvals = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Approval".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];
    let side_effects = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "Test Side Effect".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "test".to_string(),
        external_ref: None,
    }];

    let affected_items =
        AffectedItemsPreview::from_classification(artifacts, approvals, side_effects);

    for class in &[
        DecisionClass::A,
        DecisionClass::B,
        DecisionClass::C,
        DecisionClass::D,
        DecisionClass::E,
    ] {
        let deferred = DeferredFields::phase1_baseline(*class, &affected_items);
        assert!(
            !deferred.checkpoint_selection.ready,
            "checkpoint_selection::ready should be false for class {:?}",
            class
        );
        assert!(
            !deferred.approval_revalidation.ready,
            "approval_revalidation::ready should be false for class {:?}",
            class
        );
        assert!(
            !deferred.compensation.ready,
            "compensation::ready should be false for class {:?}",
            class
        );
    }
}

// === RuntimeAdapter Integration Tests ===

#[tokio::test]
async fn test_checkpoint_selection_adapter_integration_seam() {
    // Integration test verifying the seam between planner's CheckpointSelection
    // and runtime-adapter's MockAdapter.
    //
    // This test catches type mismatches between:
    //   - planner::CheckpointCandidate (planner.rs line 119-129)
    //   - runtime_adapter::CheckpointCandidate (runtime-adapter/src/lib.rs line 58-68)
    //
    // Both structs are structurally identical (id, label, description, validated)
    // but come from different crates. This test ensures they can interoperate.

    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use runtime_adapter::{IntentRef, MockAdapter, RuntimeAdapter};
    use uuid::Uuid;

    // Create affected items with workflow context
    let artifacts = vec![
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "workflow-artifact-1".to_string(),
            impact: ClassificationImpact::Direct,
            reason: "Affected by scope change".to_string(),
            external_ref: None,
        },
        AffectedItem {
            node_id: Uuid::new_v4(),
            label: "workflow-artifact-2".to_string(),
            impact: ClassificationImpact::Transitive,
            reason: "Transitively affected".to_string(),
            external_ref: None,
        },
    ];

    let affected_items =
        AffectedItemsPreview::from_classification(artifacts.clone(), vec![], vec![]);

    // Create a CheckpointSelection using the planner's heuristic
    let selection = CheckpointSelection::with_heuristic(DecisionClass::B, &affected_items);

    // Verify planner's selection has candidates with the expected pattern
    assert!(
        !selection.candidates.is_empty(),
        "Planner should produce checkpoint candidates for Class B"
    );
    let planner_candidate_ids: Vec<&str> =
        selection.candidates.iter().map(|c| c.id.as_str()).collect();
    assert!(
        planner_candidate_ids.contains(&"checkpoint-most-recent"),
        "Class B should have 'most-recent' candidate"
    );

    // Now use MockAdapter to resolve checkpoints
    let adapter = MockAdapter::ready();
    let adapter_checkpoints = adapter.get_checkpoints().await.unwrap();

    // Verify adapter returns structurally valid checkpoints
    assert!(
        !adapter_checkpoints.is_empty(),
        "MockAdapter should return checkpoint candidates"
    );

    // KEY ASSERTION: Verify the checkpoint ID pattern is compatible
    // The adapter uses "checkpoint-XXX" format while planner uses descriptive IDs
    // Both should follow the "checkpoint-" prefix convention
    for cp in &adapter_checkpoints {
        assert!(
            cp.id.starts_with("checkpoint-"),
            "Adapter checkpoint ID should follow 'checkpoint-' prefix pattern: {}",
            cp.id
        );
    }

    // Verify planner candidate IDs also follow the convention
    for id in &planner_candidate_ids {
        assert!(
            id.starts_with("checkpoint-"),
            "Planner candidate ID should follow 'checkpoint-' prefix pattern: {}",
            id
        );
    }

    // Create an IntentRef to test adapter's map_intent_to_checkpoint
    let intent_ref = IntentRef::new(
        "test-intent-id".to_string(),
        "test-tenant".to_string(),
        "test-workflow-id".to_string(),
        "active".to_string(),
    );

    // Verify the adapter can map the intent to a checkpoint
    let mapped = adapter.map_intent_to_checkpoint(intent_ref).await.unwrap();
    assert!(
        mapped.id.starts_with("checkpoint-"),
        "Mapped checkpoint should follow the checkpoint prefix pattern"
    );

    // Verify adapter status
    let status = adapter.is_adapter_ready().await.unwrap();
    assert_eq!(
        status,
        runtime_adapter::AdapterStatus::Ready,
        "MockAdapter should report Ready status"
    );

    // Verify selected checkpoint from planner can be cross-checked with adapter
    if let Some(selected) = &selection.selected {
        // The selected checkpoint ID should be recognizable by the adapter
        // (even if the adapter returns different concrete checkpoints)
        assert!(
            selected.id.starts_with("checkpoint-"),
            "Selected checkpoint ID should follow adapter pattern: {}",
            selected.id
        );
        assert!(
            !selected.label.is_empty(),
            "Selected checkpoint should have a label"
        );
    }
}

#[tokio::test]
async fn test_deferred_fields_checkpoint_adapter_type_compatibility() {
    // Test that DeferredFields::checkpoint_selection candidates are type-compatible
    // with the runtime adapter's checkpoint representation.
    //
    // This catches any field mismatches early (Phase 1) before Phase 2 integration.

    use intent_rebase_types::{AffectedItem, AffectedItemsPreview, ClassificationImpact};
    use runtime_adapter::{IntentRef, MockAdapter, RuntimeAdapter};
    use uuid::Uuid;

    // Create affected items that will drive checkpoint candidate generation
    let side_effects = vec![AffectedItem {
        node_id: Uuid::new_v4(),
        label: "side-effect-1".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Direct side effect".to_string(),
        external_ref: None,
    }];

    let affected_items = AffectedItemsPreview::from_classification(vec![], vec![], side_effects);

    // Generate DeferredFields with checkpoint selection for Class D
    // (which has side-effects checkpoint candidates)
    let deferred = DeferredFields::phase1_baseline(DecisionClass::D, &affected_items);

    // Verify checkpoint selection has candidates
    assert!(
        !deferred.checkpoint_selection.candidates.is_empty(),
        "Class D should have checkpoint candidates"
    );

    // Get adapter checkpoints
    let adapter = MockAdapter::ready();
    let adapter_checkpoints = adapter.get_checkpoints().await.unwrap();

    // CRITICAL: Both checkpoint types must have the same field structure
    // This is the key type compatibility check at the integration seam

    // Check planner checkpoint candidate fields
    let planner_cp = &deferred.checkpoint_selection.candidates[0];
    assert!(
        !planner_cp.id.is_empty(),
        "Planner checkpoint must have id field"
    );
    assert!(
        !planner_cp.label.is_empty(),
        "Planner checkpoint must have label field"
    );
    assert!(
        !planner_cp.description.is_empty(),
        "Planner checkpoint must have description field"
    );
    // validated field exists but is false in Phase 1 (not yet validated by runtime)

    // Check adapter checkpoint candidate fields
    let adapter_cp = &adapter_checkpoints[0];
    assert!(
        !adapter_cp.id.is_empty(),
        "Adapter checkpoint must have id field"
    );
    assert!(
        !adapter_cp.label.is_empty(),
        "Adapter checkpoint must have label field"
    );
    assert!(
        !adapter_cp.description.is_empty(),
        "Adapter checkpoint must have description field"
    );
    assert!(
        adapter_cp.validated,
        "Adapter checkpoint should be validated (MockAdapter default)"
    );

    // Both should use "checkpoint-" prefix pattern in IDs
    assert!(
        planner_cp.id.starts_with("checkpoint-") || planner_cp.id.starts_with("checkpoint_"),
        "Planner checkpoint ID should follow pattern: {}",
        planner_cp.id
    );
    assert!(
        adapter_cp.id.starts_with("checkpoint-"),
        "Adapter checkpoint ID should follow pattern: {}",
        adapter_cp.id
    );

    // Test replay_from_checkpoint with adapter
    let checkpoint = runtime_adapter::Checkpoint {
        id: adapter_cp.id.clone(),
        label: adapter_cp.label.clone(),
        description: adapter_cp.description.clone(),
        timestamp: chrono::Utc::now(),
        validated: adapter_cp.validated,
    };

    let intent_ref = IntentRef::new(
        "test-intent".to_string(),
        "tenant".to_string(),
        "workflow".to_string(),
        "active".to_string(),
    );

    // Verify replay can work with the adapter
    let replay_result = adapter.replay_from_checkpoint(checkpoint, intent_ref).await;
    assert!(
        replay_result.is_ok(),
        "Adapter replay_from_checkpoint should succeed"
    );
}
