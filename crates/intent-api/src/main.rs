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
//! - `INTENT_API_NATS_FULL_CONSUMER` — Enable full NATS consumer with DLQ publishing and additional consumers (default: false, requires INTENT_API_NATS_CONSUMER=true + NATS_URL)
//!   - **NON-PRODUCTION**: This is a local-dev bounded path. Not production-ready.
//! - `INTENT_API_WEBHOOK_OUTBOX_WORKER` — Enable webhook outbox background worker (default: false)
//!   - **NON-PRODUCTION**: Bounded local-dev only. Not production-ready.
//! - `INTENT_API_REQUIRE_JWT` — Require JWT authentication (default: false)
//!   - When true, fails startup if JWT_SECRET is missing or weak
//!   - When false (default), uses dev fallback secret if JWT_SECRET not set

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
    BundleStorage, ForensicArchiveGenerator, ForensicVerificationService, InMemoryBundleStorage,
    InMemoryForensicArchiveGenerator, InMemoryForensicVerificationService,
    RealForensicDataCollector, RealForensicVerificationService, S3BundleStorage,
};
use graph_service::{GraphService, InMemoryGraphRepository, SqlxGraphRepository};
use intent_api::{
    build_router, build_router_with_sql_audit_and_approval, init_panic_hook, init_tracing,
    nats_jetstream::{
        ConsumerRegistry, ConsumerRegistryHandle, DlqMetricsWorkerBuilder, DlqMetricsWorkerConfig,
        DlqMetricsWorkerHandle, DlqReplayWorkerBuilder, DlqReplayWorkerConfig,
        DlqReplayWorkerHandle, JetStreamInitializer,
    },
    webhook_delivery::build_webhook_client,
    webhook_dispatcher::WebhookDeliveryDispatcher,
    webhook_outbox_repo::{InMemoryWebhookOutboxRepository, SqlxWebhookOutboxRepository},
    webhook_outbox_worker::{
        is_webhook_outbox_worker_enabled, maybe_start_webhook_outbox_worker,
        WebhookOutboxWorkerHandle,
    },
    NatsEventPublisher,
};

#[cfg(feature = "jwt-auth")]
use intent_api::build_router_with_sql_audit_and_approval_jwt;
use intent_rebase_types::{EventPublisher, InMemoryEventPublisher, SqlxAuditRepository};
use intent_service::event_consumer::{
    CheckpointCreatorConsumer, InMemoryNotificationStore, NotifierConsumer, SnapshotCreatorConsumer,
};
use intent_service::{
    ApprovalRequestRepository, CheckpointService, InMemoryApprovalRequestRepository,
    InMemoryCheckpointRepository, InMemoryIntentRepository, InMemoryPolicySnapshotRepository,
    IntentService, PolicySnapshotRepository, SqlxCheckpointRepository, SqlxIntentRepository,
    SqlxPolicySnapshotRepository, SqlxPropagationRecordRepository,
};
use rebase_orchestrator::RebaseOrchestrator;
use runtime_adapter::{MockAdapter, RuntimeAdapter};

/// Select the runtime adapter based on INTENT_API_RUNTIME_ADAPTER env var.
///
/// - `INTENT_API_RUNTIME_ADAPTER=temporal` + temporal feature → TemporalAdapter
/// - Otherwise → MockAdapter (default for dev/testing)
///
/// Temporal readiness is NOT claimed; trace propagation (W3C traceparent) is not supported.
async fn select_runtime_adapter() -> Arc<dyn RuntimeAdapter> {
    let adapter_name = std::env::var("INTENT_API_RUNTIME_ADAPTER")
        .unwrap_or_default()
        .to_lowercase();

    if adapter_name == "temporal" {
        #[cfg(feature = "temporal")]
        {
            let config = runtime_adapter::TemporalAdapterConfig {
                target_url: std::env::var("TEMPORAL_ADDRESS")
                    .unwrap_or_else(|_| "http://localhost:7233".to_string()),
                namespace: std::env::var("TEMPORAL_NAMESPACE")
                    .unwrap_or_else(|_| "default".to_string()),
                identity: std::env::var("TEMPORAL_IDENTITY")
                    .unwrap_or_else(|_| "intent-rebase-runtime-adapter".to_string()),
                workflow_query: "ExecutionStatus = 'Running'".to_string(),
                max_checkpoints: 25,
            };
            tracing::info!(
                "INTENT_API_RUNTIME_ADAPTER=temporal — connecting to Temporal at {} (namespace: {})",
                config.target_url,
                config.namespace
            );
            match runtime_adapter::TemporalAdapter::connect(config).await {
                Ok(adapter) => {
                    tracing::info!("TemporalAdapter connected successfully");
                    return Arc::new(adapter);
                }
                Err(e) => {
                    tracing::warn!(
                        "TemporalAdapter connection failed: {} — falling back to MockAdapter",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "temporal"))]
        {
            tracing::warn!(
                "INTENT_API_RUNTIME_ADAPTER=temporal but temporal feature is not enabled — falling back to MockAdapter"
            );
        }
    }

    tracing::info!(
        "Using MockAdapter (default) — set INTENT_API_RUNTIME_ADAPTER=temporal for Temporal"
    );
    Arc::new(MockAdapter::ready())
}

/// Select forensic bundle storage based on FORENSIC_BUNDLE_STORAGE env var.
///
/// - `FORENSIC_BUNDLE_STORAGE=s3` → S3BundleStorage (requires S3_ENDPOINT, S3_ACCESS_KEY, S3_SECRET_KEY, FORENSIC_BUNDLE_BUCKET)
/// - Otherwise → InMemoryBundleStorage (dev/testing only)
///
/// S3 Object Lock, retention enforcement, and chain-hash remain Phase 4+ deferred scope.
async fn select_forensic_bundle_storage() -> Arc<dyn BundleStorage> {
    let storage_type = std::env::var("FORENSIC_BUNDLE_STORAGE")
        .unwrap_or_default()
        .to_lowercase();

    if storage_type == "s3" {
        let bucket = std::env::var("FORENSIC_BUNDLE_BUCKET").unwrap_or_else(|_| {
            tracing::warn!("FORENSIC_BUNDLE_STORAGE=s3 but FORENSIC_BUNDLE_BUCKET not set");
            "intent-rebase-artifacts".to_string()
        });
        let endpoint = std::env::var("S3_ENDPOINT").unwrap_or_else(|_| {
            tracing::warn!("FORENSIC_BUNDLE_STORAGE=s3 but S3_ENDPOINT not set");
            "http://localhost:9000".to_string()
        });
        let access_key = std::env::var("S3_ACCESS_KEY").unwrap_or_else(|_| {
            tracing::warn!("FORENSIC_BUNDLE_STORAGE=s3 but S3_ACCESS_KEY not set");
            "minioadmin".to_string()
        });
        let secret_key = std::env::var("S3_SECRET_KEY").unwrap_or_else(|_| {
            tracing::warn!("FORENSIC_BUNDLE_STORAGE=s3 but S3_SECRET_KEY not set");
            "minioadmin".to_string()
        });

        tracing::info!(
            "FORENSIC_BUNDLE_STORAGE=s3 — using S3BundleStorage with bucket '{}', endpoint '{}'",
            bucket,
            endpoint
        );
        let storage =
            S3BundleStorage::with_endpoint(&endpoint, &access_key, &secret_key, bucket).await;
        Arc::new(storage) as Arc<dyn BundleStorage>
    } else {
        tracing::info!(
            "Using InMemoryBundleStorage (default) — set FORENSIC_BUNDLE_STORAGE=s3 for S3"
        );
        Arc::new(InMemoryBundleStorage::new("prod-bucket")) as Arc<dyn BundleStorage>
    }
}

/// Optionally start the DLQ metrics worker based on INTENT_API_NATS_DLQ_WORKER env var.
///
/// - `INTENT_API_NATS_DLQ_WORKER=true` + JetStream available → starts DlqMetricsWorker
/// - Otherwise → returns None
///
/// The DLQ metrics worker polls DLQ subjects and emits gauge metrics for monitoring.
async fn maybe_start_dlq_metrics_worker(
    jetstream_ctx: Option<async_nats::jetstream::Context>,
    subject_filter: &str,
) -> Option<DlqMetricsWorkerHandle> {
    let dlq_worker_enabled = std::env::var("INTENT_API_NATS_DLQ_WORKER")
        .unwrap_or_default()
        .to_lowercase();

    if dlq_worker_enabled != "true" {
        tracing::info!(
            "INTENT_API_NATS_DLQ_WORKER not set to 'true' — DLQ metrics worker not started"
        );
        return None;
    }

    let jetstream_ctx = match jetstream_ctx {
        Some(ctx) => ctx,
        None => {
            tracing::warn!(
                "INTENT_API_NATS_DLQ_WORKER=true but JetStream context not available — DLQ metrics worker not started"
            );
            return None;
        }
    };

    tracing::info!("INTENT_API_NATS_DLQ_WORKER=true — starting DLQ metrics worker");

    // Derive DLQ subject from subject filter (e.g., "audit.events.v1.>" → "audit.events.v1.DLQ")
    let dlq_subject = subject_filter.replace(".>", ".DLQ");
    let config = DlqMetricsWorkerConfig::new()
        .add_dlq_subject(&dlq_subject)
        .with_poll_interval(std::time::Duration::from_secs(30))
        .with_max_peek(100);

    match DlqMetricsWorkerBuilder::new(jetstream_ctx, config)
        .start()
        .await
    {
        Ok(handle) => {
            tracing::info!("DLQ metrics worker started successfully");
            Some(handle)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to start DLQ metrics worker: {} — continuing without it",
                e
            );
            None
        }
    }
}

/// Optionally start the DLQ replay worker based on INTENT_API_NATS_DLQ_REPLAY_WORKER env var.
///
/// - `INTENT_API_NATS_DLQ_REPLAY_WORKER=true` + JetStream available → starts DlqReplayWorker
/// - Otherwise → returns None
///
/// The DLQ replay worker polls a DLQ subject and replays messages to their original
/// subjects via `DlqHelper::replay_from_dlq()`. Messages are ACKed only on successful replay.
async fn maybe_start_dlq_replay_worker(
    jetstream_ctx: Option<async_nats::jetstream::Context>,
    subject_filter: &str,
) -> Option<DlqReplayWorkerHandle> {
    let replay_worker_enabled = std::env::var("INTENT_API_NATS_DLQ_REPLAY_WORKER")
        .unwrap_or_default()
        .to_lowercase();

    if replay_worker_enabled != "true" {
        tracing::info!(
            "INTENT_API_NATS_DLQ_REPLAY_WORKER not set to 'true' — DLQ replay worker not started"
        );
        return None;
    }

    let jetstream_ctx = match jetstream_ctx {
        Some(ctx) => ctx,
        None => {
            tracing::warn!(
                "INTENT_API_NATS_DLQ_REPLAY_WORKER=true but JetStream context not available — DLQ replay worker not started"
            );
            return None;
        }
    };

    tracing::info!("INTENT_API_NATS_DLQ_REPLAY_WORKER=true — starting DLQ replay worker");

    // Derive DLQ subject from subject filter (e.g., "audit.events.v1.>" → "audit.events.v1.DLQ")
    let dlq_subject = subject_filter.replace(".>", ".DLQ");
    let config = DlqReplayWorkerConfig::new(&dlq_subject)
        .with_poll_interval(std::time::Duration::from_secs(60))
        .with_max_replay(10);

    match DlqReplayWorkerBuilder::new(jetstream_ctx, config)
        .start()
        .await
    {
        Ok(handle) => {
            tracing::info!("DLQ replay worker started successfully");
            Some(handle)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to start DLQ replay worker: {} — continuing without it",
                e
            );
            None
        }
    }
}

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
        None, // propagation_record_repo: None for in-memory mode (Slice 1 bounded)
        None, // rls_pool: None for in-memory mode
        None, // webhook_subscription_repo: None for in-memory mode (Slice 4b bounded)
        None, // webhook_outbox_repo: None for in-memory mode (Slice 5b bounded)
    )
}

/// Build a SQL-backed router (non-JWT version).
///
/// Returns (router, checkpoint_service, dlq_handle, replay_handle, policy_snapshot_repo) when SQL mode is used.
async fn build_sql_router_with_consumer(
    database_url: &str,
) -> Result<
    (
        Router,
        Option<Arc<CheckpointService>>,
        Option<DlqMetricsWorkerHandle>,
        Option<DlqReplayWorkerHandle>,
        Option<Arc<dyn PolicySnapshotRepository>>,
    ),
    Box<dyn std::error::Error>,
> {
    build_sql_router_with_consumer_impl(database_url).await
}

/// JWT-aware SQL-backed router builder that applies JWT middleware.
/// Only available when jwt-auth feature is enabled.
#[cfg(feature = "jwt-auth")]
async fn build_sql_router_with_consumer_jwt(
    database_url: &str,
    auth_config: intent_api::AuthConfig,
) -> Result<
    (
        Router,
        Option<Arc<CheckpointService>>,
        Option<DlqMetricsWorkerHandle>,
        Option<DlqReplayWorkerHandle>,
        Option<Arc<dyn PolicySnapshotRepository>>,
    ),
    Box<dyn std::error::Error>,
> {
    let pool = sqlx::PgPool::connect(database_url).await?;

    // SQL-backed intent repository
    let intent_repo = Arc::new(SqlxIntentRepository::new(pool.clone()));

    // Create RLS-aware pool for tenant-scoped transactions (Phase 3 P3-S5)
    // This enables RLS-aware create_intent/create_version paths when JWT claims are present
    let rls_pool = graph_service::RlsAwarePool::new(pool.clone());

    // Wire RLS pool to IntentService for RLS-aware method availability
    let intent_service =
        Arc::new(IntentService::new(intent_repo.clone()).with_rls_pool(rls_pool.clone()));

    // SQL-backed graph repository
    let graph_repo = Arc::new(SqlxGraphRepository::new(pool.clone()));
    let graph_service = Arc::new(GraphService::new(graph_repo));

    // SQL-backed checkpoint repository
    let checkpoint_repo = Arc::new(SqlxCheckpointRepository::new(pool.clone()));
    let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
    let runtime_adapter = select_runtime_adapter().await;
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

    // Forensic service
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
    let forensic_bundle_repo = Arc::new(forensic_service::SqlxBundleRepository::new(pool.clone()));
    let forensic_bundle_storage = select_forensic_bundle_storage().await;
    let forensic_bundle_collector: Arc<dyn forensic_service::ForensicDataCollector> =
        Arc::new(forensic_service::InMemoryForensicDataCollector::new());
    let forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait> =
        Arc::new(forensic_service::ForensicBundleService::new(
            forensic_bundle_repo,
            forensic_bundle_storage,
            forensic_bundle_collector,
        ));

    let event_publisher: Option<Arc<dyn EventPublisher>> = if std::env::var("NATS_URL").is_ok() {
        Some(Arc::new(NatsEventPublisher::new()) as Arc<dyn EventPublisher>)
    } else {
        None
    };

    // Phase 3 bounded JetStream initialization and DLQ workers startup.
    // Also starts DLQ metrics/replay workers if their respective env gates are true.
    let (dlq_handle, replay_handle): (
        Option<DlqMetricsWorkerHandle>,
        Option<DlqReplayWorkerHandle>,
    ) = if let Ok(nats_url) = std::env::var("NATS_URL") {
        let jetstream_initializer = JetStreamInitializer::new();
        match jetstream_initializer.ensure_stream(&nats_url).await {
            Ok(jetstream_ctx) => {
                tracing::info!(
                    "JetStream stream '{}' ready (subject: {})",
                    jetstream_initializer.stream_name(),
                    jetstream_initializer.subject_filter()
                );
                let subject_filter = jetstream_initializer.subject_filter();
                let dlq_metrics_handle =
                    maybe_start_dlq_metrics_worker(Some(jetstream_ctx.clone()), subject_filter)
                        .await;
                let dlq_replay_handle =
                    maybe_start_dlq_replay_worker(Some(jetstream_ctx), subject_filter).await;
                (dlq_metrics_handle, dlq_replay_handle)
            }
            Err(e) => {
                tracing::warn!(
                    "JetStream initialization failed (NATS may be unavailable): {} — continuing without JetStream",
                    e
                );
                let dlq_metrics_handle =
                    maybe_start_dlq_metrics_worker(None, "audit.events.v1.>").await;
                let dlq_replay_handle =
                    maybe_start_dlq_replay_worker(None, "audit.events.v1.>").await;
                (dlq_metrics_handle, dlq_replay_handle)
            }
        }
    } else {
        let dlq_metrics_handle = maybe_start_dlq_metrics_worker(None, "audit.events.v1.>").await;
        let dlq_replay_handle = maybe_start_dlq_replay_worker(None, "audit.events.v1.>").await;
        (dlq_metrics_handle, dlq_replay_handle)
    };

    // Slice 2: SQL-backed propagation record repository
    let propagation_record_repo: Option<Arc<dyn intent_service::PropagationRecordRepository>> =
        Some(Arc::new(SqlxPropagationRecordRepository::new(pool.clone())));

    // SQL-backed policy snapshot repository (needed for SnapshotCreatorConsumer behind FULL_CONSUMER gate)
    let policy_snapshot_repo: Arc<dyn PolicySnapshotRepository> =
        Arc::new(SqlxPolicySnapshotRepository::new(pool.clone()));

    let router = build_router_with_sql_audit_and_approval_jwt(
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
        auth_config,
        propagation_record_repo,
        Some(rls_pool),
        policy_snapshot_repo.clone(),
        None, // webhook_subscription_repo: local-dev only, not wired in main.rs yet
        None, // webhook_outbox_repo: local-dev only, not wired in main.rs yet
    );

    Ok((
        router,
        Some(checkpoint_service),
        dlq_handle,
        replay_handle,
        Some(policy_snapshot_repo),
    ))
}

/// Non-JWT implementation of SQL-backed router.
async fn build_sql_router_with_consumer_impl(
    database_url: &str,
) -> Result<
    (
        Router,
        Option<Arc<CheckpointService>>,
        Option<DlqMetricsWorkerHandle>,
        Option<DlqReplayWorkerHandle>,
        Option<Arc<dyn PolicySnapshotRepository>>,
    ),
    Box<dyn std::error::Error>,
> {
    let pool = sqlx::PgPool::connect(database_url).await?;

    // SQL-backed intent repository
    let intent_repo = Arc::new(SqlxIntentRepository::new(pool.clone()));

    // Create RLS-aware pool for tenant-scoped transactions (Phase 3 P3-S5)
    // This enables RLS-aware create_intent/create_version paths when JWT claims are present
    let rls_pool = graph_service::RlsAwarePool::new(pool.clone());

    // Wire RLS pool to IntentService for RLS-aware method availability
    let intent_service =
        Arc::new(IntentService::new(intent_repo.clone()).with_rls_pool(rls_pool.clone()));

    // SQL-backed graph repository for production graph service
    let graph_repo = Arc::new(SqlxGraphRepository::new(pool.clone()));
    let graph_service = Arc::new(GraphService::new(graph_repo));

    // SQL-backed checkpoint repository
    let checkpoint_repo = Arc::new(SqlxCheckpointRepository::new(pool.clone()));
    let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
    let runtime_adapter = select_runtime_adapter().await;
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
    let forensic_bundle_repo = Arc::new(forensic_service::SqlxBundleRepository::new(pool.clone()));
    let forensic_bundle_storage = select_forensic_bundle_storage().await;
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
    // Also starts DLQ metrics/replay workers if their respective env gates are true.
    // Fail-safe: if NATS is unavailable, log warning and continue without JetStream.
    let (dlq_handle, replay_handle): (
        Option<DlqMetricsWorkerHandle>,
        Option<DlqReplayWorkerHandle>,
    ) = if let Ok(nats_url) = std::env::var("NATS_URL") {
        let jetstream_initializer = JetStreamInitializer::new();
        match jetstream_initializer.ensure_stream(&nats_url).await {
            Ok(jetstream_ctx) => {
                tracing::info!(
                    "JetStream stream '{}' ready (subject: {})",
                    jetstream_initializer.stream_name(),
                    jetstream_initializer.subject_filter()
                );
                let subject_filter = jetstream_initializer.subject_filter();
                let dlq_metrics_handle =
                    maybe_start_dlq_metrics_worker(Some(jetstream_ctx.clone()), subject_filter)
                        .await;
                let dlq_replay_handle =
                    maybe_start_dlq_replay_worker(Some(jetstream_ctx), subject_filter).await;
                (dlq_metrics_handle, dlq_replay_handle)
            }
            Err(e) => {
                tracing::warn!(
                    "JetStream initialization failed (NATS may be unavailable): {} — continuing without JetStream",
                    e
                );
                let dlq_metrics_handle =
                    maybe_start_dlq_metrics_worker(None, "audit.events.v1.>").await;
                let dlq_replay_handle =
                    maybe_start_dlq_replay_worker(None, "audit.events.v1.>").await;
                (dlq_metrics_handle, dlq_replay_handle)
            }
        }
    } else {
        let dlq_metrics_handle = maybe_start_dlq_metrics_worker(None, "audit.events.v1.>").await;
        let dlq_replay_handle = maybe_start_dlq_replay_worker(None, "audit.events.v1.>").await;
        (dlq_metrics_handle, dlq_replay_handle)
    };

    // Slice 2: SQL-backed propagation record repository
    let propagation_record_repo: Option<Arc<dyn intent_service::PropagationRecordRepository>> =
        Some(Arc::new(SqlxPropagationRecordRepository::new(pool.clone())));

    // SQL-backed policy snapshot repository (needed for SnapshotCreatorConsumer behind FULL_CONSUMER gate)
    let policy_snapshot_repo: Arc<dyn PolicySnapshotRepository> =
        Arc::new(SqlxPolicySnapshotRepository::new(pool.clone()));

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
        propagation_record_repo,
        Some(rls_pool),
        policy_snapshot_repo.clone(),
        None, // webhook_subscription_repo: local-dev only, not wired in main.rs yet
        None, // webhook_outbox_repo: local-dev only, not wired in main.rs yet
    );

    Ok((
        router,
        Some(checkpoint_service),
        dlq_handle,
        replay_handle,
        Some(policy_snapshot_repo),
    ))
}

/// Wait for OS shutdown signals to trigger graceful shutdown.
///
/// On Unix, waits for either SIGINT or SIGTERM.
/// On non-Unix, waits for Ctrl-C.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT, starting graceful shutdown");
            }
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, starting graceful shutdown");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Received Ctrl-C, starting graceful shutdown");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize panic hook for observability (Phase 2b bounded slice)
    // Registered before any async tasks spawn to ensure panics are caught
    init_panic_hook();

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
        if std::env::var("NATS_URL").is_err() {
            return Err("INTENT_API_NATS_CONSUMER=true but NATS_URL is not set. \
                 Set NATS_URL to enable NATS consumer lifecycle."
                .into());
        }
    }

    // =============================================================================
    // NON-PRODUCTION: Full NATS consumer gate (additive, local-dev only)
    // =============================================================================
    // INTENT_API_NATS_FULL_CONSUMER enables additional consumers (SnapshotCreatorConsumer,
    // NotifierConsumer) and app-level DLQ publishing on Failed/Retryable outcomes.
    // This gate is additive: it requires INTENT_API_NATS_CONSUMER=true + NATS_URL.
    // It defaults to OFF and is NOT production-ready.
    let full_consumer_enabled = std::env::var("INTENT_API_NATS_FULL_CONSUMER")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");

    if full_consumer_enabled {
        if !nats_consumer_enabled {
            return Err(
                "INTENT_API_NATS_FULL_CONSUMER=true but INTENT_API_NATS_CONSUMER is not true. \
                 Full consumer requires INTENT_API_NATS_CONSUMER=true."
                    .into(),
            );
        }
        if std::env::var("NATS_URL").is_err() {
            return Err(
                "INTENT_API_NATS_FULL_CONSUMER=true but NATS_URL is not set. \
                 Set NATS_URL to enable the full NATS consumer path."
                    .into(),
            );
        }
        tracing::info!(
            "INTENT_API_NATS_FULL_CONSUMER=true — full consumer path enabled (NON-PRODUCTION local-dev only)"
        );
    }

    // =============================================================================
    // NON-PRODUCTION: Webhook outbox background worker (WEB-LOCAL-2, bounded local-dev)
    // =============================================================================
    // INTENT_API_WEBHOOK_OUTBOX_WORKER enables a background polling loop that
    // discovers tenants with pending outbox records and drives delivery via
    // WebhookDeliveryDispatcher. Default-off; local-dev only.
    //
    // When DATABASE_URL is set, uses SqlxWebhookOutboxRepository.
    // When DATABASE_URL is unset, falls back to InMemoryWebhookOutboxRepository.
    //
    // This is NOT production-ready: no horizontal scaling, no lease semantics,
    // no backpressure, and tenant discovery is bounded to the outbox table.
    let webhook_outbox_worker_enabled = is_webhook_outbox_worker_enabled();
    if webhook_outbox_worker_enabled {
        tracing::info!(
            "INTENT_API_WEBHOOK_OUTBOX_WORKER=true — webhook outbox background worker will be started"
        );
    }

    // =============================================================================
    // Phase 2b: JWT Production Guard (bounded auth first slice)
    // =============================================================================
    //
    // INTENT_API_REQUIRE_JWT=true activates production JWT guard:
    // - Fails startup if JWT_SECRET is missing
    // - Fails startup if JWT_SECRET is too short (< 32 bytes for HS256)
    // - Fails startup if JWT_SECRET matches known weak patterns
    //
    // When INTENT_API_REQUIRE_JWT is false/unset, dev fallback is used (backwards compatible).
    //
    // This guard is a scaffold — full JWT→SQL RLS enforcement remains pending.
    let jwt_required = std::env::var("INTENT_API_REQUIRE_JWT")
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");

    if jwt_required {
        tracing::info!("INTENT_API_REQUIRE_JWT=true — activating JWT production guard");
        #[cfg(feature = "jwt-auth")]
        {
            use intent_api::AuthConfig;

            match AuthConfig::from_env() {
                Ok(config) => {
                    if config.is_production_ready() {
                        tracing::info!(
                            "JWT production guard passed: JWT_SECRET is properly configured"
                        );
                    } else {
                        return Err("INTENT_API_REQUIRE_JWT=true but JWT_SECRET is not production-ready: \
                             secret is too short or matches weak patterns. \
                             Set a strong JWT_SECRET (≥32 bytes, not containing 'dev', 'secret', 'password', etc.)"
                        .into());
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "INTENT_API_REQUIRE_JWT=true but JWT configuration failed: {}",
                        e
                    )
                    .into());
                }
            }
        }
        #[cfg(not(feature = "jwt-auth"))]
        {
            return Err(
                "INTENT_API_REQUIRE_JWT=true but jwt-auth feature is not enabled. \
                             Rebuild with --features jwt-auth to enable JWT authentication."
                    .into(),
            );
        }
    }

    // Load auth_config for SQL router when jwt-auth feature is enabled
    #[cfg(feature = "jwt-auth")]
    let auth_config: Option<intent_api::AuthConfig> = if jwt_required {
        Some(intent_api::AuthConfig::from_env().expect("auth_config already validated above"))
    } else {
        None
    };

    // Build router and get optional checkpoint service for NATS consumer
    let (router, checkpoint_service, sql_dlq_handle, sql_replay_handle, policy_snapshot_repo) =
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            tracing::info!("DATABASE_URL set — using SQL-backed repositories");
            tracing::info!("Connecting to PostgreSQL...");

            // Route to JWT or non-JWT SQL builder based on jwt_required
            #[cfg(feature = "jwt-auth")]
            let sql_result = if jwt_required {
                build_sql_router_with_consumer_jwt(&database_url, auth_config.unwrap()).await
            } else {
                build_sql_router_with_consumer(&database_url).await
            };
            #[cfg(not(feature = "jwt-auth"))]
            let sql_result = build_sql_router_with_consumer(&database_url).await;

            match sql_result {
                Ok((router, checkpoint_service, dlq_handle, replay_handle, policy_repo)) => {
                    tracing::info!("SQL-backed router initialized successfully");
                    (
                        router,
                        checkpoint_service,
                        dlq_handle,
                        replay_handle,
                        policy_repo,
                    )
                }
                Err(e) => {
                    tracing::error!("Failed to connect to database: {}", e);
                    if is_strict_mode {
                        tracing::error!("INTENT_API_STRICT or INTENT_API_PRODUCTION is set — exiting instead of falling back");
                        return Err(
                            format!("Database connection failed in strict mode: {}", e).into()
                        );
                    }
                    tracing::warn!("Falling back to in-memory repositories");
                    tracing::warn!("Set DATABASE_URL properly for production use");
                    (build_inmemory_router(), None, None, None, None)
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
            (build_inmemory_router(), None, None, None, None)
        };

    // Spawn NATS consumer registry if enabled and NATS_URL is configured
    // **Phase 4 bounded slice:** Only CheckpointCreatorConsumer is registered.
    // SnapshotCreatorConsumer and DLQ worker are NOT enabled (Phase 4+ future scope).
    let nats_consumer_handle: Option<ConsumerRegistryHandle> = if nats_consumer_enabled {
        let nats_url = match std::env::var("NATS_URL") {
            Ok(url) => url,
            Err(_) => {
                // This path is unreachable because of the early validation above,
                // but we keep it defensively to avoid any unwrap.
                return Err("INTENT_API_NATS_CONSUMER=true but NATS_URL is not set. \
                     Set NATS_URL to enable NATS consumer lifecycle."
                    .into());
            }
        };

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
        // NON-PRODUCTION: Additional consumers and DLQ publishing are gated behind
        // INTENT_API_NATS_FULL_CONSUMER (requires INTENT_API_NATS_CONSUMER=true + NATS_URL).
        let registry_result: Result<
            ConsumerRegistry,
            intent_api::nats_jetstream::ConsumerRegistryError,
        > = {
            let mut reg = ConsumerRegistry::new();
            reg = reg.register(
                "checkpoint_creator",
                Arc::new(CheckpointCreatorConsumer::new(checkpoint_service)),
                "audit_events",
            )?;

            // NON-PRODUCTION: Full consumer path — register additional consumers and enable DLQ publishing
            if full_consumer_enabled {
                // SnapshotCreatorConsumer requires SQL-backed policy snapshot repository
                if let Some(ref policy_repo) = policy_snapshot_repo {
                    reg = reg.register(
                        "snapshot_creator",
                        Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone())),
                        "audit_events",
                    )?;
                } else {
                    tracing::warn!(
                        "INTENT_API_NATS_FULL_CONSUMER=true but no SQL-backed policy snapshot repository available — SnapshotCreatorConsumer not registered"
                    );
                }

                // NotifierConsumer uses in-memory notification store (bounded local-dev only)
                let notification_store = Arc::new(InMemoryNotificationStore::new());
                reg = reg.register(
                    "notifier",
                    Arc::new(NotifierConsumer::new(notification_store)),
                    "audit_events",
                )?;

                reg = reg.with_full_consumer(true);
            }

            Ok(reg)
        };

        match registry_result {
            Ok(registry) => {
                // Start all registered consumers
                match registry.start_all(&nats_url).await {
                    Ok(handle) => {
                        if full_consumer_enabled {
                            tracing::info!(
                                "NATS consumer registry started (NON-PRODUCTION full-consumer path: checkpoint_creator, snapshot_creator, notifier)"
                            );
                        } else {
                            tracing::info!(
                                "NATS consumer registry started (bounded Phase 4 first slice: checkpoint_creator)"
                            );
                        }
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
        None
    };

    // =============================================================================
    // NON-PRODUCTION: Start webhook outbox background worker (WEB-LOCAL-2)
    // =============================================================================
    let webhook_outbox_worker_handle: Option<WebhookOutboxWorkerHandle> =
        if webhook_outbox_worker_enabled {
            let sender = Arc::new(build_webhook_client());
            let dispatcher = Arc::new(WebhookDeliveryDispatcher::new(sender));

            if let Ok(database_url) = std::env::var("DATABASE_URL") {
                tracing::info!(
                    "DATABASE_URL set — using SQL-backed webhook outbox repository for worker"
                );
                match sqlx::PgPool::connect(&database_url).await {
                    Ok(pool) => {
                        let repo = Arc::new(SqlxWebhookOutboxRepository::new(pool));
                        maybe_start_webhook_outbox_worker(repo, dispatcher)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to connect to database for webhook outbox worker: {} — falling back to in-memory repository",
                            e
                        );
                        let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
                        maybe_start_webhook_outbox_worker(repo, dispatcher)
                    }
                }
            } else {
                tracing::info!(
                    "DATABASE_URL not set — using in-memory webhook outbox repository for worker"
                );
                let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
                maybe_start_webhook_outbox_worker(repo, dispatcher)
            }
        } else {
            None
        };

    tracing::info!("Intent API server starting on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Wait for DLQ metrics worker to finish if it was started
    if let Some(dlq_handle) = sql_dlq_handle {
        tracing::info!("Waiting for DLQ metrics worker to finish...");
        dlq_handle.shutdown();
        dlq_handle.wait_for_all().await;
        tracing::info!("DLQ metrics worker shutdown complete");
    }

    // Wait for DLQ replay worker to finish if it was started
    if let Some(replay_handle) = sql_replay_handle {
        tracing::info!("Waiting for DLQ replay worker to finish...");
        replay_handle.shutdown();
        replay_handle.wait_for_all().await;
        tracing::info!("DLQ replay worker shutdown complete");
    }

    // Wait for NATS consumer to finish if it was started
    if let Some(handle) = nats_consumer_handle {
        tracing::info!("Waiting for NATS consumer to finish...");
        // Signal consumers to stop via handle.shutdown() - this sends on the
        // same watch channel that consumer poll loops are listening to
        handle.shutdown();
        handle.wait_for_all().await;
        tracing::info!("NATS consumer shutdown complete");
    }

    // Wait for webhook outbox worker to finish if it was started
    if let Some(handle) = webhook_outbox_worker_handle {
        tracing::info!("Waiting for webhook outbox worker to finish...");
        handle.shutdown();
        handle.wait_for_all().await;
        tracing::info!("Webhook outbox worker shutdown complete");
    }

    Ok(())
}
