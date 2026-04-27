//! HTTP handler benchmarks
//!
//! Scope: Benchmarks synchronous processing paths and full HTTP server paths.
//! - bench_diff_compute: Diff computation without HTTP overhead
//! - bench_validation: Request validation overhead
//! - bench_intent_service_create: Intent service create path
//! - bench_http_server: Full HTTP server benchmarks with real requests
//!
//! NOT covered (future scope):
//! - Database-backed handler benchmarks (requires live Postgres)
//! - Graph-service integration benchmarks (requires full stack)
//! - Full production load testing with realistic traffic patterns
//!
//! Note: HTTP server benchmarks measure end-to-end latency including
//! serialization, routing, and handler processing overhead.

use criterion::{criterion_group, criterion_main, Criterion};
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
use rebase_engine::diff::diff_intent_version;
use rebase_engine::planner::RebasePlan;
use rebase_engine::rules::analyze_diff_risk;
use std::sync::Arc;
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
        tenant_id: None,
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
        tenant_id: None,
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
        tenant_id: None,
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
                    return Err(
                        intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                            "workflow_id cannot be nil".into(),
                        ),
                    );
                }
                if request.payload.objective.summary.trim().is_empty() {
                    return Err(
                        intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                            "payload.objective.summary cannot be empty".into(),
                        ),
                    );
                }
                if request
                    .payload
                    .objective
                    .success_statement
                    .trim()
                    .is_empty()
                {
                    return Err(
                        intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                            "payload.objective.success_statement cannot be empty".into(),
                        ),
                    );
                }
                if request.payload.objective.domain.trim().is_empty() {
                    return Err(
                        intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                            "payload.objective.domain cannot be empty".into(),
                        ),
                    );
                }
                Ok(())
            })();
            criterion::black_box(result)
        });
    });

    // Validation that should fail (empty summary)
    group.bench_with_input(
        "invalid_empty_summary",
        &invalid_request_empty_summary,
        |b, request| {
            b.iter(|| {
                let result: Result<(), intent_rebase_types::IntentRebaseError> = (|| {
                    if request.payload.objective.summary.trim().is_empty() {
                        return Err(
                            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                                "payload.objective.summary cannot be empty".into(),
                            ),
                        );
                    }
                    Ok(())
                })(
                );
                criterion::black_box(result)
            });
        },
    );

    // Validation that should fail (nil workflow)
    group.bench_with_input(
        "invalid_nil_workflow",
        &invalid_request_nil_workflow,
        |b, request| {
            b.iter(|| {
                let result: Result<(), intent_rebase_types::IntentRebaseError> = (|| {
                    if request.workflow_id == Uuid::nil() {
                        return Err(
                            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                                "workflow_id cannot be nil".into(),
                            ),
                        );
                    }
                    Ok(())
                })(
                );
                criterion::black_box(result)
            });
        },
    );

    group.finish();
}

/// Benchmark intent service create intent path (synchronous wrapper)
fn bench_intent_service_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("intent_service_create");

    let repo = Arc::new(InMemoryIntentRepository::new());
    let service = IntentService::new(repo);

    let valid_request = intent_rebase_types::CreateIntentRequest {
        tenant_id: None,
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

    // Use tokio runtime for async service calls
    let rt = tokio::runtime::Runtime::new().unwrap();
    let service = Arc::new(service);
    let request = Arc::new(valid_request);

    group.bench_function("create_intent", |b| {
        b.iter(|| {
            let service = service.clone();
            let request = request.clone();
            let _ = rt.block_on(async { service.create_intent((*request).clone()).await });
        });
    });

    group.finish();
}

/// Full HTTP server benchmark — spins up axum server on ephemeral port and sends real requests.
///
/// Scope: End-to-end HTTP request/response cycle with real routing, serialization,
/// and in-memory repository backends. Does NOT include:
/// - Live Postgres (uses InMemoryIntentRepository)
/// - Full graph service (uses InMemoryGraphRepository)
/// - Production load testing with realistic traffic patterns
fn bench_http_server(c: &mut Criterion) {
    // Build a minimal app state using in-memory repositories
    let port = {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(rebase_orchestrator::RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(runtime_adapter::MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        let forensic_svc = Arc::new(forensic_service::InMemoryForensicVerificationService::new());
        let forensic_archive_gen = Arc::new(
            forensic_service::InMemoryForensicArchiveGenerator::new()
                .with_intent_version_count(5)
                .with_artifact_count(10)
                .with_audit_event_count(100)
                .with_policy_snapshot_count(3),
        );
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_storage =
            Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket"));
        let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
            Arc::new(forensic_service::InMemoryForensicDataCollector::new());
        let forensic_bundle_svc: Arc<dyn forensic_service::ForensicBundleServiceTrait> =
            Arc::new(forensic_service::ForensicBundleService::new(
                forensic_bundle_repo,
                forensic_bundle_storage,
                forensic_bundle_collector,
            ));

        let state = intent_api::AppState {
            service,
            graph_service: graph_svc,
            side_effect_service: side_effect_svc,
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: None,
            forensic_service: forensic_svc,
            forensic_archive_generator: forensic_archive_gen,
            forensic_bundle_service: forensic_bundle_svc,
            start_time: std::time::Instant::now(),
        };

        let app = intent_api::build_router(
            state.service.clone(),
            state.graph_service.clone(),
            state.side_effect_service.clone(),
            state.compensation_action_service.clone(),
            state.orchestration_runtime.clone(),
            state.orchestrator.clone(),
            state.audit_service.clone(),
            state.approval_request_repo.clone(),
            state.policy_snapshot_repo.clone(),
            state.event_publisher.clone(),
            state.forensic_service.clone(),
            state.forensic_archive_generator.clone(),
            state.forensic_bundle_service.clone(),
        );

        // Bind to ephemeral port using tokio
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let port = listener.local_addr().unwrap().port();

        // Spawn server in background thread
        std::thread::spawn(move || {
            rt.block_on(async {
                axum::serve(listener, app).await.unwrap();
            });
        });

        // Give server a moment to start
        std::thread::sleep(std::time::Duration::from_millis(50));

        port
    };

    let mut group = c.benchmark_group("http_server");

    // Pre-serialize request to avoid serialization overhead in benchmark loop
    let create_request = intent_rebase_types::CreateIntentRequest {
        tenant_id: None,
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
    let request_json = serde_json::to_string(&create_request).unwrap();

    // Benchmark: POST /intents (create intent)
    group.bench_function("create_intent", |b| {
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/intents", port);
        b.iter(|| {
            let _ = client
                .post(&url)
                .body(request_json.clone())
                .header("Content-Type", "application/json")
                .send();
        });
    });

    // Benchmark: GET /health (health check)
    group.bench_function("health_check", |b| {
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/health", port);
        b.iter(|| {
            let _ = client.get(&url).send();
        });
    });

    // Benchmark: GET /ready (readiness check)
    group.bench_function("ready_check", |b| {
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/ready", port);
        b.iter(|| {
            let _ = client.get(&url).send();
        });
    });

    // Benchmark: POST /intents/validate (validate intent)
    group.bench_function("validate_intent", |b| {
        let client = reqwest::blocking::Client::new();
        let url = format!("http://127.0.0.1:{}/v1/intents/validate", port);
        b.iter(|| {
            let _ = client
                .post(&url)
                .body(request_json.clone())
                .header("Content-Type", "application/json")
                .send();
        });
    });

    let _ = port; // suppress unused warning

    group.finish();
}

criterion_group!(
    benches,
    bench_diff_compute,
    bench_validation,
    bench_intent_service_create,
    bench_http_server,
);
criterion_main!(benches);
