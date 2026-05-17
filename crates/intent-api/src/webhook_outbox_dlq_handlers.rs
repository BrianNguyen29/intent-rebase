//! Webhook outbox DLQ handlers (Slice 5b — bounded local-dev failed-status DLQ)
//!
//! Provides HTTP handlers for viewing and replaying failed webhook outbox records:
//! - GET /webhooks/outbox/dlq?tenant_id=<uuid>[&limit=<i64>]
//! - POST /webhooks/outbox/dlq/:id/replay?tenant_id=<uuid>
//!
//! Bounded scope: no separate DLQ table; uses existing `WebhookOutboxStatus::Failed`
//! records. Replay resets attempt_count and clears error/lock state.
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 5b)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::types::{
    ListWebhookOutboxDlqQuery, ListWebhookOutboxDlqResponse, ReplayWebhookOutboxDlqQuery,
    ReplayWebhookOutboxDlqResponse,
};
use crate::{ApiErrorResponse, AppState};

// =============================================================================
// GET /webhooks/outbox/dlq
// =============================================================================

/// List failed webhook outbox records for a tenant.
///
/// Returns 200 OK with failed records ordered by `updated_at` desc.
/// If the outbox repository is not configured, returns an empty list.
pub async fn list_dlq(
    State(state): State<AppState>,
    Query(query): Query<ListWebhookOutboxDlqQuery>,
) -> Result<Json<ListWebhookOutboxDlqResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Ok(Json(ListWebhookOutboxDlqResponse {
                records: vec![],
                total: 0,
            }));
        }
    };

    let limit = query.limit.unwrap_or(100);
    match repo.list_failed(query.tenant_id, limit).await {
        Ok(records) => {
            let total = records.len();
            Ok(Json(ListWebhookOutboxDlqResponse { records, total }))
        }
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// POST /webhooks/outbox/dlq/:id/replay
// =============================================================================

/// Replay a failed webhook outbox record.
///
/// Transitions the record from `Failed` to `Pending`, resets `attempt_count`
/// to 0, clears `last_error`, `locked_at`, and `locked_by`, and increments
/// `lock_version`.
///
/// Returns 200 OK with the replayed record, or 400/404/500 on error.
/// Idempotency-bounded: only `Failed` records can be replayed; a second replay
/// of the same record will fail because the status is no longer `Failed`.
pub async fn replay_dlq(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReplayWebhookOutboxDlqQuery>,
) -> Result<(StatusCode, Json<ReplayWebhookOutboxDlqResponse>), ApiErrorResponse> {
    let repo = match &state.webhook_outbox_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(
                "Webhook outbox repository is not configured".to_string(),
            )));
        }
    };

    match repo
        .replay_failed(id, query.tenant_id, query.replayed_by)
        .await
    {
        Ok(record) => Ok((
            StatusCode::OK,
            Json(ReplayWebhookOutboxDlqResponse { record }),
        )),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_service;
    use crate::webhook_outbox_repo::{
        InMemoryWebhookOutboxRepository, WebhookOutboxRecord, WebhookOutboxRepository,
        WebhookOutboxStatus,
    };
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn sample_record(
        tenant_id: Uuid,
        intent_id: Uuid,
        subscription_id: Uuid,
    ) -> WebhookOutboxRecord {
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
}
