use super::*;
use crate::simulation_handlers::compensation_simulation_run;
use crate::test_helpers::create_test_payload;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

// =========================================================================
// N4-4 POST: Compensation Simulation Run Tests (Phase 3 Batch 1 bounded simulation slice)
// =========================================================================

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_empty_side_effects() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Run simulation with POST request (no side effects)
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("deterministic".to_string()),
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("Should run simulation");

    // With no side effects, report should have 0 total actions
    assert_eq!(result.total_actions, 0);
    assert_eq!(result.successful_count, 0);
    assert_eq!(result.failed_count, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_with_side_effects() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Record a side effect
    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "test_effect",
            "test_target",
        )
        .await
        .expect("Should record side effect");

    // Run simulation with POST request
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("deterministic".to_string()),
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("Should run simulation");

    // Report should have 1 action and it should succeed (S1 + Automatic)
    assert_eq!(result.total_actions, 1);
    assert_eq!(result.successful_count, 1);
    assert_eq!(result.failed_count, 0);
    assert!(result.outcomes[0].predicted_success);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_invalid_version_ordering() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Run simulation with reversed version order
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 2,
        to_version: 1, // Invalid: from > to
        mode: None,
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;

    // Should return error for invalid version ordering
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_invalid_version_bounds() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Test with from_version = 0 (invalid, must be >= 1)
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 0,
        to_version: 2,
        mode: None,
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state.clone()),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;

    // Should return error for invalid version bounds
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test with to_version = 0 (invalid, must be >= 1)
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 1,
        to_version: 0,
        mode: None,
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state.clone()),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;

    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test with negative versions
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: -1,
        to_version: 2,
        mode: None,
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;

    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_intent_not_found() {
    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let non_existent_intent_id = Uuid::new_v4();

    let request = CompensationSimulationRequest {
        intent_id: non_existent_intent_id,
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: None,
        seed: None,
        side_effect_ids: None,
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;

    // Should return error for non-existent intent
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_compensation_simulation_run_with_side_effect_ids_filter() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Record two side effects
    let se1 = state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "test_effect_1",
            "test_target",
        )
        .await
        .expect("Should record side effect 1");

    let _se2 = state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S2ExternalReversible,
            "test_effect_2",
            "test_target",
        )
        .await
        .expect("Should record side effect 2");

    // Run simulation with only first side effect ID
    let request = CompensationSimulationRequest {
        intent_id,
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("deterministic".to_string()),
        seed: None,
        side_effect_ids: Some(vec![se1.id]), // Only simulate se1
    };

    let result = compensation_simulation_run(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("Should run simulation");

    // Report should only have 1 action (se1 only)
    assert_eq!(result.total_actions, 1);
    // S1 + Automatic = success
    assert_eq!(result.successful_count, 1);
    assert_eq!(result.failed_count, 0);
}
