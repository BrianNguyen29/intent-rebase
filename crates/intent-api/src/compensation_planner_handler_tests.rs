#[cfg(feature = "jwt-auth")]
use crate::test_helpers::create_test_optional_rls_claims;
use crate::test_helpers::create_test_service;
use crate::types::{OrchestrationDryRunRequest, OrchestrationQuery};
use axum::extract::{Query, State};
use axum::Json;
use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};
use uuid::Uuid;

// -------------------------------------------------------------------------
// orchestration_dry_run Tenant Mismatch Tests (P1-S5i)
// -------------------------------------------------------------------------

/// Tests that orchestration_dry_run rejects JWT tenant mismatch.
/// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match query tenant_id.
#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dry_run_rejects_tenant_mismatch() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
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

    // Try to run dry-run with TenantB (mismatch)
    let tenant_b = Uuid::new_v4();
    let query = OrchestrationQuery {
        tenant_id: tenant_a, // Query has TenantA
    };
    let request = OrchestrationDryRunRequest {
        action_ids: vec![created.id],
    };

    let result = crate::compensation_planner_handlers::orchestration_dry_run(
        State(state),
        create_test_optional_rls_claims(tenant_b), // JWT has TenantB - mismatch
        Query(query),
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

/// Tests that orchestration_dry_run succeeds when JWT tenant matches query tenant.
/// P1-S5i: Validates the happy path for tenant-matched requests.
#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dry_run_succeeds_with_matching_tenant() {
    let state = create_test_service();

    // Create a compensation action with TenantA
    let tenant_a = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_a,
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

    // Run dry-run with TenantA (matching)
    let query = OrchestrationQuery {
        tenant_id: tenant_a,
    };
    let request = OrchestrationDryRunRequest {
        action_ids: vec![created.id],
    };

    let result = crate::compensation_planner_handlers::orchestration_dry_run(
        State(state),
        create_test_optional_rls_claims(tenant_a), // Tenant A matches
        Query(query),
        Json(request),
    )
    .await;

    // Should succeed
    assert!(
        result.is_ok(),
        "Expected success with matching tenant, got: {:?}",
        result
    );
}
