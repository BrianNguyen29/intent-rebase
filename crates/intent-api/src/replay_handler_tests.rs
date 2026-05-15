use crate::types::{ReplayRequest, ReplayResponse};
use crate::{ApiErrorResponse, AppState};
use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;

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
        crate::auth::OptionalRlsTenantClaims(None), // No JWT - tests basic replay without tenant isolation
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

// -------------------------------------------------------------------------
// replay_intent Tenant Mismatch Tests (P1-S5i)
// -------------------------------------------------------------------------

/// Tests that replay_intent rejects JWT tenant mismatch.
/// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match intent's tenant_id.
#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_replay_intent_rejects_tenant_mismatch() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
        IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
        IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
    };

    let state = create_test_service();

    // Create an intent (tenant is assigned by the service)
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

    // Get the intent head (tenant_a not used in this test - we test mismatch with tenant_b)
    let _intent_head = state.service.get_intent_head(intent_id).await.unwrap();

    // Create version 2 to enable replay from v1 to v2
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
        .create_version(intent_id, version_request, Some(1), None)
        .await
        .unwrap();

    // Try to replay with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let replay_request = ReplayRequest {
        from_version: Some(1),
        to_version: 2,
        checkpoint_id: None,
    };

    let result = crate::replay_handlers::replay_intent(
        State(state),
        create_test_optional_rls_claims(tenant_b), // JWT has TenantB - mismatch
        Path(intent_id),
        Json(replay_request),
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

/// Tests that replay_intent succeeds when JWT tenant matches intent's tenant.
/// P1-S5i: Validates the happy path for tenant-matched requests.
#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_replay_intent_succeeds_with_matching_tenant() {
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

    // Get the intent head to find the assigned tenant
    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_a = intent_head.intent.tenant_id;

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
        .create_version(intent_id, version_request, Some(1), None)
        .await
        .unwrap();

    // Replay with TenantA (matching)
    let replay_request = ReplayRequest {
        from_version: Some(1),
        to_version: 2,
        checkpoint_id: None,
    };

    let result = crate::replay_handlers::replay_intent(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Path(intent_id),
        Json(replay_request),
    )
    .await;

    // Should succeed (returns NoCheckpointFound since no checkpoints available)
    assert!(
        result.is_ok(),
        "Expected success with matching tenant, got: {:?}",
        result
    );
}
