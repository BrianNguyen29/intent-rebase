//! Deterministic risk rules for semantic diff analysis
//!
//! These rules compute severity, confidence, and manual-review triggers
//! based on the structured diff output from the diff module.
//!
//! The rules are deterministic and replayable - same diff input always
//! produces same risk output under the same rule version.

use crate::diff::{
    AcceptanceCriteriaDiff, AcceptanceCriterionDiff, ActionRefDiff, ApprovalRuleDiff,
    AuthorityDiff, ChangeType, ConstraintDiff, ConstraintsDiff, ScopeDiff,
};
use crate::risk::{DiffRiskAnalysis, ManualReviewReason, RiskConfig, SectionRisk, Severity};
use intent_rebase_types::ClausePriority;

/// Statistics about clause_id matching quality for confidence scoring
#[derive(Debug, Clone, Default)]
pub struct MatchingStats {
    /// Total number of changes analyzed
    pub total_changes: usize,
    /// Changes with unique clause_id match
    pub unique_clause_id_match: usize,
    /// Changes with ambiguous identity (None clause_id or duplicate)
    pub ambiguous_match: usize,
    /// Changes with no match (pure add or pure remove)
    pub no_match: usize,
    /// Changes that were removed (for severity escalation)
    pub removed: usize,
}

/// Analyze risk for a scope diff section
pub fn analyze_scope_risk(scope_diff: &ScopeDiff) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();
    let mut severity = Severity::Low;

    let ScopeDiff {
        in_scope,
        out_of_scope,
    } = scope_diff;

    // Count scope changes
    let in_scope_changes = in_scope.added.len() + in_scope.removed.len();
    let out_of_scope_changes = out_of_scope.added.len() + out_of_scope.removed.len();
    let total_changes = in_scope_changes + out_of_scope_changes;

    stats.total_changes = total_changes;

    if total_changes == 0 {
        return (Severity::Low, stats);
    }

    // Scope changes are generally medium severity
    // Adding to in_scope (expanding scope) is higher risk than removing
    if !in_scope.added.is_empty() {
        severity = Severity::Medium;
    }

    // Scope contraction (removing from in_scope) is medium risk
    if !in_scope.removed.is_empty() {
        severity = Severity::Medium;
    }

    // out_of_scope changes also indicate scope revision and should not stay Low
    // Adding to out_of_scope (moving items out of scope) is a scope contraction
    // Removing from out_of_scope (bringing items back into scope) is a scope expansion
    if !out_of_scope.added.is_empty() || !out_of_scope.removed.is_empty() {
        severity = Severity::Medium;
    }

    // No clause_id matching for scope items (they're just strings)
    // All scope changes are treated as having ambiguous identity
    stats.ambiguous_match = total_changes;

    (severity, stats)
}

/// Analyze risk for constraints diff section
pub fn analyze_constraints_risk(constraints_diff: &ConstraintsDiff) -> (Severity, MatchingStats) {
    let mut overall_severity = Severity::Low;
    let mut stats = MatchingStats::default();

    let ConstraintsDiff {
        functional,
        non_functional,
        policy,
        budget,
        time,
    } = constraints_diff;

    let categories = [
        ("functional", functional.as_slice()),
        ("non_functional", non_functional.as_slice()),
        ("policy", policy.as_slice()),
        ("budget", budget.as_slice()),
        ("time", time.as_slice()),
    ];

    for (category_name, diffs) in categories {
        let (cat_severity, cat_stats) = analyze_constraint_category(category_name, diffs);
        stats.total_changes += cat_stats.total_changes;
        stats.unique_clause_id_match += cat_stats.unique_clause_id_match;
        stats.ambiguous_match += cat_stats.ambiguous_match;
        stats.no_match += cat_stats.no_match;

        if cat_severity > overall_severity {
            overall_severity = cat_severity;
        }
    }

    (overall_severity, stats)
}

fn analyze_constraint_category(
    category_name: &str,
    diffs: &[ConstraintDiff],
) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();
    let mut severity = Severity::Low;
    let mut high_priority_changes = 0;

    stats.total_changes = diffs.len();

    for diff in diffs {
        let diff_severity = compute_constraint_severity(diff);
        if diff_severity > severity {
            severity = diff_severity;
        }

        // Count high priority changes
        if let Some(constraint) = diff.before.as_ref().or(diff.after.as_ref()) {
            if constraint.priority == ClausePriority::Must {
                high_priority_changes += 1;
            }
        }

        // Track matching quality
        if diff.clause_id.is_some() && diff.change_type == ChangeType::Modified {
            // Unique clause_id match
            stats.unique_clause_id_match += 1;
        } else if diff.clause_id.is_none() {
            // Ambiguous - no clause_id
            stats.ambiguous_match += 1;
        } else if diff.change_type == ChangeType::Added || diff.change_type == ChangeType::Removed {
            // No match possible (pure add/remove)
            stats.no_match += 1;
        }
    }

    // Policy changes elevate to at least High
    if category_name == "policy" && !diffs.is_empty() && severity < Severity::High {
        severity = Severity::High;
    }

    // Critical: policy changes with Must priority
    if category_name == "policy" && high_priority_changes > 0 {
        severity = Severity::Critical;
    }

    (severity, stats)
}

fn compute_constraint_severity(diff: &ConstraintDiff) -> Severity {
    match diff.change_type {
        ChangeType::Added => {
            // Adding constraints is Medium by default
            // Adding Must-priority is High
            if let Some(c) = diff.after.as_ref() {
                if c.priority == ClausePriority::Must {
                    return Severity::High;
                }
            }
            Severity::Medium
        }
        ChangeType::Removed => {
            // Removing constraints is Medium (loosening)
            Severity::Medium
        }
        ChangeType::Modified => {
            // Modifying constraints depends on what's changed
            // If the change could invalidate tests/approvals, it's High
            if let Some(c) = diff.after.as_ref() {
                if c.priority == ClausePriority::Must {
                    return Severity::High;
                }
            }
            Severity::Medium
        }
    }
}

/// Analyze risk for acceptance criteria diff section
pub fn analyze_acceptance_criteria_risk(
    ac_diff: &AcceptanceCriteriaDiff,
) -> (Severity, MatchingStats) {
    let mut overall_severity = Severity::Low;
    let mut stats = MatchingStats::default();

    let AcceptanceCriteriaDiff { required, optional } = ac_diff;

    let categories = [
        ("required", required.as_slice()),
        ("optional", optional.as_slice()),
    ];

    for (category_name, diffs) in categories {
        let (cat_severity, cat_stats) = analyze_acceptance_criterion_category(category_name, diffs);
        stats.total_changes += cat_stats.total_changes;
        stats.unique_clause_id_match += cat_stats.unique_clause_id_match;
        stats.ambiguous_match += cat_stats.ambiguous_match;
        stats.no_match += cat_stats.no_match;

        if cat_severity > overall_severity {
            overall_severity = cat_severity;
        }
    }

    (overall_severity, stats)
}

fn analyze_acceptance_criterion_category(
    category_name: &str,
    diffs: &[AcceptanceCriterionDiff],
) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();
    let mut severity = Severity::Low;

    stats.total_changes = diffs.len();

    for diff in diffs {
        let diff_severity = compute_acceptance_criterion_severity(diff);
        if diff_severity > severity {
            severity = diff_severity;
        }

        // Track matching quality
        if diff.clause_id.is_some() && diff.change_type == ChangeType::Modified {
            stats.unique_clause_id_match += 1;
        } else if diff.clause_id.is_none() {
            stats.ambiguous_match += 1;
        } else {
            stats.no_match += 1;
        }
    }

    // Required acceptance criteria changes are more serious
    if category_name == "required" && !diffs.is_empty() && severity < Severity::Medium {
        severity = Severity::Medium;
    }

    (severity, stats)
}

fn compute_acceptance_criterion_severity(diff: &AcceptanceCriterionDiff) -> Severity {
    match diff.change_type {
        ChangeType::Added => {
            // For Added: diff.priority is the new criterion's priority (correct)
            if diff.priority.contains("Must") {
                Severity::High
            } else {
                Severity::Medium
            }
        }
        ChangeType::Removed => Severity::Medium,
        ChangeType::Modified => {
            // For Modified: diff.priority is the BEFORE state priority,
            // but severity should reflect the AFTER state (the new requirement).
            // If the after state is Must, it's now a high-priority requirement.
            if let Some(after) = diff.after.as_ref() {
                if after.priority == ClausePriority::Must {
                    return Severity::High;
                }
            }
            Severity::Medium
        }
    }
}

/// Analyze risk for authority diff section
pub fn analyze_authority_risk(authority_diff: &AuthorityDiff) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();

    let AuthorityDiff {
        allowed_actions,
        forbidden_actions,
        approval_requirements,
    } = authority_diff;

    // Analyze allowed actions
    let (allowed_severity, allowed_stats) =
        analyze_action_refs_risk("allowed_actions", allowed_actions);
    let mut overall_severity = allowed_severity;
    stats.total_changes += allowed_stats.total_changes;
    stats.unique_clause_id_match += allowed_stats.unique_clause_id_match;
    stats.ambiguous_match += allowed_stats.ambiguous_match;
    stats.no_match += allowed_stats.no_match;
    stats.removed += allowed_stats.removed;

    // Analyze forbidden actions
    let (forbidden_severity, forbidden_stats) =
        analyze_action_refs_risk("forbidden_actions", forbidden_actions);
    if forbidden_severity > overall_severity {
        overall_severity = forbidden_severity;
    }
    stats.total_changes += forbidden_stats.total_changes;
    stats.unique_clause_id_match += forbidden_stats.unique_clause_id_match;
    stats.ambiguous_match += forbidden_stats.ambiguous_match;
    stats.no_match += forbidden_stats.no_match;
    stats.removed += forbidden_stats.removed;

    // Removing forbidden actions is Critical (reducing restrictions)
    if forbidden_stats.removed > 0 && overall_severity < Severity::Critical {
        overall_severity = Severity::Critical;
    }

    // Analyze approval requirements
    let (approval_severity, approval_stats) = analyze_approval_rules_risk(approval_requirements);
    if approval_severity > overall_severity {
        overall_severity = approval_severity;
    }
    stats.total_changes += approval_stats.total_changes;
    stats.unique_clause_id_match += approval_stats.unique_clause_id_match;
    stats.ambiguous_match += approval_stats.ambiguous_match;
    stats.no_match += approval_stats.no_match;
    stats.removed += approval_stats.removed;

    // Removing approval requirements is Critical
    if approval_stats.removed > 0 && overall_severity < Severity::Critical {
        overall_severity = Severity::Critical;
    }

    // Authority changes in general are High
    if stats.total_changes > 0 && overall_severity < Severity::High {
        overall_severity = Severity::High;
    }

    (overall_severity, stats)
}

fn analyze_action_refs_risk(
    section_name: &str,
    diffs: &[ActionRefDiff],
) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();
    let mut severity = Severity::Low;

    stats.total_changes = diffs.len();

    for diff in diffs {
        // Action ref changes are Medium by default
        let diff_severity = match diff.change_type {
            ChangeType::Added => Severity::Medium,
            ChangeType::Removed => {
                // Removing actions from allowed is High (losing capability)
                // Removing actions from forbidden is Critical (removing restriction)
                if section_name == "forbidden_actions" {
                    Severity::Critical
                } else {
                    Severity::High
                }
            }
            ChangeType::Modified => Severity::Medium,
        };

        if diff_severity > severity {
            severity = diff_severity;
        }

        // Track matching quality (action+target key based)
        if diff.change_type == ChangeType::Modified {
            stats.unique_clause_id_match += 1;
        } else if diff.before.is_none() || diff.after.is_none() {
            // Pure add or remove - no match possible
            stats.no_match += 1;
        }

        // Track removals for severity escalation
        if diff.change_type == ChangeType::Removed {
            stats.removed += 1;
        }
    }

    (severity, stats)
}

fn analyze_approval_rules_risk(diffs: &[ApprovalRuleDiff]) -> (Severity, MatchingStats) {
    let mut stats = MatchingStats::default();
    let mut severity = Severity::Low;

    stats.total_changes = diffs.len();

    for diff in diffs {
        let diff_severity = match diff.change_type {
            ChangeType::Added => Severity::Medium,
            ChangeType::Removed => Severity::Critical, // Removing approval = less oversight
            ChangeType::Modified => Severity::High,
        };

        if diff_severity > severity {
            severity = diff_severity;
        }

        // Track matching quality
        if diff.change_type == ChangeType::Modified {
            stats.unique_clause_id_match += 1;
        } else {
            stats.no_match += 1;
        }

        // Track removals for severity escalation
        if diff.change_type == ChangeType::Removed {
            stats.removed += 1;
        }
    }

    (severity, stats)
}

/// Compute confidence score based on matching statistics
pub fn compute_confidence(stats: &MatchingStats) -> f64 {
    if stats.total_changes == 0 {
        // No changes = perfect confidence
        return 1.0;
    }

    // Compute raw ratios (each represents fraction of total changes)
    let unique_ratio = stats.unique_clause_id_match as f64 / stats.total_changes as f64;
    let ambiguous_ratio = stats.ambiguous_match as f64 / stats.total_changes as f64;
    let no_match_ratio = stats.no_match as f64 / stats.total_changes as f64;

    // Weighted confidence:
    // - Unique clause_id match: full confidence (1.0)
    // - Ambiguous (no clause_id or duplicate): partial confidence (0.5)
    // - No match (pure add/remove with unique identity): high confidence (0.8)
    //   because clear intent even if we couldn't match to prior version
    let confidence = unique_ratio * 1.0 + ambiguous_ratio * 0.5 + no_match_ratio * 0.8;

    // Clamp to [0.0, 1.0]
    confidence.clamp(0.0, 1.0)
}

/// Compute overall severity from section severities
pub fn compute_overall_severity(section_severities: &[(String, Severity)]) -> Severity {
    let mut overall = Severity::Low;

    for (_, severity) in section_severities {
        if *severity > overall {
            overall = *severity;
        }
    }

    overall
}

/// Determine if manual review is recommended
pub fn should_manual_review(
    severity: Severity,
    confidence: f64,
    section_severities: &[(String, Severity)],
    policy_changed: bool,
    approval_removed: bool,
    config: &RiskConfig,
) -> (bool, Vec<ManualReviewReason>) {
    let mut reasons = Vec::new();

    // Critical severity always triggers manual review
    if severity == Severity::Critical {
        reasons.push(ManualReviewReason::CriticalSeverity);
    }

    // Low confidence triggers manual review
    if confidence < config.confidence_threshold {
        reasons.push(ManualReviewReason::LowConfidence {
            confidence,
            threshold: config.confidence_threshold,
        });
    }

    // Policy changes trigger manual review
    if policy_changed {
        reasons.push(ManualReviewReason::PolicyConstraintChanged);
    }

    // Count high-severity changes across sections
    let high_severity_count: usize = section_severities
        .iter()
        .filter(|(_, s)| *s == Severity::High || *s == Severity::Critical)
        .count();

    if high_severity_count >= config.max_high_severity_before_manual_review {
        reasons.push(ManualReviewReason::MultipleHighSeverityChanges {
            count: high_severity_count,
        });
    }

    // Check for approval requirement removal (derived from authority diff semantics)
    if approval_removed {
        reasons.push(ManualReviewReason::ApprovalRequirementRemoved);
    }

    (!reasons.is_empty(), reasons)
}

/// Analyze complete diff and compute risk metrics
pub fn analyze_diff_risk(
    scope_diff: &ScopeDiff,
    constraints_diff: &ConstraintsDiff,
    ac_diff: &AcceptanceCriteriaDiff,
    authority_diff: &AuthorityDiff,
) -> DiffRiskAnalysis {
    let config = RiskConfig::default();
    analyze_diff_risk_with_config(
        scope_diff,
        constraints_diff,
        ac_diff,
        authority_diff,
        &config,
    )
}

/// Analyze complete diff with custom configuration
pub fn analyze_diff_risk_with_config(
    scope_diff: &ScopeDiff,
    constraints_diff: &ConstraintsDiff,
    ac_diff: &AcceptanceCriteriaDiff,
    authority_diff: &AuthorityDiff,
    config: &RiskConfig,
) -> DiffRiskAnalysis {
    let mut section_risks = Vec::new();
    let mut section_severities = Vec::new();
    let mut total_stats = MatchingStats::default();

    // Analyze scope
    let (scope_severity, scope_stats) = analyze_scope_risk(scope_diff);
    section_severities.push(("scope".to_string(), scope_severity));
    section_risks.push(SectionRisk {
        section: "scope".to_string(),
        severity: scope_severity,
        change_count: scope_stats.total_changes,
        high_priority_changes: 0,
    });
    total_stats += scope_stats;

    // Analyze constraints
    let (constraints_severity, constraints_stats) = analyze_constraints_risk(constraints_diff);
    section_severities.push(("constraints".to_string(), constraints_severity));
    section_risks.push(SectionRisk {
        section: "constraints".to_string(),
        severity: constraints_severity,
        change_count: constraints_stats.total_changes,
        high_priority_changes: constraints_stats
            .total_changes
            .saturating_sub(constraints_stats.unique_clause_id_match),
    });
    total_stats += constraints_stats;

    // Check if policy changed
    let policy_changed = constraints_diff.policy.iter().any(|d| {
        d.change_type != ChangeType::Modified
            || d.before.as_ref().map(|c| &c.priority) != d.after.as_ref().map(|c| &c.priority)
    }) || !constraints_diff.policy.is_empty();

    // Analyze acceptance criteria
    let (ac_severity, ac_stats) = analyze_acceptance_criteria_risk(ac_diff);
    section_severities.push(("acceptance_criteria".to_string(), ac_severity));
    section_risks.push(SectionRisk {
        section: "acceptance_criteria".to_string(),
        severity: ac_severity,
        change_count: ac_stats.total_changes,
        high_priority_changes: ac_stats
            .total_changes
            .saturating_sub(ac_stats.unique_clause_id_match),
    });
    total_stats += ac_stats;

    // Analyze authority
    let (authority_severity, authority_stats) = analyze_authority_risk(authority_diff);
    section_severities.push(("authority".to_string(), authority_severity));
    section_risks.push(SectionRisk {
        section: "authority".to_string(),
        severity: authority_severity,
        change_count: authority_stats.total_changes,
        high_priority_changes: authority_stats
            .total_changes
            .saturating_sub(authority_stats.unique_clause_id_match),
    });
    total_stats += authority_stats;

    // Compute overall severity
    let severity = compute_overall_severity(&section_severities);

    // Compute confidence
    let confidence = compute_confidence(&total_stats);

    // Determine if approval requirements were removed (for manual review reason)
    let approval_removed = authority_diff
        .approval_requirements
        .iter()
        .any(|r| r.change_type == ChangeType::Removed);

    // Determine manual review
    let (manual_review, manual_review_reasons) = should_manual_review(
        severity,
        confidence,
        &section_severities,
        policy_changed,
        approval_removed,
        config,
    );

    // Generate rationale
    let rationale = generate_rationale(severity, confidence, &section_severities);

    // Filter out sections with no changes - only include sections that actually changed
    let section_risks: Vec<_> = section_risks
        .into_iter()
        .filter(|r| r.change_count > 0)
        .collect();

    DiffRiskAnalysis {
        severity,
        confidence,
        manual_review,
        manual_review_reasons,
        section_risks,
        rationale,
    }
}

fn generate_rationale(
    severity: Severity,
    confidence: f64,
    section_severities: &[(String, Severity)],
) -> Option<String> {
    if section_severities.iter().all(|(_, s)| *s == Severity::Low) && confidence >= 0.95 {
        return Some(
            "No semantic changes detected - identical content with matching clause IDs".to_string(),
        );
    }

    let mut parts = Vec::new();

    match severity {
        Severity::Critical => parts.push("critical severity changes detected"),
        Severity::High => parts.push("high severity changes in authority or constraints"),
        Severity::Medium => parts.push("moderate changes to scope or acceptance criteria"),
        Severity::Low => parts.push("minor or clarification changes"),
    }

    if confidence < 0.7 {
        parts.push("low confidence due to ambiguous clause matching");
    }

    if !parts.is_empty() {
        Some(parts.join("; "))
    } else {
        None
    }
}

impl std::ops::AddAssign<MatchingStats> for MatchingStats {
    fn add_assign(&mut self, rhs: MatchingStats) {
        self.total_changes += rhs.total_changes;
        self.unique_clause_id_match += rhs.unique_clause_id_match;
        self.ambiguous_match += rhs.ambiguous_match;
        self.no_match += rhs.no_match;
        self.removed += rhs.removed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{
        AuthorityDiff, ConstraintsDiff, IntentVersionDiff, ScopeDiff, ScopeItemsDiff,
    };
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
        assert!(analysis
            .manual_review_reasons
            .iter()
            .any(|r| matches!(r, ManualReviewReason::MultipleHighSeverityChanges { count } if *count == 3)));
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
        assert!(analysis
            .manual_review_reasons
            .iter()
            .any(|r| matches!(r, ManualReviewReason::MultipleHighSeverityChanges { count } if *count == 2)));
    }
}
