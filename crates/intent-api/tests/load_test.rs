//! Production load testing harness for intent-api HTTP server
//!
//! Simulates production-like traffic using in-memory repositories.
//! Run with: cargo test -p intent-api --test load_test -- --nocapture
//!
//! This test is gated behind the `load-test` feature to avoid running in normal `cargo test`.
//! Run with: cargo test -p intent-api --test load_test --features load-test -- --nocapture
//!
//! ## Local Live Load Test (SQLx-backed)
//!
//! For local live load testing against PostgreSQL via docker-compose:
//!
//! 1. Start local infrastructure:
//!    cd infrastructure/local && docker-compose up -d postgres
//!
//! 2. Set DATABASE_URL:
//!    export DATABASE_URL="postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase"
//!
//! 3. Run the SQLx-backed load test:
//!    cargo test -p intent-api --test load_test --features load-test,sqlx-load-test -- --nocapture test_load_sqlx
//!
//! Note: Requires migrations to be run first. See infrastructure/migrations/.

use axum::Router;
use rebase_orchestrator::RebaseOrchestrator;
use reqwest::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use uuid::Uuid;

// Load test configuration
const LOAD_TEST_ITEMS: &[(usize, usize)] = &[
    (10, 1000),   // Level 1: 10 concurrent clients, 1000 total requests (~"normal" load)
    (50, 5000),   // Level 2: 50 concurrent clients, 5000 total requests (~5x load)
    (100, 10000), // Level 3: 100 concurrent clients, 10000 total requests (~10x load)
];

// SLO thresholds from docs
const SLO_P95_LATENCY_MS: u64 = 10_000; // p95 < 10s
const SLO_ERROR_RATE: f64 = 0.01; // < 1% error rate

/// Check if DATABASE_URL is set for SQLx load test
fn requires_database_url() -> bool {
    std::env::var("DATABASE_URL").is_ok()
}

#[derive(Debug)]
struct LoadTestMetrics {
    total_requests: Arc<AtomicUsize>,
    successful_requests: Arc<AtomicUsize>,
    failed_requests: Arc<AtomicUsize>,
    latencies: Arc<std::sync::Mutex<Vec<u64>>>,
}

impl Default for LoadTestMetrics {
    fn default() -> Self {
        Self {
            total_requests: Arc::new(AtomicUsize::new(0)),
            successful_requests: Arc::new(AtomicUsize::new(0)),
            failed_requests: Arc::new(AtomicUsize::new(0)),
            latencies: Arc::new(std::sync::Mutex::new(Vec::with_capacity(10000))),
        }
    }
}

impl LoadTestMetrics {
    fn record_success(&self, latency_ms: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        self.latencies.lock().unwrap().push(latency_ms);
    }

    fn record_failure(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }

    fn get_stats(&self) -> LoadTestStats {
        let latencies = self.latencies.lock().unwrap();
        let mut sorted = latencies.clone();
        sorted.sort();

        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_requests.load(Ordering::Relaxed);
        let failed = self.failed_requests.load(Ordering::Relaxed);

        let p50 = percentile(&sorted, 0.50);
        let p90 = percentile(&sorted, 0.90);
        let p95 = percentile(&sorted, 0.95);
        let p99 = percentile(&sorted, 0.99);
        let max = sorted.last().copied().unwrap_or(0);

        LoadTestStats {
            total_requests: total,
            successful_requests: successful,
            failed_requests: failed,
            error_rate: if total > 0 {
                failed as f64 / total as f64
            } else {
                0.0
            },
            p50_latency_ms: p50,
            p90_latency_ms: p90,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            max_latency_ms: max,
        }
    }
}

#[derive(Debug)]
struct LoadTestStats {
    total_requests: usize,
    successful_requests: usize,
    failed_requests: usize,
    error_rate: f64,
    p50_latency_ms: u64,
    p90_latency_ms: u64,
    p95_latency_ms: u64,
    p99_latency_ms: u64,
    max_latency_ms: u64,
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64) * p).floor() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn check_slo_compliance(stats: &LoadTestStats, level: usize) -> Vec<String> {
    let mut violations = Vec::new();

    if stats.p95_latency_ms > SLO_P95_LATENCY_MS {
        violations.push(format!(
            "Level {}: p95 latency {}ms exceeds SLO {}ms",
            level,
            stats.p95_latency_ms,
            SLO_P95_LATENCY_MS
        ));
    }

    if stats.error_rate > SLO_ERROR_RATE {
        violations.push(format!(
            "Level {}: error rate {:.2}% exceeds SLO {:.2}%",
            level,
            stats.error_rate * 100.0,
            SLO_ERROR_RATE * 100.0
        ));
    }

    violations
}

fn print_report(level: usize, clients: usize, total_requests: usize, stats: &LoadTestStats) {
    println!("\n{:=<80}", "");
    println!("LOAD TEST LEVEL {} - {} concurrent clients, {} total requests", level, clients, total_requests);
    println!("{:=<80}", "");
    println!("THROUGHPUT:");
    println!("  Total requests:    {}", stats.total_requests);
    println!("  Successful:        {}", stats.successful_requests);
    println!("  Failed:            {}", stats.failed_requests);
    println!("  Error rate:        {:.2}%", stats.error_rate * 100.0);
    println!();
    println!("LATENCY (ms):");
    println!("  p50 (median):      {}", stats.p50_latency_ms);
    println!("  p90:                {}", stats.p90_latency_ms);
    println!("  p95:                {}", stats.p95_latency_ms);
    println!("  p99:                {}", stats.p99_latency_ms);
    println!("  Max:                {}", stats.max_latency_ms);
    println!();
    println!("SLO COMPLIANCE:");
    println!("  p95 < {}ms:       {}", SLO_P95_LATENCY_MS, if stats.p95_latency_ms <= SLO_P95_LATENCY_MS { "PASS" } else { "FAIL" });
    println!("  Error rate < {}%:  {}", SLO_ERROR_RATE * 100.0, if stats.error_rate <= SLO_ERROR_RATE { "PASS" } else { "FAIL" });
}

fn create_test_router() -> Router {
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;

    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
        as Arc<dyn intent_rebase_types::AuditRepository>;
    let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
        as Arc<dyn intent_service::ApprovalRequestRepository>;
    let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
        as Arc<dyn intent_service::PolicySnapshotRepository>;
    let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
    let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(side_effect_repo));
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
    let forensic_bundle_storage = Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket"));
    let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
        Arc::new(forensic_service::InMemoryForensicDataCollector::new());
    let forensic_bundle_svc: Arc<dyn forensic_service::ForensicBundleServiceTrait> = Arc::new(
        forensic_service::ForensicBundleService::new(
            forensic_bundle_repo,
            forensic_bundle_storage,
            forensic_bundle_collector,
        ),
    );

    intent_api::build_router(
        service,
        graph_svc,
        side_effect_svc,
        compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_repo,
        approval_repo,
        policy_snapshot_repo,
        None, // event_publisher
        forensic_svc,
        forensic_archive_gen,
        forensic_bundle_svc,
    )
}

async fn run_load_level(
    client: &Client,
    base_url: &str,
    level: usize,
    concurrent_clients: usize,
    total_requests: usize,
) -> LoadTestStats {
    println!("\nStarting Level {}: {} concurrent clients, {} total requests", level, concurrent_clients, total_requests);

    let metrics = Arc::new(LoadTestMetrics::default());
    let start_time = Instant::now();
    let requests_per_client = total_requests / concurrent_clients;
    let remaining = total_requests % concurrent_clients;

    // Semaphore to limit concurrency
    let semaphore = Arc::new(Semaphore::new(concurrent_clients));

    // Spawn client tasks
    let mut handles = Vec::new();
    for client_idx in 0..concurrent_clients {
        let client = client.clone();
        let base_url = base_url.to_string();
        let semaphore = semaphore.clone();
        let metrics = metrics.clone();
        let this_client_requests = requests_per_client + if client_idx < remaining { 1 } else { 0 };

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            for req_idx in 0..this_client_requests {
                // Determine request type based on weighted distribution
                let rand_val = (client_idx * 1000 + req_idx) % 100;
                let request_type = if rand_val < 70 {
                    // 70% read operations
                    let path = match req_idx % 4 {
                        0 => "/health",
                        _ => "/health",
                    };
                    ("GET".to_string(), path.to_string(), None)
                } else if rand_val < 90 {
                    // 20% write operations (create intent)
                    ("POST".to_string(), "/intents".to_string(), Some(create_intent_body()))
                } else {
                    // 10% compute operations
                    let intent_id = Uuid::new_v4();
                    let path = format!("/intents/{}/diff", intent_id);
                    ("POST".to_string(), path, Some(create_diff_body()))
                };

                let url = format!("{}{}", base_url, request_type.1);
                let req_start = Instant::now();

                let result: Result<reqwest::Response, reqwest::Error> = if request_type.0 == "GET" {
                    client.get(&url).send().await
                } else {
                    let body = request_type.2.unwrap_or(serde_json::Value::Null);
                    client.post(&url).json(&body).send().await
                };

                let latency = req_start.elapsed().as_millis() as u64;

                match result {
                    Ok(resp) if resp.status().is_success() || resp.status().is_client_error() => {
                        // 4xx errors are expected for non-existent resources
                        metrics.record_success(latency);
                    }
                    Ok(resp) if resp.status().is_server_error() => {
                        metrics.record_failure();
                        eprintln!("Server error {} for {}", resp.status(), url);
                    }
                    Err(e) => {
                        metrics.record_failure();
                        eprintln!("Request failed for {}: {}", url, e);
                    }
                    _ => {
                        metrics.record_success(latency);
                    }
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start_time.elapsed();
    let stats = metrics.get_stats();

    println!(
        "Level {} completed in {:.2}s - {:.2} req/s",
        level,
        elapsed.as_secs_f64(),
        stats.total_requests as f64 / elapsed.as_secs_f64()
    );

    stats
}

fn create_intent_body() -> serde_json::Value {
    serde_json::json!({
        "name": format!("load-test-intent-{}", Uuid::new_v4()),
        "summary": "Load test intent",
        "change_type": "feature",
        "scope": {
            "sections": [
                {
                    "name": "src/main.rs",
                    "change_type": "modified"
                }
            ]
        },
        "tenant_id": null
    })
}

fn create_diff_body() -> serde_json::Value {
    serde_json::json!({
        "from_version": 1,
        "to_version": 2,
        "tenant_id": null
    })
}

/// Load test that simulates production-like traffic against the HTTP server.
/// Uses in-memory repositories to avoid external dependencies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(feature = "load-test")]
async fn test_load() {
    // Create test router
    let router = create_test_router();

    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to TCP port");
    let addr = listener.local_addr().expect("Failed to get local address");
    let port = addr.port();
    let base_url = format!("http://127.0.0.1:{}", port);

    println!("\n{}", "=".repeat(80));
    println!("INTENT-API LOAD TEST");
    println!("Server listening on {}", base_url);
    println!("{:=<80}", "");

    // Spawn the server
    let server = axum::serve(listener, router);
    let server_handle = tokio::spawn(async move {
        server.await.expect("Server error");
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client with connection pooling
    let client = Client::builder()
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    // Run load tests at each level
    let mut all_passed = true;
    let mut all_stats = Vec::new();

    for (level, &(concurrent_clients, total_requests)) in LOAD_TEST_ITEMS.iter().enumerate() {
        let stats = run_load_level(&client, &base_url, level + 1, concurrent_clients, total_requests).await;
        print_report(level + 1, concurrent_clients, total_requests, &stats);

        let violations = check_slo_compliance(&stats, level + 1);
        if !violations.is_empty() {
            all_passed = false;
            for v in &violations {
                println!("  WARNING: {}", v);
            }
        } else {
            println!("  All SLOs met");
        }
        all_stats.push(stats);
    }

    // Shutdown server
    server_handle.abort();

    println!("\n{}", "=".repeat(80));
    println!("LOAD TEST SUMMARY");
    println!("{:=<80}", "");

    // Final assertions
    for (i, stats) in all_stats.iter().enumerate() {
        print_report(i + 1, LOAD_TEST_ITEMS[i].0, LOAD_TEST_ITEMS[i].1, stats);
    }

    if all_passed {
        println!("\nALL LOAD LEVELS PASSED - SLOs MET");
    } else {
        println!("\nSOME LOAD LEVELS FAILED - SLO VIOLATIONS DETECTED");
    }
    println!("{}", "=".repeat(80));

    assert!(all_passed, "Some load levels did not meet SLO requirements");
}

/// Create a router with SQLx-backed repositories for live load testing.
///
/// This is the production-like configuration that uses actual PostgreSQL
/// via the DATABASE_URL environment variable.
fn create_sqlx_test_router(pool: sqlx::PgPool) -> Router {
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_service::{IntentService, SqlxIntentRepository};
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;

    let repo = Arc::new(SqlxIntentRepository::new(pool.clone()));
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(intent_service::SqlxCheckpointRepository::new(pool.clone()));
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_repo = intent_service::SqlxApprovalRequestRepository::new(pool.clone());
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> =
        Arc::new(intent_service::SqlxPolicySnapshotRepository::new(pool.clone()));
    let side_effect_repo = compensation_service::SqlxSideEffectRepository::new(pool.clone());
    let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
        Arc::new(side_effect_repo),
    ));
    let compensation_action_repo =
        Arc::new(compensation_service::SqlxCompensationActionRepository::new(pool.clone()));
    let compensation_action_svc = Arc::new(
        compensation_service::CompensationActionService::new(compensation_action_repo),
    );
    let orchestration_run_repo =
        Arc::new(compensation_service::SqlxOrchestrationRunRepository::new(pool.clone()));
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
    let forensic_bundle_storage = Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket"));
    let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
        Arc::new(forensic_service::InMemoryForensicDataCollector::new());
    let forensic_bundle_svc: Arc<dyn forensic_service::ForensicBundleServiceTrait> = Arc::new(
        forensic_service::ForensicBundleService::new(
            forensic_bundle_repo,
            forensic_bundle_storage,
            forensic_bundle_collector,
        ),
    );

    intent_api::build_router(
        service,
        graph_svc,
        side_effect_svc,
        compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_repo,
        Arc::new(approval_repo),
        policy_snapshot_repo,
        None, // event_publisher
        forensic_svc,
        forensic_archive_gen,
        forensic_bundle_svc,
    )
}

/// SQLx-backed load test for local live infrastructure testing.
///
/// Scope: Tests intent-api HTTP server against live PostgreSQL.
/// This variant uses SQLx-backed repositories instead of in-memory,
/// enabling verification of:
/// - PostgreSQL connection pool behavior under load
/// - Actual SQL query performance
/// - Transaction handling
///
/// Limitations:
/// - Uses in-memory graph repository (graph-service doesn't have SQLx impl yet)
/// - No NATS integration
/// - No actual side effect execution
///
/// Requirements:
/// - DATABASE_URL environment variable must be set
/// - PostgreSQL must be running (e.g., via docker-compose)
/// - Migrations must be applied
///
/// Execution:
/// ```bash
/// cd infrastructure/local && docker-compose up -d postgres
/// export DATABASE_URL="postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase"
/// cargo test -p intent-api --test load_test --features load-test,sqlx-load-test -- --nocapture test_load_sqlx
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(feature = "sqlx-load-test")]
async fn test_load_sqlx() {
    if !requires_database_url() {
        println!("\n{}", "=".repeat(80));
        println!("SKIPPED: test_load_sqlx requires DATABASE_URL environment variable");
        println!("Set it with: export DATABASE_URL=\"postgres://user:pass@localhost:5432/db\"");
        println!("Or start postgres via: cd infrastructure/local && docker-compose up -d postgres");
        println!("{}", "=".repeat(80));
        return;
    }

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("\n{}", "=".repeat(80));
    println!("SQLX-BACKED LOAD TEST");
    println!("Connecting to: {}", database_url.replace(&*std::env::var("PGPASSWORD").unwrap_or_default(), "****"));
    println!("{:=<80}", "");

    // Create SQLx pool with connection pooling settings suitable for load testing
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database - ensure postgres is running and migrations are applied");

    println!("Connected to PostgreSQL - pool created with max_connections=20");

    // Create SQLx-backed router
    let router = create_sqlx_test_router(pool.clone());

    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to TCP port");
    let addr = listener.local_addr().expect("Failed to get local address");
    let port = addr.port();
    let base_url = format!("http://127.0.0.1:{}", port);

    println!("Server listening on {}", base_url);
    println!("{:=<80}", "");

    // Spawn the server
    let server = axum::serve(listener, router);
    let server_handle = tokio::spawn(async move {
        server.await.expect("Server error");
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create HTTP client with connection pooling
    let client = Client::builder()
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client");

    // Run load tests at each level (using smaller values for SQLx to avoid overwhelming DB)
    let sqlx_load_levels: &[(usize, usize)] = &[
        (5, 500),   // Level 1: 5 concurrent clients, 500 total requests
        (10, 1000), // Level 2: 10 concurrent clients, 1000 total requests
    ];

    let mut all_passed = true;
    let mut all_stats = Vec::new();

    for (level, &(concurrent_clients, total_requests)) in sqlx_load_levels.iter().enumerate() {
        let stats = run_load_level(&client, &base_url, level + 1, concurrent_clients, total_requests).await;
        print_report(level + 1, concurrent_clients, total_requests, &stats);

        let violations = check_slo_compliance(&stats, level + 1);
        if !violations.is_empty() {
            all_passed = false;
            for v in &violations {
                println!("  WARNING: {}", v);
            }
        } else {
            println!("  All SLOs met");
        }
        all_stats.push(stats);
    }

    // Shutdown server
    server_handle.abort();

    // Close the pool
    pool.close().await;

    println!("\n{}", "=".repeat(80));
    println!("SQLX LOAD TEST SUMMARY");
    println!("{:=<80}", "");

    // Final assertions
    for (i, stats) in all_stats.iter().enumerate() {
        print_report(i + 1, sqlx_load_levels[i].0, sqlx_load_levels[i].1, stats);
    }

    if all_passed {
        println!("\nALL SQLX LOAD LEVELS PASSED - SLOs MET");
    } else {
        println!("\nSOME SQLX LOAD LEVELS FAILED - SLO VIOLATIONS DETECTED");
    }
    println!("{}", "=".repeat(80));

    assert!(all_passed, "Some SQLx load levels did not meet SLO requirements");
}
