//! ImpactReport handler tests.
//!
//! Phase 2 bounded MVP: Tests the on-demand read-only projection.

use axum::extract::{Path, State};
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
use crate::auth::OptionalRlsTenantClaims;
#[cfg(feature = "jwt-auth")]
use crate::query_handlers::get_impact_report;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;
use crate::types::ImpactReportQuery;

// =========================================================================
// ImpactReport Tests (Phase 2 bounded MVP)
// =========================================================================

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_impact_report_empty_state() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let query = ImpactReportQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
    };

    let result = get_impact_report(
        State(state),
        OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await;

    // Intent does not exist → should return an error (404)
    assert!(result.is_err());
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_impact_report_response_shape() {
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
        IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
        IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
    };

    let state = create_test_service();
    let tenant_id = Uuid::new_v4();

    // Create an intent
    let create_request = CreateIntentRequest {
        tenant_id: Some(tenant_id),
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
                in_scope: vec!["feature-a".to_string()],
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
                in_scope: vec!["feature-a".to_string(), "feature-b".to_string()],
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

    let query = ImpactReportQuery {
        tenant_id,
        from_version: 1,
        to_version: 2,
    };

    let result = get_impact_report(
        State(state),
        OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("ImpactReport should return for existing intent");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.tenant_id, tenant_id);
    assert_eq!(result.provenance.from_version, 1);
    assert_eq!(result.provenance.to_version, 2);

    // Trigger should be populated from rebase preview
    assert!(!result.trigger.change_summary.is_empty());
    assert!(!result.trigger.risk_tier.is_empty());
    assert!(!result.trigger.decision_class.is_empty());

    // Scope counts should be non-negative (graph may be empty in in-memory test)
    // Since graph is empty, affected items status is Unavailable → counts are 0
    assert_eq!(result.scope.affected_artifacts_count, 0);
    assert_eq!(result.scope.affected_approvals_count, 0);
    assert_eq!(result.scope.affected_side_effects_count, 0);

    // Compensation should reflect empty state
    assert_eq!(result.compensation.total_actions, 0);

    // Safety gates should reflect empty state
    assert_eq!(result.safety_gates.open_gates, 0);
    assert_eq!(result.safety_gates.blocked_gates, 0);
    assert_eq!(result.safety_gates.manual_review_gates, 0);

    // Unsupported items should list deferred features
    assert!(!result.unsupported_items.is_empty());
}
