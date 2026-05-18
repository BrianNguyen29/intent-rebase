//! Router building and authentication middleware for intent-api.
//!
//! This module contains the canonical router builders used to wire up the HTTP transport layer.
//! It is extracted from lib.rs as a bounded module decomposition slice.

use axum::http::{header, Method};
use axum::Router;
use graph_service::GraphService;
use intent_service::IntentService;
use rebase_orchestrator::RebaseOrchestrator;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::health_routes;
use crate::routes;

/// Build a CORS layer from the `INTENT_API_CORS_ALLOWED_ORIGINS` env var.
///
/// Defaults to safe localhost origins for local-dev use.
/// Origins are comma-separated (e.g. `http://localhost:3000,http://127.0.0.1:3000`).
/// Unparseable origins are silently ignored.
pub fn build_cors_layer() -> CorsLayer {
    let raw = std::env::var("INTENT_API_CORS_ALLOWED_ORIGINS").unwrap_or_default();
    let origins: Vec<axum::http::HeaderValue> = if raw.trim().is_empty() {
        vec![
            "http://localhost:3000".parse().unwrap(),
            "http://127.0.0.1:3000".parse().unwrap(),
        ]
    } else {
        raw.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    };

    let allow_origin = if origins.len() == 1 {
        AllowOrigin::exact(origins[0].clone())
    } else {
        AllowOrigin::list(origins)
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
}

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
    propagation_record_repo: Option<Arc<dyn intent_service::PropagationRecordRepository>>,
    rls_pool: Option<graph_service::RlsAwarePool>,
    webhook_subscription_repo: Option<
        Arc<dyn crate::webhook_subscription_repo::WebhookSubscriptionRepository>,
    >,
    webhook_outbox_repo: Option<Arc<dyn crate::webhook_outbox_repo::WebhookOutboxRepository>>,
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
        propagation_record_repo,
        start_time: Instant::now(),
        rls_pool,
        webhook_subscription_repo,
        webhook_outbox_repo,
    };

    let router = routes::health::add_routes(Router::new());
    let router = routes::intent::add_routes(router);
    let router = routes::propagation::add_routes(router);
    let router = routes::compensation::add_routes(router);
    let router = routes::graph::add_routes(router);
    let router = routes::approval::add_routes(router);
    let router = routes::policy::add_routes(router);
    let router = routes::forensic::add_routes(router);
    let router = routes::webhook::add_routes(router);

    router
        .with_state(state)
        .layer(build_cors_layer())
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

pub mod auth_middleware;
pub mod jwt_builders;

#[cfg(feature = "jwt-auth")]
pub use jwt_builders::{build_router_with_jwt_auth, build_router_with_sql_audit_and_approval_jwt};

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
    propagation_record_repo: Option<Arc<dyn intent_service::PropagationRecordRepository>>,
    rls_pool: Option<graph_service::RlsAwarePool>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    webhook_subscription_repo: Option<
        Arc<dyn crate::webhook_subscription_repo::WebhookSubscriptionRepository>,
    >,
    webhook_outbox_repo: Option<Arc<dyn crate::webhook_outbox_repo::WebhookOutboxRepository>>,
) -> Router {
    // Construct SQL-backed audit and approval repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
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
        propagation_record_repo,
        rls_pool,
        webhook_subscription_repo,
        webhook_outbox_repo,
    )
}

#[cfg(test)]
mod tests {
    use super::build_cors_layer;

    #[test]
    fn test_build_cors_layer_default_does_not_panic() {
        let _layer = build_cors_layer();
    }

    #[test]
    fn test_build_cors_layer_custom_origins() {
        temp_env::with_var(
            "INTENT_API_CORS_ALLOWED_ORIGINS",
            Some("http://example.com,http://example.org"),
            || {
                let _layer = build_cors_layer();
            },
        );
    }

    #[test]
    fn test_build_cors_layer_invalid_origins_ignored() {
        temp_env::with_var(
            "INTENT_API_CORS_ALLOWED_ORIGINS",
            Some("not-a-valid-origin,http://valid.com"),
            || {
                let _layer = build_cors_layer();
            },
        );
    }
}
