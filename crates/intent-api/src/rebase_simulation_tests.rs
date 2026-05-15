use crate::simulation_handlers::rebase_simulation;
use crate::types::RebaseSimulationQuery;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use uuid::Uuid;

use crate::test_helpers::create_test_payload;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

// =========================================================================
// N4-4: Rebase Simulation Tests (Phase 3 Batch 1 bounded simulation slice)
// =========================================================================

#[tokio::test]
async fn test_rebase_simulation_empty_side_effects() {
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

    // Run simulation with no side effects (deterministic mode by default)
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("deterministic".to_string()),
        seed: None,
    };

    let result = rebase_simulation(
        State(state.clone()),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Should run simulation");

    // With no side effects, report should have 0 total actions
    assert_eq!(result.total_actions, 0);
    assert_eq!(result.successful_count, 0);
    assert_eq!(result.failed_count, 0);
}

#[tokio::test]
async fn test_rebase_simulation_with_side_effects() {
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

    // Run simulation with deterministic mode
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("deterministic".to_string()),
        seed: None,
    };

    let result = rebase_simulation(
        State(state.clone()),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Should run simulation");

    // Report should have 1 action and it should succeed (S1 + Automatic)
    assert_eq!(result.total_actions, 1);
    assert_eq!(result.successful_count, 1);
    assert_eq!(result.failed_count, 0);
    assert!(result.outcomes[0].predicted_success);
}

#[tokio::test]
async fn test_rebase_simulation_intent_not_found() {
    let state = create_test_service();
    let tenant_id = Uuid::new_v4();
    let non_existent_intent_id = Uuid::new_v4();

    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: None,
        seed: None,
    };

    let result = rebase_simulation(
        State(state),
        Path(non_existent_intent_id),
        axum::extract::Query(query),
    )
    .await;

    // Should return error for non-existent intent
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rebase_simulation_stochastic_mode_with_seed() {
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

    // Run simulation with stochastic mode and a seed
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("stochastic".to_string()),
        seed: Some(42),
    };

    let result = rebase_simulation(
        State(state.clone()),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Should run simulation");

    // Verify stochastic mode was used
    assert_eq!(
        result.config.mode,
        compensation_service::SimulationMode::Stochastic
    );
    assert_eq!(result.total_actions, 0); // No side effects
}

#[tokio::test]
async fn test_rebase_simulation_invalid_version_ordering() {
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

    // Test with reversed version order (from_version > to_version) — should fail
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 2,
        to_version: 1,
        mode: None,
        seed: None,
    };

    let err_response =
        rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
            .await
            .unwrap_err();

    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_rebase_simulation_invalid_version_bounds() {
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
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 0,
        to_version: 2,
        mode: None,
        seed: None,
    };

    let err_response = rebase_simulation(
        State(state.clone()),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .unwrap_err();

    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test with to_version = 0 (invalid, must be >= 1)
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 0,
        mode: None,
        seed: None,
    };

    let err_response = rebase_simulation(
        State(state.clone()),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .unwrap_err();

    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test with negative versions
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: -1,
        to_version: 2,
        mode: None,
        seed: None,
    };

    let err_response =
        rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
            .await
            .unwrap_err();

    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_rebase_simulation_invalid_mode_fallback() {
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

    // Run simulation with invalid mode — should fall back to deterministic
    let query = RebaseSimulationQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
        mode: Some("invalid_mode".to_string()),
        seed: None,
    };

    let result = rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
        .await
        .expect("Invalid mode should fall back to deterministic");

    // Verify fallback to deterministic mode
    assert_eq!(
        result.config.mode,
        compensation_service::SimulationMode::Deterministic
    );
}
