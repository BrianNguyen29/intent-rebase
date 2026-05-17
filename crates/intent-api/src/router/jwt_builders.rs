//! JWT-gated router builders.
//!
//! Bounded router decomposition slice: provides `build_router_with_jwt_auth`
//! and `build_router_with_sql_audit_and_approval_jwt` behind the `jwt-auth`
//! feature. Delegates to the canonical `build_router` and applies JWT
//! middleware as the outermost layer.

use axum::Router;
use graph_service::GraphService;
use intent_service::IntentService;
use rebase_orchestrator::RebaseOrchestrator;
use std::sync::Arc;

use super::auth_middleware::jwt_auth_async;
use super::build_router;

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
    webhook_subscription_repo: Option<
        Arc<dyn crate::webhook_subscription_repo::WebhookSubscriptionRepository>,
    >,
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
        None,
        webhook_subscription_repo,
    );

    // Apply JWT middleware
    router.layer(axum::middleware::from_fn(move |request, next| {
        jwt_auth_async(auth_config.clone(), request, next)
    }))
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
    propagation_record_repo: Option<Arc<dyn intent_service::PropagationRecordRepository>>,
    rls_pool: Option<graph_service::RlsAwarePool>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    webhook_subscription_repo: Option<
        Arc<dyn crate::webhook_subscription_repo::WebhookSubscriptionRepository>,
    >,
) -> Router {
    // Construct SQL-backed audit and approval repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
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
        propagation_record_repo,
        rls_pool,
        webhook_subscription_repo,
    );

    // Apply JWT middleware
    router.layer(axum::middleware::from_fn(move |request, next| {
        jwt_auth_async(auth_config.clone(), request, next)
    }))
}
