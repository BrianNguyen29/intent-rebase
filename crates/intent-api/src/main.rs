//! Intent API Server Binary
//!
//! Phase C: Minimal runnable server binary with Docker build path.
//!
//! **Development server** — uses in-memory repositories when DATABASE_URL is not set.
//! This is suitable for local development and smoke testing only.
//!
//! **Production server** — set DATABASE_URL to use SQL-backed repositories.
//!
//! # Environment Variables
//!
//! - `DATABASE_URL` — PostgreSQL connection string. If not set, uses in-memory repositories.
//! - `INTENT_API_BIND_ADDR` — Address to bind to (default: `0.0.0.0:8080`)
//! - `RUST_LOG` — Logging filter (default: `info`)
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` — Optional OTLP endpoint for tracing export
//! - `NATS_URL` — NATS server URL (optional, for event publishing)
//! - `INTENT_API_NATS_CONSUMER` — Enable NATS consumer lifecycle (default: false, requires NATS_URL)

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use compensation_service::{
    CompensationActionRepository, InMemoryCompensationActionRepository,
    InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
    SideEffectRepository, SqlxCompensationActionRepository, SqlxOrchestrationRunRepository,
    SqlxSideEffectRepository,
};
use forensic_service::{
    ForensicArchiveGenerator, ForensicVerificationService, InMemoryForensicArchiveGenerator,
    InMemoryForensicVerificationService, RealForensicDataCollector,
    RealForensicVerificationService,
};
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_api::{
    build_router, build_router_with_sql_audit_and_approval, init_tracing,
    nats_jetstream::{ConsumerRegistry, ConsumerRegistryHandle, JetStreamInitializer},
    NatsEventPublisher,
};
use intent_rebase_types::{EventPublisher, InMemoryEventPublisher, SqlxAuditRepository};
use intent_service::event_consumer::CheckpointCreatorConsumer;
use intent_service::{
    ApprovalRequestRepository, CheckpointService, InMemoryApprovalRequestRepository,
    InMemoryCheckpointRepository, InMemoryIntentRepository, InMemoryPolicySnapshotRepository,
    IntentService, PolicySnapshotRepository, SqlxCheckpointRepository, SqlxIntentRepository,
    SqlxPolicySnapshotRepository,
};
use rebase_orchestrator::RebaseOrchestrator;
use runtime_adapter::MockAdapter;

/// Build an in-memory-based router for development/smoke testing
fn build_inmemory_router() -> Router {
    // In-memory intent repository
    let intent_repo = Arc::new(InMemoryIntentRepository::new());
    let intent_service = Arc::new(IntentService::new(intent_repo));

    // In-memory graph repository
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let graph_service = Arc::new(GraphService::new(graph_repo));

    // In-memory checkpoint repository
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let runtime_adapter = Arc::new(MockAdapter::ready());
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo.clone(),
        graph_service.clone(),
        runtime_adapter,
    ));

    // In-memory audit repository
    let audit_repo: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::InMemoryAuditRepository::new());

    // In-memory approval request repository
    let approval_repo: Arc<dyn ApprovalRequestRepository> =
        Arc::new(InMemoryApprovalRequestRepository::new());

    // In-memory policy snapshot repository
    let policy_snapshot_repo: Arc<dyn PolicySnapshotRepository> =
        Arc::new(InMemoryPolicySnapshotRepository::new());

    // In-memory side effect repository and service
    let side_effect_repo: Arc<dyn SideEffectRepository> =
        Arc::new(InMemorySideEffectRepository::new());
    let side_effect_service = Arc::new(compensation_service::SideEffectService::new(
        side_effect_repo,
    ));

    // In-memory compensation action repository and service
    let compensation_action_repo: Arc<dyn CompensationActionRepository> =
        Arc::new(InMemoryCompensationActionRepository::new());
    let compensation_action_service = Arc::new(
        compensation_service::CompensationActionService::new(compensation_action_repo),
    );

    // In-memory orchestration run repository and runtime
    let orchestration_run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
        compensation_action_service.clone(),
        orchestration_run_repo,
    ));

    // In-memory forensic services
    let forensic_service: Arc<dyn ForensicVerificationService> =
        Arc::new(InMemoryForensicVerificationService::new());
    let forensic_archive_generator: Arc<dyn ForensicArchiveGenerator> =
        Arc::new(InMemoryForensicArchiveGenerator::new());
    let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
    let forensic_bundle_storage =
        Arc::new(forensic_service::InMemoryBundleStorage::new("dev-bucket"));
    let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
        Arc::new(forensic_service::InMemoryForensicDataCollector::new());
    let forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait> =
        Arc::new(forensic_service::ForensicBundleService::new(
            forensic_bundle_repo,
            forensic_bundle_storage,
            forensic_bundle_collector,
        ));

    // In-memory event publisher (no-op for dev)
    let event_publisher: Option<Arc<dyn EventPublisher>> =
        Some(Arc::new(InMemoryEventPublisher::new()));

    build_router(
        intent_service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_repo,
        approval_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
    )
}

/// Build a SQL-backed router and return checkpoint service for NATS consumer.
///
/// Returns (router, checkpoint_service) when SQL mode is used.
/// When falling back to in-memory, returns (router, None).
async fn build_sql_router_with_consumer(
    database_url: &str,
) -> Result<(Router, Option<Arc<CheckpointService>>), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url).await?;

    // SQL-backed intent repository
    let intent_repo = Arc::new(SqlxIntentRepository::new(pool.clone()));
    let intent_service = Arc::new(IntentService::new(intent_repo.clone()));

    // In-memory graph repository (production graph service TBD)
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let graph_service = Arc::new(GraphService::new(graph_repo));

    // SQL-backed checkpoint repository
    let checkpoint_repo = Arc::new(SqlxCheckpointRepository::new(pool.clone()));
    let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
    let runtime_adapter = Arc::new(MockAdapter::ready());
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_service.clone(),
        runtime_adapter,
    ));

    // SQL-backed side effect repository and service
    let side_effect_repo: Arc<dyn SideEffectRepository> =
        Arc::new(SqlxSideEffectRepository::new(pool.clone()));
    let side_effect_service = Arc::new(compensation_service::SideEffectService::new(
        side_effect_repo,
    ));

    let compensation_action_repo: Arc<dyn CompensationActionRepository> =
        Arc::new(SqlxCompensationActionRepository::new(pool.clone()));
    let compensation_action_service = Arc::new(
        compensation_service::CompensationActionService::new(compensation_action_repo),
    );

    let orchestration_run_repo = Arc::new(SqlxOrchestrationRunRepository::new(pool.clone()));
    let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
        compensation_action_service.clone(),
        orchestration_run_repo,
    ));

    // SQL-backed forensic verification service using real collector counts
    let forensic_audit_repo: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(SqlxAuditRepository::new(pool.clone()));
    let forensic_policy_repo: Arc<dyn PolicySnapshotRepository> =
        Arc::new(SqlxPolicySnapshotRepository::new(pool.clone()));
    let forensic_collector = Arc::new(RealForensicDataCollector::new(
        intent_repo.clone(),
        forensic_audit_repo,
        forensic_policy_repo,
    ));
    let forensic_service: Arc<dyn ForensicVerificationService> =
        Arc::new(RealForensicVerificationService::new(forensic_collector));
    let forensic_archive_generator: Arc<dyn ForensicArchiveGenerator> =
        Arc::new(InMemoryForensicArchiveGenerator::new());
    let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
    let forensic_bundle_storage =
        Arc::new(forensic_service::InMemoryBundleStorage::new("prod-bucket"));
    let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
        Arc::new(forensic_service::InMemoryForensicDataCollector::new());
    let forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait> =
        Arc::new(forensic_service::ForensicBundleService::new(
            forensic_bundle_repo,
            forensic_bundle_storage,
            forensic_bundle_collector,
        ));

    // Phase 2b bounded core publisher: NATS event publisher when NATS_URL is configured
    // Uses async-nats with core publish (no JetStream). Fails open on connection/publish errors.
    let event_publisher: Option<Arc<dyn EventPublisher>> = if std::env::var("NATS_URL").is_ok() {
        tracing::info!("NATS_URL configured — enabling NATS event publisher");
        Some(Arc::new(NatsEventPublisher::new()) as Arc<dyn EventPublisher>)
    } else {
        tracing::info!(
            "NATS_URL not set — event publishing disabled (use InMemoryEventPublisher for testing)"
        );
        None
    };

    // Phase 3 bounded JetStream initialization: ensure audit_events stream exists when NATS_URL is configured.
    // Fail-safe: if NATS is unavailable, log warning and continue without JetStream.
    // This is intentional bounded behavior — NATS unavailability should not crash the service.
    if std::env::var("NATS_URL").is_ok() {
        let nats_url = std::env::var("NATS_URL").unwrap();
        let jetstream_initializer = JetStreamInitializer::new();
        match jetstream_initializer.ensure_stream(&nats_url).await {
            Ok(jetstream_ctx) => {
                tracing::info!(
                    "JetStream stream '{}' ready (subject: {})",
                    jetstream_initializer.stream_name(),
                    jetstream_initializer.subject_filter()
                );
                // JetStream context is available for future consumer use (Phase 3 consumer wiring deferred)
                let _ = jetstream_ctx;
            }
            Err(e) => {
                tracing::warn!(
                    "JetStream initialization failed (NATS may be unavailable): {} — continuing without JetStream",
                    e
                );
            }
        }
    }

    let router = build_router_with_sql_audit_and_approval(
        pool,
        intent_service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
    );

    Ok((router, Some(checkpoint_service)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (supports OTLP when OTEL_EXPORTER_OTLP_ENDPOINT is set)
    init_tracing();

    // Get bind address from environment or use default
    let bind_addr: SocketAddr = std::env::var("INTENT_API_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .expect("Failed to parse INTENT_API_BIND_ADDR");

    // Check if DATABASE_URL is set to determine which router to use
    let is_strict_mode = std::env::var("INTENT_API_STRICT")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true")
        || std::env::var("INTENT_API_PRODUCTION")
            .unwrap_or_default()
            .eq_ignore_ascii_case("true");

    // Check if NATS consumer is enabled
    let nats_consumer_enabled = std::env::var("INTENT_API_NATS_CONSUMER")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");

    if nats_consumer_enabled {
        tracing::info!(
            "INTENT_API_NATS_CONSUMER=true — NATS consumer lifecycle enabled (bounded Phase 4 first slice)"
        );
    }

    // Build router and get optional checkpoint service for NATS consumer
    let (router, checkpoint_service) = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        tracing::info!("DATABASE_URL set — using SQL-backed repositories");
        tracing::info!("Connecting to PostgreSQL...");

        match build_sql_router_with_consumer(&database_url).await {
            Ok((router, checkpoint_service)) => {
                tracing::info!("SQL-backed router initialized successfully");
                (router, checkpoint_service)
            }
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                if is_strict_mode {
                    tracing::error!("INTENT_API_STRICT or INTENT_API_PRODUCTION is set — exiting instead of falling back");
                    return Err(format!("Database connection failed in strict mode: {}", e).into());
                }
                tracing::warn!("Falling back to in-memory repositories");
                tracing::warn!("Set DATABASE_URL properly for production use");
                (build_inmemory_router(), None)
            }
        }
    } else {
        if is_strict_mode {
            tracing::error!(
                "INTENT_API_STRICT or INTENT_API_PRODUCTION is set but DATABASE_URL is not set"
            );
            return Err("DATABASE_URL must be set in strict/production mode".into());
        }
        tracing::info!("DATABASE_URL not set — using in-memory repositories");
        tracing::warn!("This is suitable for development/smoke testing only");
        tracing::warn!("Set DATABASE_URL for production deployments");
        (build_inmemory_router(), None)
    };

    // Spawn NATS consumer registry if enabled and NATS_URL is configured
    // **Phase 4 bounded slice:** Only CheckpointCreatorConsumer is registered.
    // SnapshotCreatorConsumer and DLQ worker are NOT enabled (Phase 4+ future scope).
    let nats_consumer_handle: Option<ConsumerRegistryHandle> = if nats_consumer_enabled
        && std::env::var("NATS_URL").is_ok()
    {
        let nats_url = std::env::var("NATS_URL").unwrap();

        // Get checkpoint service (required for CheckpointCreatorConsumer)
        let checkpoint_service = match checkpoint_service {
            Some(cs) => cs,
            None => {
                tracing::warn!(
                        "INTENT_API_NATS_CONSUMER=true but no SQL-backed checkpoint repository available",
                    );
                tracing::warn!("Falling back to in-memory checkpoint repository for consumer");
                Arc::new(CheckpointService::new(Arc::new(
                    InMemoryCheckpointRepository::new(),
                )))
            }
        };

        // Build the consumer registry with CheckpointCreatorConsumer
        // **Bounded:** Only checkpoint consumer enabled; room for future Snapshot/DLQ consumers
        let registry_result = ConsumerRegistry::new().register(
            "checkpoint_creator",
            Arc::new(CheckpointCreatorConsumer::new(checkpoint_service)),
            "audit_events",
        );

        match registry_result {
            Ok(registry) => {
                // Start all registered consumers
                match registry.start_all(&nats_url).await {
                    Ok(handle) => {
                        tracing::info!(
                                "NATS consumer registry started (bounded Phase 4 first slice: checkpoint_creator)"
                            );
                        Some(handle)
                    }
                    Err(e) => {
                        tracing::warn!(
                                "Failed to start NATS consumer registry: {} — continuing without consumer",
                                e
                            );
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to register NATS consumer: {} — continuing without consumer",
                    e
                );
                None
            }
        }
    } else {
        if nats_consumer_enabled {
            tracing::info!(
                "INTENT_API_NATS_CONSUMER=true but NATS_URL not set — consumer not started"
            );
        }
        None
    };

    tracing::info!("Intent API server starting on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, router).await?;

    // Wait for NATS consumer to finish if it was started
    if let Some(handle) = nats_consumer_handle {
        tracing::info!("Waiting for NATS consumer to finish...");
        // Signal consumers to stop via handle.shutdown() - this sends on the
        // same watch channel that consumer poll loops are listening to
        handle.shutdown();
        handle.wait_for_all().await;
        tracing::info!("NATS consumer shutdown complete");
    }

    Ok(())
}
