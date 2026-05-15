use crate::test_helpers::create_minimal_low_risk_payload;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;
use crate::{
    cancel_existing_approved_and_audit, cancel_specific_approved_and_audit, CancelApprovalContext,
};
use uuid::Uuid;

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
