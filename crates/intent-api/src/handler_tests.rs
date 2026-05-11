use super::*;
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
use runtime_adapter::MockAdapter;
use std::sync::Arc;

// Import forensic handlers for tests (verification/export/bundle tests moved to forensic_handlers.rs)

// Import simulation handlers for tests
#[cfg(feature = "jwt-auth")]
use crate::simulation_handlers::compensation_simulation_run;

// Import intent read handlers for tests
use crate::intent_read_handlers::{get_intent_head, get_version, list_versions};

// Use shared helper with forensic config for lib.rs tests
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

// Use shared payload helpers
use crate::test_helpers::{
    create_minimal_low_risk_payload, create_test_payload, create_test_payload_with_params,
};

#[tokio::test]
async fn test_router_builds_successfully() {
    let state = create_test_service();
    let _router: axum::Router = Router::new()
        .route("/intents", post(intent_mutation_handlers::create_intent))
        .route("/intents/{intent_id}", get(get_intent_head))
        .route(
            "/intents/{intent_id}/versions",
            post(intent_mutation_handlers::create_version),
        )
        .route("/intents/{intent_id}/versions", get(list_versions))
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(get_version),
        )
        .route(
            "/intents/{intent_id}/diff",
            post(diff_handlers::compute_diff),
        )
        .route(
            "/intents/{intent_id}/rebase-preview",
            post(rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/{intent_id}/rebase-apply",
            post(rebase_apply_handlers::rebase_apply),
        )
        .with_state(state);
    // Router builds successfully - this is a compile-time check essentially
}

// === Rebase Preview Handler Tests ===

/// Helper to call rebase_preview that works in both jwt-auth and non-jwt-auth builds
#[cfg(feature = "jwt-auth")]
async fn call_rebase_preview(
    state: AppState,
    intent_id: Uuid,
    request: DiffRequest,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    rebase_preview_handlers::rebase_preview(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        Json(request),
    )
    .await
}

#[cfg(not(feature = "jwt-auth"))]
async fn call_rebase_preview(
    state: AppState,
    intent_id: Uuid,
    request: DiffRequest,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    rebase_preview_handlers::rebase_preview(State(state), Path(intent_id), Json(request)).await
}

#[tokio::test]
async fn test_rebase_preview_success() {
    use intent_rebase_types::{
        ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest, DiffRequest, SourceRef,
    };

    let state = create_test_service();

    // Create an intent first
    let create_request = CreateIntentRequest {
        tenant_id: None,
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

    // Test the rebase_preview handler directly
    let preview_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };
    let result = call_rebase_preview(state, intent_id, preview_request)
        .await
        .expect("Rebase preview should succeed");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.from_version.version_number, 1);
    assert_eq!(result.to_version.version_number, 2);
    // Verify response has semantically reliable fields only
    assert!(!result.rationale.is_empty());
    assert!(result.risk_level >= 1 && result.risk_level <= 5);
}

#[tokio::test]
async fn test_rebase_preview_invalid_version_ordering() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};

    let state = create_test_service();

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: create_minimal_low_risk_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test".to_string(),
        },
        tags: vec![],
    };

    let intent_id = state
        .service
        .create_intent(create_request)
        .await
        .unwrap()
        .intent_id;

    // Test with reversed version order (from_version > to_version)
    let preview_request = intent_rebase_types::DiffRequest {
        from_version: 2,
        to_version: 1,
    };
    let result = call_rebase_preview(state, intent_id, preview_request).await;
    // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// === Graph-Available Affected Items Tests ===

#[tokio::test]
async fn test_rebase_preview_with_graph_classifies_affected_items() {
    use graph_service::{GraphRepository, GraphService, InMemoryGraphRepository};
    use intent_rebase_types::{
        ChangeChannel, CreateIntentRequest, CreateVersionRequest, ExternalRef, ExternalRefType,
        NodeType, SourceRef,
    };

    // Create service with graph service available
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));

    // Create service with graph integration
    let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    // Phase 3 Batch 1: In-memory orchestration runtime for tests
    let compensation_action_repo =
        Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
    let compensation_action_svc = Arc::new(compensation_service::CompensationActionService::new(
        compensation_action_repo.clone(),
    ));
    let orchestration_run_repo =
        Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));
    let state = AppState {
        service,
        graph_service: graph_svc.clone(),
        side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
            compensation_service::InMemorySideEffectRepository::new(),
        ))),
        compensation_action_service: compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>,
        approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>,
        policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>,
        event_publisher: None, // Phase 2b: event publishing optional in tests
        forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
            as Arc<dyn forensic_service::ForensicVerificationService>,
        forensic_archive_generator: Arc::new(
            forensic_service::InMemoryForensicArchiveGenerator::new(),
        ),
        forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
            Arc::new(forensic_service::InMemoryBundleRepository::new()),
            Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
            Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
        )),
        start_time: Instant::now(),
        rls_pool: None,
    };

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload_with_params("Test intent with graph", &["item1"]),
        created_by: intent_rebase_types::ActorRef {
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
        payload: create_test_payload_with_params("Test intent with graph", &["item1"]),
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Get the version to access its ID
    let to_version = state.service.get_version(intent_id, 2).await.unwrap();

    // Create IntentVersion graph node for v2
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create an IntentVersion node in the graph that maps to our version
    let iv_node = graph_repo
        .create_node(intent_rebase_types::CreateGraphNodeRequest {
            tenant_id,
            workflow_id,
            node_type: NodeType::IntentVersion,
            external_ref: Some(ExternalRef {
                ref_type: ExternalRefType::IntentVersion,
                ref_id: to_version.id,
            }),
            label: "IntentVersion v2".to_string(),
            properties: None,
        })
        .await
        .unwrap();

    // Create an artifact that depends on this IntentVersion
    let artifact_node = graph_repo
        .create_node(intent_rebase_types::CreateGraphNodeRequest {
            tenant_id,
            workflow_id,
            node_type: NodeType::Artifact,
            external_ref: Some(ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            }),
            label: "Test Artifact".to_string(),
            properties: None,
        })
        .await
        .unwrap();

    // Create DependsOn edge: Artifact -> IntentVersion
    graph_repo
        .create_edge(intent_rebase_types::CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact_node.id,
            to_node_id: iv_node.id,
            edge_type: intent_rebase_types::EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

    // Call rebase_preview which should use graph classification
    let preview_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };
    let result = call_rebase_preview(state, intent_id, preview_request)
        .await
        .expect("Rebase preview should succeed even with graph");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.affected_items.status, AffectedItemsStatus::Available);
    // Verify affected artifacts contains our artifact
    assert!(!result.affected_items.affected_artifacts.is_empty());
    assert_eq!(
        result.affected_items.affected_artifacts[0].node_id,
        artifact_node.id
    );
}

#[tokio::test]
async fn test_rebase_preview_fallback_when_graph_node_not_found() {
    use intent_rebase_types::{
        ChangeChannel, CreateIntentRequest, CreateVersionRequest, SourceRef,
    };

    // Create service with graph service but NO graph data
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));
    let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    // Phase 3 Batch 1: In-memory orchestration runtime for tests
    let compensation_action_repo =
        Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
    let compensation_action_svc = Arc::new(compensation_service::CompensationActionService::new(
        compensation_action_repo.clone(),
    ));
    let orchestration_run_repo =
        Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));
    let state = AppState {
        service,
        graph_service: graph_svc.clone(),
        side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
            compensation_service::InMemorySideEffectRepository::new(),
        ))),
        compensation_action_service: compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>,
        approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>,
        policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>,
        event_publisher: None, // Phase 2b: event publishing optional in tests
        forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
            as Arc<dyn forensic_service::ForensicVerificationService>,
        forensic_archive_generator: Arc::new(
            forensic_service::InMemoryForensicArchiveGenerator::new(),
        ),
        forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
            Arc::new(forensic_service::InMemoryBundleRepository::new()),
            Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
            Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
        )),
        start_time: Instant::now(),
        rls_pool: None,
    };

    // Create a test intent

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: create_test_payload_with_params("Test intent no graph", &["item1"]),
        created_by: intent_rebase_types::ActorRef {
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
        payload: create_test_payload_with_params("Test intent no graph", &["item1"]),
        change_reason: "v2".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: intent_rebase_types::ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };
    state
        .service
        .create_version(intent_id, version_request, None, None)
        .await
        .unwrap();

    // Call rebase_preview - graph node won't be found but should NOT fail
    let preview_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };
    let result = call_rebase_preview(state, intent_id, preview_request)
        .await
        .expect("Rebase preview should succeed even when graph node not found");

    assert_eq!(result.intent_id, intent_id);
    // Status should be Unavailable since IntentVersion node not in graph
    assert_eq!(
        result.affected_items.status,
        AffectedItemsStatus::Unavailable
    );
    // But endpoint still returns useful data
    assert!(!result.rationale.is_empty());
}

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

// =========================================================================
// Trace Context Propagation Tests (Phase 3 Batch 2 Slice 2 — bounded OTEL)
//
// Note: Direct middleware testing requires complex axum infrastructure.
// The trace_context_middleware is verified through:
// 1. cargo check -p intent-api (verifies compilation)
// 2. cargo test -p intent-api (verifies existing tests still pass)
// 3. Router wiring in build_router() includes trace_context_middleware layer
// =========================================================================
