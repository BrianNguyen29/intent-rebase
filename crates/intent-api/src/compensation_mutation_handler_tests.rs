use crate::test_helpers::create_test_service;
use crate::types::{
    ApproveCompensationActionBody, CompensationActionResponse, ExecuteCompensationActionBody,
};
#[cfg(feature = "jwt-auth")]
use crate::types::{ReapproveCompensationActionBody, WaiveCompensationActionBody};

#[cfg(not(feature = "jwt-auth"))]
use axum::http::StatusCode;
#[cfg(not(feature = "jwt-auth"))]
use axum::response::IntoResponse;
use axum::{extract::Path, extract::State, Json};
use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};

use uuid::Uuid;

// === Compensation Action API Tests ===

#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_approve_compensation_action_success() {
    let state = create_test_service();

    // Create a compensation action directly via the service
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Approve the action via the API
    let request = ApproveCompensationActionBody {
        lock_version: created.lock_version,
        approved_by: Some("test-approver".to_string()),
    };
    let result = crate::compensation_mutation_handlers::approve_compensation_action(
        State(state),
        Path(created.id),
        Json(request),
    )
    .await
    .unwrap();

    assert_eq!(result.status, "approved");
    assert_eq!(result.approved_by, Some("test-approver".to_string()));
}

#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_approve_compensation_action_not_found() {
    let state = create_test_service();

    let request = ApproveCompensationActionBody {
        lock_version: 0,
        approved_by: None,
    };
    let result = crate::compensation_mutation_handlers::approve_compensation_action(
        State(state),
        Path(Uuid::new_v4()),
        Json(request),
    )
    .await;
    assert!(result.is_err());
}

#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_waive_compensation_action_success() {
    let state = create_test_service();

    // Create a compensation action
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Waive the action via the API
    let request = WaiveCompensationActionBody {
        lock_version: created.lock_version,
        waived_by: Some("test-waiver".to_string()),
    };
    let result = crate::compensation_mutation_handlers::waive_compensation_action(
        State(state),
        Path(created.id),
        Json(request),
    )
    .await
    .unwrap();

    assert_eq!(result.status, "waived");
}

#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_execute_compensation_action_success() {
    let state = create_test_service();

    // Create a compensation action
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // First approve it
    let approved = state
        .compensation_action_service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute the action via the API
    let request = ExecuteCompensationActionBody {
        executed_by: Some("test-executor".to_string()),
    };
    let result = crate::compensation_mutation_handlers::execute_compensation_action(
        State(state),
        Path(approved.id),
        Json(request),
    )
    .await
    .unwrap();

    assert_eq!(result.status, "executed");
    assert_eq!(result.executed_by, Some("test-executor".to_string()));
}

#[cfg(not(feature = "jwt-auth"))]
#[tokio::test]
async fn test_execute_compensation_action_fails_on_pending() {
    let state = create_test_service();

    // Create a compensation action (starts in Pending status)
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Try to execute without approval - should fail
    let request = ExecuteCompensationActionBody {
        executed_by: Some("test-executor".to_string()),
    };
    let result = crate::compensation_mutation_handlers::execute_compensation_action(
        State(state),
        Path(created.id),
        Json(request),
    )
    .await;

    assert!(result.is_err());
    let err_response = result.unwrap_err();
    let response = err_response.into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_compensation_action_response_serialization() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Rollback,
        "Test rollback",
    );

    let response = CompensationActionResponse::from(action);

    assert_eq!(response.status, "pending");
    assert_eq!(response.strategy_type, "rollback");
    assert_eq!(response.feasibility, "manual_only");
    assert_eq!(response.tenant_id, tenant_id);
    assert_eq!(response.intent_id, intent_id);
}

// =====================================================================
// Phase B: RLC Compensation Action Tenant Mismatch Tests (jwt-auth only)
// =====================================================================

#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;

// -------------------------------------------------------------------------
// approve_compensation_action Tenant Mismatch Tests
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_approve_compensation_action_rejects_tenant_mismatch() {
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

    // Try to approve with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = ApproveCompensationActionBody {
        lock_version: created.lock_version,
        approved_by: Some("test-approver".to_string()),
    };

    let result = crate::compensation_mutation_handlers::approve_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Path(created.id),
        Json(request),
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

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_approve_compensation_action_succeeds_with_matching_tenant() {
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

    // Approve with TenantA (matching)
    let request = ApproveCompensationActionBody {
        lock_version: created.lock_version,
        approved_by: Some("test-approver".to_string()),
    };

    let result = crate::compensation_mutation_handlers::approve_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Path(created.id),
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
    assert_eq!(response.status, "approved");
}

// -------------------------------------------------------------------------
// execute_compensation_action Tenant Mismatch Tests
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_execute_compensation_action_rejects_tenant_mismatch() {
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

    // Approve the action first (necessary for execution)
    state
        .compensation_action_service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Try to execute with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = ExecuteCompensationActionBody {
        executed_by: Some("test-executor".to_string()),
    };

    let result = crate::compensation_mutation_handlers::execute_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Path(created.id),
        Json(request),
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

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_execute_compensation_action_succeeds_with_matching_tenant() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    // Use Automatic feasibility so execution succeeds
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execution
        compensation_service::StrategyType::Rollback,
        "Test rollback",
    );
    let created = state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    // Approve the action first (necessary for execution)
    state
        .compensation_action_service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute with TenantA (matching)
    let request = ExecuteCompensationActionBody {
        executed_by: Some("test-executor".to_string()),
    };

    let result = crate::compensation_mutation_handlers::execute_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Path(created.id),
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
    assert_eq!(response.status, "executed");
}

// -------------------------------------------------------------------------
// waive_compensation_action Tenant Mismatch Tests (RLC-2)
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_waive_compensation_action_rejects_tenant_mismatch() {
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

    // Try to waive with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = WaiveCompensationActionBody {
        lock_version: created.lock_version,
        waived_by: Some("test-waiver".to_string()),
    };

    let result = crate::compensation_mutation_handlers::waive_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Path(created.id),
        Json(request),
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

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_waive_compensation_action_succeeds_with_matching_tenant() {
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

    // Waive with TenantA (matching)
    let request = WaiveCompensationActionBody {
        lock_version: created.lock_version,
        waived_by: Some("test-waiver".to_string()),
    };

    let result = crate::compensation_mutation_handlers::waive_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Path(created.id),
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
    assert_eq!(response.status, "waived");
}

// -------------------------------------------------------------------------
// reapprove_compensation_action Tenant Mismatch Tests (RLC-2)
// -------------------------------------------------------------------------

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_reapprove_compensation_action_rejects_tenant_mismatch() {
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
    // (can't easily create a Failed action through normal flow in test)
    use compensation_service::CompensationStatus;
    let failed_action = state
        .compensation_action_service
        .update_status(created.id, CompensationStatus::Failed, created.lock_version)
        .await
        .unwrap();

    // Try to reapprove with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let request = ReapproveCompensationActionBody {
        lock_version: failed_action.lock_version,
    };

    let result = crate::compensation_mutation_handlers::reapprove_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
        Path(created.id),
        Json(request),
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

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_reapprove_compensation_action_succeeds_with_matching_tenant() {
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
    let failed_action = state
        .compensation_action_service
        .update_status(created.id, CompensationStatus::Failed, created.lock_version)
        .await
        .unwrap();

    // Reapprove with TenantA (matching)
    let request = ReapproveCompensationActionBody {
        lock_version: failed_action.lock_version,
    };

    let result = crate::compensation_mutation_handlers::reapprove_compensation_action(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Path(created.id),
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
    assert_eq!(response.status, "pending");
}
