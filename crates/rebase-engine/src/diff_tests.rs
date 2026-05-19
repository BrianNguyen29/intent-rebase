use crate::diff::*;
use intent_rebase_types::*;
use uuid::Uuid;

fn make_test_constraint(id: Option<Uuid>, key: &str) -> Constraint {
    Constraint {
        clause_id: id,
        constraint_type: ClauseType::Functional,
        key: key.to_string(),
        operator: ConstraintOperator::Eq,
        value: serde_json::json!("test"),
        rationale: None,
        priority: ClausePriority::Must,
    }
}

fn make_test_acceptance_criterion(id: Option<Uuid>, desc: &str) -> AcceptanceCriterion {
    AcceptanceCriterion {
        clause_id: id,
        description: desc.to_string(),
        priority: ClausePriority::Must,
    }
}

#[test]
fn test_diff_scope_no_change() {
    let before = IntentScope {
        in_scope: vec!["item1".to_string(), "item2".to_string()],
        out_of_scope: vec!["excluded1".to_string()],
    };
    let after = IntentScope {
        in_scope: vec!["item1".to_string(), "item2".to_string()],
        out_of_scope: vec!["excluded1".to_string()],
    };

    let diff = diff_scope(&before, &after);
    assert!(diff.in_scope.added.is_empty());
    assert!(diff.in_scope.removed.is_empty());
    assert!(diff.out_of_scope.added.is_empty());
    assert!(diff.out_of_scope.removed.is_empty());
}

#[test]
fn test_diff_scope_add_remove() {
    let before = IntentScope {
        in_scope: vec!["item1".to_string(), "item2".to_string()],
        out_of_scope: vec!["excluded1".to_string()],
    };
    let after = IntentScope {
        in_scope: vec!["item2".to_string(), "item3".to_string()],
        out_of_scope: vec!["excluded1".to_string(), "excluded2".to_string()],
    };

    let diff = diff_scope(&before, &after);
    assert_eq!(diff.in_scope.added, vec!["item3"]);
    assert_eq!(diff.in_scope.removed, vec!["item1"]);
    assert_eq!(diff.out_of_scope.added, vec!["excluded2"]);
    assert!(diff.out_of_scope.removed.is_empty());
}

#[test]
fn test_diff_constraints_with_clause_id() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    let before = vec![
        make_test_constraint(Some(id1), "key1"),
        make_test_constraint(Some(id2), "key2"),
    ];
    let after = vec![
        make_test_constraint(Some(id1), "key1"),
        make_test_constraint(Some(id3), "key3"),
    ];

    let diffs = diff_constraints(&before, &after);
    assert_eq!(diffs.len(), 2);

    let removed = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Removed)
        .unwrap();
    assert_eq!(removed.clause_id, Some(id2));

    let added = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Added)
        .unwrap();
    assert_eq!(added.clause_id, Some(id3));
}

#[test]
fn test_diff_constraints_modified() {
    let id1 = Uuid::new_v4();
    let before_constraint = make_test_constraint(Some(id1), "key1");
    let mut after_constraint = make_test_constraint(Some(id1), "key1");
    after_constraint.operator = ConstraintOperator::Neq;

    let diffs = diff_constraints(&[before_constraint], &[after_constraint]);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].change_type, ChangeType::Modified);
    assert_eq!(diffs[0].clause_id, Some(id1));
}

#[test]
fn test_diff_constraints_no_clause_id_conservative() {
    // When clause_id is None, identity is ambiguous - conservative matching
    // treats all None items as Removed (before) and Added (after).
    // We never speculative modify because we can't reliably match them.
    let before = vec![make_test_constraint(None, "key1")];
    let after = vec![make_test_constraint(None, "key1")];

    let diffs = diff_constraints(&before, &after);
    // Conservative: None clause_id items are never matched, always produce add+remove
    assert_eq!(diffs.len(), 2);
    assert!(diffs
        .iter()
        .any(|d| d.change_type == ChangeType::Removed && d.clause_id.is_none()));
    assert!(diffs
        .iter()
        .any(|d| d.change_type == ChangeType::Added && d.clause_id.is_none()));
}

#[test]
fn test_diff_constraints_multiple_none_clause_ids() {
    // Multiple None clause_id items: each is treated independently (no collapse)
    let before = vec![
        make_test_constraint(None, "key1"),
        make_test_constraint(None, "key2"),
    ];
    let after = vec![make_test_constraint(None, "key1")];

    let diffs = diff_constraints(&before, &after);
    // before: 2 None items -> 2 Removed
    // after: 1 None item -> 1 Added
    // Total: 3 diffs (no silent overwrite)
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2);
    assert_eq!(added.len(), 1);
}

#[test]
fn test_diff_constraints_duplicate_clause_ids() {
    // Duplicate clause_ids in before: ALL treated as ambiguous (conservative)
    // since we can't reliably match them. All before items -> Removed, after item -> Added.
    let id1 = Uuid::new_v4();
    let before = vec![
        make_test_constraint(Some(id1), "key1"),
        make_test_constraint(Some(id1), "key2"), // duplicate id
    ];
    let after = vec![make_test_constraint(Some(id1), "key1")];

    let diffs = diff_constraints(&before, &after);
    // Conservative: all duplicate items treated as ambiguous, showing Removed + Added
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2); // both before items with id1 are Removed
    assert_eq!(added.len(), 1); // after item with id1 is Added
}

#[test]
fn test_diff_acceptance_criteria() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let before = vec![
        make_test_acceptance_criterion(Some(id1), "desc1"),
        make_test_acceptance_criterion(Some(id2), "desc2"),
    ];
    let after = vec![
        make_test_acceptance_criterion(Some(id1), "desc1"),
        make_test_acceptance_criterion(None, "desc3"),
    ];

    let diffs = diff_acceptance_criteria(&before, &after);
    assert_eq!(diffs.len(), 2);

    let removed = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Removed)
        .unwrap();
    assert_eq!(removed.clause_id, Some(id2));

    let added = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Added)
        .unwrap();
    assert!(added.clause_id.is_none());
}

#[test]
fn test_diff_acceptance_criteria_no_clause_id_conservative() {
    // When clause_id is None, identity is ambiguous - conservative matching
    // treats all None items as Removed (before) and Added (after).
    let before = vec![make_test_acceptance_criterion(None, "desc1")];
    let after = vec![make_test_acceptance_criterion(None, "desc1")];

    let diffs = diff_acceptance_criteria(&before, &after);
    // Conservative: None clause_id items are never matched, always produce add+remove
    assert_eq!(diffs.len(), 2);
    assert!(diffs
        .iter()
        .any(|d| d.change_type == ChangeType::Removed && d.clause_id.is_none()));
    assert!(diffs
        .iter()
        .any(|d| d.change_type == ChangeType::Added && d.clause_id.is_none()));
}

#[test]
fn test_diff_acceptance_criteria_multiple_none_clause_ids() {
    // Multiple None clause_id items: each is treated independently (no collapse)
    let before = vec![
        make_test_acceptance_criterion(None, "desc1"),
        make_test_acceptance_criterion(None, "desc2"),
    ];
    let after = vec![make_test_acceptance_criterion(None, "desc1")];

    let diffs = diff_acceptance_criteria(&before, &after);
    // before: 2 None items -> 2 Removed
    // after: 1 None item -> 1 Added
    // Total: 3 diffs (no silent overwrite)
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2);
    assert_eq!(added.len(), 1);
}

#[test]
fn test_diff_acceptance_criteria_duplicate_clause_ids() {
    // Duplicate clause_ids: ALL treated as ambiguous (conservative)
    // All before items -> Removed, after item -> Added
    let id1 = Uuid::new_v4();
    let before = vec![
        make_test_acceptance_criterion(Some(id1), "desc1"),
        make_test_acceptance_criterion(Some(id1), "desc2"), // duplicate id
    ];
    let after = vec![make_test_acceptance_criterion(Some(id1), "desc1")];

    let diffs = diff_acceptance_criteria(&before, &after);
    // Conservative: all duplicate items treated as ambiguous, showing Removed + Added
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2); // both before items with id1 are Removed
    assert_eq!(added.len(), 1); // after item with id1 is Added
}

#[test]
fn test_diff_authority_action_refs() {
    let before = vec![
        ActionRef {
            action: "read".to_string(),
            target: Some("file1".to_string()),
        },
        ActionRef {
            action: "write".to_string(),
            target: None,
        },
    ];
    let after = vec![
        ActionRef {
            action: "read".to_string(),
            target: Some("file1".to_string()),
        },
        ActionRef {
            action: "delete".to_string(),
            target: None,
        },
    ];

    let diffs = diff_action_refs(&before, &after);
    assert_eq!(diffs.len(), 2);

    let removed = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Removed)
        .unwrap();
    assert_eq!(removed.action, "write");

    let added = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Added)
        .unwrap();
    assert_eq!(added.action, "delete");
}

#[test]
fn test_diff_authority_approval_rules() {
    let before = vec![
        ApprovalRuleRef {
            rule_id: "rule1".to_string(),
            description: "desc1".to_string(),
        },
        ApprovalRuleRef {
            rule_id: "rule2".to_string(),
            description: "desc2".to_string(),
        },
    ];
    let after = vec![
        ApprovalRuleRef {
            rule_id: "rule1".to_string(),
            description: "desc1".to_string(),
        },
        ApprovalRuleRef {
            rule_id: "rule3".to_string(),
            description: "desc3".to_string(),
        },
    ];

    let diffs = diff_approval_rules(&before, &after);
    assert_eq!(diffs.len(), 2);

    let removed = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Removed)
        .unwrap();
    assert_eq!(removed.rule_id, "rule2");

    let added = diffs
        .iter()
        .find(|d| d.change_type == ChangeType::Added)
        .unwrap();
    assert_eq!(added.rule_id, "rule3");
}

#[test]
fn test_deterministic_output_ordering() {
    // Running diff twice with same inputs should produce identical output
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();
    let id4 = Uuid::new_v4();

    let before = vec![
        make_test_constraint(Some(id4), "key4"),
        make_test_constraint(Some(id2), "key2"),
        make_test_constraint(Some(id1), "key1"),
    ];
    let after = vec![
        make_test_constraint(Some(id3), "key3"),
        make_test_constraint(Some(id1), "key1"),
        make_test_constraint(Some(id2), "key2"),
    ];

    let diffs1 = diff_constraints(&before, &after);
    let diffs2 = diff_constraints(&before, &after);

    assert_eq!(diffs1.len(), diffs2.len());
    for (d1, d2) in diffs1.iter().zip(diffs2.iter()) {
        assert_eq!(d1.clause_id, d2.clause_id);
        assert_eq!(d1.change_type, d2.change_type);
        assert_eq!(d1.key, d2.key);
    }
}

#[test]
fn test_conservative_fallback_add_remove() {
    // When clause_ids differ and content differs, conservative matching treats as add+remove
    // since we can't be certain they're the same clause
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let before = vec![make_test_constraint(Some(id1), "key1")];
    let mut after = vec![make_test_constraint(Some(id2), "key2")];
    after[0].operator = ConstraintOperator::Neq;

    let diffs = diff_constraints(&before, &after);
    // Conservative: since clause_ids differ, treat as remove + add
    assert_eq!(diffs.len(), 2);
    assert!(diffs.iter().any(|d| d.change_type == ChangeType::Removed));
    assert!(diffs.iter().any(|d| d.change_type == ChangeType::Added));
}

#[test]
fn test_diff_action_refs_duplicate_keys_conservative() {
    // Duplicate (action, target) keys in before: ALL treated as ambiguous (conservative)
    // since we can't reliably match them. All before items -> Removed, after item -> Added.
    let before = vec![
        ActionRef {
            action: "read".to_string(),
            target: Some("file1".to_string()),
        },
        ActionRef {
            action: "read".to_string(),
            target: Some("file1".to_string()), // duplicate key
        },
    ];
    let after = vec![ActionRef {
        action: "read".to_string(),
        target: Some("file1".to_string()),
    }];

    let diffs = diff_action_refs(&before, &after);
    // Conservative: all duplicate items treated as ambiguous, showing Removed + Added
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2); // both before items with key are Removed
    assert_eq!(added.len(), 1); // after item is Added
}

#[test]
fn test_diff_action_refs_duplicate_in_both_sides() {
    // Duplicate keys in both before and after: conservative approach
    let before = vec![
        ActionRef {
            action: "write".to_string(),
            target: None,
        },
        ActionRef {
            action: "write".to_string(),
            target: None, // duplicate
        },
    ];
    let after = vec![
        ActionRef {
            action: "write".to_string(),
            target: None,
        },
        ActionRef {
            action: "write".to_string(),
            target: None, // duplicate
        },
    ];

    let diffs = diff_action_refs(&before, &after);
    // Conservative: all items with duplicate keys treated as ambiguous
    // 2 Removed (before) + 2 Added (after) = 4 diffs
    assert_eq!(diffs.len(), 4);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2);
    assert_eq!(added.len(), 2);
}

#[test]
fn test_diff_approval_rules_duplicate_rule_ids_conservative() {
    // Duplicate rule_ids in before: ALL treated as ambiguous (conservative)
    // All before items -> Removed, after item -> Added.
    let before = vec![
        ApprovalRuleRef {
            rule_id: "rule1".to_string(),
            description: "desc1".to_string(),
        },
        ApprovalRuleRef {
            rule_id: "rule1".to_string(), // duplicate id
            description: "desc2".to_string(),
        },
    ];
    let after = vec![ApprovalRuleRef {
        rule_id: "rule1".to_string(),
        description: "desc1".to_string(),
    }];

    let diffs = diff_approval_rules(&before, &after);
    // Conservative: all duplicate items treated as ambiguous, showing Removed + Added
    assert_eq!(diffs.len(), 3);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2); // both before items with rule1 are Removed
    assert_eq!(added.len(), 1); // after item is Added
}

#[test]
fn test_diff_approval_rules_duplicate_in_both_sides() {
    // Duplicate rule_ids in both before and after: conservative approach
    let before = vec![
        ApprovalRuleRef {
            rule_id: "rule1".to_string(),
            description: "desc1".to_string(),
        },
        ApprovalRuleRef {
            rule_id: "rule1".to_string(), // duplicate
            description: "desc2".to_string(),
        },
    ];
    let after = vec![
        ApprovalRuleRef {
            rule_id: "rule1".to_string(),
            description: "desc1".to_string(),
        },
        ApprovalRuleRef {
            rule_id: "rule1".to_string(), // duplicate
            description: "desc2".to_string(),
        },
    ];

    let diffs = diff_approval_rules(&before, &after);
    // Conservative: all items with duplicate keys treated as ambiguous
    // 2 Removed (before) + 2 Added (after) = 4 diffs
    assert_eq!(diffs.len(), 4);
    let removed: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Removed)
        .collect();
    let added: Vec<_> = diffs
        .iter()
        .filter(|d| d.change_type == ChangeType::Added)
        .collect();
    assert_eq!(removed.len(), 2);
    assert_eq!(added.len(), 2);
}
