//! Rebase planner module — preview-only baseline
//!
//! Phase 1 implements a preview-only planner that maps diff+risk analysis
//! to deterministic decision classes A-E.
//!
//! This module does NOT include:
//! - Graph-based impact classification integration (requires graph HTTP API)
//! - Checkpoint selection beyond preview fields (TODO/None in Phase 1)
//! - Approval revalidation hooks (TODO/None in Phase 1)
//! - Runtime adapter integration (Phase 2)
//!
//! The planner is deterministic: same diff+risk input always produces
//! the same decision class output.

use serde::{Deserialize, Serialize};

use crate::diff::IntentVersionDiff;
use crate::risk::{DiffRiskAnalysis, Severity};

/// Decision class for rebase planning
///
/// Phase 1 baseline uses these classes based on diff severity and risk analysis:
/// - A: No semantic changes detected
/// - B: Low/Medium severity or review-flagged changes requiring preview review
/// - C: High severity changes with limited scope that may be auto-repairable
/// - D: High severity with manual review or multiple high-severity sections (also Medium + manual_review)
/// - E: Critical severity or 3+ high-severity sections requiring manual handoff
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DecisionClass {
    /// No-op / Metadata update — no execution semantics affected
    A,
    /// Soft review — no immediate invalidation but review recommended
    B,
    /// Partial repair — local invalidation, selective checkpoint resume
    C,
    /// Compensation + repair — side effects need mitigation
    D,
    /// Hard restart / manual handoff — auto-repair not safe
    E,
}

impl DecisionClass {
    /// Human-readable description of the decision class
    pub fn description(&self) -> &'static str {
        match self {
            DecisionClass::A => "No semantic changes — no rebase needed",
            DecisionClass::B => "Soft review recommended — no immediate invalidation",
            DecisionClass::C => "Partial repair candidate — limited scope changes",
            DecisionClass::D => "Compensation and repair needed — manual review advised",
            DecisionClass::E => "Hard restart required — manual handoff needed",
        }
    }
}

/// Phase 1 status for features not yet implemented
///
/// These fields are spec-adjacent but deferred to Phase 2+.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeferredFields {
    /// TODO: Checkpoint selection (Phase 2)
    pub checkpoint_selection: Option<String>,
    /// TODO: Approval revalidation status (Phase 2)
    pub approval_revalidation: Option<String>,
    /// TODO: Compensation action list (Phase 2)
    pub compensation_actions: Vec<String>,
}

impl DeferredFields {
    /// Create new deferred fields with TODO markers
    pub fn phase1_baseline() -> Self {
        Self {
            checkpoint_selection: Some("TODO: Phase 2".to_string()),
            approval_revalidation: Some("TODO: Phase 2".to_string()),
            compensation_actions: vec!["TODO: Phase 2".to_string()],
        }
    }
}

/// A single change decision for a section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionDecision {
    /// Section name (scope, constraints, acceptance_criteria, authority)
    pub section: String,
    /// Change type summary for this section
    pub change_summary: String,
    /// Recommended action for this section
    pub recommended_action: String,
}

/// Preview of affected items (Phase 1 baseline — not yet integrated with graph)
///
/// These fields use TODO/None because graph HTTP API is not yet integrated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AffectedItemsPreview {
    /// TODO: List of affected artifact IDs (requires graph integration — Phase 2)
    pub affected_artifacts: Vec<String>,
    /// TODO: List of affected approval IDs requiring revalidation (requires graph — Phase 2)
    pub affected_approvals: Vec<String>,
    /// TODO: List of side effects needing compensation (requires graph — Phase 2)
    pub side_effects_requiring_compensation: Vec<String>,
}

impl AffectedItemsPreview {
    /// Create Phase 1 baseline with empty/None values
    pub fn phase1_baseline() -> Self {
        Self {
            affected_artifacts: vec![],
            affected_approvals: vec![],
            side_effects_requiring_compensation: vec![],
        }
    }
}

/// Complete rebase plan output from the planner
///
/// Phase 1 baseline provides typed decision class mapping from diff+risk
/// analysis without graph integration. Future PRs will enhance with:
/// - Graph-based affected node classification
/// - Checkpoint selection heuristics
/// - Approval revalidation hooks
/// - Compensation action generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebasePlan {
    /// The assigned decision class (A-E)
    pub decision_class: DecisionClass,
    /// Human-readable decision rationale
    pub rationale: String,
    /// Section-level decisions
    pub section_decisions: Vec<SectionDecision>,
    /// Affected items preview (Phase 1: empty, TODO in Phase 2)
    pub affected_items: AffectedItemsPreview,
    /// Deferred fields (Phase 1: TODO markers)
    pub deferred: DeferredFields,
    /// Whether manual review is recommended
    pub manual_review_recommended: bool,
    /// Risk level from 1-5 (1=lowest, 5=highest)
    pub risk_level: u8,
}

impl RebasePlan {
    /// Create a Phase 1 baseline rebase plan from diff and risk analysis
    ///
    /// This function implements deterministic decision class mapping:
    /// - Class A: No changes (all diff sections empty)
    /// - Class B: Low/Medium severity OR manual_review flagged
    /// - Class C: High severity, no manual_review, limited scope; also Medium + 2 changed sections
    /// - Class D: High severity with manual_review OR 2+ high-severity sections; also Medium + manual_review OR 3+ changed sections
    /// - Class E: Critical severity OR 3+ high-severity sections
    pub fn from_diff_and_risk(diff: &IntentVersionDiff, risk: &DiffRiskAnalysis) -> Self {
        let decision_class = compute_decision_class(diff, risk);
        let rationale = compute_rationale(&decision_class, diff, risk);
        let section_decisions = compute_section_decisions(diff);

        // Manual review is recommended when:
        // - The risk analysis already flags manual review
        // - Severity is High or Critical
        // - Decision class is B, C, D, or E (Class A = no changes)
        let manual_review_recommended = risk.manual_review
            || risk.severity == Severity::Critical
            || risk.severity == Severity::High
            || decision_class != DecisionClass::A;

        let risk_level = compute_risk_level(&decision_class, risk);

        Self {
            decision_class,
            rationale,
            section_decisions,
            affected_items: AffectedItemsPreview::phase1_baseline(),
            deferred: DeferredFields::phase1_baseline(),
            manual_review_recommended,
            risk_level,
        }
    }
}

/// Compute the decision class deterministically from diff and risk
fn compute_decision_class(diff: &IntentVersionDiff, risk: &DiffRiskAnalysis) -> DecisionClass {
    // Class A: No semantic changes
    if is_diff_empty(diff) {
        return DecisionClass::A;
    }

    // Class E: Critical severity OR multiple high-severity sections
    if risk.severity == Severity::Critical {
        return DecisionClass::E;
    }

    // Count high-severity sections
    let high_severity_count = risk
        .section_risks
        .iter()
        .filter(|s| s.severity == Severity::High || s.severity == Severity::Critical)
        .count();

    if high_severity_count >= 3 {
        return DecisionClass::E;
    }

    // Class D: High severity with manual review OR multiple high-severity sections (2+)
    if risk.severity == Severity::High {
        if risk.manual_review || high_severity_count >= 2 {
            return DecisionClass::D;
        }
        // Single high-severity section, no manual review → Class C
        return DecisionClass::C;
    }

    // Class C: Medium severity with multiple section changes
    if risk.severity == Severity::Medium {
        // Only count sections that actually have changes (change_count > 0)
        let changed_sections = risk
            .section_risks
            .iter()
            .filter(|s| s.change_count > 0)
            .count();
        if changed_sections >= 3 || risk.manual_review {
            return DecisionClass::D;
        }
        if changed_sections >= 2 {
            return DecisionClass::C;
        }
        // Single medium change
        return DecisionClass::B;
    }

    // Class B: Low severity OR manual review flagged
    if risk.severity == Severity::Low {
        if risk.manual_review {
            return DecisionClass::B;
        }
        return DecisionClass::B;
    }

    // Default to B (soft review)
    DecisionClass::B
}

/// Check if the diff represents no semantic changes
fn is_diff_empty(diff: &IntentVersionDiff) -> bool {
    // Check scope
    if !diff.scope.in_scope.added.is_empty()
        || !diff.scope.in_scope.removed.is_empty()
        || !diff.scope.out_of_scope.added.is_empty()
        || !diff.scope.out_of_scope.removed.is_empty()
    {
        return false;
    }

    // Check constraints (all categories)
    if !diff.constraints.functional.is_empty()
        || !diff.constraints.non_functional.is_empty()
        || !diff.constraints.policy.is_empty()
        || !diff.constraints.budget.is_empty()
        || !diff.constraints.time.is_empty()
    {
        return false;
    }

    // Check acceptance criteria
    if !diff.acceptance_criteria.required.is_empty()
        || !diff.acceptance_criteria.optional.is_empty()
    {
        return false;
    }

    // Check authority
    if !diff.authority.allowed_actions.is_empty()
        || !diff.authority.forbidden_actions.is_empty()
        || !diff.authority.approval_requirements.is_empty()
    {
        return false;
    }

    true
}

/// Compute human-readable rationale for the decision
fn compute_rationale(
    decision: &DecisionClass,
    _diff: &IntentVersionDiff,
    risk: &DiffRiskAnalysis,
) -> String {
    match decision {
        DecisionClass::A => "No semantic changes detected across all sections".to_string(),
        DecisionClass::B => {
            let mut parts = vec![];
            parts.push(format!(
                "{} severity with {} confidence",
                format!("{:?}", risk.severity).to_lowercase(),
                format!("{:.0}%", risk.confidence * 100.0)
            ));
            if risk.manual_review {
                parts.push("manual review flagged".to_string());
            }
            if let Some(r) = &risk.rationale {
                parts.push(r.clone());
            }
            parts.join("; ")
        }
        DecisionClass::C => {
            let mut parts = vec![];
            parts.push(format!(
                "{} severity changes in {:?} sections",
                format!("{:?}", risk.severity).to_lowercase(),
                risk.section_risks
                    .iter()
                    .map(|s| s.section.clone())
                    .collect::<Vec<_>>()
            ));
            if let Some(r) = &risk.rationale {
                parts.push(r.clone());
            }
            parts.join("; ")
        }
        DecisionClass::D => {
            let mut parts = vec!["Manual review or compensation may be required".to_string()];
            if risk.manual_review {
                let reasons: Vec<String> = risk
                    .manual_review_reasons
                    .iter()
                    .map(|r| format!("{:?}", r))
                    .collect();
                parts.push(format!("Review reasons: {}", reasons.join(", ")));
            }
            if let Some(r) = &risk.rationale {
                parts.push(r.clone());
            }
            parts.join("; ")
        }
        DecisionClass::E => {
            let mut parts = vec!["Critical or multi-section high severity detected".to_string()];
            if let Some(r) = &risk.rationale {
                parts.push(r.clone());
            }
            parts.join("; ")
        }
    }
}

/// Compute section-level decisions for each changed section
fn compute_section_decisions(diff: &IntentVersionDiff) -> Vec<SectionDecision> {
    let mut decisions = Vec::new();

    // Scope section
    if !diff.scope.in_scope.added.is_empty()
        || !diff.scope.in_scope.removed.is_empty()
        || !diff.scope.out_of_scope.added.is_empty()
        || !diff.scope.out_of_scope.removed.is_empty()
    {
        let mut summary_parts = vec![];
        if !diff.scope.in_scope.added.is_empty() {
            summary_parts.push(format!("+{} in_scope", diff.scope.in_scope.added.len()));
        }
        if !diff.scope.in_scope.removed.is_empty() {
            summary_parts.push(format!("-{} in_scope", diff.scope.in_scope.removed.len()));
        }
        if !diff.scope.out_of_scope.added.is_empty() {
            summary_parts.push(format!(
                "+{} out_of_scope",
                diff.scope.out_of_scope.added.len()
            ));
        }
        if !diff.scope.out_of_scope.removed.is_empty() {
            summary_parts.push(format!(
                "-{} out_of_scope",
                diff.scope.out_of_scope.removed.len()
            ));
        }
        decisions.push(SectionDecision {
            section: "scope".to_string(),
            change_summary: summary_parts.join(", "),
            recommended_action: "Review scope deltas before proceeding".to_string(),
        });
    }

    // Constraints section
    let total_constraints = diff.constraints.functional.len()
        + diff.constraints.non_functional.len()
        + diff.constraints.policy.len()
        + diff.constraints.budget.len()
        + diff.constraints.time.len();
    if total_constraints > 0 {
        let mut policy_change = false;
        if !diff.constraints.policy.is_empty() {
            policy_change = true;
        }
        decisions.push(SectionDecision {
            section: "constraints".to_string(),
            change_summary: format!("{} total constraint changes", total_constraints),
            recommended_action: if policy_change {
                "Policy constraint changes — verify against governing policies".to_string()
            } else {
                "Review constraint changes for impact on execution".to_string()
            },
        });
    }

    // Acceptance criteria section
    let total_ac =
        diff.acceptance_criteria.required.len() + diff.acceptance_criteria.optional.len();
    if total_ac > 0 {
        decisions.push(SectionDecision {
            section: "acceptance_criteria".to_string(),
            change_summary: format!("{} acceptance criteria changes", total_ac),
            recommended_action: "Verify acceptance criteria changes maintain success conditions"
                .to_string(),
        });
    }

    // Authority section
    let total_auth = diff.authority.allowed_actions.len()
        + diff.authority.forbidden_actions.len()
        + diff.authority.approval_requirements.len();
    if total_auth > 0 {
        decisions.push(SectionDecision {
            section: "authority".to_string(),
            change_summary: format!("{} authority changes", total_auth),
            recommended_action: "Review authority changes for approval and action implications"
                .to_string(),
        });
    }

    decisions
}

/// Compute risk level (1-5) from decision class and risk analysis
fn compute_risk_level(decision: &DecisionClass, risk: &DiffRiskAnalysis) -> u8 {
    match decision {
        DecisionClass::A => 1,
        DecisionClass::B => {
            if risk.confidence < 0.5 {
                3
            } else {
                2
            }
        }
        DecisionClass::C => 3,
        DecisionClass::D => 4,
        DecisionClass::E => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{
        AcceptanceCriteriaDiff, AuthorityDiff, ConstraintsDiff, ScopeDiff, ScopeItemsDiff,
    };
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
    fn test_deferred_fields_are_todo() {
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

        assert!(plan.deferred.checkpoint_selection.is_some());
        assert!(plan.deferred.approval_revalidation.is_some());
        assert!(!plan.deferred.compensation_actions.is_empty());
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
}
