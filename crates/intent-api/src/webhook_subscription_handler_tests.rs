//! Webhook subscription handler tests (Slice 4b — bounded local-dev subscription CRUD)
//!
//! DB-free tests covering the CRUD happy path and soft-delete behavior.
//! Uses in-memory repository via `create_test_service`.

use crate::test_helpers::create_test_service;
use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

/// Helper to build a router with test state.
fn build_test_router() -> axum::Router {
    let state = create_test_service();
    crate::router::build_router(
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
        state.webhook_subscription_repo.clone(),
        state.webhook_outbox_repo.clone(),
    )
}

#[tokio::test]
async fn test_create_subscription_success() {
    let router = build_test_router();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_id": intent_id,
        "subscription_id": subscription_id,
        "webhook_url": "https://example.com/webhook",
        "downstream_system_id": "system-a",
        "event_types": ["intent_changed", "intent_deleted"],
        "max_attempts": 5
    }))
    .unwrap();

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/webhooks/subscriptions")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();

    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp["tenant_id"], tenant_id.to_string());
    assert_eq!(resp["intent_id"], intent_id.to_string());
    assert_eq!(resp["subscription_id"], subscription_id.to_string());
    assert_eq!(resp["webhook_url"], "https://example.com/webhook");
    assert_eq!(resp["downstream_system_id"], "system-a");
    assert_eq!(resp["status"], "active");
    assert_eq!(resp["max_attempts"], 5);
    assert!(resp["id"].is_string());
}

#[tokio::test]
async fn test_list_subscriptions_success() {
    let router = build_test_router();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    // Create a subscription first
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_id": intent_id,
        "subscription_id": subscription_id,
        "webhook_url": "https://example.com/webhook"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/webhooks/subscriptions")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // List subscriptions for the intent
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/webhooks/subscriptions?intent_id={}&tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resp: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp["total"], 1);
    assert_eq!(resp["subscriptions"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_subscription_success_and_not_found() {
    let router = build_test_router();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    // Create a subscription first
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_id": intent_id,
        "subscription_id": subscription_id,
        "webhook_url": "https://example.com/webhook"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/webhooks/subscriptions")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["id"].as_str().unwrap();

    // Get the subscription
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Get a non-existent subscription
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            Uuid::new_v4(),
            tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_subscription_success_and_validation_error() {
    let router = build_test_router();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    // Create a subscription first
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_id": intent_id,
        "subscription_id": subscription_id,
        "webhook_url": "https://example.com/webhook"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/webhooks/subscriptions")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["id"].as_str().unwrap();

    // Update the subscription
    let body = serde_json::to_vec(&serde_json::json!({
        "webhook_url": "https://new.example.com/webhook",
        "status": "paused",
        "max_attempts": 7
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("PATCH")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let updated: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(updated["webhook_url"], "https://new.example.com/webhook");
    assert_eq!(updated["status"], "paused");
    assert_eq!(updated["max_attempts"], 7);

    // Try invalid status
    let body = serde_json::to_vec(&serde_json::json!({
        "status": "invalid_status"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("PATCH")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Try invalid max_attempts
    let body = serde_json::to_vec(&serde_json::json!({
        "max_attempts": 0
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("PATCH")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_delete_subscription_soft_delete() {
    let router = build_test_router();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    // Create a subscription first
    let body = serde_json::to_vec(&serde_json::json!({
        "tenant_id": tenant_id,
        "intent_id": intent_id,
        "subscription_id": subscription_id,
        "webhook_url": "https://example.com/webhook"
    }))
    .unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/webhooks/subscriptions")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = created["id"].as_str().unwrap();

    // Soft-delete the subscription
    let req = axum::http::Request::builder()
        .method("DELETE")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let deleted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(deleted["status"], "deleted");

    // Verify it still exists but with deleted status
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/webhooks/subscriptions/{}?tenant_id={}",
            id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let fetched: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fetched["status"], "deleted");

    // Verify it appears in list
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!(
            "/webhooks/subscriptions?intent_id={}&tenant_id={}",
            intent_id, tenant_id
        ))
        .body(axum::body::Body::from(""))
        .unwrap();
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list["total"], 1);
    assert_eq!(list["subscriptions"][0]["status"], "deleted");
}
