//! Standalone route regression test for parameterized route 404 issue.
//!
//! Run with:
//!   cargo test -p intent-api --test route_regression -- --nocapture

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use rebase_orchestrator::RebaseOrchestrator;
use std::sync::Arc;

fn create_test_router() -> axum::Router {
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
    use runtime_adapter::MockAdapter;

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
    let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
        side_effect_repo,
    ));
    let compensation_action_repo =
        Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
    let compensation_action_svc = Arc::new(compensation_service::CompensationActionService::new(
        compensation_action_repo,
    ));
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
        None,
        forensic_svc,
        forensic_archive_gen,
        forensic_bundle_svc,
        None,
        None,
        None,
        None, // webhook_outbox_repo
    )
}

#[tokio::test]
async fn test_parameterized_routes_do_not_return_404() {
    let router = create_test_router();

    let test_cases = vec![
        ("GET", "/intents/test-id-123"),
        ("POST", "/intents/test-id-123/versions"),
        ("POST", "/intents/test-id-123/diff"),
        ("POST", "/intents/test-id-123/rebase-preview"),
        ("POST", "/intents/test-id-123/rebase-apply"),
    ];

    for (method, path) in test_cases {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();

        let response = router
            .clone()
            .oneshot(req)
            .await
            .expect("Request should not fail at transport level");

        let status = response.status();
        println!("{} {} -> {}", method, path, status);

        // We expect NOT 404 — the route should match.
        // 400/422/500 are acceptable (handler-level errors), but 404 means route didn't match.
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "Route {} {} returned 404 — route parameter syntax may be wrong",
            method,
            path
        );
    }
}
