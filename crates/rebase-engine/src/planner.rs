//! Rebase planner module — preview-only baseline
//!
//! Phase 1 implements a preview-only planner that maps diff+risk analysis
//! to deterministic decision classes A-E.
//!
//! Checkpoint selection heuristic (PR #18):
//! - Generates candidates based on decision class and affected items
//! - Selects best checkpoint per heuristic rules (closest, before invalidation, etc.)
//! - Keeps `ready: false` since execution requires runtime adapter integration (Phase 2)
//!
//! This module does NOT include:
//! - Graph-based impact classification integration (requires graph HTTP API)
//! - Approval revalidation hooks (deferred to Phase 2+; baseline scaffolding present)
//! - Runtime adapter integration (Phase 2)
//!
//! The planner is deterministic: same diff+risk input always produces
//! the same decision class output.

use serde::{Deserialize, Serialize};

use crate::diff::IntentVersionDiff;
use crate::risk::{DiffRiskAnalysis, Severity};

// Re-export AffectedItemsPreview and ClassificationImpact from intent_rebase_types
pub use intent_rebase_types::{AffectedItemsPreview, ClassificationImpact, RiskTier};

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
/// PR #18: Added internal heuristic baseline that populates candidates/selected/rationale.
/// `ready` remains `false` because actual execution requires runtime adapter integration (Phase 2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckpointSelection {
    /// Whether checkpoint selection is ready to execute (false in Phase 1)
    pub ready: bool,
    /// Candidate checkpoint descriptions (populated by heuristic in PR #18)
    pub candidates: Vec<CheckpointCandidate>,
    /// Selected checkpoint (populated by heuristic in PR #18)
    pub selected: Option<CheckpointCandidate>,
    /// Selection rationale (populated by heuristic in PR #18)
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

    /// Phase 1 checkpoint selection heuristic baseline (PR #18)
    ///
    /// Internal heuristic that generates candidates and selects a checkpoint
    /// based on decision class and affected items analysis. Keeps `ready: false`
    /// because actual execution requires runtime adapter integration (Phase 2).
    ///
    /// Heuristic rules (per spec):
    /// - Prefer closest checkpoint (most recent)
    /// - Before first invalid node
    /// - Don't miss mandatory dependencies
    /// - Avoid rerunning irreversible side effects if not needed
    pub fn with_heuristic(
        decision_class: DecisionClass,
        affected_items: &AffectedItemsPreview,
    ) -> Self {
        let candidates = compute_checkpoint_candidates(decision_class, affected_items);
        let (selected, rationale) =
            select_best_checkpoint(&candidates, decision_class, affected_items);

        Self {
            ready: false, // Execution requires runtime adapter integration (Phase 2)
            candidates,
            selected,
            rationale: Some(rationale),
        }
    }
}

/// A candidate checkpoint for rebase resume
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Phase 1 heuristic baseline: populate approvals from affected items preview
    ///
    /// PR #18 pattern: populate internal candidates/logic while keeping `ready: false`.
    /// Execution requires runtime adapter integration (Phase 2).
    pub fn with_affected_approvals(
        decision_class: DecisionClass,
        affected_items: &AffectedItemsPreview,
    ) -> Self {
        let approvals_needing_revalidation: Vec<ApprovalNeedingRevalidation> = affected_items
            .affected_approvals
            .iter()
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

        let strategy = match decision_class {
            DecisionClass::A | DecisionClass::B => RevalidationStrategy::Drop,
            DecisionClass::C => RevalidationStrategy::Incremental,
            DecisionClass::D | DecisionClass::E => RevalidationStrategy::Full,
        };

        let rationale = if approvals_needing_revalidation.is_empty() {
            Some("No affected approvals detected".to_string())
        } else {
            Some(format!(
                "{} approval(s) need revalidation (class {:?}, strategy {:?})",
                approvals_needing_revalidation.len(),
                decision_class,
                strategy
            ))
        };

        Self {
            ready: false, // Execution requires runtime adapter integration (Phase 2)
            approvals_needing_revalidation,
            strategy,
            rationale,
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
#[derive(Default)]
pub enum RevalidationStrategy {
    /// Revalidation deferred to Phase 2
    #[default]
    Deferred,
    /// Full revalidation required
    Full,
    /// Incremental revalidation (only changed scope)
    Incremental,
    /// Stale approvals to be dropped
    Drop,
    /// Log change and notify approvers; no approval cancellation (Medium risk)
    LogNotify,
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

    /// Phase 1 heuristic baseline: populate compensation readiness from side effects
    ///
    /// PR #18 pattern: populate internal candidates/logic while keeping `ready: false`.
    /// Execution requires runtime adapter integration (Phase 2).
    pub fn with_side_effects(
        decision_class: DecisionClass,
        affected_items: &AffectedItemsPreview,
    ) -> Self {
        let has_irreversible_effects = affected_items.side_effects.iter().any(|item| {
            matches!(
                item.impact,
                ClassificationImpact::Direct | ClassificationImpact::Transitive
            )
        });

        let potential_actions: Vec<CompensationAction> = affected_items
            .side_effects
            .iter()
            .map(|item| CompensationAction {
                id: item.node_id.to_string(),
                label: item.label.clone(),
                description: item.reason.clone(),
                reversible: matches!(item.impact, ClassificationImpact::Unchanged),
                priority: match item.impact {
                    ClassificationImpact::Direct => 1,
                    ClassificationImpact::Transitive => 2,
                    ClassificationImpact::Unchanged => 3,
                },
            })
            .collect();

        let rationale = if potential_actions.is_empty() {
            Some("No side effects requiring compensation".to_string())
        } else {
            Some(format!(
                "{} potential compensation action(s) identified (irreversible: {}, class {:?})",
                potential_actions.len(),
                has_irreversible_effects,
                decision_class
            ))
        };

        Self {
            ready: false, // Execution requires runtime adapter integration (Phase 2)
            potential_actions,
            has_irreversible_effects,
            rationale,
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

/// Phase 3 Batch 1: Public compensation planning summary for API responses.
///
/// This is a read-only summary of compensation planning output from the rebase planner.
/// It exposes the skeleton/preview compensation data without claiming execution support.
///
/// **Distinction from actual compensation actions:**
/// - This summary represents planner-generated potential compensation actions
///   derived from affected items analysis during rebase preview
/// - Actual compensation actions (stored records) are queried via
///   `GET /intents/{intent_id}/compensation-actions` using CompensationActionService
///
/// **Execution readiness:**
/// - `ready: true` indicates full compensation planning is available
/// - `ready: false` (Phase 3 Batch 1) indicates compensation planning is deferred;
///   the action list will be empty and execution is not supported
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationPlanningSummary {
    /// Whether full compensation planning is ready to execute.
    /// When false (Phase 3 Batch 1), compensation planning is deferred and
    /// the potential_actions list will be empty.
    pub ready: bool,
    /// Potential compensation actions identified by the planner.
    /// Phase 3 Batch 1: This is a skeleton/preview list; actual execution
    /// requires Batch 1+ planner implementation.
    pub potential_actions: Vec<CompensationAction>,
    /// Whether any irreversible side effects are present in the affected items.
    /// When true, manual intervention may be required even for automatic compensation.
    pub has_irreversible_effects: bool,
    /// Human-readable rationale for compensation planning decisions.
    pub rationale: Option<String>,
}

impl From<&CompensationReadiness> for CompensationPlanningSummary {
    fn from(readiness: &CompensationReadiness) -> Self {
        Self {
            ready: readiness.ready,
            potential_actions: readiness.potential_actions.clone(),
            has_irreversible_effects: readiness.has_irreversible_effects,
            rationale: readiness.rationale.clone(),
        }
    }
}

/// Phase 1 status for features not yet implemented
///
/// PR #18: all three deferred fields now have internal heuristic baselines.
/// `ready` remains `false` for all because actual execution requires
/// runtime adapter integration (Phase 2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeferredFields {
    /// Checkpoint selection (PR #18: heuristic baseline, ready=false)
    pub checkpoint_selection: CheckpointSelection,
    /// Approval revalidation readiness (PR #18: heuristic baseline, ready=false)
    pub approval_revalidation: ApprovalRevalidation,
    /// Compensation action readiness (PR #18: heuristic baseline, ready=false)
    pub compensation: CompensationReadiness,
}

impl DeferredFields {
    /// Create new deferred fields with Phase 1 baseline
    ///
    /// PR #18: all three fields use heuristic baselines that populate
    /// internal candidates/logic while keeping `ready: false`.
    /// Execution requires runtime adapter integration (Phase 2).
    pub fn phase1_baseline(
        decision_class: DecisionClass,
        affected_items: &AffectedItemsPreview,
    ) -> Self {
        Self {
            checkpoint_selection: CheckpointSelection::with_heuristic(
                decision_class,
                affected_items,
            ),
            approval_revalidation: ApprovalRevalidation::with_affected_approvals(
                decision_class,
                affected_items,
            ),
            compensation: CompensationReadiness::with_side_effects(decision_class, affected_items),
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
/// analysis. PR #18 added internal checkpoint selection heuristic baseline:
/// - Generates candidates based on decision class and affected items
/// - Selects best checkpoint per heuristic rules
/// - `deferred.checkpoint_selection.ready` remains false (Phase 2 execution)
///
/// Phase 2b: `risk_tier` is the canonical public risk field, mapping from
/// `DiffRiskAnalysis.severity`. `risk_level` (u8 1-5) and `decision_class`
/// remain available as supporting fields.
///
/// Future PRs will enhance with:
/// - Graph-based affected node classification (already in rebase-preview)
/// - Approval revalidation hooks
/// - Compensation action generation
/// - Runtime adapter integration for actual apply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebasePlan {
    /// The assigned decision class (A-E)
    pub decision_class: DecisionClass,
    /// Human-readable decision rationale
    pub rationale: String,
    /// Section-level decisions
    pub section_decisions: Vec<SectionDecision>,
    /// Affected items preview (Phase 1 baseline: empty; deferred to Phase 2)
    pub affected_items: AffectedItemsPreview,
    /// Deferred fields (Phase 1 baseline: deferred placeholders)
    pub deferred: DeferredFields,
    /// Whether manual review is recommended
    pub manual_review_recommended: bool,
    /// Canonical public risk tier (Phase 2b): derived from `DiffRiskAnalysis.severity`.
    /// Use this as the primary public risk field in API responses.
    pub risk_tier: RiskTier,
    /// Risk level from 1-5 (1=lowest, 5=highest) — supporting field
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

        // Phase 2b: Canonical public risk_tier from DiffRiskAnalysis severity
        let risk_tier = risk.severity.to_risk_tier();

        Self {
            decision_class,
            rationale,
            section_decisions,
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: DeferredFields::phase1_baseline(
                decision_class,
                &AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended,
            risk_tier,
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
                "{:?} severity with {:.0}% confidence",
                risk.severity,
                risk.confidence * 100.0
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

/// Compute checkpoint candidates based on decision class and affected items
///
/// Phase 1 heuristic baseline: generates candidates that represent different
/// resume points depending on the decision class. Actual checkpoint data
/// requires runtime adapter integration (Phase 2).
fn compute_checkpoint_candidates(
    decision_class: DecisionClass,
    affected_items: &AffectedItemsPreview,
) -> Vec<CheckpointCandidate> {
    use intent_rebase_types::AffectedItemsStatus;

    // For Class A (no-op), no checkpoint needed
    if decision_class == DecisionClass::A {
        return vec![];
    }

    let mut candidates = Vec::new();

    // Check if we have graph-derived affected items
    let has_affected_items = affected_items.status == AffectedItemsStatus::Available
        && (!affected_items.affected_artifacts.is_empty()
            || !affected_items.affected_approvals.is_empty()
            || !affected_items.side_effects.is_empty());

    // Candidate 1: Most recent checkpoint (closest to current state)
    // This is the preferred candidate for soft review and partial repair
    candidates.push(CheckpointCandidate {
        id: "checkpoint-most-recent".to_string(),
        label: "Most Recent Checkpoint".to_string(),
        description: "Resume from the most recent checkpoint before changes".to_string(),
        validated: false, // Phase 2: would be true if runtime adapter confirms
    });

    // Candidate 2: Before first invalidation point
    // Preferred when there are affected items that may have invalid states
    if has_affected_items {
        candidates.push(CheckpointCandidate {
            id: "checkpoint-before-invalidation".to_string(),
            label: "Before First Invalidation".to_string(),
            description: "Resume before any affected items were invalidated by the changes"
                .to_string(),
            validated: false,
        });
    }

    // Candidate 3: Before side effects (for D-class decisions)
    // Preferred when compensation planning is needed
    if decision_class == DecisionClass::D || decision_class == DecisionClass::E {
        candidates.push(CheckpointCandidate {
            id: "checkpoint-before-side-effects".to_string(),
            label: "Before Side Effects".to_string(),
            description:
                "Resume before any irreversible side effects were triggered by the changes"
                    .to_string(),
            validated: false,
        });
    }

    // Candidate 4: Last known good state (for E-class hard restart)
    // Preferred when manual handoff is required
    if decision_class == DecisionClass::E {
        candidates.push(CheckpointCandidate {
            id: "checkpoint-last-known-good".to_string(),
            label: "Last Known Good State".to_string(),
            description: "Resume from the last fully validated state before issues began"
                .to_string(),
            validated: false,
        });
    }

    // Candidate 5: Minimal checkpoint (for C-class partial repair)
    // Preferred when only limited scope changes need repair
    if decision_class == DecisionClass::C {
        candidates.push(CheckpointCandidate {
            id: "checkpoint-minimal".to_string(),
            label: "Minimal Checkpoint".to_string(),
            description: "Resume at the minimal checkpoint needed to repair scope changes"
                .to_string(),
            validated: false,
        });
    }

    candidates
}

/// Select the best checkpoint from candidates based on heuristic rules
///
/// Selection rules (per spec):
/// - Prefer closest checkpoint (most recent)
/// - Before first invalid node
/// - Don't miss mandatory dependencies
/// - Avoid rerunning irreversible side effects if not needed
///
/// Returns (selected_checkpoint, rationale)
fn select_best_checkpoint(
    candidates: &[CheckpointCandidate],
    decision_class: DecisionClass,
    affected_items: &AffectedItemsPreview,
) -> (Option<CheckpointCandidate>, String) {
    // No candidates means no checkpoint needed (Class A)
    if candidates.is_empty() {
        return (
            None,
            "No checkpoint needed for Class A (no semantic changes)".to_string(),
        );
    }

    // For Class A, no checkpoint
    if decision_class == DecisionClass::A {
        return (
            None,
            "No checkpoint needed for Class A (no semantic changes)".to_string(),
        );
    }

    // Heuristic selection based on decision class
    let selected_id = match decision_class {
        // Class A: handled above
        DecisionClass::A => unreachable!(),
        // Class B: prefer most recent (minimal rollback)
        DecisionClass::B => "checkpoint-most-recent".to_string(),
        // Class C: prefer minimal checkpoint (limited scope repair)
        DecisionClass::C => {
            if candidates.iter().any(|c| c.id == "checkpoint-minimal") {
                "checkpoint-minimal".to_string()
            } else {
                "checkpoint-most-recent".to_string()
            }
        }
        // Class D: prefer before side effects (compensation planning)
        DecisionClass::D => {
            if candidates
                .iter()
                .any(|c| c.id == "checkpoint-before-side-effects")
            {
                "checkpoint-before-side-effects".to_string()
            } else if candidates
                .iter()
                .any(|c| c.id == "checkpoint-before-invalidation")
            {
                "checkpoint-before-invalidation".to_string()
            } else {
                "checkpoint-most-recent".to_string()
            }
        }
        // Class E: prefer last known good (manual handoff required)
        DecisionClass::E => {
            if candidates
                .iter()
                .any(|c| c.id == "checkpoint-last-known-good")
            {
                "checkpoint-last-known-good".to_string()
            } else if candidates
                .iter()
                .any(|c| c.id == "checkpoint-before-side-effects")
            {
                "checkpoint-before-side-effects".to_string()
            } else {
                "checkpoint-most-recent".to_string()
            }
        }
    };

    let selected = candidates.iter().find(|c| c.id == selected_id).cloned();
    let rationale = build_selection_rationale(decision_class, affected_items, &selected_id);

    (selected, rationale)
}

/// Build human-readable rationale for the checkpoint selection
fn build_selection_rationale(
    decision_class: DecisionClass,
    affected_items: &AffectedItemsPreview,
    selected_id: &str,
) -> String {
    use intent_rebase_types::AffectedItemsStatus;

    let has_affected_items = affected_items.status == AffectedItemsStatus::Available
        && (!affected_items.affected_artifacts.is_empty()
            || !affected_items.affected_approvals.is_empty()
            || !affected_items.side_effects.is_empty());

    let affected_count = if has_affected_items {
        affected_items.affected_artifacts.len()
            + affected_items.affected_approvals.len()
            + affected_items.side_effects.len()
    } else {
        0
    };

    let class_reason = match decision_class {
        DecisionClass::A => "no semantic changes".to_string(),
        DecisionClass::B => "soft review recommended".to_string(),
        DecisionClass::C => "partial repair candidate".to_string(),
        DecisionClass::D => "compensation and repair needed".to_string(),
        DecisionClass::E => "hard restart / manual handoff required".to_string(),
    };

    let checkpoint_reason = match selected_id {
        "checkpoint-most-recent" => "closest checkpoint minimizes re-execution".to_string(),
        "checkpoint-before-invalidation" => "ensures affected items are in valid state".to_string(),
        "checkpoint-before-side-effects" => {
            "avoids irreversible side effect re-execution".to_string()
        }
        "checkpoint-last-known-good" => "provides clean state for manual review".to_string(),
        "checkpoint-minimal" => "minimizes scope of repair".to_string(),
        _ => "fallback to most recent checkpoint".to_string(),
    };

    if affected_count > 0 {
        format!(
            "Selected {} for {} with {} affected item(s). {}",
            selected_id.replace("checkpoint-", "").replace("-", " "),
            class_reason,
            affected_count,
            checkpoint_reason
        )
    } else {
        format!(
            "Selected {} for {}. {}",
            selected_id.replace("checkpoint-", "").replace("-", " "),
            class_reason,
            checkpoint_reason
        )
    }
}
