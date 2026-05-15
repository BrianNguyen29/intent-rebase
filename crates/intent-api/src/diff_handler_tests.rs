use crate::diff_handlers::compute_diff;
use crate::test_helpers::create_test_payload;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service_for_diff;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use intent_rebase_types::{
    AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
    DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
    IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
};
use uuid::Uuid;

#[tokio::test]
async fn test_compute_diff_success() {
    let state = create_test_service_for_diff();

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

    // Test the compute_diff handler directly
    let diff_request = DiffRequest {
        from_version: 1,
        to_version: 2,
    };
    let result = compute_diff(State(state), Path(intent_id), Json(diff_request))
        .await
        .expect("Diff computation should succeed");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.from_version.version_number, 1);
    assert_eq!(result.to_version.version_number, 2);
}

#[tokio::test]
async fn test_compute_diff_invalid_version_ordering() {
    let state = create_test_service_for_diff();

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: IntentPayload {
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
        },
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
    let diff_request = DiffRequest {
        from_version: 2,
        to_version: 1,
    };
    let result = compute_diff(State(state), Path(intent_id), Json(diff_request)).await;
    // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
