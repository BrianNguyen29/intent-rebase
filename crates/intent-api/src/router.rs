//! Router building and authentication middleware for intent-api.
//!
//! This module contains the canonical router builders used to wire up the HTTP transport layer.
//! It is extracted from lib.rs as a bounded module decomposition slice.

#[allow(unused_imports)]
use axum::http::StatusCode;
use axum::{
    routing::{get, post},
    Router,
};
use graph_service::GraphService;
use intent_service::IntentService;
use rebase_orchestrator::RebaseOrchestrator;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// Re-export handler modules for use in router building
pub use crate::approval_handlers_readonly;
pub use crate::approval_mutation_handlers;
pub use crate::batch_handlers;
pub use crate::compensation_mutation_handlers;
pub use crate::compensation_planner_handlers;
pub use crate::compensation_query_handlers;
pub use crate::diff_handlers;
pub use crate::forensic_handlers;
pub use crate::graph_handlers;
pub use crate::health_routes;
pub use crate::ingest_handlers;
pub use crate::intent_mutation_handlers;
pub use crate::intent_read_handlers;
pub use crate::intent_validation_handlers;
pub use crate::orchestration_run_handlers;
pub use crate::policy_snapshot_handlers;
pub use crate::rebase_apply_handlers;
pub use crate::rebase_preview_handlers;
pub use crate::replay_handlers;
pub use crate::simulation_handlers;
pub use crate::trigger_reapproval_handlers;

/// Build the Phase 1 router with CORS enabled
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
/// When `None` (default), audit events are persisted but NOT streamed.
/// When `Some`, events are also published to the event stream (best-effort, fail-open).
#[allow(clippy::too_many_arguments)]
pub fn build_router(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    let state = crate::AppState {
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        start_time: Instant::now(),
        rls_pool,
    };

    Router::new()
        .route("/health", get(health_routes::health_handler))
        .route("/ready", get(health_routes::ready_handler))
        .route("/metrics", get(health_routes::metrics_handler))
        .route(
            "/v1/intents/validate",
            post(crate::intent_validation_handlers::validate_intent),
        )
        .route(
            "/intents",
            post(crate::intent_mutation_handlers::create_intent),
        )
        .route(
            "/intents/{intent_id}",
            get(crate::intent_read_handlers::get_intent_head),
        )
        .route(
            "/intents/{intent_id}/versions",
            post(crate::intent_mutation_handlers::create_version),
        )
        .route(
            "/intents/{intent_id}/versions",
            get(crate::intent_read_handlers::list_versions),
        )
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(crate::intent_read_handlers::get_version),
        )
        .route(
            "/intents/{intent_id}/diff",
            post(crate::diff_handlers::compute_diff),
        )
        .route(
            "/intents/{intent_id}/rebase-preview",
            post(crate::rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/{intent_id}/rebase-apply",
            post(crate::rebase_apply_handlers::rebase_apply),
        )
        // Replay endpoint (Phase 2b bounded replay slice)
        .route(
            "/intents/{intent_id}/replay",
            post(crate::replay_handlers::replay_intent),
        )
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route(
            "/intents/{intent_id}/side-effects",
            get(crate::query_handlers::list_side_effects),
        )
        // N4-4: Rebase simulation endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/intents/{intent_id}/rebase-simulation",
            get(crate::simulation_handlers::rebase_simulation),
        )
        // N4-4 POST: Compensation simulation run endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/compensation-simulation/run",
            post(crate::simulation_handlers::compensation_simulation_run),
        )
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/orchestration-dashboard",
            get(crate::query_handlers::get_orchestration_dashboard),
        )
        // Compensation actions query endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/compensation-actions",
            get(crate::compensation_query_handlers::list_compensation_actions),
        )
        // Compensation action mutation endpoints (Phase 3 Batch 1 bounded execution slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/{action_id}/approve",
            post(crate::compensation_mutation_handlers::approve_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/waive",
            post(crate::compensation_mutation_handlers::waive_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/execute",
            post(crate::compensation_mutation_handlers::execute_compensation_action),
        )
        // Compensation action manual retry and DLQ endpoints (Phase 3 Batch 1 bounded manual retry slice)
        .route(
            "/compensation-actions/{action_id}/reapprove",
            post(crate::compensation_mutation_handlers::reapprove_compensation_action),
        )
        // Bounded compensation planner endpoint (Phase 3 bounded planner slice)
        .route(
            "/compensation-actions/plan",
            post(crate::compensation_planner_handlers::plan_compensation_actions),
        )
        .route(
            "/compensation-actions/dlq",
            get(crate::compensation_query_handlers::list_dlq_candidates),
        )
        // Batch candidates query endpoint (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
        .route(
            "/compensation-actions/batch-candidates",
            get(crate::compensation_query_handlers::list_batch_candidates),
        )
        // Policy gate evaluation endpoints (Phase 3 Batch 1 bounded read-only slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/policy-gate",
            get(crate::compensation_query_handlers::get_compensation_policy_gate),
        )
        .route(
            "/intents/{intent_id}/compensation-policy-gate",
            get(crate::compensation_query_handlers::get_intent_compensation_policy_gate),
        )
        // Orchestration coordination status endpoints (Phase 3 Batch 1 bounded read-only orchestration view)
        .route(
            "/compensation-actions/orchestration-coordination",
            get(crate::compensation_query_handlers::get_orchestration_coordination),
        )
        .route(
            "/intents/{intent_id}/orchestration-coordination",
            get(crate::compensation_query_handlers::get_intent_orchestration_coordination),
        )
        // Manual orchestration & dry-run planner endpoints (Phase 3 Batch 1 bounded slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/orchestration-dry-run",
            post(crate::compensation_planner_handlers::orchestration_dry_run),
        )
        .route(
            "/compensation-actions/batch-approve",
            post(crate::batch_handlers::batch_approve_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-reapprove",
            post(crate::batch_handlers::batch_reapprove_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-execute",
            post(crate::batch_handlers::batch_execute_compensation_actions),
        )
        // Orchestration run endpoints (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
        .route(
            "/compensation-actions/runs",
            post(crate::orchestration_run_handlers::create_orchestration_run),
        )
        .route(
            "/compensation-actions/runs/{run_id}",
            get(crate::orchestration_run_handlers::get_orchestration_run),
        )
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route(
            "/v1/graph/nodes",
            post(crate::graph_handlers::create_graph_node),
        )
        .route(
            "/v1/graph/nodes",
            get(crate::graph_handlers::list_graph_nodes),
        )
        .route(
            "/v1/graph/nodes/{node_id}",
            get(crate::graph_handlers::get_graph_node),
        )
        .route(
            "/v1/graph/edges",
            post(crate::graph_handlers::create_graph_edge),
        )
        .route(
            "/v1/graph/edges",
            get(crate::graph_handlers::list_graph_edges),
        )
        .route(
            "/v1/graph/nodes/{node_id}/edges",
            get(crate::graph_handlers::list_edges_from_node),
        )
        // Artifact ingest with optional side effect capture (Phase 3 Batch 1 groundwork)
        .route(
            "/v1/graph/artifacts",
            post(crate::ingest_handlers::ingest_artifact),
        )
        // Approval request endpoints (Phase 2b bounded slice)
        .route(
            "/approval-requests/pending",
            get(crate::approval_handlers_readonly::list_pending_approval_requests),
        )
        .route(
            "/approval-requests/{approval_request_id}/approve",
            post(crate::approval_mutation_handlers::approve_approval_request),
        )
        .route(
            "/approval-requests/{approval_request_id}/reject",
            post(crate::approval_mutation_handlers::reject_approval_request),
        )
        // POST expire - bounded manual expiry transition (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/expire",
            post(crate::approval_mutation_handlers::expire_approval_request),
        )
        // GET revalidate - bounded read-only scope comparison (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/revalidate",
            get(crate::approval_handlers_readonly::revalidate_approval_request),
        )
        // ADR-07: POST trigger-reapproval - bounded re-approval trigger (Phase 2b)
        .route(
            "/approval-requests/trigger-reapproval",
            post(crate::trigger_reapproval_handlers::trigger_reapproval),
        )
        // Policy snapshot endpoints (Phase 2 bounded read-only slice)
        .route(
            "/policy-snapshots/{snapshot_id}",
            get(crate::policy_snapshot_handlers::get_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/latest",
            get(crate::policy_snapshot_handlers::get_latest_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/versions/{version}",
            get(crate::policy_snapshot_handlers::get_policy_snapshot_by_version),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}",
            get(crate::policy_snapshot_handlers::list_policy_snapshots),
        )
        // Forensic verification endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/verify",
            post(crate::forensic_handlers::verify_forensic_bundle),
        )
        // Forensic archive export endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/export",
            post(crate::forensic_handlers::export_forensic_archive),
        )
        // Forensic bundle generation endpoint (P4 bounded slice)
        .route(
            "/forensic/bundle",
            post(crate::forensic_handlers::create_forensic_bundle),
        )
        // Forensic bundle listing endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles",
            get(crate::forensic_handlers::list_forensic_bundles),
        )
        // Forensic bundle download endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles/{bundle_id}/download",
            get(crate::forensic_handlers::download_forensic_bundle),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        // Trace context middleware must run AFTER request_id_middleware so that
        // the span created here is a child of any extracted trace context.
        .layer(axum::middleware::from_fn(
            health_routes::request_id_middleware,
        ))
        .layer(axum::middleware::from_fn(
            health_routes::trace_context_middleware,
        ))
        .layer(TraceLayer::new_for_http())
}

/// JWT authentication middleware for protected routes.
///
/// Public paths (/health, /ready, /metrics) bypass JWT validation.
#[cfg(feature = "jwt-auth")]
async fn jwt_auth_async(
    auth_config: crate::auth::AuthConfig,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    const PUBLIC_PATHS: &[&str] = &["/health", "/ready", "/metrics"];
    let path = request.uri().path();

    // Skip JWT check for public paths
    if PUBLIC_PATHS.contains(&path) {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok());

    match auth_header {
        Some(auth_value) if auth_value.starts_with("Bearer ") => {
            let token = &auth_value[7..];
            match decode::<crate::auth::Claims>(
                token,
                &DecodingKey::from_secret(auth_config.jwt_secret.as_bytes()),
                &Validation::new(auth_config.algorithm),
            ) {
                Ok(token_data) => {
                    let mut request = request;
                    request.extensions_mut().insert(token_data.claims);
                    next.run(request).await
                }
                Err(_) => axum::response::Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("Invalid or expired token".into())
                    .unwrap(),
            }
        }
        _ => axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body("Missing or invalid Authorization header".into())
            .unwrap(),
    }
}

/// Build a router with JWT authentication middleware applied to protected routes.
///
/// Public routes (health, ready, metrics) are NOT protected by JWT.
/// All other routes require a valid JWT in the Authorization header.
///
/// This builder delegates to [`build_router`] with `rls_pool: None`, so it is
/// intended for in-memory or testing JWT setups. Production deployments that
/// require both JWT authentication and SQL-backed audit/approval repositories
/// (with optional RLS) should use [`build_router_with_sql_audit_and_approval_jwt`]
/// instead.
#[cfg(feature = "jwt-auth")]
#[allow(clippy::too_many_arguments)]
#[allow(unused_variables)]
pub fn build_router_with_jwt_auth(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    auth_config: crate::auth::AuthConfig,
) -> Router {
    let router = build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        None,
    );

    // Apply JWT middleware
    router.layer(axum::middleware::from_fn(move |request, next| {
        jwt_auth_async(auth_config.clone(), request, next)
    }))
}

/// Build the router with SQL-backed audit and approval repositories.
///
/// This is the production bootstrap helper that constructs SQL-backed repositories
/// from a `PgPool` and injects them into the router. Use this in production
/// deployments where PostgreSQL-backed persistence is required.
///
/// For testing or in-memory deployments, use `build_router` directly with
/// `InMemoryAuditRepository` and `InMemoryApprovalRequestRepository`.
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_sql_audit_and_approval(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    // Construct SQL-backed audit, approval, and policy snapshot repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
    );
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> = Arc::new(
        intent_service::SqlxPolicySnapshotRepository::new(pool.clone()),
    );

    build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        rls_pool,
    )
}

/// Build the router with SQL-backed audit and approval repositories AND JWT authentication.
///
/// This is the production bootstrap helper for deployments that require both SQL-backed
/// repositories and JWT authentication. Use this when `INTENT_API_REQUIRE_JWT=true`.
///
/// Requires `jwt-auth` feature to be enabled.
#[cfg(feature = "jwt-auth")]
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_sql_audit_and_approval_jwt(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    auth_config: crate::auth::AuthConfig,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    // Construct SQL-backed audit, approval, and policy snapshot repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
    );
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> = Arc::new(
        intent_service::SqlxPolicySnapshotRepository::new(pool.clone()),
    );

    let router = build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        rls_pool,
    );

    // Apply JWT middleware
    router.layer(axum::middleware::from_fn(move |request, next| {
        jwt_auth_async(auth_config.clone(), request, next)
    }))
}
