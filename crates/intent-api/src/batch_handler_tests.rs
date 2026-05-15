#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;
#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_service;
use crate::types::{BatchOrchestrationRequest, OrchestrationQuery};

use axum::extract::{Query, State};
use axum::Json;
use uuid::Uuid;

// -------------------------------------------------------------------------
// batch_approve_compensation_actions Tenant Mismatch Tests (RLC-2)
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_approve_compensation_actions_rejects_tenant_mismatch() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Try to batch approve with TenantB (mismatch) - request includes the action
    let tenant_b = Uuid::new_v4();
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_approve_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Query(OrchestrationQuery {
            tenant_id: tenant_b,
        }),
        Json(request),
    )
    .await;

    // Should fail with Unauthorized (fail-closed on tenant mismatch)
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.0.to_string();
    assert!(
        err_msg.contains("Tenant mismatch"),
        "Expected tenant mismatch error, got: {}",
        err_msg
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_approve_compensation_actions_succeeds_with_matching_tenant() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Batch approve with TenantA (matching)
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_approve_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Query(OrchestrationQuery {
            tenant_id: tenant_a,
        }),
        Json(request),
    )
    .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Expected success with matching tenant, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
}

// -------------------------------------------------------------------------
// batch_reapprove_compensation_actions Tenant Mismatch Tests (RLC-2)
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_reapprove_compensation_actions_rejects_tenant_mismatch() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Manually set to Failed status to make it reapprovable
    use compensation_service::CompensationStatus;
    let _failed_action = state
        .compensation_action_service
        .update_status(created.id, CompensationStatus::Failed, created.lock_version)
        .await
        .unwrap();

    // Try to batch reapprove with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_reapprove_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Query(OrchestrationQuery {
            tenant_id: tenant_b,
        }),
        Json(request),
    )
    .await;

    // Should fail with Unauthorized (fail-closed on tenant mismatch)
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.0.to_string();
    assert!(
        err_msg.contains("Tenant mismatch"),
        "Expected tenant mismatch error, got: {}",
        err_msg
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_reapprove_compensation_actions_succeeds_with_matching_tenant() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Manually set to Failed status to make it reapprovable
    use compensation_service::CompensationStatus;
    let _failed_action = state
        .compensation_action_service
        .update_status(created.id, CompensationStatus::Failed, created.lock_version)
        .await
        .unwrap();

    // Batch reapprove with TenantA (matching)
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_reapprove_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Query(OrchestrationQuery {
            tenant_id: tenant_a,
        }),
        Json(request),
    )
    .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Expected success with matching tenant, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
}

// -------------------------------------------------------------------------
// batch_execute_compensation_actions Tenant Mismatch Tests (RLC-2)
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_execute_compensation_actions_rejects_tenant_mismatch() {
    let state = create_test_service();

    // Create an Approved compensation action with TenantA
    // Must be Approved + Automatic feasibility for batch_execute
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execute
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Manually set to Approved status (necessary for batch_execute)
    use compensation_service::CompensationStatus;
    let _approved_action = state
        .compensation_action_service
        .update_status(
            created.id,
            CompensationStatus::Approved,
            created.lock_version,
        )
        .await
        .unwrap();

    // Try to batch execute with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_execute_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Query(OrchestrationQuery {
            tenant_id: tenant_b,
        }),
        Json(request),
    )
    .await;

    // Phase 1 P1-S5h: Per-item fail-closed on tenant mismatch - batch continues
    // but the mismatched item is recorded as failed with error message
    assert!(
        result.is_ok(),
        "Expected Ok response with per-item failure, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.total, 1);
    assert_eq!(response.summary.failed, 1);
    assert_eq!(response.summary.succeeded, 0);
    // The error message should indicate tenant mismatch / access denied
    let outcome = &response.outcomes[0];
    assert!(!outcome.success);
    assert!(outcome.error.is_some());
    let error_msg = outcome.error.as_ref().unwrap();
    assert!(
        error_msg.contains("Tenant mismatch") || error_msg.contains("access denied"),
        "Expected tenant mismatch or access denied error, got: {}",
        error_msg
    );
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_batch_execute_compensation_actions_succeeds_with_matching_tenant() {
    let state = create_test_service();

    // Create an Approved compensation action with TenantA
    // Must be Approved + Automatic feasibility for batch_execute
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execute
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Manually set to Approved status (necessary for batch_execute)
    use compensation_service::CompensationStatus;
    let _approved_action = state
        .compensation_action_service
        .update_status(
            created.id,
            CompensationStatus::Approved,
            created.lock_version,
        )
        .await
        .unwrap();

    // Batch execute with TenantA (matching)
    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_execute_compensation_actions(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Query(OrchestrationQuery {
            tenant_id: tenant_a,
        }),
        Json(request),
    )
    .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Expected success with matching tenant, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
}

// -------------------------------------------------------------------------
// Non-JWT Fallback Tests (no-default-features)
// -------------------------------------------------------------------------

/// Smoke test: non-JWT batch_approve_compensation_actions returns valid response shape.
#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_batch_approve_no_jwt_smoke() {
    let state = crate::test_helpers::create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_approve_compensation_actions(
        State(state),
        Query(OrchestrationQuery { tenant_id }),
        Json(request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected Ok with valid action, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.total, 1);
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
    assert!(response.outcomes[0].success);
}

/// Smoke test: non-JWT batch_reapprove_compensation_actions returns valid response shape.
#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_batch_reapprove_no_jwt_smoke() {
    let state = crate::test_helpers::create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::ManualOnly,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Set action to Failed so it can be reapproved
    use compensation_service::CompensationStatus;
    state
        .compensation_action_service
        .update_status(created.id, CompensationStatus::Failed, created.lock_version)
        .await
        .unwrap();

    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_reapprove_compensation_actions(
        State(state),
        Query(OrchestrationQuery { tenant_id }),
        Json(request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected Ok with valid Failed action, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.total, 1);
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
    assert!(response.outcomes[0].success);
}

/// Smoke test: non-JWT batch_execute_compensation_actions returns valid response shape.
#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_batch_execute_no_jwt_smoke() {
    let state = crate::test_helpers::create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Set action to Approved so it can be executed
    use compensation_service::CompensationStatus;
    state
        .compensation_action_service
        .update_status(
            created.id,
            CompensationStatus::Approved,
            created.lock_version,
        )
        .await
        .unwrap();

    let request = BatchOrchestrationRequest {
        action_ids: vec![created.id],
        initiated_by: Some("test-initiator".to_string()),
    };

    let result = crate::batch_handlers::batch_execute_compensation_actions(
        State(state),
        Query(OrchestrationQuery { tenant_id }),
        Json(request),
    )
    .await;

    assert!(
        result.is_ok(),
        "Expected Ok with valid Approved action, got: {:?}",
        result
    );
    let response = result.unwrap();
    assert_eq!(response.summary.total, 1);
    assert_eq!(response.summary.succeeded, 1);
    assert_eq!(response.summary.failed, 0);
    assert!(response.outcomes[0].success);
}
