//! Diff latency benchmarks for rebase-engine
//!
//! This module measures the latency of computing semantic diffs between intent versions
//! using the public API `compute_diff_sync`.
//!
//! Note: This is a benchmark harness only — it verifies the benchmark infrastructure
//! builds and runs in principle. Actual performance targets and production load testing
//! remain outstanding (gated on P2 — Phase 3 Batch 2 Observability + SRE completion).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use intent_rebase_types::*;
use uuid::Uuid;

/// Helper to create a test intent version with the given parameters
fn create_test_version(intent_id: Uuid, version_num: i32) -> IntentVersion {
    IntentVersion {
        id: Uuid::new_v4(),
        intent_id,
        version_number: version_num,
        parent_version_id: None,
        created_at: chrono::Utc::now(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "bench".to_string(),
        },
        change_reason: "benchmark".to_string(),
        change_channel: ChangeChannel::UserEdit,
        status: VersionStatus::Active,
        hash: format!("hash_{}_{}", intent_id, version_num),
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Benchmark objective".to_string(),
                success_statement: "Benchmark success".to_string(),
                domain: "bench".to_string(),
            },
            scope: IntentScope {
                in_scope: vec!["item1".to_string(), "item2".to_string()],
                out_of_scope: vec!["excluded1".to_string()],
            },
            constraints: IntentConstraints {
                functional: vec![Constraint {
                    clause_id: Some(Uuid::new_v4()),
                    constraint_type: ClauseType::Functional,
                    key: "cpu".to_string(),
                    operator: ConstraintOperator::Eq,
                    value: serde_json::json!("4"),
                    rationale: None,
                    priority: ClausePriority::Must,
                }],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![AcceptanceCriterion {
                    clause_id: Some(Uuid::new_v4()),
                    description: "AC1: must work".to_string(),
                    priority: ClausePriority::Must,
                }],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![ActionRef {
                    action: "read".to_string(),
                    target: Some("data".to_string()),
                }],
                forbidden_actions: vec![],
                approval_requirements: vec![],
            },
            preferences: IntentPreferences { tradeoffs: vec![] },
            references: IntentReferences {
                specs: vec![],
                tickets: vec![],
                repos: vec![],
                policies: vec![],
            },
            assumptions: IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Low,
                urgency: Urgency::Low,
                confidence: 0.9,
            },
        },
    }
}

/// Benchmark: diff computation with no changes between versions
fn bench_diff_no_change(c: &mut Criterion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    c.bench_function("diff_latency_no_change", |b| {
        b.iter(|| {
            let result = rebase_engine::compute_diff_sync(black_box(&v1), black_box(&v2));
            black_box(result)
        });
    });
}

/// Benchmark: diff computation with scope changes (add/remove items)
fn bench_diff_scope_change(c: &mut Criterion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);
    // Add a new item to in_scope
    v2.payload.scope.in_scope.push("item3".to_string());
    // Remove first item from in_scope
    v2.payload.scope.in_scope.remove(0);

    c.bench_function("diff_latency_scope_change", |b| {
        b.iter(|| {
            let result = rebase_engine::compute_diff_sync(black_box(&v1), black_box(&v2));
            black_box(result)
        });
    });
}

/// Benchmark: diff computation with constraints changes
fn bench_diff_constraints_change(c: &mut Criterion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);
    // Add a new constraint
    v2.payload.constraints.functional.push(Constraint {
        clause_id: Some(Uuid::new_v4()),
        constraint_type: ClauseType::Functional,
        key: "memory".to_string(),
        operator: ConstraintOperator::Eq,
        value: serde_json::json!("8"),
        rationale: None,
        priority: ClausePriority::Should,
    });

    c.bench_function("diff_latency_constraints_change", |b| {
        b.iter(|| {
            let result = rebase_engine::compute_diff_sync(black_box(&v1), black_box(&v2));
            black_box(result)
        });
    });
}

/// Benchmark: diff computation with acceptance criteria changes
fn bench_diff_acceptance_criteria_change(c: &mut Criterion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);
    // Add a new acceptance criterion
    v2.payload
        .acceptance_criteria
        .required
        .push(AcceptanceCriterion {
            clause_id: Some(Uuid::new_v4()),
            description: "AC2: must also work".to_string(),
            priority: ClausePriority::Must,
        });

    c.bench_function("diff_latency_acceptance_criteria_change", |b| {
        b.iter(|| {
            let result = rebase_engine::compute_diff_sync(black_box(&v1), black_box(&v2));
            black_box(result)
        });
    });
}

/// Benchmark: diff computation with all sections changed
fn bench_diff_all_sections_change(c: &mut Criterion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);
    // Scope change
    v2.payload.scope.in_scope.push("item3".to_string());
    // Constraint change
    v2.payload.constraints.functional.push(Constraint {
        clause_id: Some(Uuid::new_v4()),
        constraint_type: ClauseType::Functional,
        key: "memory".to_string(),
        operator: ConstraintOperator::Eq,
        value: serde_json::json!("8"),
        rationale: None,
        priority: ClausePriority::Should,
    });
    // AC change
    v2.payload
        .acceptance_criteria
        .required
        .push(AcceptanceCriterion {
            clause_id: Some(Uuid::new_v4()),
            description: "AC2: new criterion".to_string(),
            priority: ClausePriority::Must,
        });
    // Authority change
    v2.payload.authority.allowed_actions.push(ActionRef {
        action: "write".to_string(),
        target: Some("data".to_string()),
    });

    c.bench_function("diff_latency_all_sections_change", |b| {
        b.iter(|| {
            let result = rebase_engine::compute_diff_sync(black_box(&v1), black_box(&v2));
            black_box(result)
        });
    });
}

criterion_group!(
    benches,
    bench_diff_no_change,
    bench_diff_scope_change,
    bench_diff_constraints_change,
    bench_diff_acceptance_criteria_change,
    bench_diff_all_sections_change
);
criterion_main!(benches);
