use super::*;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

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
