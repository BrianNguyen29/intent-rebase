use crate::intent_mutation_handlers::{
    parse_optional_header, validate_create_intent_request, validate_create_version_request,
};
use axum::http::{HeaderMap, HeaderValue};
use intent_rebase_types::{CreateIntentRequest, CreateVersionRequest, IntentRebaseError};
use uuid::Uuid;

#[test]
fn test_parse_optional_header_absent() {
    let headers = HeaderMap::new();
    let result = parse_optional_header(&headers, "x-expected-version").unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_parse_optional_header_valid_integer() {
    let mut headers = HeaderMap::new();
    headers.insert("x-expected-version", HeaderValue::from_static("5"));
    let result = parse_optional_header(&headers, "x-expected-version").unwrap();
    assert_eq!(result, Some(5));
}

#[test]
fn test_parse_optional_header_malformed_non_integer() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-expected-version",
        HeaderValue::from_static("not-a-number"),
    );
    let result = parse_optional_header(&headers, "x-expected-version");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidHeader(_)));
    let msg = err.to_string();
    assert!(msg.contains("x-expected-version"));
    assert!(msg.contains("not-a-number"));
}

#[test]
fn test_parse_optional_header_malformed_negative_integer() {
    let mut headers = HeaderMap::new();
    headers.insert("x-expected-row-version", HeaderValue::from_static("-1"));
    let result = parse_optional_header(&headers, "x-expected-row-version");
    // -1 is a valid i32, so it should parse successfully
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(-1));
}

// === Input Validation Tests ===

#[test]
fn test_validate_create_intent_request_valid() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
        IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier,
        SourceRef, Urgency,
    };

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
        tags: vec![],
    };

    let result = validate_create_intent_request(&request);
    assert!(result.is_ok());
}

#[test]
fn test_validate_create_intent_request_nil_workflow_id() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
        IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier,
        Urgency,
    };

    let request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::nil(),
        source_refs: vec![],
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
        tags: vec![],
    };

    let result = validate_create_intent_request(&request);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    assert!(err.to_string().contains("workflow_id"));
}

#[test]
fn test_validate_create_intent_request_empty_summary() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
        IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier,
        Urgency,
    };

    let request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "".to_string(),
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
        tags: vec![],
    };

    let result = validate_create_intent_request(&request);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    assert!(err.to_string().contains("summary"));
}

#[test]
fn test_validate_create_intent_request_whitespace_summary() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
        IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier,
        Urgency,
    };

    let request = CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![],
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "   ".to_string(),
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
        tags: vec![],
    };

    let result = validate_create_intent_request(&request);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    assert!(err.to_string().contains("summary"));
}

#[test]
fn test_validate_create_version_request_valid() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
        IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
        IntentScope, RiskTier, Urgency,
    };

    let request = CreateVersionRequest {
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
        change_reason: "Updating".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };

    let result = validate_create_version_request(&request);
    assert!(result.is_ok());
}

#[test]
fn test_validate_create_version_request_empty_domain() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
        IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
        IntentScope, RiskTier, Urgency,
    };

    let request = CreateVersionRequest {
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Test intent".to_string(),
                success_statement: "Success".to_string(),
                domain: "".to_string(),
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
        change_reason: "Updating".to_string(),
        change_channel: ChangeChannel::UserEdit,
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test-user".to_string(),
        },
    };

    let result = validate_create_version_request(&request);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    assert!(err.to_string().contains("domain"));
}
