use crate::test_helpers::create_test_service;
use crate::types::{
    BulkReplayWebhookOutboxDlqRequest, BulkReplayWebhookOutboxDlqResponse,
    ListWebhookOutboxDlqResponse, ListWebhookOutboxReplayedResponse,
    ReplayWebhookOutboxDlqResponse, WebhookOutboxDlqStatsResponse,
};
use crate::webhook_outbox_repo::{
    InMemoryWebhookOutboxRepository, WebhookOutboxRecord, WebhookOutboxRepository,
    WebhookOutboxStatus,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn sample_record(tenant_id: Uuid, intent_id: Uuid, subscription_id: Uuid) -> WebhookOutboxRecord {
    WebhookOutboxRecord::new(
        tenant_id,
        intent_id,
        subscription_id,
        "intent_changed".to_string(),
        serde_json::json!({"foo": "bar"}),
        None,
    )
}

#[tokio::test]
async fn test_list_dlq_returns_failed_records() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "boom".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 1);
    assert_eq!(parsed.records[0].id, r1.id);
    assert_eq!(parsed.records[0].status, WebhookOutboxStatus::Failed);
}

#[tokio::test]
async fn test_list_dlq_empty_when_no_failures() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
    assert!(parsed.records.is_empty());
}

#[tokio::test]
async fn test_list_dlq_tenant_boundary() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant_a, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant_a, "boom".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq?tenant_id={}", tenant_b))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
}

#[tokio::test]
async fn test_replay_dlq_transitions_to_pending() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.record.status, WebhookOutboxStatus::Pending);
    assert_eq!(parsed.record.attempt_count, 0);
    assert_eq!(parsed.record.last_error, None);
    assert_eq!(parsed.record.locked_at, None);
    assert_eq!(parsed.record.locked_by, None);
}

#[tokio::test]
async fn test_replay_dlq_second_call_errors() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    // First replay succeeds
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Second replay fails
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        !response.status().is_success(),
        "expected error status, got {:?}",
        response.status()
    );
}

#[tokio::test]
async fn test_replay_dlq_non_failed_errors() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        !response.status().is_success(),
        "expected error status, got {:?}",
        response.status()
    );
}

#[tokio::test]
async fn test_replayed_record_can_be_delivered_by_worker() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    // Replay via API
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify the record is now Pending and can be listed as pending
    let pending = repo.list_pending(tenant, 10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, r.id);
    assert_eq!(pending[0].status, WebhookOutboxStatus::Pending);
    assert_eq!(pending[0].attempt_count, 0);
}

#[tokio::test]
async fn test_replay_dlq_with_replayed_by() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/webhooks/outbox/dlq/{}/replay?tenant_id={}&replayed_by=operator-7",
                    r.id, tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.record.status, WebhookOutboxStatus::Pending);
    assert_eq!(parsed.record.replay_count, 1);
    assert!(parsed.record.replayed_at.is_some());
    assert_eq!(parsed.record.replayed_by, Some("operator-7".to_string()));
}

// =============================================================================
// Phase 1.3 — Replayed audit query handler tests
// =============================================================================

#[tokio::test]
async fn test_list_replayed_returns_replayed_records() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.replay_failed(r.id, tenant, Some("operator-1".to_string()))
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}",
                    tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 1);
    assert_eq!(parsed.records[0].id, r.id);
    assert_eq!(parsed.records[0].replay_count, 1);
    assert!(parsed.records[0].replayed_at.is_some());
    assert_eq!(
        parsed.records[0].replayed_by,
        Some("operator-1".to_string())
    );
}

#[tokio::test]
async fn test_list_replayed_empty_when_no_replays() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}",
                    tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
    assert!(parsed.records.is_empty());
}

#[tokio::test]
async fn test_list_replayed_tenant_boundary() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant_a, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant_a, "boom".to_string())
        .await
        .unwrap();
    repo.replay_failed(r.id, tenant_a, None).await.unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}",
                    tenant_b
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
}

#[tokio::test]
async fn test_list_replayed_excludes_unreplayed_failed() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    // Do NOT replay

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}",
                    tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
}

#[tokio::test]
async fn test_list_replayed_with_limit() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();
    repo.replay_failed(r1.id, tenant, None).await.unwrap();
    repo.replay_failed(r2.id, tenant, None).await.unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}&limit=1",
                    tenant
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 1);
}

#[tokio::test]
async fn test_list_replayed_with_since() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let before_replay = Utc::now();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    repo.replay_failed(r.id, tenant, None).await.unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    // Since before replay — should include
    let since_before = before_replay.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}&since={}",
                    tenant, since_before
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 1);

    // Since after replay — should exclude
    let after_replay = Utc::now() + chrono::Duration::seconds(1);
    let since_after = after_replay.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/webhooks/outbox/dlq/replayed?tenant_id={}&since={}",
                    tenant, since_after
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: ListWebhookOutboxReplayedResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.total, 0);
}

// =============================================================================
// Phase 2.2 — Bulk replay handler tests
// =============================================================================

#[tokio::test]
async fn test_bulk_replay_replays_multiple_failed_records() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: None,
        replayed_by: Some("operator-bulk".to_string()),
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 2);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);
    assert_eq!(parsed.records.len(), 2);
    assert_eq!(parsed.records[0].status, WebhookOutboxStatus::Pending);
    assert_eq!(parsed.records[1].status, WebhookOutboxStatus::Pending);

    // Verify replay metadata
    for record in &parsed.records {
        assert_eq!(record.replay_count, 1);
        assert!(record.replayed_at.is_some());
        assert_eq!(record.replayed_by, Some("operator-bulk".to_string()));
    }
}

#[tokio::test]
async fn test_bulk_replay_empty_when_no_failures() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: None,
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 0);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);
    assert!(parsed.records.is_empty());
}

#[tokio::test]
async fn test_bulk_replay_tenant_boundary() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant_a, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant_a, "boom".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant_b,
        max_records: None,
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 0);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);
}

#[tokio::test]
async fn test_bulk_replay_cap_enforcement() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    // Create 5 failed records
    for _ in 0..5 {
        let r = sample_record(tenant, intent, sub);
        repo.create(r.clone()).await.unwrap();
        repo.mark_failed(r.id, tenant, "err".to_string())
            .await
            .unwrap();
    }

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    // Request max 2 — should only replay 2
    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: Some(2),
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 2);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);

    // Verify 3 remain failed
    let remaining = repo.list_failed(tenant, 10).await.unwrap();
    assert_eq!(remaining.len(), 3);
}

#[tokio::test]
async fn test_bulk_replay_idempotency_second_call_skips() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    // First bulk replay
    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: None,
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 1);
    assert_eq!(parsed.skipped, 0);

    // Second bulk replay — record is no longer Failed, so list_failed returns empty
    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: None,
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 0);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);
}

#[tokio::test]
async fn test_bulk_replay_mixed_status_only_replays_failed() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let failed = sample_record(tenant, intent, sub);
    let pending = sample_record(tenant, intent, sub);
    let delivered = sample_record(tenant, intent, sub);
    repo.create(failed.clone()).await.unwrap();
    repo.create(pending.clone()).await.unwrap();
    repo.create(delivered.clone()).await.unwrap();
    repo.mark_failed(failed.id, tenant, "err".to_string())
        .await
        .unwrap();
    repo.mark_delivered(delivered.id, tenant).await.unwrap();
    // pending stays pending

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let body = serde_json::to_vec(&BulkReplayWebhookOutboxDlqRequest {
        tenant_id: tenant,
        max_records: None,
        replayed_by: None,
    })
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/outbox/dlq/bulk-replay")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: BulkReplayWebhookOutboxDlqResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.replayed, 1);
    assert_eq!(parsed.skipped, 0);
    assert_eq!(parsed.errors, 0);
    assert_eq!(parsed.records[0].id, failed.id);
    assert_eq!(parsed.records[0].status, WebhookOutboxStatus::Pending);
}

// =============================================================================
// Phase 2.3 — DLQ stats handler tests
// =============================================================================

#[tokio::test]
async fn test_dlq_stats_empty() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq/stats?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: WebhookOutboxDlqStatsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.stats.total_failed, 0);
    assert_eq!(parsed.stats.oldest_failed_age_seconds, None);
    assert_eq!(parsed.stats.replayed_count, 0);
    assert!(parsed.stats.by_error_summary.is_empty());
}

#[tokio::test]
async fn test_dlq_stats_failed_depth() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq/stats?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: WebhookOutboxDlqStatsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.stats.total_failed, 2);
    assert_eq!(parsed.stats.replayed_count, 0);
    assert_eq!(parsed.stats.by_error_summary.len(), 1);
    assert_eq!(parsed.stats.by_error_summary[0].error_pattern, "timeout");
    assert_eq!(parsed.stats.by_error_summary[0].count, 2);
}

#[tokio::test]
async fn test_dlq_stats_oldest_age() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant, "boom".to_string())
        .await
        .unwrap();

    // Small sleep to ensure age > 0
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq/stats?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: WebhookOutboxDlqStatsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.stats.total_failed, 1);
    assert!(
        parsed.stats.oldest_failed_age_seconds.unwrap_or(0) >= 0,
        "expected non-negative age"
    );
}

#[tokio::test]
async fn test_dlq_stats_tenant_boundary() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r = sample_record(tenant_a, intent, sub);
    repo.create(r.clone()).await.unwrap();
    repo.mark_failed(r.id, tenant_a, "err".to_string())
        .await
        .unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq/stats?tenant_id={}", tenant_b))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: WebhookOutboxDlqStatsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.stats.total_failed, 0);
    assert_eq!(parsed.stats.replayed_count, 0);
}

#[tokio::test]
async fn test_dlq_stats_replayed_count() {
    let mut state = create_test_service();
    let repo = Arc::new(InMemoryWebhookOutboxRepository::new());
    state.webhook_outbox_repo = Some(repo.clone());
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();
    repo.replay_failed(r1.id, tenant, None).await.unwrap();

    let app = crate::router::build_router(
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
        Some(repo.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/webhooks/outbox/dlq/stats?tenant_id={}", tenant))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: WebhookOutboxDlqStatsResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.stats.total_failed, 1); // r2 still failed
    assert_eq!(parsed.stats.replayed_count, 1); // r1 was replayed
    assert_eq!(parsed.stats.by_error_summary.len(), 1);
    assert_eq!(parsed.stats.by_error_summary[0].error_pattern, "e2");
    assert_eq!(parsed.stats.by_error_summary[0].count, 1);
}
