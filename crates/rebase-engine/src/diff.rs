//! Semantic diff module for intent versions
//!
//! This module implements deterministic structured diff for the following sections:
//! - scope
//! - constraints
//! - acceptance_criteria
//! - authority
//!
//! Conservative matching rules:
//! - Prefer clause_id matching when available
//! - Fallback to add/remove when identity is ambiguous (no clause_id or content match)
//! - Output ordering is deterministic (sorted by section, then by clause_id or content)

use intent_rebase_types::{
    AcceptanceCriterion, ActionRef, ApprovalRuleRef, ClauseType, Constraint, IntentAuthority,
    IntentConstraints, IntentScope,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents the type of change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Removed,
    Modified,
}

/// A single change to a constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintDiff {
    pub clause_id: Option<Uuid>,
    pub change_type: ChangeType,
    pub constraint_type: ClauseType,
    pub key: String,
    /// Present if change_type is Added or Modified
    pub after: Option<Box<Constraint>>,
    /// Present if change_type is Removed or Modified
    pub before: Option<Box<Constraint>>,
}

/// Diff for the constraints section
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConstraintsDiff {
    #[serde(rename = "functional")]
    pub functional: Vec<ConstraintDiff>,
    #[serde(rename = "non_functional")]
    pub non_functional: Vec<ConstraintDiff>,
    #[serde(rename = "policy")]
    pub policy: Vec<ConstraintDiff>,
    #[serde(rename = "budget")]
    pub budget: Vec<ConstraintDiff>,
    #[serde(rename = "time")]
    pub time: Vec<ConstraintDiff>,
}

/// A single acceptance criterion change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriterionDiff {
    pub clause_id: Option<Uuid>,
    pub change_type: ChangeType,
    pub priority: String,
    /// Present if change_type is Added or Modified
    pub after: Option<Box<AcceptanceCriterion>>,
    /// Present if change_type is Removed or Modified
    pub before: Option<Box<AcceptanceCriterion>>,
}

/// Diff for the acceptance criteria section
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcceptanceCriteriaDiff {
    pub required: Vec<AcceptanceCriterionDiff>,
    pub optional: Vec<AcceptanceCriterionDiff>,
}

/// A single action reference change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRefDiff {
    pub change_type: ChangeType,
    pub action: String,
    pub target: Option<String>,
    pub before: Option<Box<ActionRef>>,
    pub after: Option<Box<ActionRef>>,
}

/// A single approval rule change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRuleDiff {
    pub change_type: ChangeType,
    pub rule_id: String,
    pub description: String,
    pub before: Option<Box<ApprovalRuleRef>>,
    pub after: Option<Box<ApprovalRuleRef>>,
}

/// Diff for the authority section
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthorityDiff {
    #[serde(rename = "allowed_actions")]
    pub allowed_actions: Vec<ActionRefDiff>,
    #[serde(rename = "forbidden_actions")]
    pub forbidden_actions: Vec<ActionRefDiff>,
    #[serde(rename = "approval_requirements")]
    pub approval_requirements: Vec<ApprovalRuleDiff>,
}

/// Diff for the scope section (in_scope and out_of_scope items)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeDiff {
    #[serde(rename = "in_scope")]
    pub in_scope: ScopeItemsDiff,
    #[serde(rename = "out_of_scope")]
    pub out_of_scope: ScopeItemsDiff,
}

/// Diff for scope items (added and removed items)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeItemsDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Complete diff for an intent version change
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentVersionDiff {
    pub scope: ScopeDiff,
    pub constraints: ConstraintsDiff,
    #[serde(rename = "acceptance_criteria")]
    pub acceptance_criteria: AcceptanceCriteriaDiff,
    pub authority: AuthorityDiff,
}

/// Compute deterministic diff between two intent scopes
pub fn diff_scope(before: &IntentScope, after: &IntentScope) -> ScopeDiff {
    ScopeDiff {
        in_scope: diff_string_lists(&before.in_scope, &after.in_scope),
        out_of_scope: diff_string_lists(&before.out_of_scope, &after.out_of_scope),
    }
}

/// Compute added/removed between two sorted string lists deterministically
fn diff_string_lists(before: &[String], after: &[String]) -> ScopeItemsDiff {
    let mut before_sorted = before.to_vec();
    let mut after_sorted = after.to_vec();
    before_sorted.sort();
    after_sorted.sort();

    let mut added = Vec::new();
    let mut removed = Vec::new();

    let mut before_idx = 0;
    let mut after_idx = 0;

    while before_idx < before_sorted.len() || after_idx < after_sorted.len() {
        let before_has_more = before_idx < before_sorted.len();
        let after_has_more = after_idx < after_sorted.len();

        match (before_has_more, after_has_more) {
            (true, false) => {
                removed.push(before_sorted[before_idx].clone());
                before_idx += 1;
            }
            (false, true) => {
                added.push(after_sorted[after_idx].clone());
                after_idx += 1;
            }
            (true, true) => {
                match before_sorted[before_idx].cmp(&after_sorted[after_idx]) {
                    std::cmp::Ordering::Less => {
                        removed.push(before_sorted[before_idx].clone());
                        before_idx += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        added.push(after_sorted[after_idx].clone());
                        after_idx += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        // Unchanged - no diff entry
                        before_idx += 1;
                        after_idx += 1;
                    }
                }
            }
            (false, false) => break,
        }
    }

    ScopeItemsDiff { added, removed }
}

/// Compute deterministic diff between two constraint lists
///
/// Conservative matching rules:
/// - clause_id match (Some): match if both have same Some id AND id is unique in both lists
/// - clause_id is None OR duplicate keys: fall back to add/remove (never speculative modify)
///   because we cannot reliably identify which items match
pub(crate) fn diff_constraints(before: &[Constraint], after: &[Constraint]) -> Vec<ConstraintDiff> {
    let mut diffs = Vec::new();

    // Count occurrences of each clause_id to detect duplicates
    let mut before_id_count: std::collections::HashMap<Option<Uuid>, usize> =
        std::collections::HashMap::new();
    for c in before {
        *before_id_count.entry(c.clause_id).or_insert(0) += 1;
    }

    let mut after_id_count: std::collections::HashMap<Option<Uuid>, usize> =
        std::collections::HashMap::new();
    for c in after {
        *after_id_count.entry(c.clause_id).or_insert(0) += 1;
    }

    // Collect items with unique Some clause_ids for matching
    // Items with None or duplicate clause_ids are handled conservatively
    let mut before_unique: std::collections::VecDeque<&Constraint> =
        std::collections::VecDeque::new();
    let mut before_ambiguous: Vec<&Constraint> = Vec::new();
    for c in before {
        match c.clause_id {
            Some(id) if before_id_count.get(&Some(id)) == Some(&1) => {
                before_unique.push_back(c);
            }
            _ => {
                before_ambiguous.push(c);
            }
        }
    }

    let mut after_unique: std::collections::VecDeque<&Constraint> =
        std::collections::VecDeque::new();
    let mut after_ambiguous: Vec<&Constraint> = Vec::new();
    for c in after {
        match c.clause_id {
            Some(id) if after_id_count.get(&Some(id)) == Some(&1) => {
                after_unique.push_back(c);
            }
            _ => {
                after_ambiguous.push(c);
            }
        }
    }

    // Match unique items by clause_id
    let mut matched_before: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let mut matched_after: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    // Build maps for matching unique items only
    let mut before_by_unique_id: std::collections::HashMap<Uuid, &Constraint> =
        std::collections::HashMap::new();
    for c in &before_unique {
        if let Some(id) = c.clause_id {
            before_by_unique_id.insert(id, c);
        }
    }

    let mut after_by_unique_id: std::collections::HashMap<Uuid, &Constraint> =
        std::collections::HashMap::new();
    for c in &after_unique {
        if let Some(id) = c.clause_id {
            after_by_unique_id.insert(id, c);
        }
    }

    // Match by clause_id
    for (id, before_c) in &before_by_unique_id {
        if let Some(after_c) = after_by_unique_id.get(id) {
            if !constraints_equal(before_c, after_c) {
                diffs.push(ConstraintDiff {
                    clause_id: Some(*id),
                    change_type: ChangeType::Modified,
                    constraint_type: before_c.constraint_type.clone(),
                    key: before_c.key.clone(),
                    before: Some(Box::new((*before_c).clone())),
                    after: Some(Box::new((**after_c).clone())),
                });
            }
            // else: identical, no diff
            matched_before.insert(*id);
            matched_after.insert(*id);
        }
    }

    // Unmatched unique items in before are Removed
    for (id, before_c) in &before_by_unique_id {
        if !matched_before.contains(id) {
            diffs.push(ConstraintDiff {
                clause_id: Some(*id),
                change_type: ChangeType::Removed,
                constraint_type: before_c.constraint_type.clone(),
                key: before_c.key.clone(),
                before: Some(Box::new((*before_c).clone())),
                after: None,
            });
        }
    }

    // Unmatched unique items in after are Added
    for (id, after_c) in &after_by_unique_id {
        if !matched_after.contains(id) {
            diffs.push(ConstraintDiff {
                clause_id: Some(*id),
                change_type: ChangeType::Added,
                constraint_type: after_c.constraint_type.clone(),
                key: after_c.key.clone(),
                before: None,
                after: Some(Box::new((*after_c).clone())),
            });
        }
    }

    // Handle ambiguous items (None clause_id OR duplicate clause_ids):
    // Conservative approach: ALL are Removed from before, ALL are Added to after
    // Never speculative modify because we can't reliably match them
    for before_c in &before_ambiguous {
        diffs.push(ConstraintDiff {
            clause_id: before_c.clause_id,
            change_type: ChangeType::Removed,
            constraint_type: before_c.constraint_type.clone(),
            key: before_c.key.clone(),
            before: Some(Box::new((*before_c).clone())),
            after: None,
        });
    }

    for after_c in &after_ambiguous {
        diffs.push(ConstraintDiff {
            clause_id: after_c.clause_id,
            change_type: ChangeType::Added,
            constraint_type: after_c.constraint_type.clone(),
            key: after_c.key.clone(),
            before: None,
            after: Some(Box::new((*after_c).clone())),
        });
    }

    // Sort by clause_id for deterministic output (None at end)
    diffs.sort_by_key(|d| d.clause_id);

    diffs
}

/// Check if two constraints are equal by content
fn constraints_equal(a: &Constraint, b: &Constraint) -> bool {
    a.key == b.key
        && a.operator == b.operator
        && a.value == b.value
        && a.constraint_type == b.constraint_type
        && a.priority == b.priority
        && a.rationale == b.rationale
}

/// Create a content-based signature for constraint matching when clause_id is missing
#[allow(dead_code)]
fn constraint_signature(c: &Constraint) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        c.key, c.operator, c.value, c.constraint_type
    )
}

/// Compute deterministic diff between two constraint sections
pub fn diff_constraints_section(
    before: &IntentConstraints,
    after: &IntentConstraints,
) -> ConstraintsDiff {
    ConstraintsDiff {
        functional: diff_constraints(&before.functional, &after.functional),
        non_functional: diff_constraints(&before.non_functional, &after.non_functional),
        policy: diff_constraints(&before.policy, &after.policy),
        budget: diff_constraints(&before.budget, &after.budget),
        time: diff_constraints(&before.time, &after.time),
    }
}

/// Compute deterministic diff between two acceptance criterion lists
///
/// Conservative matching rules:
/// - clause_id match (Some): match if both have same Some id AND id is unique in both lists
/// - clause_id is None OR duplicate keys: fall back to add/remove (never speculative modify)
///   because we cannot reliably identify which items match
pub(crate) fn diff_acceptance_criteria(
    before: &[AcceptanceCriterion],
    after: &[AcceptanceCriterion],
) -> Vec<AcceptanceCriterionDiff> {
    let mut diffs = Vec::new();

    // Count occurrences of each clause_id to detect duplicates
    let mut before_id_count: std::collections::HashMap<Option<Uuid>, usize> =
        std::collections::HashMap::new();
    for c in before {
        *before_id_count.entry(c.clause_id).or_insert(0) += 1;
    }

    let mut after_id_count: std::collections::HashMap<Option<Uuid>, usize> =
        std::collections::HashMap::new();
    for c in after {
        *after_id_count.entry(c.clause_id).or_insert(0) += 1;
    }

    // Collect items with unique Some clause_ids for matching
    // Items with None or duplicate clause_ids are handled conservatively
    let mut before_unique: std::collections::VecDeque<&AcceptanceCriterion> =
        std::collections::VecDeque::new();
    let mut before_ambiguous: Vec<&AcceptanceCriterion> = Vec::new();
    for c in before {
        match c.clause_id {
            Some(id) if before_id_count.get(&Some(id)) == Some(&1) => {
                before_unique.push_back(c);
            }
            _ => {
                before_ambiguous.push(c);
            }
        }
    }

    let mut after_unique: std::collections::VecDeque<&AcceptanceCriterion> =
        std::collections::VecDeque::new();
    let mut after_ambiguous: Vec<&AcceptanceCriterion> = Vec::new();
    for c in after {
        match c.clause_id {
            Some(id) if after_id_count.get(&Some(id)) == Some(&1) => {
                after_unique.push_back(c);
            }
            _ => {
                after_ambiguous.push(c);
            }
        }
    }

    // Match unique items by clause_id
    let mut matched_before: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let mut matched_after: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    // Build maps for matching unique items only
    let mut before_by_unique_id: std::collections::HashMap<Uuid, &AcceptanceCriterion> =
        std::collections::HashMap::new();
    for c in &before_unique {
        if let Some(id) = c.clause_id {
            before_by_unique_id.insert(id, c);
        }
    }

    let mut after_by_unique_id: std::collections::HashMap<Uuid, &AcceptanceCriterion> =
        std::collections::HashMap::new();
    for c in &after_unique {
        if let Some(id) = c.clause_id {
            after_by_unique_id.insert(id, c);
        }
    }

    // Match by clause_id
    for (id, before_c) in &before_by_unique_id {
        if let Some(after_c) = after_by_unique_id.get(id) {
            if !acceptance_criteria_equal(before_c, after_c) {
                diffs.push(AcceptanceCriterionDiff {
                    clause_id: Some(*id),
                    change_type: ChangeType::Modified,
                    priority: format!("{:?}", before_c.priority),
                    before: Some(Box::new((*before_c).clone())),
                    after: Some(Box::new((**after_c).clone())),
                });
            }
            matched_before.insert(*id);
            matched_after.insert(*id);
        }
    }

    // Unmatched unique items in before are Removed
    for (id, c) in &before_by_unique_id {
        if !matched_before.contains(id) {
            diffs.push(AcceptanceCriterionDiff {
                clause_id: Some(*id),
                change_type: ChangeType::Removed,
                priority: format!("{:?}", c.priority),
                before: Some(Box::new((*c).clone())),
                after: None,
            });
        }
    }

    // Unmatched unique items in after are Added
    for (id, c) in &after_by_unique_id {
        if !matched_after.contains(id) {
            diffs.push(AcceptanceCriterionDiff {
                clause_id: Some(*id),
                change_type: ChangeType::Added,
                priority: format!("{:?}", c.priority),
                before: None,
                after: Some(Box::new((*c).clone())),
            });
        }
    }

    // Handle ambiguous items (None clause_id OR duplicate clause_ids):
    // Conservative approach: ALL are Removed from before, ALL are Added to after
    // Never speculative modify because we can't reliably match them
    for before_c in &before_ambiguous {
        diffs.push(AcceptanceCriterionDiff {
            clause_id: before_c.clause_id,
            change_type: ChangeType::Removed,
            priority: format!("{:?}", before_c.priority),
            before: Some(Box::new((*before_c).clone())),
            after: None,
        });
    }

    for after_c in &after_ambiguous {
        diffs.push(AcceptanceCriterionDiff {
            clause_id: after_c.clause_id,
            change_type: ChangeType::Added,
            priority: format!("{:?}", after_c.priority),
            before: None,
            after: Some(Box::new((*after_c).clone())),
        });
    }

    // Sort by clause_id for deterministic output (None at end)
    diffs.sort_by_key(|d| d.clause_id);

    diffs
}

/// Check if two acceptance criteria are equal
fn acceptance_criteria_equal(a: &AcceptanceCriterion, b: &AcceptanceCriterion) -> bool {
    a.description == b.description && a.priority == b.priority
}

/// Compute deterministic diff between two acceptance criteria sections
pub fn diff_acceptance_criteria_section(
    before: &intent_rebase_types::AcceptanceCriteria,
    after: &intent_rebase_types::AcceptanceCriteria,
) -> AcceptanceCriteriaDiff {
    AcceptanceCriteriaDiff {
        required: diff_acceptance_criteria(&before.required, &after.required),
        optional: diff_acceptance_criteria(&before.optional, &after.optional),
    }
}

/// Compute deterministic diff between two action ref lists
///
/// Conservative matching rules:
/// - (action, target) match: match if key is unique in both before and after
/// - Duplicate keys: fall back to add/remove (never speculative modify)
///   because we cannot reliably identify which items match
pub(crate) fn diff_action_refs(before: &[ActionRef], after: &[ActionRef]) -> Vec<ActionRefDiff> {
    let mut diffs = Vec::new();

    // Count occurrences of each (action, target) key to detect duplicates
    let mut before_key_count: std::collections::HashMap<(String, Option<String>), usize> =
        std::collections::HashMap::new();
    for a in before {
        *before_key_count
            .entry((a.action.clone(), a.target.clone()))
            .or_insert(0) += 1;
    }

    let mut after_key_count: std::collections::HashMap<(String, Option<String>), usize> =
        std::collections::HashMap::new();
    for a in after {
        *after_key_count
            .entry((a.action.clone(), a.target.clone()))
            .or_insert(0) += 1;
    }

    // Collect items with unique keys for matching
    // Items with duplicate keys are handled conservatively
    let mut before_unique: Vec<&ActionRef> = Vec::new();
    let mut before_ambiguous: Vec<&ActionRef> = Vec::new();
    for a in before {
        let key = (a.action.clone(), a.target.clone());
        match before_key_count.get(&key) {
            Some(&1) => before_unique.push(a),
            _ => before_ambiguous.push(a),
        }
    }

    let mut after_unique: Vec<&ActionRef> = Vec::new();
    let mut after_ambiguous: Vec<&ActionRef> = Vec::new();
    for a in after {
        let key = (a.action.clone(), a.target.clone());
        match after_key_count.get(&key) {
            Some(&1) => after_unique.push(a),
            _ => after_ambiguous.push(a),
        }
    }

    // Build maps for matching unique items only
    let mut before_by_unique_key: std::collections::HashMap<(String, Option<String>), &ActionRef> =
        std::collections::HashMap::new();
    for a in &before_unique {
        before_by_unique_key.insert((a.action.clone(), a.target.clone()), a);
    }

    let mut after_by_unique_key: std::collections::HashMap<(String, Option<String>), &ActionRef> =
        std::collections::HashMap::new();
    for a in &after_unique {
        after_by_unique_key.insert((a.action.clone(), a.target.clone()), a);
    }

    let mut matched_before: std::collections::HashSet<(String, Option<String>)> =
        std::collections::HashSet::new();
    let mut matched_after: std::collections::HashSet<(String, Option<String>)> =
        std::collections::HashSet::new();

    // Match unique items by action+target
    for (key, before_a) in &before_by_unique_key {
        if let Some(after_a) = after_by_unique_key.get(key) {
            if !action_refs_equal(before_a, after_a) {
                diffs.push(ActionRefDiff {
                    change_type: ChangeType::Modified,
                    action: before_a.action.clone(),
                    target: before_a.target.clone(),
                    before: Some(Box::new((**before_a).clone())),
                    after: Some(Box::new((**after_a).clone())),
                });
            }
            matched_before.insert(key.clone());
            matched_after.insert(key.clone());
        }
    }

    // Unmatched unique items in before are Removed
    for (key, a) in &before_by_unique_key {
        if !matched_before.contains(key) {
            diffs.push(ActionRefDiff {
                change_type: ChangeType::Removed,
                action: a.action.clone(),
                target: a.target.clone(),
                before: Some(Box::new((**a).clone())),
                after: None,
            });
        }
    }

    // Unmatched unique items in after are Added
    for (key, a) in &after_by_unique_key {
        if !matched_after.contains(key) {
            diffs.push(ActionRefDiff {
                change_type: ChangeType::Added,
                action: a.action.clone(),
                target: a.target.clone(),
                before: None,
                after: Some(Box::new((**a).clone())),
            });
        }
    }

    // Handle ambiguous items (duplicate keys):
    // Conservative approach: ALL are Removed from before, ALL are Added to after
    // Never speculative modify because we can't reliably match them
    for a in &before_ambiguous {
        diffs.push(ActionRefDiff {
            change_type: ChangeType::Removed,
            action: a.action.clone(),
            target: a.target.clone(),
            before: Some(Box::new((**a).clone())),
            after: None,
        });
    }

    for a in &after_ambiguous {
        diffs.push(ActionRefDiff {
            change_type: ChangeType::Added,
            action: a.action.clone(),
            target: a.target.clone(),
            before: None,
            after: Some(Box::new((**a).clone())),
        });
    }

    // Sort by action+target for deterministic output
    diffs.sort_by_key(|d| (d.action.clone(), d.target.clone()));

    diffs
}

/// Check if two action refs are equal
fn action_refs_equal(a: &ActionRef, b: &ActionRef) -> bool {
    a.action == b.action && a.target == b.target
}

/// Compute deterministic diff between two approval rule lists
///
/// Conservative matching rules:
/// - rule_id match: match if rule_id is unique in both before and after
/// - Duplicate rule_ids: fall back to add/remove (never speculative modify)
///   because we cannot reliably identify which items match
pub(crate) fn diff_approval_rules(
    before: &[ApprovalRuleRef],
    after: &[ApprovalRuleRef],
) -> Vec<ApprovalRuleDiff> {
    let mut diffs = Vec::new();

    // Count occurrences of each rule_id to detect duplicates
    let mut before_id_count: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for a in before {
        *before_id_count.entry(a.rule_id.as_str()).or_insert(0) += 1;
    }

    let mut after_id_count: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for a in after {
        *after_id_count.entry(a.rule_id.as_str()).or_insert(0) += 1;
    }

    // Collect items with unique rule_ids for matching
    // Items with duplicate rule_ids are handled conservatively
    let mut before_unique: Vec<&ApprovalRuleRef> = Vec::new();
    let mut before_ambiguous: Vec<&ApprovalRuleRef> = Vec::new();
    for a in before {
        match before_id_count.get(a.rule_id.as_str()) {
            Some(&1) => before_unique.push(a),
            _ => before_ambiguous.push(a),
        }
    }

    let mut after_unique: Vec<&ApprovalRuleRef> = Vec::new();
    let mut after_ambiguous: Vec<&ApprovalRuleRef> = Vec::new();
    for a in after {
        match after_id_count.get(a.rule_id.as_str()) {
            Some(&1) => after_unique.push(a),
            _ => after_ambiguous.push(a),
        }
    }

    // Build maps for matching unique items only
    let mut before_by_unique_id: std::collections::HashMap<&str, &ApprovalRuleRef> =
        std::collections::HashMap::new();
    for a in &before_unique {
        before_by_unique_id.insert(a.rule_id.as_str(), a);
    }

    let mut after_by_unique_id: std::collections::HashMap<&str, &ApprovalRuleRef> =
        std::collections::HashMap::new();
    for a in &after_unique {
        after_by_unique_id.insert(a.rule_id.as_str(), a);
    }

    let mut matched_before: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut matched_after: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Match unique items by rule_id
    for (rule_id, before_a) in &before_by_unique_id {
        if let Some(after_a) = after_by_unique_id.get(rule_id) {
            if !approval_rules_equal(before_a, after_a) {
                diffs.push(ApprovalRuleDiff {
                    change_type: ChangeType::Modified,
                    rule_id: before_a.rule_id.clone(),
                    description: before_a.description.clone(),
                    before: Some(Box::new((**before_a).clone())),
                    after: Some(Box::new((**after_a).clone())),
                });
            }
            matched_before.insert(*rule_id);
            matched_after.insert(*rule_id);
        }
    }

    // Unmatched unique items in before are Removed
    for (rule_id, a) in &before_by_unique_id {
        if !matched_before.contains(rule_id) {
            diffs.push(ApprovalRuleDiff {
                change_type: ChangeType::Removed,
                rule_id: a.rule_id.clone(),
                description: a.description.clone(),
                before: Some(Box::new((**a).clone())),
                after: None,
            });
        }
    }

    // Unmatched unique items in after are Added
    for (rule_id, a) in &after_by_unique_id {
        if !matched_after.contains(rule_id) {
            diffs.push(ApprovalRuleDiff {
                change_type: ChangeType::Added,
                rule_id: a.rule_id.clone(),
                description: a.description.clone(),
                before: None,
                after: Some(Box::new((**a).clone())),
            });
        }
    }

    // Handle ambiguous items (duplicate rule_ids):
    // Conservative approach: ALL are Removed from before, ALL are Added to after
    // Never speculative modify because we can't reliably match them
    for a in &before_ambiguous {
        diffs.push(ApprovalRuleDiff {
            change_type: ChangeType::Removed,
            rule_id: a.rule_id.clone(),
            description: a.description.clone(),
            before: Some(Box::new((**a).clone())),
            after: None,
        });
    }

    for a in &after_ambiguous {
        diffs.push(ApprovalRuleDiff {
            change_type: ChangeType::Added,
            rule_id: a.rule_id.clone(),
            description: a.description.clone(),
            before: None,
            after: Some(Box::new((**a).clone())),
        });
    }

    // Sort by rule_id for deterministic output
    diffs.sort_by_key(|d| d.rule_id.clone());

    diffs
}

/// Check if two approval rules are equal
fn approval_rules_equal(a: &ApprovalRuleRef, b: &ApprovalRuleRef) -> bool {
    a.rule_id == b.rule_id && a.description == b.description
}

/// Compute deterministic diff between two authority sections
pub fn diff_authority(before: &IntentAuthority, after: &IntentAuthority) -> AuthorityDiff {
    AuthorityDiff {
        allowed_actions: diff_action_refs(&before.allowed_actions, &after.allowed_actions),
        forbidden_actions: diff_action_refs(&before.forbidden_actions, &after.forbidden_actions),
        approval_requirements: diff_approval_rules(
            &before.approval_requirements,
            &after.approval_requirements,
        ),
    }
}

/// Compute complete diff between two intent versions for the covered sections
pub fn diff_intent_version(
    before: &intent_rebase_types::IntentVersion,
    after: &intent_rebase_types::IntentVersion,
) -> IntentVersionDiff {
    IntentVersionDiff {
        scope: diff_scope(&before.payload.scope, &after.payload.scope),
        constraints: diff_constraints_section(
            &before.payload.constraints,
            &after.payload.constraints,
        ),
        acceptance_criteria: diff_acceptance_criteria_section(
            &before.payload.acceptance_criteria,
            &after.payload.acceptance_criteria,
        ),
        authority: diff_authority(&before.payload.authority, &after.payload.authority),
    }
}
