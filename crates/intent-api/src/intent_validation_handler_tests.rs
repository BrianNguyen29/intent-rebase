use crate::intent_validation_handlers::validate_intent;
use axum::Json;
use intent_rebase_types::{
    AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAssumptions, IntentAuthority,
    IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences,
    IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
};
use uuid::Uuid;

#[tokio::test]
async fn test_validate_intent_valid_request() {
    let request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://test".to_string(),
        }],
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Test intent".to_string(),
                success_statement: "Success statement".to_string(),
                domain: "testing".to_string(),
            },
            scope: IntentScope {
                in_scope: vec!["item1".to_string()],
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
            assumptions: IntentAssumptions { explicit: vec![] },
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

    let result = validate_intent(Json(request)).await;
    assert!(result.valid);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_validate_intent_empty_summary() {
    let request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "".to_string(),
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
            assumptions: IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Low,
                urgency: Urgency::Low,
                confidence: 0.5,
            },
        },
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test".to_string(),
        },
        tags: vec![],
    };

    let result = validate_intent(Json(request)).await;
    assert!(!result.valid);
    assert!(!result.errors.is_empty());
    let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
    assert!(
        field_names.iter().any(|f| f.contains("summary")),
        "Expected summary validation error"
    );
}

#[tokio::test]
async fn test_validate_intent_confidence_out_of_range() {
    let request = CreateIntentRequest {
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
            assumptions: IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Low,
                urgency: Urgency::Low,
                confidence: 1.5, // Out of range (should be 0.0-1.0)
            },
        },
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test".to_string(),
        },
        tags: vec![],
    };

    let result = validate_intent(Json(request)).await;
    assert!(!result.valid);
    assert!(!result.errors.is_empty());
    let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
    assert!(
        field_names.iter().any(|f| f.contains("confidence")),
        "Expected confidence validation error"
    );
}
