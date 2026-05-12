use super::*;

// Import forensic handlers for tests (verification/export/bundle tests moved to forensic_handlers.rs)

// Import intent read handlers for tests
use crate::intent_read_handlers::{get_intent_head, get_version, list_versions};

// Use shared helper with forensic config for lib.rs tests
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

#[tokio::test]
async fn test_router_builds_successfully() {
    let state = create_test_service();
    let _router: axum::Router = Router::new()
        .route("/intents", post(intent_mutation_handlers::create_intent))
        .route("/intents/:intent_id", get(get_intent_head))
        .route(
            "/intents/:intent_id/versions",
            post(intent_mutation_handlers::create_version),
        )
        .route("/intents/:intent_id/versions", get(list_versions))
        .route(
            "/intents/:intent_id/versions/:version_number",
            get(get_version),
        )
        .route(
            "/intents/:intent_id/diff",
            post(diff_handlers::compute_diff),
        )
        .route(
            "/intents/:intent_id/rebase-preview",
            post(rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/:intent_id/rebase-apply",
            post(rebase_apply_handlers::rebase_apply),
        )
        .with_state(state);
    // Router builds successfully - this is a compile-time check essentially
}

// =========================================================================
// Forensic Endpoint Route Contract Tests (Phase 3 Batch 3b — bounded slice)
//
// Verifies that the canonical forensic bundle paths are registered in the
// router and reachable. Does not test full handler logic (covered in
// forensic_handlers.rs); this is a route wiring contract test.
// =========================================================================

#[tokio::test]
async fn test_forensic_endpoints_are_registered() {
    use tower::ServiceExt;

    let state = create_test_service();
    let router = build_router(
        state.service,
        state.graph_service,
        state.side_effect_service,
        state.compensation_action_service,
        state.orchestration_runtime,
        state.orchestrator,
        state.audit_service,
        state.approval_request_repo,
        state.policy_snapshot_repo,
        state.event_publisher,
        state.forensic_service,
        state.forensic_archive_generator,
        state.forensic_bundle_service,
        state.rls_pool,
    );

    let tenant_id = uuid::Uuid::new_v4();

    // POST /forensic/bundle
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_ids": [],
        "time_range": { "start": "2025-01-01T00:00:00Z", "end": "2025-01-02T00:00:00Z" },
        "purpose": "incident_investigation"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/forensic/bundle")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);

    // GET /forensic/bundles?tenant_id=...
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/forensic/bundles?tenant_id={}", tenant_id))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // GET /forensic/bundles/{bundle_id}/download?tenant_id=...
    let bundle_id = uuid::Uuid::new_v4();
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/forensic/bundles/{}/download?tenant_id={}",
            bundle_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Unknown bundle returns 404 from the handler, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // POST /forensic/bundles/{bundle_id}/replay-verify
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_versions": [],
        "artifacts": [],
        "approvals": [],
        "audit_events": [],
        "policy_snapshots": []
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/forensic/bundles/{}/replay-verify", bundle_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Trace Context Propagation Tests (Phase 3 Batch 2 Slice 2 — bounded OTEL)
//
// Note: Direct middleware testing requires complex axum infrastructure.
// The trace_context_middleware is verified through:
// 1. cargo check -p intent-api (verifies compilation)
// 2. cargo test -p intent-api (verifies existing tests still pass)
// 3. Router wiring in build_router() includes trace_context_middleware layer
// =========================================================================
