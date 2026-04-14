//! HTTP handler benchmarks — synchronous processing paths
//!
//! Scope: Benchmarks synchronous processing paths in intent-api handlers.
//! - bench_diff_compute: Diff computation without HTTP overhead
//! - bench_validation: Request validation overhead
//!
//! NOT covered (future scope):
//! - Full HTTP server benchmarks (requires live server + load testing infrastructure)
//! - Database-backed handler benchmarks (requires live Postgres)
//! - Graph-service integration benchmarks (requires full stack)
//!
//! Note: These benchmarks measure the sync compute path only.
//! Full HTTP benchmarks require a running server with actual HTTP requests.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use intent_service::{InMemoryIntentRepository, IntentService};
use rebase_engine::diff::diff_intent_version;
use rebase_engine::planner::RebasePlan;
use rebase_engine::rules::analyze_diff_risk;
use uuid::Uuid;

// Test fixtures
fn create_test_payload() -> intent_rebase_types::IntentPayload {
    intent_rebase_types::IntentPayload {
        objective: intent_rebase_types::IntentObjective {
            summary: "Test intent".to_string(),
            success_statement: "Success".to_string(),
            domain: "testing".to_string(),
        },
        scope: intent_rebase_types::IntentScope {
            in_scope: vec!["item1".to_string(), "item2".to_string()],
            out_of_scope: vec![],
        },
        constraints: intent_rebase_types::IntentConstraints {
            functional: vec![],
            non_functional: vec![],
            policy: vec![],
            budget: vec![],
            time: vec![],
        },
        acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
            required: vec![],
            optional: vec![],
        },
        authority: intent_rebase_types::IntentAuthority {
            allowed_actions: vec![],
            forbidden_actions: vec![],
            approval_requirements: vec![],
        },
        preferences: intent_rebase_types::IntentPreferences { tradeoffs: vec![] },
        references: intent_rebase_types::IntentReferences {
            specs: vec![],
            tickets: vec![],
            repos: vec![],
            policies: vec![],
        },
        assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
        metadata: intent_rebase_types::IntentMetadataV1 {
            risk_tier: intent_rebase_types::RiskTier::Medium,
            urgency: intent_rebase_types::Urgency::Medium,
            confidence: 0.9,
        },
    }
}

fn create_medium_complexity_payload() -> intent_rebase_types::IntentPayload {
    intent_rebase_types::IntentPayload {
        objective: intent_rebase_types::IntentObjective {
            summary: "Medium complexity test intent with more details".to_string(),
            success_statement: "Success condition defined here".to_string(),
            domain: "testing".to_string(),
        },
        scope: intent_rebase_types::IntentScope {
            in_scope: vec![
                "feature_a".to_string(),
                "feature_b".to_string(),
                "feature_c".to_string(),
            ],
            out_of_scope: vec!["legacy_mode".to_string()],
        },
        constraints: intent_rebase_types::IntentConstraints {
            functional: vec![intent_rebase_types::Constraint {
                clause_id: Some(Uuid::new_v4()),
                constraint_type: intent_rebase_types::ClauseType::Functional,
                key: "perf_threshold".to_string(),
                operator: intent_rebase_types::ConstraintOperator::Lte,
                value: serde_json::json!(100),
                rationale: Some("Performance requirement".to_string()),
                priority: intent_rebase_types::ClausePriority::Must,
            }],
            non_functional: vec![],
            policy: vec![],
            budget: vec![],
            time: vec![],
        },
        acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
            required: vec![intent_rebase_types::AcceptanceCriterion {
                clause_id: Some(Uuid::new_v4()),
                description: "Response time under 200ms".to_string(),
                priority: intent_rebase_types::ClausePriority::Must,
            }],
            optional: vec![],
        },
        authority: intent_rebase_types::IntentAuthority {
            allowed_actions: vec![intent_rebase_types::ActionRef {
                action: "deploy".to_string(),
                target: Some("staging".to_string()),
            }],
            forbidden_actions: vec![],
            approval_requirements: vec![],
        },
        preferences: intent_rebase_types::IntentPreferences { tradeoffs: vec![] },
        references: intent_rebase_types::IntentReferences {
            specs: vec![],
            tickets: vec![],
            repos: vec![],
            policies: vec![],
        },
        assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
        metadata: intent_rebase_types::IntentMetadataV1 {
            risk_tier: intent_rebase_types::RiskTier::High,
            urgency: intent_rebase_types::Urgency::High,
            confidence: 0.7,
        },
    }
}

/// Benchmark diff computation path (used by compute_diff handler)
fn bench_diff_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_compute");

    let v1_base = intent_rebase_types::IntentVersion {
        id: Uuid::new_v4(),
        intent_id: Uuid::new_v4(),
        version_number: 1,
        parent_version_id: None,
        created_at: chrono::Utc::now(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "benchmark".to_string(),
        },
        change_reason: "benchmark".to_string(),
        change_channel: intent_rebase_types::ChangeChannel::UserEdit,
        status: intent_rebase_types::VersionStatus::Active,
        hash: "hash_1".to_string(),
        payload: create_test_payload(),
    };

    let v2_low = intent_rebase_types::IntentVersion {
        id: Uuid::new_v4(),
        intent_id: v1_base.intent_id,
        version_number: 2,
        parent_version_id: Some(v1_base.id),
        created_at: chrono::Utc::now(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "benchmark".to_string(),
        },
        change_reason: "benchmark".to_string(),
        change_channel: intent_rebase_types::ChangeChannel::UserEdit,
        status: intent_rebase_types::VersionStatus::Active,
        hash: "hash_2".to_string(),
        payload: create_test_payload(), // No change
    };

    let v2_medium = intent_rebase_types::IntentVersion {
        id: Uuid::new_v4(),
        intent_id: v1_base.intent_id,
        version_number: 2,
        parent_version_id: Some(v1_base.id),
        created_at: chrono::Utc::now(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "benchmark".to_string(),
        },
        change_reason: "benchmark".to_string(),
        change_channel: intent_rebase_types::ChangeChannel::UserEdit,
        status: intent_rebase_types::VersionStatus::Active,
        hash: "hash_2".to_string(),
        payload: create_medium_complexity_payload(), // Changed
    };

    // Low complexity: identical payloads
    group.bench_with_input("low_no_change", &(&v1_base, &v2_low), |b, (v1, v2)| {
        b.iter(|| {
            let diff = diff_intent_version(v1, v2);
            let risk = analyze_diff_risk(
                &diff.scope,
                &diff.constraints,
                &diff.acceptance_criteria,
                &diff.authority,
            );
            let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
            criterion::black_box((diff, risk, plan))
        });
    });

    // Medium complexity: different payloads
    group.bench_with_input("medium_change", &(&v1_base, &v2_medium), |b, (v1, v2)| {
        b.iter(|| {
            let diff = diff_intent_version(v1, v2);
            let risk = analyze_diff_risk(
                &diff.scope,
                &diff.constraints,
                &diff.acceptance_criteria,
                &diff.authority,
            );
            let plan = RebasePlan::from_diff_and_risk(&diff, &risk);
            criterion::black_box((diff, risk, plan))
        });
    });

    group.finish();
}

/// Benchmark request validation overhead
fn bench_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation");

    let valid_request = intent_rebase_types::CreateIntentRequest {
        workflow_id: Uuid::new_v4(),
        source_refs: vec![intent_rebase_types::SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let invalid_request_empty_summary = intent_rebase_types::CreateIntentRequest {
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: intent_rebase_types::IntentPayload {
            objective: intent_rebase_types::IntentObjective {
                summary: "".to_string(), // Invalid: empty
                success_statement: "Success".to_string(),
                domain: "testing".to_string(),
            },
            ..create_test_payload()
        },
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec![],
    };

    let invalid_request_nil_workflow = intent_rebase_types::CreateIntentRequest {
        workflow_id: Uuid::nil(), // Invalid: nil UUID
        source_refs: vec![],
        payload: create_test_payload(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec![],
    };

    // Validation that should pass
    group.bench_with_input("valid_request", &valid_request, |b, request| {
        b.iter(|| {
            let result: Result<(), intent_rebase_types::IntentRebaseError> = (|| {
                // Simulate validate_create_intent_request logic
                if request.workflow_id == Uuid::nil() {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "workflow_id cannot be nil".into(),
                    ));
                }
                if request.payload.objective.summary.trim().is_empty() {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "payload.objective.summary cannot be empty".into(),
                    ));
                }
                if request
                    .payload
                    .objective
                    .success_statement
                    .trim()
                    .is_empty()
                {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "payload.objective.success_statement cannot be empty".into(),
                    ));
                }
                if request.payload.objective.domain.trim().is_empty() {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "payload.objective.domain cannot be empty".into(),
                    ));
                }
                Ok(())
            })();
            criterion::black_box(result)
        });
    });

    // Validation that should fail (empty summary)
    group.bench_with_input("invalid_empty_summary", &invalid_request_empty_summary, |b, request| {
        b.iter(|| {
            let result: Result<(), intent_rebase_types::IntentRebaseError> = (|| {
                if request.payload.objective.summary.trim().is_empty() {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "payload.objective.summary cannot be empty".into(),
                    ));
                }
                Ok(())
            })();
            criterion::black_box(result)
        });
    });

    // Validation that should fail (nil workflow)
    group.bench_with_input("invalid_nil_workflow", &invalid_request_nil_workflow, |b, request| {
        b.iter(|| {
            let result: Result<(), intent_rebase_types::IntentRebaseError> = (|| {
                if request.workflow_id == Uuid::nil() {
                    return Err(intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                        "workflow_id cannot be nil".into(),
                    ));
                }
                Ok(())
            })();
            criterion::black_box(result)
        });
    });

    group.finish();
}

/// Benchmark intent service create intent path
fn bench_intent_service_create(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("intent_service_create");

    let repo = std::sync::Arc::new(InMemoryIntentRepository::new());
    let service = IntentService::new(repo.clone());

    let valid_request = intent_rebase_types::CreateIntentRequest {
        workflow_id: Uuid::new_v4(),
        source_refs: vec![intent_rebase_types::SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "benchmark".to_string(),
        },
        tags: vec!["benchmark".to_string()],
    };

    group.bench_with_input("create_intent", &valid_request, |b, request| {
        b.to_async(&rt).iter(|| {
            let service = service.clone();
            let request = request.clone();
            async move {
                let _ = service.create_intent(request).await;
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_diff_compute,
    bench_validation,
    bench_intent_service_create,
);
criterion_main!(benches);
