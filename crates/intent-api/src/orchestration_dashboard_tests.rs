//! Orchestration dashboard handler tests.
//!
//! Extracted from handler_tests.rs as a focused module.

use super::*;

#[cfg(feature = "jwt-auth")]
use crate::query_handlers::get_orchestration_dashboard;
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

// =========================================================================
// Orchestration Dashboard Tests (Phase 3 Batch 1 bounded read-only slice)
// =========================================================================

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_empty_state() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return even with no data");

    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.tenant_id, tenant_id);
    assert!(result.side_effects.is_empty());
    assert_eq!(result.side_effect_summary.total, 0);
    assert!(result.compensation_actions.is_empty());
    assert_eq!(result.compensation_action_summary.total, 0);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_with_side_effects() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Record some side effects
    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record-123",
        )
        .await
        .unwrap();

    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S4Irreversible,
            "money_transfer",
            "account-xyz",
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.side_effects.len(), 2);
    assert_eq!(result.side_effect_summary.total, 2);
    assert_eq!(result.side_effect_summary.irreversible_count, 1);
    assert_eq!(result.side_effect_summary.auto_compensatable_count, 1);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_with_compensation_actions() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create actions in different statuses
    // Pending action
    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let pending_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    state
        .compensation_action_service
        .create_action(pending_action)
        .await
        .unwrap();

    // Approved + Automatic action (auto-executable)
    let approved_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback 2",
    );
    let approved = state
        .compensation_action_service
        .create_action(approved_action)
        .await
        .unwrap();
    state
        .compensation_action_service
        .approve_action(approved.id, approved.lock_version, Some("test"))
        .await
        .unwrap();

    // Failed + retryable error (reapprovable)
    let failed_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback 3",
    );
    let failed = state
        .compensation_action_service
        .create_action(failed_action)
        .await
        .unwrap();
    // Approve then fail with retryable error
    let failed_approved = state
        .compensation_action_service
        .approve_action(failed.id, failed.lock_version, Some("test"))
        .await
        .unwrap();
    let failed_result = compensation_service::ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            failed_approved.id,
            &failed_result,
            failed_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.compensation_actions.len(), 3);
    assert_eq!(result.compensation_action_summary.total, 3);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
    assert_eq!(result.compensation_action_summary.status_counts.approved, 1);
    assert_eq!(result.compensation_action_summary.status_counts.failed, 1);
    assert_eq!(result.compensation_action_summary.retryable_failed_count, 1);
    assert_eq!(result.compensation_action_summary.reapprovable_count, 1);
    assert_eq!(result.compensation_action_summary.auto_executable_count, 1);
    // Approved + Automatic
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_dlq_candidates() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create a failed action with non-retryable error (DLQ candidate)
    let dlq_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    let dlq = state
        .compensation_action_service
        .create_action(dlq_action)
        .await
        .unwrap();
    // Approve then fail with non-retryable error
    let dlq_approved = state
        .compensation_action_service
        .approve_action(dlq.id, dlq.lock_version, Some("test"))
        .await
        .unwrap();
    let dlq_result = compensation_service::ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            dlq_approved.id,
            &dlq_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
    // Non-retryable error + exhausted budget = DLQ candidate, not reapprovable
    assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_exhausted_budget_dlq() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let rebase_context = compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with max_retries = 1
    let mut dlq_action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        compensation_service::CompensationFeasibility::Automatic,
        compensation_service::StrategyType::Rollback,
        "Auto rollback",
    );
    dlq_action.max_retries = 1; // Exhaust on first failure

    let dlq = state
        .compensation_action_service
        .create_action(dlq_action)
        .await
        .unwrap();
    // Approve then fail with retryable error (but budget exhausted)
    let dlq_approved = state
        .compensation_action_service
        .approve_action(dlq.id, dlq.lock_version, Some("test"))
        .await
        .unwrap();
    let dlq_result = compensation_service::ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    );
    state
        .compensation_action_service
        .record_result(
            dlq_approved.id,
            &dlq_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    // Exhausted budget makes it a DLQ candidate even with retryable error
    assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
    assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_response_shape() {
    use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};

    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create a side effect
    state
        .side_effect_service
        .record_side_effect(
            tenant_id,
            intent_id,
            1,
            compensation_service::SideEffectClass::S2ExternalReversible,
            "pr_opened",
            "https://github.com/example/pull/123",
        )
        .await
        .unwrap();

    // Create a compensation action
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = compensation_service::CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::FollowupNotice,
        "Send follow-up",
    );
    state
        .compensation_action_service
        .create_action(action)
        .await
        .unwrap();

    let query = OrchestrationDashboardQuery { tenant_id };
    let result = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query),
    )
    .await
    .expect("Dashboard should return data");

    // Verify response structure
    assert_eq!(result.intent_id, intent_id);
    assert_eq!(result.tenant_id, tenant_id);
    assert_eq!(result.side_effects.len(), 1);
    assert_eq!(result.compensation_actions.len(), 1);

    // Verify side effect summary
    assert_eq!(result.side_effect_summary.total, 1);
    assert_eq!(result.side_effect_summary.irreversible_count, 0);
    assert_eq!(result.side_effect_summary.auto_compensatable_count, 0); // S2 is not auto

    // Verify compensation action summary
    assert_eq!(result.compensation_action_summary.total, 1);
    assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
    assert_eq!(result.compensation_action_summary.auto_executable_count, 0);
    // SemiAutomatic is not auto
}

#[cfg(feature = "jwt-auth")]
#[tokio::test]
async fn test_orchestration_dashboard_tenant_isolation() {
    let state = create_test_service();
    let intent_id = Uuid::new_v4();
    let tenant_id_1 = Uuid::new_v4();
    let tenant_id_2 = Uuid::new_v4();

    // Record side effects for tenant 1
    state
        .side_effect_service
        .record_side_effect(
            tenant_id_1,
            intent_id,
            1,
            compensation_service::SideEffectClass::S1InternalReversible,
            "effect_1",
            "target_1",
        )
        .await
        .unwrap();

    // Record side effects for tenant 2
    state
        .side_effect_service
        .record_side_effect(
            tenant_id_2,
            intent_id,
            1,
            compensation_service::SideEffectClass::S2ExternalReversible,
            "effect_2",
            "target_2",
        )
        .await
        .unwrap();

    // Query for tenant 1
    let query1 = OrchestrationDashboardQuery {
        tenant_id: tenant_id_1,
    };
    let result1 = get_orchestration_dashboard(
        State(state.clone()),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query1),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result1.side_effect_summary.total, 1);
    assert_eq!(result1.side_effects[0].effect_type, "effect_1");

    // Query for tenant 2
    let query2 = OrchestrationDashboardQuery {
        tenant_id: tenant_id_2,
    };
    let result2 = get_orchestration_dashboard(
        State(state),
        auth::OptionalRlsTenantClaims(None),
        Path(intent_id),
        axum::extract::Query(query2),
    )
    .await
    .expect("Dashboard should return data");

    assert_eq!(result2.side_effect_summary.total, 1);
    assert_eq!(result2.side_effects[0].effect_type, "effect_2");
}
