//! Rebase apply handler tests.
//!
//! RLC-14: Tenant mismatch rejection test for rebase_apply handler.
//! Extracted from handler_tests.rs as a focused module.

use crate::rebase_apply_handlers;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;
use crate::test_helpers::create_test_payload;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_rebase_apply_rejects_tenant_mismatch() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let state = create_test_service();

    // Create an intent with TenantA (via service directly, not handler)
    let tenant_a = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_a), // Set tenant_id to TenantA
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Create version 2
    let version_request = CreateVersionRequest {
        payload: create_test_payload(),
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Now call rebase_apply with TenantB (different from intent's tenant)
    let tenant_b = Uuid::new_v4();
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };

    let result = rebase_apply_handlers::rebase_apply(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Path(intent_id),
        Json(diff_request),
    )
    .await;

    // Should fail with Unauthorized
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.0.to_string();
    assert!(
        err_msg.contains("Tenant mismatch"),
        "Expected tenant mismatch error, got: {}",
        err_msg
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_rebase_apply_non_rls_fallback_proceeds() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let state = create_test_service();

    // Create an intent
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Create version 2 with a scope change to trigger a non-NoOp diff
    let mut v2_payload = create_test_payload();
    v2_payload.scope.in_scope.push("item2".to_string());

    let version_request = CreateVersionRequest {
        payload: v2_payload,
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Call rebase_apply without RLS pool (non-RLS fallback)
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };

    let result = rebase_apply_handlers::rebase_apply(
        State(state),
        crate::test_helpers::create_test_optional_rls_claims(tenant_id),
        Path(intent_id),
        Json(diff_request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected success for non-RLS fallback proceed path: {:?}",
        result
    );
    let (status, response) = result.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert!(
        response.outcome == "auto_proceeded"
            || response.outcome == "auto_proceeded_with_notification",
        "Expected auto-proceeded outcome, got: {}",
        response.outcome
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_rebase_apply_creates_propagation_signals_for_proceed() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Create version 2 with a scope change to trigger a non-NoOp diff
    let mut v2_payload = create_test_payload();
    v2_payload.scope.in_scope.push("item2".to_string());

    let version_request = CreateVersionRequest {
        payload: v2_payload,
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Pre-seed a propagation record to simulate a registered downstream system
    let repo = state.propagation_record_repo.as_ref().unwrap();
    let record = intent_rebase_types::PropagationRecord::new(
        tenant_id,
        intent_id,
        "workflow-runner-a".to_string(),
    );
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Call rebase_apply (Proceed outcome)
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };

    let result = rebase_apply_handlers::rebase_apply(
        State(state.clone()),
        crate::test_helpers::create_test_optional_rls_claims(tenant_id),
        Path(intent_id),
        Json(diff_request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected success for proceed path: {:?}",
        result
    );
    let (status, response) = result.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert!(
        response.outcome == "auto_proceeded"
            || response.outcome == "auto_proceeded_with_notification",
        "Expected auto-proceeded outcome, got: {}",
        response.outcome
    );

    // Verify the propagation record was updated to pending with last_seen_version = 2
    let updated = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(
        updated.status,
        intent_rebase_types::PropagationStatus::Pending,
        "Propagation record should be updated to pending after apply"
    );
    assert_eq!(
        updated.last_seen_version, 2,
        "Propagation record last_seen_version should be updated to to_version"
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_rebase_apply_no_signals_when_repo_none() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let mut state = create_test_service();
    // Simulate in-memory mode without propagation repo
    state.propagation_record_repo = None;

    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Create version 2 with a scope change to trigger a non-NoOp diff
    let mut v2_payload = create_test_payload();
    v2_payload.scope.in_scope.push("item2".to_string());

    let version_request = CreateVersionRequest {
        payload: v2_payload,
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Call rebase_apply with no propagation repo — should still succeed
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };

    let result = rebase_apply_handlers::rebase_apply(
        State(state.clone()),
        crate::test_helpers::create_test_optional_rls_claims(tenant_id),
        Path(intent_id),
        Json(diff_request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected success when propagation repo is None: {:?}",
        result
    );
    let (status, response) = result.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert!(
        response.outcome == "auto_proceeded"
            || response.outcome == "auto_proceeded_with_notification",
        "Expected auto-proceeded outcome, got: {}",
        response.outcome
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_rebase_apply_no_signals_when_no_downstream_records() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Create version 2 with a scope change to trigger a non-NoOp diff
    let mut v2_payload = create_test_payload();
    v2_payload.scope.in_scope.push("item2".to_string());

    let version_request = CreateVersionRequest {
        payload: v2_payload,
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Do NOT pre-seed any propagation records — empty registry
    let repo = state.propagation_record_repo.as_ref().unwrap();

    // Call rebase_apply (Proceed outcome) — should succeed even with empty registry
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };

    let result = rebase_apply_handlers::rebase_apply(
        State(state.clone()),
        crate::test_helpers::create_test_optional_rls_claims(tenant_id),
        Path(intent_id),
        Json(diff_request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected success with empty downstream registry: {:?}",
        result
    );
    let (status, response) = result.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert!(
        response.outcome == "auto_proceeded"
            || response.outcome == "auto_proceeded_with_notification",
        "Expected auto-proceeded outcome, got: {}",
        response.outcome
    );

    // Verify no propagation records were created
    let records = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
    assert!(
        records.is_empty(),
        "No propagation records should exist for empty registry"
    );
}

// B16: Apply → env gate → webhook dispatch integration tests.
// Uses a pub(crate) test seam on `create_propagation_signals_after_apply`
// to verify env-gated dispatch without full HTTP handler overhead.
use tokio::sync::Mutex;

/// Serialize access to the `INTENT_API_WEBHOOK_DELIVERY` env var across
/// async tests. `std::env` is process-wide and `#[tokio::test]`s run
/// concurrently by default; holding this lock for the entire test body
/// prevents parallel env mutations from causing flaky reads.
/// Uses `tokio::sync::Mutex` so the guard can be held across await points
/// without triggering clippy `await_holding_lock`.
static WEBHOOK_ENV_LOCK: Mutex<()> = Mutex::const_new(());

#[tokio::test]
async fn test_create_propagation_signals_webhook_disabled_by_default() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest, SourceRef};

    {
        let _guard = WEBHOOK_ENV_LOCK.lock().await;
        // Ensure env var is unset (disabled by default)
        std::env::remove_var(crate::webhook_delivery::WEBHOOK_DELIVERY_ENV_VAR);
    }

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Pre-seed a propagation record
    let repo = state.propagation_record_repo.as_ref().unwrap();
    let record = intent_rebase_types::PropagationRecord::new(
        tenant_id,
        intent_id,
        "workflow-runner-a".to_string(),
    );
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Call create_propagation_signals_after_apply directly
    rebase_apply_handlers::create_propagation_signals_after_apply(&state, intent_id, tenant_id, 2)
        .await;

    // Verify signal was updated
    let updated = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(
        updated.status,
        intent_rebase_types::PropagationStatus::Pending,
        "Propagation record should be updated to pending when webhook disabled"
    );
    assert_eq!(
        updated.last_seen_version, 2,
        "last_seen_version should be updated to to_version"
    );

    // No panic should occur; webhook dispatch is skipped when disabled.
}

#[tokio::test]
async fn test_create_propagation_signals_webhook_enabled_no_panic_with_empty_resolver() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest, SourceRef};

    {
        let _guard = WEBHOOK_ENV_LOCK.lock().await;
        // Enable webhook delivery
        std::env::set_var(crate::webhook_delivery::WEBHOOK_DELIVERY_ENV_VAR, "true");
    }

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Pre-seed a propagation record
    let repo = state.propagation_record_repo.as_ref().unwrap();
    let record = intent_rebase_types::PropagationRecord::new(
        tenant_id,
        intent_id,
        "workflow-runner-b".to_string(),
    );
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Call create_propagation_signals_after_apply directly.
    // rls_pool is None → EmptyWebhookSubscriptionResolver → no HTTP calls.
    rebase_apply_handlers::create_propagation_signals_after_apply(&state, intent_id, tenant_id, 3)
        .await;

    // Verify signal was updated
    let updated = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(
        updated.status,
        intent_rebase_types::PropagationStatus::Pending,
        "Propagation record should be updated to pending even with webhook enabled"
    );
    assert_eq!(
        updated.last_seen_version, 3,
        "last_seen_version should be updated to to_version"
    );

    // Clean up env var
    {
        let _guard = WEBHOOK_ENV_LOCK.lock().await;
        std::env::remove_var(crate::webhook_delivery::WEBHOOK_DELIVERY_ENV_VAR);
    }
}

#[tokio::test]
async fn test_create_propagation_signals_webhook_enabled_wiremock_dispatch() {
    use crate::webhook_delivery::{InMemoryWebhookSubscriptionResolver, WebhookSubscription};
    use intent_rebase_types::{ActorRef, CreateIntentRequest, SourceRef};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // Hold the env lock for the entire test to prevent concurrent env mutations
    // from other tests running in parallel.
    let _guard = WEBHOOK_ENV_LOCK.lock().await;
    std::env::set_var(crate::webhook_delivery::WEBHOOK_DELIVERY_ENV_VAR, "true");

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Pre-seed a propagation record
    let repo = state.propagation_record_repo.as_ref().unwrap();
    let record = intent_rebase_types::PropagationRecord::new(
        tenant_id,
        intent_id,
        "workflow-runner-wiremock".to_string(),
    );
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Create a subscription pointing to the wiremock server
    let subscription = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: format!("{}/webhook", mock_server.uri()),
        downstream_system_id: Some("workflow-runner-wiremock".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(subscription);

    // Call with injected resolver through the test seam
    rebase_apply_handlers::create_propagation_signals_after_apply_with_resolver(
        &state, intent_id, tenant_id, 2, &resolver,
    )
    .await;

    // Verify propagation record was updated to acknowledged
    let updated = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(
        updated.status,
        intent_rebase_types::PropagationStatus::Acknowledged,
        "Propagation record should be acknowledged after successful webhook delivery"
    );
    assert_eq!(
        updated.delivery_attempt_count, 1,
        "Delivery attempt count should be 1 after successful delivery"
    );
    assert!(
        updated.acknowledged_at.is_some(),
        "acknowledged_at should be set after successful delivery"
    );

    // wiremock's expect(1) will fail the test if the mock was not called exactly once

    // env var cleaned up when lock is dropped at end of scope
}

#[tokio::test]
async fn test_create_propagation_signals_webhook_enabled_wiremock_500_failure() {
    use crate::webhook_delivery::{InMemoryWebhookSubscriptionResolver, WebhookSubscription};
    use intent_rebase_types::{ActorRef, CreateIntentRequest, SourceRef};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // Hold the env lock for the entire test to prevent concurrent env mutations
    // from other tests running in parallel.
    let _guard = WEBHOOK_ENV_LOCK.lock().await;
    std::env::set_var(crate::webhook_delivery::WEBHOOK_DELIVERY_ENV_VAR, "true");

    let mock_server = MockServer::start().await;

    // Return 500 for every request; the retry loop will exhaust WEBHOOK_MAX_ATTEMPTS.
    Mock::given(method("POST"))
        .and(path("/webhook"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
        tags: vec!["test".to_string()],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Pre-seed a propagation record
    let repo = state.propagation_record_repo.as_ref().unwrap();
    let record = intent_rebase_types::PropagationRecord::new(
        tenant_id,
        intent_id,
        "workflow-runner-wiremock-500".to_string(),
    );
    let record_id = record.id;
    repo.create_record(record).await.unwrap();

    // Create a subscription pointing to the wiremock server
    let subscription = WebhookSubscription {
        id: Uuid::new_v4(),
        tenant_id,
        intent_id,
        subscription_id: Uuid::new_v4(),
        webhook_url: format!("{}/webhook", mock_server.uri()),
        downstream_system_id: Some("workflow-runner-wiremock-500".to_string()),
    };
    let resolver = InMemoryWebhookSubscriptionResolver::new();
    resolver.add(subscription);

    // Call with injected resolver through the test seam
    rebase_apply_handlers::create_propagation_signals_after_apply_with_resolver(
        &state, intent_id, tenant_id, 2, &resolver,
    )
    .await;

    // Verify propagation record was updated to failed after retry exhaustion
    let updated = repo.get_record(record_id, tenant_id).await.unwrap();
    assert_eq!(
        updated.status,
        intent_rebase_types::PropagationStatus::Failed,
        "Propagation record should be failed after webhook 500 retry exhaustion"
    );
    assert_eq!(
        updated.delivery_attempt_count, 1,
        "Delivery attempt count should be 1 (one dispatch block, retries are in-loop)"
    );
    assert!(
        updated.failed_at.is_some(),
        "failed_at should be set after delivery failure"
    );
    assert!(
        updated
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("retry exhausted"),
        "failure_reason should indicate retry exhaustion, got: {:?}",
        updated.failure_reason
    );

    // env var cleaned up when lock is dropped at end of scope
}
