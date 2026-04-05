//! Rebase planner module — preview-only baseline
//!
//! Phase 1 implements a preview-only planner that maps diff+risk analysis
//! to deterministic decision classes A-E.
//!
//! This module does NOT include:
//! - Runtime-backed checkpoint discovery/execution
//! - Approval revalidation hooks
//! - Runtime adapter integration (Phase 2)
//!
//! The planner is deterministic: same diff+risk input always produces
//! the same decision class output.

use serde::{Deserialize, Serialize};

use crate::diff::IntentVersionDiff;
use crate::risk::{DiffRiskAnalysis, Severity};

// Re-export AffectedItemsPreview from intent_rebase_types for use in RebasePlan
pub use intent_rebase_types::AffectedItemsPreview;

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

/// Checkpoint selection readiness for apply phase
///
/// Phase 1 groundwork: execution remains deferred.
///
/// PR #18 adds an internal heuristic baseline that can rank checkpoint strategy
/// hints by decision class, while Phase 2+ will replace those hints with
/// runtime-backed checkpoint discovery and execution logic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckpointSelection {
    /// Whether checkpoint selection is ready to execute
    pub ready: bool,
    /// Candidate checkpoint strategy hints (runtime-backed candidates remain Phase 2)
    pub candidates: Vec<CheckpointCandidate>,
    /// Selected internal checkpoint hint (runtime-backed selection remains Phase 2)
    pub selected: Option<CheckpointCandidate>,
    /// Selection rationale for the internal baseline
    pub rationale: Option<String>,
}

impl CheckpointSelection {
    /// Phase 1 baseline: checkpoint selection not yet ready
    pub fn deferred() -> Self {
        Self {
            ready: false,
            candidates: vec![],
            selected: None,
            rationale: None,
        }
    }

    /// Internal checkpoint heuristic baseline.
    ///
    /// This remains non-executable (`ready=false`) until runtime adapter support
    /// exists, but it can surface deterministic internal hints about which
    /// checkpoint strategy would be preferred for a given decision class.
    pub fn heuristic_baseline(decision_class: DecisionClass) -> Self {
        let candidates = compute_checkpoint_candidates(decision_class);
        let selected = select_best_checkpoint(decision_class, &candidates);
        let rationale = build_checkpoint_selection_rationale(decision_class, selected.as_ref());

        Self {
            ready: false,
            candidates,
            selected,
            rationale,
        }
    }
}

/// A candidate checkpoint for rebase resume
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointCandidate {
    /// Checkpoint identifier
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Description of what state this checkpoint captures
    pub description: String,
    /// Whether this checkpoint has been validated
    pub validated: bool,
}

/// Approval revalidation readiness for apply phase
///
/// Phase 1 groundwork: typed placeholder indicating approval revalidation is deferred.
/// Phase 2+ will replace this with actual revalidation logic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApprovalRevalidation {
    /// Whether approval revalidation is ready to execute
    pub ready: bool,
    /// Approvals that need revalidation (populated in Phase 2)
    pub approvals_needing_revalidation: Vec<ApprovalNeedingRevalidation>,
    /// Revalidation strategy hint
    pub strategy: RevalidationStrategy,
    /// Detailed rationale for revalidation decisions
    pub rationale: Option<String>,
}

impl ApprovalRevalidation {
    /// Phase 1 baseline: approval revalidation not yet ready
    pub fn deferred() -> Self {
        Self {
            ready: false,
            approvals_needing_revalidation: vec![],
            strategy: RevalidationStrategy::Deferred,
            rationale: None,
        }
    }
}

/// An approval that may need revalidation due to intent changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalNeedingRevalidation {
    /// Approval node ID
    pub node_id: String,
    /// Human-readable label
    pub label: String,
    /// Original approval rule that was satisfied
    pub original_rule_id: String,
    /// Reason why revalidation may be needed
    pub reason: String,
}

/// Strategy for approval revalidation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationStrategy {
    /// Revalidation deferred to Phase 2
    Deferred,
    /// Full revalidation required
    Full,
    /// Incremental revalidation (only changed scope)
    Incremental,
    /// Stale approvals to be dropped
    Drop,
}

impl Default for RevalidationStrategy {
    fn default() -> Self {
        RevalidationStrategy::Deferred
    }
}

/// Compensation action readiness for apply phase
///
/// Phase 1 groundwork: typed placeholder indicating compensation planning is deferred.
/// Phase 2+ will replace this with actual compensation action generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompensationReadiness {
    /// Whether compensation planning is ready to execute
    pub ready: bool,
    /// Potential compensation actions identified (populated in Phase 2)
    pub potential_actions: Vec<CompensationAction>,
    /// Whether any irreversible side effects are present
    pub has_irreversible_effects: bool,
    /// Detailed rationale for compensation decisions
    pub rationale: Option<String>,
}

impl CompensationReadiness {
    /// Phase 1 baseline: compensation planning not yet ready
    pub fn deferred() -> Self {
        Self {
            ready: false,
            potential_actions: vec![],
            has_irreversible_effects: false,
            rationale: None,
        }
    }
}

/// A potential compensation action to mitigate side effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationAction {
    /// Action identifier
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Description of the action
    pub description: String,
    /// Whether this action is reversible
    pub reversible: bool,
    /// Priority (lower = higher priority)
    pub priority: u8,
}

/// Phase 1 status for features not yet implemented
///
/// These fields are spec-adjacent but deferred to Phase 2+.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeferredFields {
    /// Checkpoint selection readiness (Phase 2)
    pub checkpoint_selection: CheckpointSelection,
    /// Approval revalidation readiness (Phase 2)
    pub approval_revalidation: ApprovalRevalidation,
    /// Compensation action readiness (Phase 2)
    pub compensation: CompensationReadiness,
}

impl DeferredFields {
    /// Create new deferred fields with Phase 1 baseline
    pub fn phase1_baseline() -> Self {
        Self::phase1_baseline_for(DecisionClass::A)
    }

    /// Create new deferred fields with Phase 1 baseline plus internal
    /// checkpoint-selection hints for the computed decision class.
    pub fn phase1_baseline_for(decision_class: DecisionClass) -> Self {
        Self {
            checkpoint_selection: CheckpointSelection::heuristic_baseline(decision_class),
            approval_revalidation: ApprovalRevalidation::deferred(),
            compensation: CompensationReadiness::deferred(),
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

/// Complete rebase plan output from the planner
///
/// Phase 1 baseline provides typed decision class mapping from diff+risk
/// analysis, graph-integrated affected items, and internal checkpoint heuristic
/// hints. Future PRs will enhance with:
/// - Runtime-backed checkpoint lookup/execution
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
    /// Affected items preview (graph integration may populate this downstream)
    pub affected_items: AffectedItemsPreview,
    /// Deferred internal apply/readiness fields
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
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: DeferredFields::phase1_baseline_for(decision_class),
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

fn compute_checkpoint_candidates(decision_class: DecisionClass) -> Vec<CheckpointCandidate> {
    match decision_class {
        DecisionClass::A | DecisionClass::B => vec![],
        DecisionClass::C => vec![
            CheckpointCandidate {
                id: "nearest-validated".to_string(),
                label: "Nearest validated checkpoint".to_string(),
                description:
                    "Resume from the nearest validated checkpoint before the first invalidated node."
                        .to_string(),
                validated: true,
            },
            CheckpointCandidate {
                id: "last-known-good".to_string(),
                label: "Last known good checkpoint".to_string(),
                description:
                    "Fallback checkpoint that preserves required dependencies while limiting reruns."
                        .to_string(),
                validated: true,
            },
            CheckpointCandidate {
                id: "minimal-rerun-boundary".to_string(),
                label: "Minimal rerun boundary".to_string(),
                description:
                    "Fallback boundary when a validated checkpoint is unavailable but reruns should stay narrow."
                        .to_string(),
                validated: false,
            },
        ],
        DecisionClass::D => vec![
            CheckpointCandidate {
                id: "pre-side-effect".to_string(),
                label: "Checkpoint before side effects".to_string(),
                description:
                    "Prefer a checkpoint before irreversible or compensating side effects when available."
                        .to_string(),
                validated: true,
            },
            CheckpointCandidate {
                id: "before-invalidated-node".to_string(),
                label: "Checkpoint before first invalidated node".to_string(),
                description:
                    "Fallback checkpoint immediately before the first invalidated node in the repair path."
                        .to_string(),
                validated: true,
            },
            CheckpointCandidate {
                id: "last-known-good".to_string(),
                label: "Last known good checkpoint".to_string(),
                description:
                    "Broader rollback point that favors dependency completeness over minimal reruns."
                        .to_string(),
                validated: true,
            },
        ],
        DecisionClass::E => vec![CheckpointCandidate {
            id: "manual-handoff-boundary".to_string(),
            label: "Manual handoff boundary".to_string(),
            description:
                "Execution restart boundary must be confirmed manually before any runtime apply path is attempted."
                    .to_string(),
            validated: false,
        }],
    }
}

fn select_best_checkpoint(
    decision_class: DecisionClass,
    candidates: &[CheckpointCandidate],
) -> Option<CheckpointCandidate> {
    match decision_class {
        DecisionClass::A | DecisionClass::B => None,
        DecisionClass::C => candidates
            .iter()
            .find(|candidate| candidate.id == "nearest-validated")
            .cloned()
            .or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.id == "last-known-good")
                    .cloned()
            })
            .or_else(|| candidates.first().cloned()),
        DecisionClass::D => candidates
            .iter()
            .find(|candidate| candidate.id == "pre-side-effect")
            .cloned()
            .or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.id == "before-invalidated-node")
                    .cloned()
            })
            .or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.id == "last-known-good")
                    .cloned()
            })
            .or_else(|| candidates.first().cloned()),
        DecisionClass::E => None,
    }
}

fn build_checkpoint_selection_rationale(
    decision_class: DecisionClass,
    selected: Option<&CheckpointCandidate>,
) -> Option<String> {
    match decision_class {
        DecisionClass::A => None,
        DecisionClass::B => Some(
            "Soft-review path: no rerun checkpoint is suggested by the internal baseline yet."
                .to_string(),
        ),
        DecisionClass::C => selected.map(|candidate| {
            format!(
                "Class C favors the nearest safe checkpoint before the first invalidated node; selected internal hint '{}'.",
                candidate.label
            )
        }),
        DecisionClass::D => selected.map(|candidate| {
            format!(
                "Class D favors a checkpoint before irreversible work or the first invalidated node; selected internal hint '{}'.",
                candidate.label
            )
        }),
        DecisionClass::E => Some(
            "Class E requires manual handoff; the internal baseline does not auto-select a restart checkpoint."
                .to_string(),
        ),
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

        // Phase 1 groundwork: typed placeholders with ready=false
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
        assert!(plan.deferred.checkpoint_selection.candidates.is_empty());
        assert!(plan
            .deferred
            .approval_revalidation
            .approvals_needing_revalidation
            .is_empty());
        assert!(plan.deferred.compensation.potential_actions.is_empty());

        // Selected/rationale should be None for deferred state
        assert!(plan.deferred.checkpoint_selection.selected.is_none());
        assert!(plan.deferred.checkpoint_selection.rationale.is_none());
        assert!(plan.deferred.approval_revalidation.rationale.is_none());
        assert!(plan.deferred.compensation.rationale.is_none());
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
    fn test_checkpoint_selection_heuristic_class_a_is_empty() {
        let cs = CheckpointSelection::heuristic_baseline(DecisionClass::A);

        assert!(!cs.ready);
        assert!(cs.candidates.is_empty());
        assert!(cs.selected.is_none());
        assert!(cs.rationale.is_none());
    }

    #[test]
    fn test_checkpoint_selection_heuristic_class_b_skips_checkpoint_selection() {
        let cs = CheckpointSelection::heuristic_baseline(DecisionClass::B);

        assert!(!cs.ready);
        assert!(cs.candidates.is_empty());
        assert!(cs.selected.is_none());
        assert_eq!(
            cs.rationale.as_deref(),
            Some(
                "Soft-review path: no rerun checkpoint is suggested by the internal baseline yet."
            )
        );
    }

    #[test]
    fn test_checkpoint_selection_heuristic_class_c_prefers_nearest_validated() {
        let cs = CheckpointSelection::heuristic_baseline(DecisionClass::C);

        assert!(!cs.ready);
        assert_eq!(cs.candidates.len(), 3);
        assert_eq!(
            cs.selected.as_ref().map(|candidate| candidate.id.as_str()),
            Some("nearest-validated")
        );
        assert!(cs
            .candidates
            .iter()
            .any(|candidate| candidate.id == "last-known-good"));
        assert!(cs
            .rationale
            .as_deref()
            .is_some_and(|rationale| rationale.contains("nearest safe checkpoint")));
    }

    #[test]
    fn test_checkpoint_selection_heuristic_class_d_prefers_pre_side_effect() {
        let cs = CheckpointSelection::heuristic_baseline(DecisionClass::D);

        assert!(!cs.ready);
        assert_eq!(cs.candidates.len(), 3);
        assert_eq!(
            cs.selected.as_ref().map(|candidate| candidate.id.as_str()),
            Some("pre-side-effect")
        );
        assert!(cs
            .rationale
            .as_deref()
            .is_some_and(|rationale| rationale.contains("irreversible work")));
    }

    #[test]
    fn test_checkpoint_selection_heuristic_class_e_requires_manual_handoff() {
        let cs = CheckpointSelection::heuristic_baseline(DecisionClass::E);

        assert!(!cs.ready);
        assert_eq!(cs.candidates.len(), 1);
        assert_eq!(cs.candidates[0].id, "manual-handoff-boundary");
        assert!(cs.selected.is_none());
        assert!(cs
            .rationale
            .as_deref()
            .is_some_and(|rationale| rationale.contains("manual handoff")));
    }

    #[test]
    fn test_rebase_plan_populates_internal_checkpoint_hint_for_class_c() {
        let mut diff = empty_intent_version_diff();
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
        assert!(!plan.deferred.checkpoint_selection.ready);
        assert_eq!(
            plan.deferred
                .checkpoint_selection
                .selected
                .as_ref()
                .map(|candidate| candidate.id.as_str()),
            Some("nearest-validated")
        );
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
}
