use crate::test_helpers::{create_test_optional_rls_claims, create_test_service};
use crate::types::TriggerReapprovalRequest;
use axum::extract::State;
use axum::Json;
use intent_rebase_types::{
    AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
    IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
    IntentScope, RiskTier, Urgency,
};
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
use axum::http::StatusCode;
#[cfg(feature = "jwt-auth")]
use axum::response::IntoResponse;
#[cfg(feature = "jwt-auth")]
use intent_service::ApprovalRequestStatus;
// =====================================================================
// ADR-07: Approval Revalidation/Re-approval Trigger Tests (bounded slice)
// =====================================================================

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_creates_pending_approval_when_scope_differs() {
    let state = create_test_service();

    // Create an intent first (we need it to exist for get_intent_head to work)
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Call trigger_reapproval with different scope hashes
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "hash_v1".to_string(),
        current_scope_hash: "hash_v2".to_string(), // Different hash
        reapproval_reason: "Scope has changed since approval was granted".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state.clone()),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("trigger_reapproval should succeed when scope hashes differ");

    // Verify response
    assert_eq!(result.1.intent_id, intent_id);
    assert_eq!(result.1.intent_version_from, 1);
    assert_eq!(result.1.intent_version_to, 2);
    assert_eq!(result.1.status, "Pending");
    assert!(result.1.notification_intent); // Always true (advisory only)
    assert_eq!(
        result.1.reason,
        "Scope has changed since approval was granted"
    );

    // Verify the approval request was created in the repository
    let created_approval = state
        .approval_request_repo
        .get_approval_request(result.1.approval_request_id)
        .await
        .unwrap();
    assert_eq!(created_approval.status, ApprovalRequestStatus::Pending);
    assert_eq!(created_approval.intent_version_from, 1);
    assert_eq!(created_approval.intent_version_to, 2);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_returns_bad_request_when_scope_matches() {
    let state = create_test_service();

    // Create an intent
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Call trigger_reapproval with SAME scope hashes (no drift)
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "same_hash".to_string(),
        current_scope_hash: "same_hash".to_string(), // Same hash
        reapproval_reason: "Should not trigger".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_returns_not_found_when_intent_missing() {
    let state = create_test_service();

    let request = TriggerReapprovalRequest {
        intent_id: Uuid::new_v4(), // Non-existent intent
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "hash_v1".to_string(),
        current_scope_hash: "hash_v2".to_string(),
        reapproval_reason: "Test".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_cancels_existing_approved_approvals() {
    let state = create_test_service();

    // Create an intent
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Get intent head to get tenant_id
    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create an existing approved approval request
    let existing_approved = intent_service::ApprovalRequest::new_pending(
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
    let existing_approved_id = existing_approved.id;
    state
        .approval_request_repo
        .create_approval_request(existing_approved)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            existing_approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

    // Verify the existing approval is Approved
    let verified_approved = state
        .approval_request_repo
        .get_approval_request(existing_approved_id)
        .await
        .unwrap();
    assert_eq!(verified_approved.status, ApprovalRequestStatus::Approved);

    // Call trigger_reapproval with different scope hashes
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "hash_v1".to_string(),
        current_scope_hash: "hash_v2".to_string(), // Different hash
        reapproval_reason: "Scope has changed since approval was granted".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state.clone()),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("trigger_reapproval should succeed when scope hashes differ");

    // Verify a new pending approval was created
    assert_eq!(result.1.status, "Pending");

    // Verify the existing approved approval was cancelled
    let cancelled_approved = state
        .approval_request_repo
        .get_approval_request(existing_approved_id)
        .await
        .unwrap();
    assert_eq!(
        cancelled_approved.status,
        ApprovalRequestStatus::Cancelled,
        "Existing approved approval should be cancelled"
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_does_not_cancel_pending_approvals() {
    let state = create_test_service();

    // Create an intent
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Get intent head to get tenant_id
    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create an existing pending approval request
    let existing_pending = intent_service::ApprovalRequest::new_pending(
        intent_id,
        1,
        2,
        workflow_id,
        tenant_id,
        "external-api/previous",
        "external-api",
        "D",
        "Previous pending approval",
    );
    let existing_pending_id = existing_pending.id;
    state
        .approval_request_repo
        .create_approval_request(existing_pending)
        .await
        .unwrap();

    // Verify the existing approval is Pending
    let verified_pending = state
        .approval_request_repo
        .get_approval_request(existing_pending_id)
        .await
        .unwrap();
    assert_eq!(verified_pending.status, ApprovalRequestStatus::Pending);

    // Call trigger_reapproval with different scope hashes
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "hash_v1".to_string(),
        current_scope_hash: "hash_v2".to_string(), // Different hash
        reapproval_reason: "Scope has changed since approval was granted".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state.clone()),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await
    .expect("trigger_reapproval should succeed when scope hashes differ");

    // Verify a new pending approval was created
    assert_eq!(result.1.status, "Pending");

    // Verify the existing pending approval is still Pending (not cancelled)
    let still_pending = state
        .approval_request_repo
        .get_approval_request(existing_pending_id)
        .await
        .unwrap();
    assert_eq!(
        still_pending.status,
        ApprovalRequestStatus::Pending,
        "Existing pending approval should NOT be cancelled"
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_trigger_reapproval_does_not_create_or_cancel_when_scope_matches() {
    let state = create_test_service();

    // Create an intent
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Get intent head to get tenant_id
    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_id = intent_head.intent.tenant_id;

    // Create an existing approved approval request
    let existing_approved = intent_service::ApprovalRequest::new_pending(
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
    let existing_approved_id = existing_approved.id;
    state
        .approval_request_repo
        .create_approval_request(existing_approved)
        .await
        .unwrap();
    state
        .approval_request_repo
        .update_approval_request_status(
            existing_approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

    // Verify the existing approval is Approved
    let verified_approved = state
        .approval_request_repo
        .get_approval_request(existing_approved_id)
        .await
        .unwrap();
    assert_eq!(verified_approved.status, ApprovalRequestStatus::Approved);

    // Call trigger_reapproval with SAME scope hashes (should return 400)
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "same_hash".to_string(),
        current_scope_hash: "same_hash".to_string(), // Same hash - no drift
        reapproval_reason: "Should not trigger".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state.clone()),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(request),
    )
    .await;
    assert!(result.is_err());

    // Verify error is BAD_REQUEST
    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Verify the existing approved approval is still Approved (not cancelled)
    let still_approved = state
        .approval_request_repo
        .get_approval_request(existing_approved_id)
        .await
        .unwrap();
    assert_eq!(
        still_approved.status,
        ApprovalRequestStatus::Approved,
        "Existing approved approval should NOT be cancelled when scope hashes match"
    );
}

// =====================================================================
// ADR-07: trigger_reapproval JWT Tenant Mismatch Tests (Phase 3 P3-S5)
// =====================================================================

#[tokio::test]
#[cfg(feature = "jwt-auth")]
async fn test_trigger_reapproval_rejects_tenant_mismatch() {
    let state = create_test_service();

    // Create an intent first (we need it to exist for get_intent_head to work)
    let workflow_id = Uuid::new_v4();

    let create_request = CreateIntentRequest {
        tenant_id: None,
        workflow_id,
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

    // Get intent head to find the tenant_id (TenantA)
    let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
    let tenant_a = intent_head.intent.tenant_id;

    // Create JWT claims for a different tenant (TenantB)
    let tenant_b = Uuid::new_v4();

    // Call trigger_reapproval with tenant mismatch (JWT has TenantB, intent has TenantA)
    let request = TriggerReapprovalRequest {
        intent_id,
        original_version_from: 1,
        current_version_to: 2,
        original_scope_hash: "hash_v1".to_string(),
        current_scope_hash: "hash_v2".to_string(), // Different hash - would normally succeed
        reapproval_reason: "Scope has changed since approval was granted".to_string(),
    };

    let result = crate::trigger_reapproval_handlers::trigger_reapproval(
        State(state.clone()),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Json(request),
    )
    .await;

    // Verify the request was rejected with Unauthorized
    assert!(
        result.is_err(),
        "trigger_reapproval should fail on tenant mismatch"
    );
    let err = result.unwrap_err();
    let response = err.into_response();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Tenant mismatch should return 401 Unauthorized"
    );

    // Verify no approval request was created (fail-closed before mutation)
    let approvals = state
        .approval_request_repo
        .list_by_intent(intent_id, tenant_a)
        .await
        .unwrap();
    assert!(
        approvals.is_empty(),
        "No approval should be created when tenant mismatch is detected"
    );
}
