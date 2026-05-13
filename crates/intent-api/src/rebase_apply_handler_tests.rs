//! Rebase apply handler tests.
//!
//! RLC-14: Tenant mismatch rejection test for rebase_apply handler.
//! Extracted from handler_tests.rs as a focused module.

use super::*;

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
