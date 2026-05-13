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
        state.propagation_record_repo.clone(),
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
// ImpactReport Route Contract Test (Phase 2 bounded MVP)
// =========================================================================

#[tokio::test]
async fn test_impact_report_route_is_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let tenant_id = uuid::Uuid::new_v4();
    let intent_id = uuid::Uuid::new_v4();

    // GET /intents/{intent_id}/impact-report?tenant_id=...&from_version=1&to_version=2
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/intents/{}/impact-report?tenant_id={}&from_version=1&to_version=2",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();

    // Intent does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Rebase Preview / Apply Route Contract Tests
//
// Verifies that the core rebase endpoints are registered and reachable.
// Does not test full handler logic (covered in rebase_preview_tests.rs
// and rebase_apply_handler_tests.rs); this is a route wiring contract test.
// =========================================================================

#[tokio::test]
async fn test_rebase_preview_apply_routes_are_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let intent_id = uuid::Uuid::new_v4();

    // POST /intents/{intent_id}/rebase-preview
    let body = serde_json::to_vec(&serde_json::json!({
        "from_version": 1,
        "to_version": 2
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/intents/{}/rebase-preview", intent_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Intent does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // POST /intents/{intent_id}/rebase-apply
    let body = serde_json::to_vec(&serde_json::json!({
        "from_version": 1,
        "to_version": 2
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/intents/{}/rebase-apply", intent_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Intent does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Policy Snapshot Route Contract Tests
//
// Verifies that policy snapshot endpoints are registered and reachable.
// Does not test full handler logic (covered in policy_snapshot_handlers.rs);
// this is a route wiring contract test.
// =========================================================================

#[tokio::test]
async fn test_policy_snapshot_routes_are_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let tenant_id = uuid::Uuid::new_v4();
    let intent_id = uuid::Uuid::new_v4();
    let snapshot_id = uuid::Uuid::new_v4();

    // GET /policy-snapshots/{snapshot_id}
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/policy-snapshots/{}?tenant_id={}",
            snapshot_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Snapshot does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // GET /policy-snapshots/intent/{intent_id}/latest
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/policy-snapshots/intent/{}/latest?tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // No snapshot for intent → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // GET /policy-snapshots/intent/{intent_id}/versions/{version}
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/policy-snapshots/intent/{}/versions/1?tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // No snapshot for version → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // GET /policy-snapshots/intent/{intent_id}
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/policy-snapshots/intent/{}?tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // No snapshots → handler returns 200 with empty list, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // GET /policy-snapshots/{snapshot_id}/impact-report
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/policy-snapshots/{}/impact-report?tenant_id={}&from_version=1&to_version=2",
            snapshot_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Snapshot does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Compensation Mutation Route Contract Tests
//
// Verifies that compensation action mutation endpoints are registered and
// reachable. Does not test full handler logic (covered in
// compensation_mutation_handlers.rs); this is a route wiring contract test.
// =========================================================================

#[tokio::test]
async fn test_compensation_mutation_routes_are_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let action_id = uuid::Uuid::new_v4();

    // POST /compensation-actions/{action_id}/approve
    let body = serde_json::to_vec(&serde_json::json!({
        "lock_version": 1
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/compensation-actions/{}/approve", action_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Action does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // POST /compensation-actions/{action_id}/waive
    let body = serde_json::to_vec(&serde_json::json!({
        "lock_version": 1
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/compensation-actions/{}/waive", action_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Action does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));

    // POST /compensation-actions/{action_id}/execute
    let body = serde_json::to_vec(&serde_json::json!({
        "executed_by": "test-runner"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/compensation-actions/{}/execute", action_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    // Action does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Propagation Status Route Contract Test (Phase 4+ design-only; bounded stub)
// =========================================================================

#[tokio::test]
async fn test_propagation_status_route_is_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let tenant_id = uuid::Uuid::new_v4();
    let intent_id = uuid::Uuid::new_v4();

    // GET /intents/{intent_id}/propagation-status?tenant_id=...
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/intents/{}/propagation-status?tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();

    // Intent does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// Propagation Signal Ingestion Route Contract Test (Slice 2 bounded)
// =========================================================================

#[tokio::test]
async fn test_propagation_signal_route_is_registered() {
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
        state.propagation_record_repo.clone(),
        state.rls_pool,
    );

    let tenant_id = uuid::Uuid::new_v4();
    let intent_id = uuid::Uuid::new_v4();

    // POST /intents/{intent_id}/propagation-signals
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/intents/{}/propagation-signals", intent_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(format!(
            "{{\"tenant_id\":\"{}\",\"downstream_system_id\":\"test-system\",\"last_seen_version\":1}}",
            tenant_id
        )))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();

    // Intent does not exist → handler returns 404, proving the route is wired
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(content_type, Some("application/json"));
}

// =========================================================================
// OpenAPI Drift Guard Test
//
// Lightweight string-search verification that key implemented routes are
// documented in the OpenAPI spec. This is a drift guard, not a parser test.
// If this test fails, the route was likely added to router.rs without updating
// docs/04-api/openapi.yaml.
// =========================================================================

#[test]
fn test_key_routes_are_documented_in_openapi() {
    let openapi_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/04-api/openapi.yaml"
    );
    let spec = std::fs::read_to_string(openapi_path)
        .expect("openapi.yaml should exist; run from repo root");

    let required_paths = [
        "/intents/{intent_id}/impact-report",
        "/intents/{intent_id}/rebase-preview",
        "/intents/{intent_id}/rebase-apply",
        "/intents/{intent_id}/propagation-status",
        "/intents/{intent_id}/propagation-signals",
        "/forensic/bundle",
        "/forensic/bundles",
        "/forensic/bundles/{bundle_id}/download",
        "/compensation-actions/{action_id}/approve",
        "/compensation-actions/{action_id}/execute",
        "/policy-snapshots/{snapshot_id}",
        "/policy-snapshots/intent/{intent_id}/latest",
        "/policy-snapshots/{snapshot_id}/impact-report",
    ];

    for path in &required_paths {
        assert!(
            spec.contains(path),
            "OpenAPI spec missing documented path: {}. If this route was intentionally removed, update this test and the route-openapi-contract-map.md.",
            path
        );
    }
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
