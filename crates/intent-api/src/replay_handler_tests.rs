use super::*;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

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
