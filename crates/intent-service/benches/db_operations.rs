//! DB operations benchmarks — env-gated SQLx repository
//!
//! Scope: Benchmarks SQLx-backed intent repository operations.
//! Requires DATABASE_URL environment variable to be set.
//!
//! - bench_db_create_intent: Intent creation with initial version
//! - bench_db_create_version: Version creation with OCC
//! - bench_db_get_intent: Intent retrieval
//! - bench_db_list_versions: Version listing
//!
//! Not covered (future scope):
//! - Connection pool benchmarks
//! - Concurrent operations
//! - Large payload benchmarks
//!
//! Execution:
//! ```bash
//! DATABASE_URL="postgres://user:pass@localhost/intent_rebase" cargo bench -p intent-service --bench db_operations -- --noplot
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use intent_service::SqlxIntentRepository;
use intent_rebase_types::{
    ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, IntentPayload,
    IntentObjective, IntentScope, IntentConstraints, AcceptanceCriteria, IntentAuthority,
    IntentPreferences, IntentReferences, IntentAssumptions, IntentMetadataV1, RiskTier, Urgency,
    SourceRef,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use uuid::Uuid;

/// Check if DATABASE_URL is set, skip benchmarks if not
fn requires_database_url() -> bool {
    std::env::var("DATABASE_URL").is_ok()
}

/// Create a test intent payload
fn create_test_payload(summary: &str) -> IntentPayload {
    IntentPayload {
        objective: IntentObjective {
            summary: summary.to_string(),
            success_statement: "Benchmark success".to_string(),
            domain: "benchmark".to_string(),
        },
        scope: IntentScope {
            in_scope: vec!["item1".to_string()],
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
    }
}

/// Create a test intent request
fn create_test_request() -> CreateIntentRequest {
    CreateIntentRequest {
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://benchmark".to_string(),
        }],
        payload: create_test_payload("Benchmark intent"),
        created_by: ActorRef {
            actor_type: "benchmark".to_string(),
            actor_id: "benchmark".to_string(),
        },
        tags: vec!["benchmark".to_string()],
    }
}

/// Create a test version request
fn create_version_request() -> CreateVersionRequest {
    CreateVersionRequest {
        change_reason: "Benchmark version".to_string(),
        change_channel: ChangeChannel::UserEdit,
        payload: create_test_payload("Benchmark intent v2"),
    }
}

/// Setup: create a test database pool
async fn setup_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    // Create pool with reasonable limits for benchmarking
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    pool
}

/// Benchmark intent creation with initial version
fn bench_db_create_intent(c: &mut Criterion) {
    if !requires_database_url() {
        c.benchmark_group("db_create_intent")
            .sample_size(10)
            .bench_function("skipped_no_database_url", |b| {
                b.iter(|| {
                    println!("SKIPPED: Set DATABASE_URL to run DB benchmarks");
                });
            });
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_pool());
    let repo = Arc::new(SqlxIntentRepository::new(pool));

    let mut group = c.benchmark_group("db_create_intent");
    group.sample_size(20);

    group.bench_function("create_intent_tx", |b| {
        b.to_async(&rt).iter(|| {
            let repo = repo.clone();
            let request = create_test_request();
            async move {
                let _ = repo.create_intent_tx(request).await;
            }
        });
    });

    group.finish();
}

/// Benchmark version creation with OCC
fn bench_db_create_version(c: &mut Criterion) {
    if !requires_database_url() {
        c.benchmark_group("db_create_version")
            .sample_size(10)
            .bench_function("skipped_no_database_url", |b| {
                b.iter(|| {
                    println!("SKIPPED: Set DATABASE_URL to run DB benchmarks");
                });
            });
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_pool());
    let repo = Arc::new(SqlxIntentRepository::new(pool));

    // Pre-create an intent to update
    let intent_id = rt.block_on(async {
        repo.create_intent_tx(create_test_request())
            .await
            .expect("Failed to create initial intent")
            .intent_id
    });

    let mut group = c.benchmark_group("db_create_version");
    group.sample_size(20);

    group.bench_function("create_version_with_occ", |b| {
        b.to_async(&rt).iter(|| {
            let repo = repo.clone();
            let request = create_version_request();
            async move {
                let _ = repo
                    .create_version_with_occ(intent_id, request, 1, 1)
                    .await;
            }
        });
    });

    group.finish();
}

/// Benchmark intent retrieval
fn bench_db_get_intent(c: &mut Criterion) {
    if !requires_database_url() {
        c.benchmark_group("db_get_intent")
            .sample_size(10)
            .bench_function("skipped_no_database_url", |b| {
                b.iter(|| {
                    println!("SKIPPED: Set DATABASE_URL to run DB benchmarks");
                });
            });
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_pool());
    let repo = Arc::new(SqlxIntentRepository::new(pool));

    // Pre-create an intent to retrieve
    let intent_id = rt.block_on(async {
        repo.create_intent_tx(create_test_request())
            .await
            .expect("Failed to create initial intent")
            .intent_id
    });

    let mut group = c.benchmark_group("db_get_intent");
    group.sample_size(20);

    group.bench_function("get_intent", |b| {
        b.to_async(&rt).iter(|| {
            let repo = repo.clone();
            let intent_id = intent_id;
            async move {
                let _ = repo.get_intent(intent_id).await;
            }
        });
    });

    group.finish();
}

/// Benchmark version listing
fn bench_db_list_versions(c: &mut Criterion) {
    if !requires_database_url() {
        c.benchmark_group("db_list_versions")
            .sample_size(10)
            .bench_function("skipped_no_database_url", |b| {
                b.iter(|| {
                    println!("SKIPPED: Set DATABASE_URL to run DB benchmarks");
                });
            });
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(setup_pool());
    let repo = Arc::new(SqlxIntentRepository::new(pool));

    // Pre-create an intent with multiple versions
    let intent_id = rt.block_on(async {
        let id = repo
            .create_intent_tx(create_test_request())
            .await
            .expect("Failed to create initial intent")
            .intent_id;

        // Create a few versions
        for i in 0..3 {
            let request = CreateVersionRequest {
                change_reason: format!("Benchmark version {}", i + 2),
                change_channel: ChangeChannel::UserEdit,
                payload: create_test_payload(&format!("Benchmark intent v{}", i + 2)),
            };
            repo.create_version_with_occ(id, request, i + 1, i + 1)
                .await
                .expect("Failed to create version");
        }

        id
    });

    let mut group = c.benchmark_group("db_list_versions");
    group.sample_size(20);

    group.bench_function("get_versions_by_intent", |b| {
        b.to_async(&rt).iter(|| {
            let repo = repo.clone();
            let intent_id = intent_id;
            async move {
                let _ = repo.get_versions_by_intent(intent_id).await;
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_db_create_intent,
    bench_db_create_version,
    bench_db_get_intent,
    bench_db_list_versions,
);
criterion_main!(benches);
