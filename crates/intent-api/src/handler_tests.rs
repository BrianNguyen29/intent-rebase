use super::*;
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
use runtime_adapter::MockAdapter;
use std::sync::Arc;

// Import forensic handlers for tests (verification/export/bundle tests moved to forensic_handlers.rs)

// Import simulation handlers for tests
use crate::simulation_handlers::{compensation_simulation_run, rebase_simulation};

// Import query handlers for tests
use crate::query_handlers::get_orchestration_dashboard;

// Import intent read handlers for tests
use crate::intent_read_handlers::{get_intent_head, get_version, list_versions};

// Re-export test helpers for internal use
#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;

// Use shared helper with forensic config for lib.rs tests
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

// Use shared payload helpers
use crate::test_helpers::{create_test_payload, create_test_payload_with_params};

use crate::test_helpers::create_test_service_with_publisher;

/// Create a minimal low-risk IntentPayload for tests.
///
/// Matches the 7 identical inline IntentPayload blocks in handler_tests.rs:
/// - summary: "Test"
/// - success_statement: "Success"
/// - domain: "test"
/// - empty scope/constraints/authority/references/assumptions
/// - risk_tier: Low
/// - urgency: Low
/// - confidence: 1.0
#[cfg(test)]
fn create_minimal_low_risk_payload() -> intent_rebase_types::IntentPayload {
    use intent_rebase_types::{
        AcceptanceCriteria, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
        IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, Urgency,
    };
    IntentPayload {
        objective: IntentObjective {
            summary: "Test".to_string(),
            success_statement: "Success".to_string(),
            domain: "test".to_string(),
        },
        scope: IntentScope {
            in_scope: vec![],
            out_of_scope: vec![],
        },
        constraints: IntentConstraints {
            functional: vec![],
            non_functional: vec![],
            policy: vec![],
            budget: vec![],
            time: vec![],
        },
        acceptance_criteria: AcceptanceCriteria {
            required: vec![],
            optional: vec![],
        },
        authority: IntentAuthority {
            allowed_actions: vec![],
            forbidden_actions: vec![],
            approval_requirements: vec![],
        },
        preferences: IntentPreferences { tradeoffs: vec![] },
        references: IntentReferences {
            specs: vec![],
            tickets: vec![],
            repos: vec![],
            policies: vec![],
        },
        assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
        metadata: IntentMetadataV1 {
            risk_tier: RiskTier::Low,
            urgency: Urgency::Low,
            confidence: 1.0,
        },
    }
}

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
    use graph_service::{GraphService, InMemoryGraphRepository};
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

// === Replay Endpoint Tests (Phase 2b bounded replay slice) ===

/// Helper to call replay_intent that works in both jwt-auth and non-jwt-auth builds
#[cfg(feature = "jwt-auth")]
async fn call_replay_intent(
    state: AppState,
    intent_id: Uuid,
    request: ReplayRequest,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    crate::replay_handlers::replay_intent(
        State(state),
        auth::OptionalRlsTenantClaims(None), // No JWT - tests basic replay without tenant isolation
        Path(intent_id),
        Json(request),
    )
    .await
}

#[cfg(not(feature = "jwt-auth"))]
async fn call_replay_intent(
    state: AppState,
    intent_id: Uuid,
    request: ReplayRequest,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    crate::replay_handlers::replay_intent(State(state), Path(intent_id), Json(request)).await
}

#[tokio::test]
async fn test_replay_intent_no_checkpoint_available() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
        IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
        IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
    };

    let state = create_test_service();

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Test intent".to_string(),
                success_statement: "Success".to_string(),
                domain: "testing".to_string(),
            },
            scope: IntentScope {
                in_scope: vec![],
                out_of_scope: vec![],
            },
            constraints: IntentConstraints {
                functional: vec![],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![],
                forbidden_actions: vec![],
                approval_requirements: vec![],
            },
            preferences: IntentPreferences { tradeoffs: vec![] },
            references: IntentReferences {
                specs: vec![],
                tickets: vec![],
                repos: vec![],
                policies: vec![],
            },
            assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Medium,
                urgency: Urgency::Medium,
                confidence: 0.9,
            },
        },
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
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Test intent v2".to_string(),
                success_statement: "Success".to_string(),
                domain: "testing".to_string(),
            },
            scope: IntentScope {
                in_scope: vec![],
                out_of_scope: vec![],
            },
            constraints: IntentConstraints {
                functional: vec![],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![],
                forbidden_actions: vec![],
                approval_requirements: vec![],
            },
            preferences: IntentPreferences { tradeoffs: vec![] },
            references: IntentReferences {
                specs: vec![],
                tickets: vec![],
                repos: vec![],
                policies: vec![],
            },
            assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Medium,
                urgency: Urgency::Medium,
                confidence: 0.9,
            },
        },
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

    // Test the replay endpoint - no checkpoints available, so should get no_checkpoint_found outcome
    let replay_request = ReplayRequest {
        from_version: Some(1),
        to_version: 2,
        checkpoint_id: None,
    };
    let result = call_replay_intent(state, intent_id, replay_request)
        .await
        .expect("Replay should return even with no checkpoints");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.from_version, 1);
    assert_eq!(result.to_version, 2);
    assert!(result.aligned_checkpoint_id.is_none());
    assert_eq!(result.checkpoint_selection_outcome, "NoCheckpointFound");
    // Skipped because no checkpoint and adapter not used for no-checkpoint path
    assert_eq!(result.runtime_execution_status, "skipped_not_ready");
}

// === Approval Revalidation Handler Tests ===

/// Helper to call revalidate_approval_request that works in both jwt-auth and non-jwt-auth builds
#[cfg(feature = "jwt-auth")]
async fn call_revalidate_approval_request(
    state: AppState,
    approval_request_id: Uuid,
) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
    approval_handlers_readonly::revalidate_approval_request(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(approval_request_id),
    )
    .await
}

#[cfg(not(feature = "jwt-auth"))]
async fn call_revalidate_approval_request(
    state: AppState,
    approval_request_id: Uuid,
) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
    approval_handlers_readonly::revalidate_approval_request(State(state), Path(approval_request_id))
        .await
}

#[tokio::test]
async fn test_revalidate_approval_request_valid_when_scope_unchanged() {
    use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
    use intent_service::{ApprovalRequest, ApprovalRequestStatus};

    let state = create_test_service();

    // Create an approval request
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let approval_request = ApprovalRequest {
        id: approval_id,
        intent_id,
        intent_version_from: 1,
        intent_version_to: 2,
        workflow_id,
        tenant_id,
        requestor_id: "test".to_string(),
        requestor_type: "test".to_string(),
        decision_class: "D".to_string(),
        reason: "Test".to_string(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        status: ApprovalRequestStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: None,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
    };
    state
        .approval_request_repo
        .create_approval_request(approval_request.clone())
        .await
        .unwrap();

    // Create a policy snapshot for version 1 (same as approval basis)
    let scope = ScopeDefinition {
        scope_type: ScopeType::Partial,
        affected_resources: vec![],
        required_approvers: vec![],
        min_approvals: 1,
    };
    let snapshot =
        PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
    state
        .policy_snapshot_repo
        .create_snapshot(snapshot.clone())
        .await
        .unwrap();

    // Create latest snapshot with SAME scope_hash (same scope)
    let latest_snapshot = PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope);
    state
        .policy_snapshot_repo
        .create_snapshot(latest_snapshot.clone())
        .await
        .unwrap();

    // Test revalidate - should be valid since scope_hash matches
    let result = call_revalidate_approval_request(state, approval_id)
        .await
        .expect("Revalidate should succeed");

    assert_eq!(result.approval_id, approval_id);
    assert!(result.valid);
    assert_eq!(result.approval_basis_scope_hash, snapshot.scope_hash);
    assert_eq!(result.current_scope_hash, Some(latest_snapshot.scope_hash));
    assert!(!result.revalidation_required);
}

#[tokio::test]
async fn test_revalidate_approval_request_invalid_when_scope_changed() {
    use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
    use intent_service::{ApprovalRequest, ApprovalRequestStatus};

    let state = create_test_service();

    // Create an approval request
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let approval_request = ApprovalRequest {
        id: approval_id,
        intent_id,
        intent_version_from: 1,
        intent_version_to: 2,
        workflow_id,
        tenant_id,
        requestor_id: "test".to_string(),
        requestor_type: "test".to_string(),
        decision_class: "D".to_string(),
        reason: "Test".to_string(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        status: ApprovalRequestStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: None,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
    };
    state
        .approval_request_repo
        .create_approval_request(approval_request.clone())
        .await
        .unwrap();

    // Create a policy snapshot for version 1 with Partial scope
    let scope_v1 = ScopeDefinition {
        scope_type: ScopeType::Partial,
        affected_resources: vec![],
        required_approvers: vec![],
        min_approvals: 1,
    };
    let snapshot_v1 = PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope_v1);
    state
        .policy_snapshot_repo
        .create_snapshot(snapshot_v1.clone())
        .await
        .unwrap();

    // Create latest snapshot with DIFFERENT scope (Full instead of Partial)
    let scope_v2 = ScopeDefinition {
        scope_type: ScopeType::Full,
        affected_resources: vec![],
        required_approvers: vec![],
        min_approvals: 2,
    };
    let snapshot_v2 = PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope_v2);
    state
        .policy_snapshot_repo
        .create_snapshot(snapshot_v2.clone())
        .await
        .unwrap();

    // Test revalidate - should be invalid since scope_hash differs
    let result = call_revalidate_approval_request(state, approval_id)
        .await
        .expect("Revalidate should succeed");

    assert_eq!(result.approval_id, approval_id);
    assert!(!result.valid);
    assert_eq!(result.approval_basis_scope_hash, snapshot_v1.scope_hash);
    assert_eq!(result.current_scope_hash, Some(snapshot_v2.scope_hash));
    assert!(result.revalidation_required);
}

#[tokio::test]
async fn test_revalidate_approval_request_valid_when_only_basis_snapshot_exists() {
    use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
    use intent_service::{ApprovalRequest, ApprovalRequestStatus};

    let state = create_test_service();

    // Create an approval request
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let approval_request = ApprovalRequest {
        id: approval_id,
        intent_id,
        intent_version_from: 1,
        intent_version_to: 2,
        workflow_id,
        tenant_id,
        requestor_id: "test".to_string(),
        requestor_type: "test".to_string(),
        decision_class: "D".to_string(),
        reason: "Test".to_string(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        status: ApprovalRequestStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: None,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
    };
    state
        .approval_request_repo
        .create_approval_request(approval_request.clone())
        .await
        .unwrap();

    // Create only the approval-basis snapshot (no newer snapshots exist)
    // When no newer policy snapshots exist, the approval basis is the latest,
    // so scope_hash matches and the approval is still valid
    let scope = ScopeDefinition {
        scope_type: ScopeType::Partial,
        affected_resources: vec![],
        required_approvers: vec![],
        min_approvals: 1,
    };
    let snapshot =
        PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
    state
        .policy_snapshot_repo
        .create_snapshot(snapshot.clone())
        .await
        .unwrap();

    // Test revalidate - should return valid=true because latest (only) snapshot
    // matches approval basis, meaning no newer policy exists to invalidate the approval
    let result = call_revalidate_approval_request(state, approval_id)
        .await
        .expect("Revalidate should succeed when only basis snapshot exists");

    assert_eq!(result.approval_id, approval_id);
    assert!(result.valid);
    assert!(!result.revalidation_required);
    assert_eq!(result.current_scope_hash, Some(snapshot.scope_hash));
    assert!(result.reason.contains("Scope unchanged"));
}

#[tokio::test]
async fn test_revalidate_approval_request_not_found() {
    let state = create_test_service();
    let non_existent_id = Uuid::new_v4();

    // Test revalidate - should return 404
    let result = call_revalidate_approval_request(state, non_existent_id).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_revalidate_approval_request_basis_snapshot_not_found() {
    use intent_service::{ApprovalRequest, ApprovalRequestStatus};

    let state = create_test_service();

    // Create an approval request but NO policy snapshots at all
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let approval_request = ApprovalRequest {
        id: approval_id,
        intent_id,
        intent_version_from: 1,
        intent_version_to: 2,
        workflow_id,
        tenant_id,
        requestor_id: "test".to_string(),
        requestor_type: "test".to_string(),
        decision_class: "D".to_string(),
        reason: "Test".to_string(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        status: ApprovalRequestStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: None,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
    };
    state
        .approval_request_repo
        .create_approval_request(approval_request.clone())
        .await
        .unwrap();

    // Test revalidate - should return 404 because approval basis snapshot doesn't exist
    let result = call_revalidate_approval_request(state, approval_id).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// Phase 2b: Event Publishing Tests (bounded event-streaming slice)
// =========================================================================

#[tokio::test]
async fn test_event_publisher_none_skips_publishing() {
    // Test that when event_publisher is None, publish_audit_event is a no-op
    let publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>> = None;
    let tenant_id = Uuid::new_v4();

    // Should not panic or error - just silently skip
    publish_audit_event(
        &publisher,
        tenant_id,
        "RebaseApplied",
        &serde_json::json!({ "test": true }),
    )
    .await;
}

#[tokio::test]
async fn test_event_publisher_inmemory_stores_events() {
    // Test that InMemoryEventPublisher stores events correctly
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let state = create_test_service_with_publisher(publisher.clone());

    // Verify publisher is ready
    assert!(state.event_publisher.as_ref().unwrap().is_ready());
}

#[tokio::test]
async fn test_publish_audit_event_helper_success() {
    // Test publish_audit_event helper with InMemoryEventPublisher
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let tenant_id = Uuid::new_v4();
    let payload = serde_json::json!({
    "from_version": 1,
    "to_version": 2,
    "outcome": "auto_proceeded"
    });

    let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
        Some(publisher.clone());
    publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;

    // Verify event was published
    let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
    let events = publisher.get_events_for_subject(&subject_str).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].schema_version, "v1");
    assert_eq!(events[0].payload, payload);
}

#[tokio::test]
async fn test_publish_audit_event_helper_multiple_events() {
    // Test that multiple events are published with monotonic sequences
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let tenant_id = Uuid::new_v4();

    let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
        Some(publisher.clone());

    // Publish 3 events
    for i in 1..=3 {
        let payload = serde_json::json!({ "index": i });
        publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;
    }

    // Verify sequence is monotonic
    let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
    let events = publisher.get_events_for_subject(&subject_str).await;
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[2].sequence, 3);
}

#[tokio::test]
async fn test_noop_event_publisher_skips() {
    // Test that NoOpEventPublisher skips all events (always returns Skipped)
    use intent_rebase_types::{EventPublisher, TraceContext};
    let publisher = Arc::new(intent_rebase_types::NoOpEventPublisher::new());
    let tenant_id = Uuid::new_v4();
    let payload = serde_json::json!({ "test": true });
    let subject = intent_rebase_types::EventSubject::from_audit_event(tenant_id, "RebaseApplied");

    // NoOpEventPublisher should skip (return Skipped)
    let result = publisher
        .publish(&subject, &payload, TraceContext::default())
        .await;
    match result {
        intent_rebase_types::PublishResult::Skipped { reason } => {
            assert!(reason.contains("disabled"));
        }
        _ => panic!("Expected Skipped result from NoOpEventPublisher"),
    }
}

#[tokio::test]
async fn test_build_router_accepts_event_publisher() {
    // Test that build_router accepts event_publisher parameter
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
        as Arc<dyn intent_rebase_types::AuditRepository>;
    let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
        as Arc<dyn intent_service::ApprovalRequestRepository>;
    let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
        as Arc<dyn intent_service::PolicySnapshotRepository>;
    let event_publisher = Some(Arc::new(intent_rebase_types::InMemoryEventPublisher::new())
        as Arc<dyn intent_rebase_types::EventPublisher>);
    let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(Arc::new(
        compensation_service::InMemorySideEffectRepository::new(),
    )));
    let compensation_action_svc = Arc::new(compensation_service::CompensationActionService::new(
        Arc::new(compensation_service::InMemoryCompensationActionRepository::new()),
    ));
    let orchestration_run_repo =
        Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));

    let _router: axum::Router = build_router(
        service,
        graph_svc,
        side_effect_svc,
        compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_repo,
        approval_repo,
        policy_snapshot_repo,
        event_publisher,
        Arc::new(forensic_service::InMemoryForensicVerificationService::new())
            as Arc<dyn forensic_service::ForensicVerificationService>,
        Arc::new(forensic_service::InMemoryForensicArchiveGenerator::new())
            as Arc<dyn forensic_service::ForensicArchiveGenerator>,
        Arc::new(forensic_service::ForensicBundleService::new(
            Arc::new(forensic_service::InMemoryBundleRepository::new()),
            Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
            Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
        )),
        None,
    );
    // Router builds successfully - this verifies the signature change works
}

// =========================================================================
// Orchestration Dashboard Tests (Phase 3 Batch 1 bounded read-only slice)
// =========================================================================

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_empty_state() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return even with no data");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.tenant_id, tenant_id);
    assert!(result.side_effects.is_empty());
    assert_eq!(result.side_effect_summary.total, 0);
    assert!(result.compensation_actions.is_empty());
    assert_eq!(result.compensation_action_summary.total, 0);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_with_side_effects() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Record some side effects
    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record-123",
        )
        .await
        .unwrap();

    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S4Irreversible,
            "money_transfer",
            "account-xyz",
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.side_effects.len(), 2);
    assert_eq!(result.side_effect_summary.total, 2);
    assert_eq!(result.side_effect_summary.irreversible_count, 1);
    assert_eq!(result.side_effect_summary.auto_compensatable_count, 1);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_with_compensation_actions() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create actions in different statuses
    // Pending action
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let pending_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    state
        .compensation_action_service
        .create_action(pending_action)
        .await
        .unwrap();

    // Approved + Automatic action (auto-executable)
    let approved_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback 2",
    );
    let approved = state
        .compensation_action_service
        .create_action(approved_action)
        .await
        .unwrap();
    state
        .compensation_action_service
        .approve_action(approved.id, approved.lock_version, Some("test"))
        .await
        .unwrap();

    // Failed + retryable error (reapprovable)
    let failed_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback 3",
    );
    let failed = state
        .compensation_action_service
        .create_action(failed_action)
        .await
        .unwrap();
    // Approve then fail with retryable error
    let failed_approved = state
        .compensation_action_service
        .approve_action(failed.id, failed.lock_version, Some("test"))
        .await
        .unwrap();
    let failed_result = compensation_service::ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            failed_approved.id,
            &failed_result,
            failed_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.compensation_actions.len(), 3);
    assert_eq!(result.compensation_action_summary.total, 3);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
    assert_eq!(result.compensation_action_summary.status_counts.approved, 1);
    assert_eq!(result.compensation_action_summary.status_counts.failed, 1);
    assert_eq!(result.compensation_action_summary.retryable_failed_count, 1);
    assert_eq!(result.compensation_action_summary.reapprovable_count, 1);
    assert_eq!(result.compensation_action_summary.auto_executable_count, 1);
    // Approved + Automatic
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_dlq_candidates() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create a failed action with non-retryable error (DLQ candidate)
    let dlq_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    let dlq = state
        .compensation_action_service
        .create_action(dlq_action)
        .await
        .unwrap();
    // Approve then fail with non-retryable error
    let dlq_approved = state
        .compensation_action_service
        .approve_action(dlq.id, dlq.lock_version, Some("test"))
        .await
        .unwrap();
    let dlq_result = compensation_service::ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            dlq_approved.id,
            &dlq_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
    // Non-retryable error + exhausted budget = DLQ candidate, not reapprovable
    assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_exhausted_budget_dlq() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with max_retries = 1
    let mut dlq_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    dlq_action.max_retries = 1; // Exhaust on first failure

    let dlq = state
        .compensation_action_service
        .create_action(dlq_action)
        .await
        .unwrap();
    // Approve then fail with retryable error (but budget exhausted)
    let dlq_approved = state
        .compensation_action_service
        .approve_action(dlq.id, dlq.lock_version, Some("test"))
        .await
        .unwrap();
    let dlq_result = compensation_service::ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            dlq_approved.id,
            &dlq_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    // Exhausted budget makes it a DLQ candidate even with retryable error
    assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
    assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_response_shape() {
    use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};

    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create a side effect
    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S2ExternalReversible,
            "pr_opened",
            "https://github.com/example/pull/123",
        )
        .await
        .unwrap();

    // Create a compensation action
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::FollowupNotice,
        "Send follow-up",
    );
    state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    // Verify response structure
    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.tenant_id, tenant_id);
    assert_eq!(result.side_effects.len(), 1);
    assert_eq!(result.compensation_actions.len(), 1);

    // Verify side effect summary
    assert_eq!(result.side_effect_summary.total, 1);
    assert_eq!(result.side_effect_summary.irreversible_count, 0);
    assert_eq!(result.side_effect_summary.auto_compensatable_count, 0); // S2 is not auto

    // Verify compensation action summary
    assert_eq!(result.compensation_action_summary.total, 1);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
    assert_eq!(result.compensation_action_summary.auto_executable_count, 0);
    // SemiAutomatic is not auto
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_tenant_isolation() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id_1 = Uuid::new_v4();
    let tenant_id_2 = Uuid::new_v4();

    // Record side effects for tenant 1
    state
        .side_effect_service
        .record_side_effect(
            tenant_id_1,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "effect_1",
            "target_1",
        )
        .await
        .unwrap();

    // Record side effects for tenant 2
    state
        .side_effect_service
        .record_side_effect(
            tenant_id_2,
            intent_id,
            1,
            compensation_service::SideEffectClass::S2ExternalReversible,
            "effect_2",
            "target_2",
        )
        .await
        .unwrap();

    // Query for tenant 1
    let query1 = OrchestrationDashboardQuery {
        tenant_id: tenant_id_1,
    };
    let result1 = get_orchestration_dashboard(
        State(state.clone()),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query1),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result1.side_effect_summary.total, 1);
    assert_eq!(result1.side_effects[0].effect_type, "effect_1");

    // Query for tenant 2
    let query2 = OrchestrationDashboardQuery {
        tenant_id: tenant_id_2,
    };
    let result2 = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query2),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result2.side_effect_summary.total, 1);
    assert_eq!(result2.side_effects[0].effect_type, "effect_2");
}

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
// Phase 2b: Rebase Apply BlockedManualReview Invalidation Tests
//
// Tests for bounded approval cancellation in rebase_apply BlockedManualReview path.
// Verifies that when rebase_apply creates a Pending approval request for
// BlockedManualReview, existing Approved approvals for the same intent
// are cancelled using cancel_existing_approved_and_audit helper.
// =========================================================================

#[tokio::test]
async fn test_cancel_existing_approved_and_audit_cancels_approved_approvals() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};
    use intent_service::ApprovalRequestStatus;

    let state = create_test_service();

    // Create an intent to get tenant_id
    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create an existing Approved approval request
    let approved_request = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous approval",
    );
    let approved_id = approved_request.id;
    state
        .approval_request_repo
        .create_approval_request(approved_request)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

    // Verify it's Approved
    let verified = state
        .approval_request_repo
        .get_approval_request(approved_id)
        .await
        .unwrap();
    assert_eq!(verified.status, ApprovalRequestStatus::Approved);

    // Create a new pending approval request (simulating what rebase_apply does)
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        2,
        3,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call the helper to cancel existing Approved approvals
    let cancelled_count = cancel_existing_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        intent_id,
        tenant_id,
        "external-api",
        2,
        3,
        "D",
        new_approval_id,
    )
    .await;

    // Should have cancelled 1 approval
    assert_eq!(cancelled_count, 1);

    // The approved request should now be Cancelled
    let cancelled = state
        .approval_request_repo
        .get_approval_request(approved_id)
        .await
        .unwrap();
    assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);

    // The new pending request should still be Pending
    let still_pending = state
        .approval_request_repo
        .get_approval_request(new_approval_id)
        .await
        .unwrap();
    assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
}

#[tokio::test]
async fn test_cancel_existing_approved_and_audit_does_not_cancel_pending() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};
    use intent_service::ApprovalRequestStatus;

    let state = create_test_service();

    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create a Pending approval request (not Approved)
    let pending_request = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Pending approval",
    );
    let pending_id = pending_request.id;
    state
        .approval_request_repo
        .create_approval_request(pending_request)
        .await
        .unwrap();

    // Verify it's Pending
    let verified = state
        .approval_request_repo
        .get_approval_request(pending_id)
        .await
        .unwrap();
    assert_eq!(verified.status, ApprovalRequestStatus::Pending);

    // Create a new pending approval request
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        2,
        3,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call the helper
    let cancelled_count = cancel_existing_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        intent_id,
        tenant_id,
        "external-api",
        2,
        3,
        "D",
        new_approval_id,
    )
    .await;

    // Should have cancelled 0 approvals (pending not cancelled)
    assert_eq!(cancelled_count, 0);

    // The pending request should still be Pending
    let still_pending = state
        .approval_request_repo
        .get_approval_request(pending_id)
        .await
        .unwrap();
    assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
}

#[tokio::test]
async fn test_cancel_existing_approved_and_audit_returns_zero_when_none_exist() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};

    let state = create_test_service();

    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create a new pending approval request (no existing approvals)
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call the helper with intent that has no existing approvals
    let cancelled_count = cancel_existing_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        intent_id,
        tenant_id,
        "external-api",
        1,
        2,
        "D",
        new_approval_id,
    )
    .await;

    // Should have cancelled 0 approvals
    assert_eq!(cancelled_count, 0);
}

// =========================================================================
// Slice 1: Targeted Approval Cancellation Tests
//
// Tests for classifier-driven targeted cancellation in rebase_apply.
// Verifies that cancel_specific_approved_and_audit correctly cancels
// only the specific approvals identified as stale by the classifier.
// =========================================================================

#[tokio::test]
async fn test_cancel_specific_approved_and_audit_cancels_specific_approvals() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};
    use intent_service::ApprovalRequestStatus;

    let state = create_test_service();

    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create two Approved approval requests
    let approved_request1 = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous approval 1",
    );
    let approved_id1 = approved_request1.id;
    state
        .approval_request_repo
        .create_approval_request(approved_request1)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            approved_id1,
            ApprovalRequestStatus::Approved,
            "approver1",
            None,
        )
        .await
        .unwrap();

    let approved_request2 = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous approval 2",
    );
    let approved_id2 = approved_request2.id;
    state
        .approval_request_repo
        .create_approval_request(approved_request2)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            approved_id2,
            ApprovalRequestStatus::Approved,
            "approver2",
            None,
        )
        .await
        .unwrap();

    // Create a new pending approval request
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        2,
        3,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call targeted cancellation with only approved_id1 as stale
    let stale_ids = vec![approved_id1.to_string()];
    let cancelled_count = cancel_specific_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        &stale_ids,
        CancelApprovalContext {
            intent_id,
            tenant_id,
            actor_id: "external-api".to_string(),
            from_version: 2,
            to_version: 3,
            decision_class: "D".to_string(),
            new_approval_id,
        },
    )
    .await;

    // Should have cancelled 1 approval (only the one in stale_ids)
    assert_eq!(cancelled_count, 1);

    // approved_id1 should now be Cancelled
    let cancelled = state
        .approval_request_repo
        .get_approval_request(approved_id1)
        .await
        .unwrap();
    assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);

    // approved_id2 should still be Approved (not in stale_ids)
    let still_approved = state
        .approval_request_repo
        .get_approval_request(approved_id2)
        .await
        .unwrap();
    assert_eq!(still_approved.status, ApprovalRequestStatus::Approved);

    // The new pending request should still be Pending
    let still_pending = state
        .approval_request_repo
        .get_approval_request(new_approval_id)
        .await
        .unwrap();
    assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
}

#[tokio::test]
async fn test_cancel_specific_approved_and_audit_with_empty_stale_ids() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};
    use intent_service::ApprovalRequestStatus;

    let state = create_test_service();

    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create an Approved approval request
    let approved_request = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous approval",
    );
    let approved_id = approved_request.id;
    state
        .approval_request_repo
        .create_approval_request(approved_request)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

    // Create a new pending approval request
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        2,
        3,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call targeted cancellation with empty stale_ids
    let stale_ids: Vec<String> = vec![];
    let cancelled_count = cancel_specific_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        &stale_ids,
        CancelApprovalContext {
            intent_id,
            tenant_id,
            actor_id: "external-api".to_string(),
            from_version: 2,
            to_version: 3,
            decision_class: "D".to_string(),
            new_approval_id,
        },
    )
    .await;

    // Should have cancelled 0 approvals (empty stale_ids)
    assert_eq!(cancelled_count, 0);

    // The approved request should still be Approved
    let still_approved = state
        .approval_request_repo
        .get_approval_request(approved_id)
        .await
        .unwrap();
    assert_eq!(still_approved.status, ApprovalRequestStatus::Approved);
}

#[tokio::test]
async fn test_cancel_specific_approved_and_audit_only_cancels_approved_status() {
    use intent_rebase_types::{ActorRef, CreateIntentRequest};
    use intent_service::ApprovalRequestStatus;

    let state = create_test_service();

    let workflow_id = Uuid::new_v4();
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create a Pending approval request (not Approved)
    let pending_request = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous approval",
    );
    let pending_id = pending_request.id;
    state
        .approval_request_repo
        .create_approval_request(pending_request)
        .await
        .unwrap();
    // Note: it's already Pending, don't call update_approval_request_status

    // Create a new pending approval request
    let new_approval = intent_service::ApprovalRequest::new_pending(
        intent_id,
        2,
        3,
        workflow_id,
        tenant_id,
        "external-api",
        "external-api",
        "D",
        "New blocked rebase",
    );
    let new_approval_id = new_approval.id;
    state
        .approval_request_repo
        .create_approval_request(new_approval)
        .await
        .unwrap();

    // Call targeted cancellation with pending_id as stale (but it's Pending, not Approved)
    let stale_ids = vec![pending_id.to_string()];
    let cancelled_count = cancel_specific_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        &stale_ids,
        CancelApprovalContext {
            intent_id,
            tenant_id,
            actor_id: "external-api".to_string(),
            from_version: 2,
            to_version: 3,
            decision_class: "D".to_string(),
            new_approval_id,
        },
    )
    .await;

    // Should have cancelled 0 approvals (only Approved can be cancelled)
    assert_eq!(cancelled_count, 0);

    // The pending request should still be Pending
    let still_pending = state
        .approval_request_repo
        .get_approval_request(pending_id)
        .await
        .unwrap();
    assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
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

// =========================================================================
// RLC-1 Tenant Mismatch Tests (Phase 3 P3-S5 Bounded Slice)
//
// Tests for JWT tenant ownership validation on high-risk handlers.
// These tests verify fail-closed behavior on tenant mismatch.
// =========================================================================

// -------------------------------------------------------------------------
// rebase_apply Tenant Mismatch Tests
// -------------------------------------------------------------------------

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
