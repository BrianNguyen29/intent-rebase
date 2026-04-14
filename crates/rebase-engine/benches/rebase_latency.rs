//! Rebase latency benchmarks — sync diff + plan path
//!
//! Scope: Benchmarks the synchronous diff and planning path only.
//! - compute_diff_sync: semantic diff computation for scope, constraints, AC, authority
//! - RebasePlan::from_diff_and_risk: deterministic decision class mapping
//!
//! Not covered (future scope):
//! - Async compute_diff (HTTP API path)
//! - Graph service integration
//! - Full rebase apply with runtime adapter

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use intent_rebase_types::*;
use rebase_engine::diff::diff_intent_version;
use rebase_engine::planner::RebasePlan;
use rebase_engine::rules::analyze_diff_risk;
use uuid::Uuid;

/// Create a minimal intent version for benchmarking
fn create_test_version(intent_id: Uuid, version_num: i32) -> IntentVersion {
    IntentVersion {
        id: Uuid::new_v4(),
        intent_id,
        version_number: version_num,
        parent_version_id: None,
        created_at: chrono::Utc::now(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "benchmark".to_string(),
        },
        change_reason: "benchmark".to_string(),
        change_channel: ChangeChannel::UserEdit,
        status: VersionStatus::Active,
        hash: format!("hash_{}", version_num),
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Benchmark intent".to_string(),
                success_statement: "Benchmark success".to_string(),
                domain: "benchmark".to_string(),
            },
            scope: IntentScope {
                in_scope: vec![],
                out_of_scope: vec![],
            },
            constraints: IntentConstraints {
                functional: vec![],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![],
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

/// LOW complexity: no semantic changes between versions
fn create_low_complexity_pair() -> (IntentVersion, IntentVersion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);
    (v1, v2)
}

/// MEDIUM complexity: single scope item added
fn create_medium_complexity_pair() -> (IntentVersion, IntentVersion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);
    v2.payload.scope.in_scope.push("new_item".to_string());
    (v1, v2)
}

/// HIGH complexity: multiple sections changed with multiple items
fn create_high_complexity_pair() -> (IntentVersion, IntentVersion) {
    let intent_id = Uuid::new_v4();
    let v1 = create_test_version(intent_id, 1);
    let mut v2 = create_test_version(intent_id, 2);

    // Scope: add multiple items
    v2.payload.scope.in_scope.push("item1".to_string());
    v2.payload.scope.in_scope.push("item2".to_string());
    v2.payload
        .scope
        .out_of_scope
        .push("removed_item".to_string());

    // Constraints: add functional constraints with clause_ids
    v2.payload.constraints.functional.push(Constraint {
        clause_id: Some(Uuid::new_v4()),
        constraint_type: ClauseType::Functional,
        key: "perf_threshold".to_string(),
        operator: ConstraintOperator::Lte,
        value: serde_json::json!(100),
        rationale: Some("Performance requirement".to_string()),
        priority: ClausePriority::Must,
    });
    v2.payload.constraints.functional.push(Constraint {
        clause_id: Some(Uuid::new_v4()),
        constraint_type: ClauseType::Functional,
        key: "availability".to_string(),
        operator: ConstraintOperator::Gte,
        value: serde_json::json!(99.9),
        rationale: Some("Availability requirement".to_string()),
        priority: ClausePriority::Must,
    });

    // Acceptance criteria
    v2.payload
        .acceptance_criteria
        .required
        .push(AcceptanceCriterion {
            clause_id: Some(Uuid::new_v4()),
            description: "Response time under 200ms".to_string(),
            priority: ClausePriority::Must,
        });

    // Authority
    v2.payload.authority.allowed_actions.push(ActionRef {
        action: "deploy".to_string(),
        target: Some("staging".to_string()),
    });

    (v1, v2)
}

/// Benchmark sync diff computation across complexity levels
fn bench_compute_diff_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("compute_diff_sync");

    for (name, pair_fn) in [
        ("low", create_low_complexity_pair as fn() -> _),
        ("medium", create_medium_complexity_pair),
        ("high", create_high_complexity_pair),
    ] {
        group.bench_with_input(
            BenchmarkId::new("complexity", name),
            &pair_fn,
            |b, pair_fn| {
                let (v1, v2) = pair_fn();
                b.iter(|| {
                    let result = diff_intent_version(black_box(&v1), black_box(&v2));
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark sync diff + risk analysis across complexity levels
fn bench_compute_diff_with_risk_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("compute_diff_with_risk_sync");

    for (name, pair_fn) in [
        ("low", create_low_complexity_pair as fn() -> _),
        ("medium", create_medium_complexity_pair),
        ("high", create_high_complexity_pair),
    ] {
        group.bench_with_input(
            BenchmarkId::new("complexity", name),
            &pair_fn,
            |b, pair_fn| {
                let (v1, v2) = pair_fn();
                b.iter(|| {
                    let diff = diff_intent_version(black_box(&v1), black_box(&v2));
                    let risk = analyze_diff_risk(
                        black_box(&diff.scope),
                        black_box(&diff.constraints),
                        black_box(&diff.acceptance_criteria),
                        black_box(&diff.authority),
                    );
                    black_box((diff, risk))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark full diff + plan path (sync only) across complexity levels
fn bench_diff_and_plan_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_and_plan_sync");

    for (name, pair_fn) in [
        ("low", create_low_complexity_pair as fn() -> _),
        ("medium", create_medium_complexity_pair),
        ("high", create_high_complexity_pair),
    ] {
        group.bench_with_input(
            BenchmarkId::new("complexity", name),
            &pair_fn,
            |b, pair_fn| {
                let (v1, v2) = pair_fn();
                b.iter(|| {
                    let diff = diff_intent_version(black_box(&v1), black_box(&v2));
                    let risk = analyze_diff_risk(
                        black_box(&diff.scope),
                        black_box(&diff.constraints),
                        black_box(&diff.acceptance_criteria),
                        black_box(&diff.authority),
                    );
                    let plan = RebasePlan::from_diff_and_risk(black_box(&diff), black_box(&risk));
                    black_box(plan)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark planning path only (given a pre-computed diff) for scaling analysis
fn bench_plan_from_diff_fixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan_from_diff_fixed");

    // Use high complexity diff as fixed input to isolate planning cost
    let (v1, v2) = create_high_complexity_pair();
    let diff = diff_intent_version(&v1, &v2);
    let risk = analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );

    // Vary the number of iterations to show scaling
    for size in [1, 10, 100] {
        group.bench_with_input(BenchmarkId::new("iterations", size), &size, |b, &size| {
            b.iter(|| {
                for _ in 0..size {
                    let plan = RebasePlan::from_diff_and_risk(black_box(&diff), black_box(&risk));
                    black_box(plan);
                }
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_compute_diff_sync,
    bench_compute_diff_with_risk_sync,
    bench_diff_and_plan_sync,
    bench_plan_from_diff_fixed,
);
criterion_main!(benches);
