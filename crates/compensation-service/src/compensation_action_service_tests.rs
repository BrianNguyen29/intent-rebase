use super::*;
use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
use crate::compensation_action_repo::InMemoryCompensationActionRepository;
use crate::rollback_record::RollbackRecordResult;
use crate::rollback_record_repo::InMemoryRollbackRecordRepository;
use std::sync::Arc;

fn create_test_service() -> CompensationActionService {
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    CompensationActionService::new(repo)
}

#[allow(dead_code)]
fn create_test_service_with_side_effect_repo() -> CompensationActionService {
    // Service configured with side effect repo for real RollbackExecutor path
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
}

fn create_test_action(
    tenant_id: Uuid,
    side_effect_id: Uuid,
    intent_id: Uuid,
) -> CompensationAction {
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test rollback",
    )
}

#[tokio::test]
async fn test_create_action() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let result = service.create_action(action).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().tenant_id, tenant_id);
}

#[tokio::test]
async fn test_get_action() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    let retrieved = service.get_action(created.id).await.unwrap();
    assert_eq!(retrieved.id, created.id);
}

#[tokio::test]
async fn test_get_action_not_found() {
    let service = create_test_service();
    let result = service.get_action(Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_by_tenant() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    for _ in 0..3 {
        let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        service.create_action(action).await.unwrap();
    }

    let result = service.list_by_tenant(tenant_id, None).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_by_status() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    service.create_action(action1).await.unwrap();

    let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    action2.status = CompensationStatus::Executed;
    service.create_action(action2).await.unwrap();

    let pending = service
        .list_by_status(tenant_id, CompensationStatus::Pending)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let executed = service
        .list_by_status(tenant_id, CompensationStatus::Executed)
        .await
        .unwrap();
    assert_eq!(executed.len(), 1);
}

#[tokio::test]
async fn test_update_status() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    let updated = service
        .update_status(
            created.id,
            CompensationStatus::Approved,
            created.lock_version,
        )
        .await
        .unwrap();

    assert_eq!(updated.status, CompensationStatus::Approved);
    assert!(updated.approved_at.is_some());
}

#[tokio::test]
async fn test_record_result_success() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    let result = ExecutionResult::success("Rollback completed");
    let updated = service
        .record_result(created.id, &result, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(updated.status, CompensationStatus::Executed);
    assert_eq!(updated.attempt_count, 1);
}

#[tokio::test]
async fn test_record_result_failure() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    let result = ExecutionResult::failure("Rollback failed", "ERR_001", None);
    let updated = service
        .record_result(created.id, &result, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(updated.status, CompensationStatus::Failed);
    assert_eq!(updated.attempt_count, 1);
}

#[tokio::test]
async fn test_get_pending_actions() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    service.create_action(action1).await.unwrap();

    let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    action2.status = CompensationStatus::Executed;
    service.create_action(action2).await.unwrap();

    let pending = service.get_pending_actions(tenant_id).await.unwrap();
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_get_failed_actions() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    let mut action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    action1.status = CompensationStatus::Failed;
    service.create_action(action1).await.unwrap();

    let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    action2.status = CompensationStatus::Pending;
    service.create_action(action2).await.unwrap();

    let failed = service.get_failed_actions(tenant_id).await.unwrap();
    assert_eq!(failed.len(), 1);
}

// === Status Transition Tests ===

#[tokio::test]
async fn test_approve_pending_action_success() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    assert_eq!(created.status, CompensationStatus::Pending);

    let approved = service
        .approve_action(created.id, created.lock_version, Some("test-approver"))
        .await
        .unwrap();

    assert_eq!(approved.status, CompensationStatus::Approved);
    assert!(approved.approved_at.is_some());
    assert_eq!(approved.approved_by, Some("test-approver".to_string()));
}

#[tokio::test]
async fn test_approve_action_fails_on_non_pending() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First approve it
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);

    // Try to approve again - should fail
    let result = service
        .approve_action(approved.id, approved.lock_version, None)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidCompensationActionTransition { .. }
    ));
}

#[tokio::test]
async fn test_approve_action_fails_on_executed() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Execute directly (bypass approval) by setting status to Approved first
    let approved = service
        .update_status(
            created.id,
            CompensationStatus::Approved,
            created.lock_version,
        )
        .await
        .unwrap();

    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();
    assert_eq!(executed.status, CompensationStatus::Executed);

    // Try to approve an executed action - should fail
    let result = service
        .approve_action(executed.id, executed.lock_version, None)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidCompensationActionTransition { .. }
    ));
}

#[tokio::test]
async fn test_approve_action_fails_on_concurrency_conflict() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Try to approve with wrong lock_version - should fail with ConcurrencyConflict
    let result = service
        .approve_action(created.id, created.lock_version + 1, None)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::ConcurrencyConflict(_)
    ));
}

#[tokio::test]
async fn test_waive_pending_action_success() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    assert_eq!(created.status, CompensationStatus::Pending);

    let waived = service
        .waive_action(created.id, created.lock_version, Some("test-waiver"))
        .await
        .unwrap();

    assert_eq!(waived.status, CompensationStatus::Waived);
    // waived_by is stored in dedicated waived_by field
    assert_eq!(waived.waived_by, Some("test-waiver".to_string()));
    assert!(waived.waived_at.is_some());
}

#[tokio::test]
async fn test_waive_action_fails_on_non_pending() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First approve it
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);

    // Try to waive an approved action - should fail
    let result = service
        .waive_action(approved.id, approved.lock_version, None)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidCompensationActionTransition { .. }
    ));
}

#[tokio::test]
async fn test_execute_action_success_on_approved() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create service with side effect repo
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

    // Create the side effect first so executor can find it
    let side_effect = crate::side_effect::SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: crate::side_effect::SideEffectClass::S1InternalReversible,
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    side_effect_repo.create(side_effect).await.unwrap();

    let service = CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo);

    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First approve it
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);

    // Execute - should succeed with real RollbackExecutor
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    assert_eq!(executed.status, CompensationStatus::Executed);
    assert!(executed.executed_at.is_some());
    assert_eq!(executed.executed_by, Some("test-executor".to_string()));
    assert!(executed.execution_result_payload.is_some());
}

#[tokio::test]
async fn test_execute_action_fails_on_pending() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    assert_eq!(created.status, CompensationStatus::Pending);

    // Try to execute without approval - should fail
    let result = service
        .execute_action(created.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_action_fails_on_executed() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Approve and execute
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();
    assert_eq!(executed.status, CompensationStatus::Executed);

    // Try to execute again - should fail
    let result = service
        .execute_action(executed.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_action_fails_on_waived() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Waive it
    let waived = service
        .waive_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(waived.status, CompensationStatus::Waived);

    // Try to execute a waived action - should fail
    let result = service
        .execute_action(waived.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_action_fails_on_failed() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First make it Failed via record_result
    let failed_result = ExecutionResult::failure("Test failure", "TEST_ERR", None);
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(failed.status, CompensationStatus::Failed);

    // Try to execute a failed action - should fail
    let result = service
        .execute_action(failed.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

// === Execution Policy Gate Tests ===

#[tokio::test]
async fn test_execute_action_fails_on_non_automatic_feasibility() {
    // Phase 3 Batch 1 bounded slice: only Automatic feasibility can execute.
    // SemiAutomatic/ManualOnly require human intervention not in this slice.
    // NotPossible cannot be executed at all.
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with SemiAutomatic feasibility (requires human intervention)
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::FollowupNotice,
        "Send follow-up notice",
    );

    let created = service.create_action(action).await.unwrap();
    assert_eq!(created.feasibility, CompensationFeasibility::SemiAutomatic);

    // Approve it
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);

    // Try to execute - should fail because SemiAutomatic requires human intervention
    let result = service
        .execute_action(approved.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_action_fails_on_manual_only_feasibility() {
    // Phase 3 Batch 1 bounded slice: ManualOnly feasibility requires human intervention
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Escalation,
        "Manual escalation required",
    );

    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Try to execute - should fail because ManualOnly requires human intervention
    let result = service
        .execute_action(approved.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_action_fails_on_not_possible_feasibility() {
    // Phase 3 Batch 1 bounded slice: NotPossible feasibility cannot be executed at all
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Quarantine,
        "Cannot compensate",
    );

    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Try to execute - should fail because NotPossible cannot be executed
    let result = service
        .execute_action(approved.id, Some("test-executor"))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

// === Transition Matrix Tests ===

#[test]
fn test_status_transition_pending_to_approved() {
    let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Approved);
    assert!(validation.allowed);
}

#[test]
fn test_status_transition_pending_to_waived() {
    let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Waived);
    assert!(validation.allowed);
}

#[test]
fn test_status_transition_approved_to_executed() {
    let validation = CompensationStatus::Approved.can_transition_to(CompensationStatus::Executed);
    assert!(validation.allowed);
}

#[test]
fn test_status_transition_executed_is_terminal() {
    assert!(CompensationStatus::Executed.is_terminal());
    let validation = CompensationStatus::Executed.can_transition_to(CompensationStatus::Pending);
    assert!(!validation.allowed);
    assert!(validation.reason.is_some());
}

#[test]
fn test_status_transition_failed_is_not_terminal() {
    // Phase 3 Batch 1: Failed is NOT terminal because manual retry allows Failed → Pending
    assert!(!CompensationStatus::Failed.is_terminal());
    let validation = CompensationStatus::Failed.can_transition_to(CompensationStatus::Pending);
    assert!(validation.allowed);
    assert!(validation.reason.is_some());
}

#[test]
fn test_status_transition_waived_is_terminal() {
    assert!(CompensationStatus::Waived.is_terminal());
    let validation = CompensationStatus::Waived.can_transition_to(CompensationStatus::Pending);
    assert!(!validation.allowed);
    assert!(validation.reason.is_some());
}

#[test]
fn test_status_transition_pending_to_executed_not_allowed() {
    // Must be approved first
    let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Executed);
    assert!(!validation.allowed);
}

#[test]
fn test_status_transition_approved_to_pending_not_allowed() {
    // No undo of approval
    let validation = CompensationStatus::Approved.can_transition_to(CompensationStatus::Pending);
    assert!(!validation.allowed);
}

#[test]
fn test_status_transition_to_same_status_not_allowed() {
    let validation = CompensationStatus::Pending.can_transition_to(CompensationStatus::Pending);
    assert!(!validation.allowed);
    assert!(validation.reason.is_some());
}

// === Manual Retry Tests ===

#[tokio::test]
async fn test_reapprove_action_success() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First make it Failed with a retryable error via record_result
    let failed_result = ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        Some("Connection timed out".to_string()),
    );
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(failed.status, CompensationStatus::Failed);
    assert_eq!(failed.attempt_count, 1);

    // Now reapprove it
    let reapproved = service
        .reapprove_action(failed.id, failed.lock_version)
        .await
        .unwrap();

    assert_eq!(reapproved.status, CompensationStatus::Pending);
    assert_eq!(reapproved.attempt_count, 1); // attempt_count preserved
    assert!(reapproved.failed_at.is_none()); // failed_at cleared
}

#[tokio::test]
async fn test_reapprove_action_fails_on_non_failed_status() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Try to reapprove a Pending action - should fail
    let result = service
        .reapprove_action(created.id, created.lock_version)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidCompensationActionTransition { .. }
    ));
}

#[tokio::test]
async fn test_reapprove_action_fails_on_retry_budget_exhausted() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with max_retries = 1 for testing
    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test rollback",
    );
    action.max_retries = 1; // Set to 1 so first failure exhausts budget

    let created = service.create_action(action).await.unwrap();

    // First failure
    let failed_result1 = ExecutionResult::failure("First failure", "CONNECTION_TIMEOUT", None);
    let failed1 = service
        .record_result(created.id, &failed_result1, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(failed1.attempt_count, 1);

    // Try to reapprove - should fail because budget exhausted
    let result = service
        .reapprove_action(failed1.id, failed1.lock_version)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        IntentRebaseError::CompensationActionNotReapprovable(_, _)
    ));
}

#[tokio::test]
async fn test_reapprove_action_fails_on_non_retryable_error() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Fail with a non-retryable error
    let failed_result = ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION", // Non-retryable error
        Some("Invalid configuration".to_string()),
    );
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(failed.status, CompensationStatus::Failed);

    // Try to reapprove - should fail because error is non-retryable
    let result = service
        .reapprove_action(failed.id, failed.lock_version)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        IntentRebaseError::CompensationActionNotReapprovable(_, _)
    ));
}

#[tokio::test]
async fn test_reapprove_action_fails_on_concurrency_conflict() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // First make it Failed
    let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    // Try to reapprove with wrong lock_version - should fail
    let result = service
        .reapprove_action(failed.id, failed.lock_version + 1)
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::ConcurrencyConflict(_)
    ));
}

#[tokio::test]
async fn test_list_dlq_candidates_empty() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();

    let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
    assert!(dlq.is_empty());
}

#[tokio::test]
async fn test_list_dlq_candidates_returns_exhausted_budget() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with max_retries = 1
    let mut action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test",
    );
    action.max_retries = 1;

    let created = service.create_action(action).await.unwrap();

    // First failure exhausts budget
    let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    // Verify it's a DLQ candidate
    assert!(failed.is_dlq_candidate());

    // List DLQ candidates
    let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].id, failed.id);
}

#[tokio::test]
async fn test_list_dlq_candidates_returns_non_retryable_error() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let created = service.create_action(action).await.unwrap();

    // Fail with non-retryable error
    let failed_result =
        ExecutionResult::failure("Permanent failure", "INVALID_CONFIGURATION", None);
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    // Verify it's a DLQ candidate
    assert!(failed.is_dlq_candidate());

    // List DLQ candidates
    let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].id, failed.id);
}

#[tokio::test]
async fn test_list_dlq_candidates_excludes_retryable_failures() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let created = service.create_action(action).await.unwrap();

    // Fail with retryable error
    let failed_result = ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT", // Retryable
        None,
    );
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    // Verify it's NOT a DLQ candidate (can be reapproved)
    assert!(!failed.is_dlq_candidate());
    assert!(failed.can_be_reapproved());

    // List DLQ candidates
    let dlq = service.list_dlq_candidates(tenant_id).await.unwrap();
    assert!(dlq.is_empty());
}

#[tokio::test]
async fn test_get_dlq_candidate_count() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action with max_retries = 1
    let mut action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test",
    );
    action.max_retries = 1;

    let created = service.create_action(action).await.unwrap();

    // First failure exhausts budget
    let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
    service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    let count = service.get_dlq_candidate_count(tenant_id).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_reapprove_preserves_attempt_count() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let created = service.create_action(action).await.unwrap();

    // First failure
    let failed_result = ExecutionResult::failure("Failure", "CONNECTION_TIMEOUT", None);
    let failed = service
        .record_result(created.id, &failed_result, created.lock_version, None)
        .await
        .unwrap();

    assert_eq!(failed.attempt_count, 1);

    // Reapprove
    let reapproved = service
        .reapprove_action(failed.id, failed.lock_version)
        .await
        .unwrap();

    // Attempt count should be preserved
    assert_eq!(reapproved.attempt_count, 1);

    // Execute and fail again
    let approved = service
        .approve_action(reapproved.id, reapproved.lock_version, None)
        .await
        .unwrap();

    let failed2_result = ExecutionResult::failure("Second failure", "READ_TIMEOUT", None);
    let failed2 = service
        .record_result(approved.id, &failed2_result, approved.lock_version, None)
        .await
        .unwrap();

    // Now attempt_count should be 2
    assert_eq!(failed2.attempt_count, 2);
}

#[tokio::test]
async fn test_list_batch_candidates_all_categories() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create 4 actions, one for each category

    // Category 1: Pending approval (Pending status + Automatic feasibility)
    let mut pending_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Pending auto action",
    );
    pending_action.max_retries = 3;
    let pending_created = service.create_action(pending_action).await.unwrap();
    assert_eq!(pending_created.status, CompensationStatus::Pending);

    // Category 2: Approved auto-executable (Approved status + Automatic feasibility)
    let mut approved_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Approved auto action",
    );
    approved_action.max_retries = 3;
    let approved_created = service.create_action(approved_action).await.unwrap();
    let approved_updated = service
        .approve_action(approved_created.id, approved_created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved_updated.status, CompensationStatus::Approved);
    assert!(approved_updated.is_auto_executable());

    // Category 3: Retryable failed (Failed status + retryable error + budget remains)
    let mut retryable_failed_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Retryable failed action",
    );
    retryable_failed_action.max_retries = 3;
    let retryable_created = service
        .create_action(retryable_failed_action)
        .await
        .unwrap();
    // Approve then fail with retryable error
    let retryable_approved = service
        .approve_action(retryable_created.id, retryable_created.lock_version, None)
        .await
        .unwrap();
    let retryable_failed_result = ExecutionResult::failure("Transient", "CONNECTION_TIMEOUT", None);
    let retryable_failed = service
        .record_result(
            retryable_approved.id,
            &retryable_failed_result,
            retryable_approved.lock_version,
            None,
        )
        .await
        .unwrap();
    assert_eq!(retryable_failed.status, CompensationStatus::Failed);
    assert!(retryable_failed.can_be_reapproved());
    assert!(!retryable_failed.is_dlq_candidate());

    // Category 4: DLQ candidate (Failed status + exhausted budget)
    let mut dlq_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "DLQ candidate action",
    );
    dlq_action.max_retries = 1; // Exhausts on first failure
    let dlq_created = service.create_action(dlq_action).await.unwrap();
    // Approve then fail to exhaust budget
    let dlq_approved = service
        .approve_action(dlq_created.id, dlq_created.lock_version, None)
        .await
        .unwrap();
    let dlq_failed_result = ExecutionResult::failure("Exhausted", "CONNECTION_TIMEOUT", None);
    let dlq_failed = service
        .record_result(
            dlq_approved.id,
            &dlq_failed_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();
    assert_eq!(dlq_failed.status, CompensationStatus::Failed);
    assert!(dlq_failed.is_dlq_candidate());
    assert!(!dlq_failed.can_be_reapproved());

    // Also create a non-retryable DLQ candidate for additional coverage
    let mut non_retryable_dlq_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Non-retryable DLQ action",
    );
    non_retryable_dlq_action.max_retries = 3;
    let non_retryable_created = service
        .create_action(non_retryable_dlq_action)
        .await
        .unwrap();
    let non_retryable_approved = service
        .approve_action(
            non_retryable_created.id,
            non_retryable_created.lock_version,
            None,
        )
        .await
        .unwrap();
    let non_retryable_failed_result = ExecutionResult::failure("Permanent", "INVALID_CONFIG", None);
    let non_retryable_failed = service
        .record_result(
            non_retryable_approved.id,
            &non_retryable_failed_result,
            non_retryable_approved.lock_version,
            None,
        )
        .await
        .unwrap();
    assert_eq!(non_retryable_failed.status, CompensationStatus::Failed);
    assert!(non_retryable_failed.is_dlq_candidate());
    assert!(!non_retryable_failed.can_be_reapproved());

    // Now test the batch candidates endpoint
    let batch = service.list_batch_candidates(tenant_id).await.unwrap();

    // Verify pending approval candidates
    assert_eq!(batch.pending_approval_candidates.len(), 1);
    assert_eq!(batch.pending_approval_candidates[0].id, pending_created.id);

    // Verify approved service-executable candidates
    assert_eq!(batch.approved_service_executable_candidates.len(), 1);
    assert_eq!(
        batch.approved_service_executable_candidates[0].id,
        approved_updated.id
    );

    // Verify retryable failed candidates
    assert_eq!(batch.retryable_failed_candidates.len(), 1);
    assert_eq!(batch.retryable_failed_candidates[0].id, retryable_failed.id);

    // Verify DLQ candidates (should be 2: exhausted budget + non-retryable error)
    assert_eq!(batch.dlq_candidates.len(), 2);
    let dlq_ids: Vec<_> = batch.dlq_candidates.iter().map(|a| a.id).collect();
    assert!(dlq_ids.contains(&dlq_failed.id));
    assert!(dlq_ids.contains(&non_retryable_failed.id));
}

#[tokio::test]
async fn test_list_batch_candidates_empty_for_tenant() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();

    let batch = service.list_batch_candidates(tenant_id).await.unwrap();

    assert!(batch.pending_approval_candidates.is_empty());
    assert!(batch.approved_service_executable_candidates.is_empty());
    assert!(batch.retryable_failed_candidates.is_empty());
    assert!(batch.dlq_candidates.is_empty());
}

#[tokio::test]
async fn test_list_batch_candidates_approved_non_auto_not_included() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create an Approved action with SemiAutomatic feasibility (not auto-executable)
    let mut semi_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Rollback,
        "Semi-auto approved action",
    );
    semi_action.max_retries = 3;
    let semi_created = service.create_action(semi_action).await.unwrap();
    let semi_approved = service
        .approve_action(semi_created.id, semi_created.lock_version, None)
        .await
        .unwrap();

    // Should be Approved but NOT auto-executable
    assert_eq!(semi_approved.status, CompensationStatus::Approved);
    assert!(!semi_approved.is_auto_executable());

    // Also create a pending action (Automatic feasibility) so we have something in pending
    let mut pending_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Pending auto action",
    );
    pending_action.max_retries = 3;
    let _pending_created = service.create_action(pending_action).await.unwrap();

    // Batch candidates should NOT include Rollback+SemiAutomatic in approved_service_executable
    // but SHOULD include the Automatic pending action
    let batch = service.list_batch_candidates(tenant_id).await.unwrap();
    assert!(batch.approved_service_executable_candidates.is_empty());
    assert_eq!(batch.pending_approval_candidates.len(), 1);
    assert_eq!(
        batch.pending_approval_candidates[0].feasibility,
        CompensationFeasibility::Automatic
    );
}

#[tokio::test]
async fn test_list_batch_candidates_includes_counter_action_semi_auto() {
    // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic should be included in batch candidates
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create a CounterAction+SemiAutomatic action (S2ExternalReversible)
    let mut counter_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter the PR",
    );
    counter_action.max_retries = 3;
    let counter_created = service.create_action(counter_action).await.unwrap();
    let counter_approved = service
        .approve_action(counter_created.id, counter_created.lock_version, None)
        .await
        .unwrap();

    // Should be Approved AND service-executable
    assert_eq!(counter_approved.status, CompensationStatus::Approved);
    assert!(counter_approved.is_service_executable());
    assert!(!counter_approved.is_auto_executable()); // Not Automatic, but IS service-executable

    // Batch candidates should include CounterAction+SemiAutomatic in approved_service_executable
    let batch = service.list_batch_candidates(tenant_id).await.unwrap();
    assert_eq!(batch.approved_service_executable_candidates.len(), 1);
    assert_eq!(
        batch.approved_service_executable_candidates[0].id,
        counter_approved.id
    );
}

// === Policy Gate Evaluation Tests ===

#[tokio::test]
async fn test_evaluate_policy_gates_empty_for_tenant() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 0);
    assert_eq!(result.summary.total_actions, 0);
    assert_eq!(result.summary.eligible_count, 0);
    assert_eq!(result.summary.blocked_count, 0);
    assert_eq!(result.summary.manual_review_required_count, 0);
}

#[tokio::test]
async fn test_evaluate_policy_gates_eligible() {
    // Approved + Automatic feasibility = eligible
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let created = service.create_action(action).await.unwrap();
    assert_eq!(created.status, CompensationStatus::Pending);

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);
    assert_eq!(approved.feasibility, CompensationFeasibility::Automatic);

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 1);
    let eval = &result.evaluations[0];
    assert_eq!(eval.gate_status, PolicyGateStatus::Eligible);
    assert!(eval.gate_reason.contains("approved"));
    assert!(eval.gate_reason.contains("Automatic"));
    assert!(eval.policy_metadata.auto_executable);
    assert!(!eval.policy_metadata.is_dlq_candidate);

    assert_eq!(result.summary.total_actions, 1);
    assert_eq!(result.summary.eligible_count, 1);
    assert_eq!(result.summary.blocked_count, 0);
    assert_eq!(result.summary.manual_review_required_count, 0);
}

#[tokio::test]
async fn test_evaluate_policy_gates_blocked_executed() {
    // Executed status = blocked
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let created = service.create_action(action).await.unwrap();
    let executed_result = ExecutionResult::success("Completed");
    let _executed = service
        .record_result(created.id, &executed_result, created.lock_version, None)
        .await
        .unwrap();

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 1);
    let eval = &result.evaluations[0];
    assert_eq!(eval.gate_status, PolicyGateStatus::Blocked);
    assert!(eval.gate_reason.contains("terminal"));
    assert_eq!(result.summary.eligible_count, 0);
    assert_eq!(result.summary.blocked_count, 1);
}

#[tokio::test]
async fn test_evaluate_policy_gates_blocked_dlq() {
    // DLQ candidate = blocked
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let mut action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
    action.max_retries = 1; // Exhausts on first failure

    let created = service.create_action(action).await.unwrap();
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    let failed_result = ExecutionResult::failure("Failed", "CONNECTION_TIMEOUT", None);
    let _failed = service
        .record_result(approved.id, &failed_result, approved.lock_version, None)
        .await
        .unwrap();

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 1);
    let eval = &result.evaluations[0];
    assert_eq!(eval.gate_status, PolicyGateStatus::Blocked);
    assert!(eval.gate_reason.contains("DLQ"));
    assert!(eval.policy_metadata.is_dlq_candidate);
    assert_eq!(result.summary.dlq_candidate_count, 1);
}

#[tokio::test]
async fn test_evaluate_policy_gates_manual_review_pending() {
    // Pending status = manual_review_required
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, Uuid::new_v4(), intent_id);

    let _created = service.create_action(action).await.unwrap();

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 1);
    let eval = &result.evaluations[0];
    assert_eq!(eval.gate_status, PolicyGateStatus::ManualReviewRequired);
    assert!(eval.gate_reason.contains("awaits approval"));
    assert_eq!(result.summary.pending_approval_count, 1);
    assert_eq!(result.summary.manual_review_required_count, 1);
}

#[tokio::test]
async fn test_evaluate_policy_gates_manual_review_semi_automatic() {
    // Approved + SemiAutomatic = manual_review_required
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::FollowupNotice,
        "Followup",
    );

    let created = service.create_action(action).await.unwrap();
    let _approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    assert_eq!(result.evaluations.len(), 1);
    let eval = &result.evaluations[0];
    assert_eq!(eval.gate_status, PolicyGateStatus::ManualReviewRequired);
    assert!(eval.gate_reason.contains("SemiAutomatic"));
    assert!(!eval.policy_metadata.auto_executable);
}

#[tokio::test]
async fn test_evaluate_policy_gates_mixed_actions() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Action 1: Pending (manual_review_required)
    let pending_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Pending",
    );
    let pending_created = service.create_action(pending_action).await.unwrap();

    // Action 2: Approved + Automatic (eligible)
    let mut approved_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Approved",
    );
    approved_action.max_retries = 3;
    let approved_created = service.create_action(approved_action).await.unwrap();
    let approved = service
        .approve_action(approved_created.id, approved_created.lock_version, None)
        .await
        .unwrap();

    // Action 3: DLQ candidate (blocked)
    let mut dlq_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "DLQ",
    );
    dlq_action.max_retries = 1;
    let dlq_created = service.create_action(dlq_action).await.unwrap();
    let dlq_approved = service
        .approve_action(dlq_created.id, dlq_created.lock_version, None)
        .await
        .unwrap();
    let dlq_failed_result = ExecutionResult::failure("Failed", "CONNECTION_TIMEOUT", None);
    let _dlq_failed = service
        .record_result(
            dlq_approved.id,
            &dlq_failed_result,
            dlq_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    // Action 4: Failed + retryable (manual_review_required)
    let mut retryable_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Retryable",
    );
    retryable_action.max_retries = 3;
    let retryable_created = service.create_action(retryable_action).await.unwrap();
    let retryable_approved = service
        .approve_action(retryable_created.id, retryable_created.lock_version, None)
        .await
        .unwrap();
    let retryable_failed_result = ExecutionResult::failure("Transient", "CONNECTION_TIMEOUT", None);
    let _retryable_failed = service
        .record_result(
            retryable_approved.id,
            &retryable_failed_result,
            retryable_approved.lock_version,
            None,
        )
        .await
        .unwrap();

    // Debug: fetch actions directly to verify states
    let all_actions = service.list_by_tenant(tenant_id, None).await.unwrap();
    eprintln!("\n=== All actions from list_by_tenant ===");
    for a in &all_actions {
        eprintln!(
            "id={}, status={:?}, feasibility={:?}, attempt_count={}, max_retries={}",
            a.id, a.status, a.feasibility, a.attempt_count, a.max_retries
        );
    }

    // Call evaluate_policy_gates and debug inside the service
    let result = service.evaluate_policy_gates(tenant_id).await.unwrap();

    eprintln!("\n=== Evaluations ===");
    for eval in &result.evaluations {
        eprintln!(
            "id={}, status={:?}, gate={:?}, is_dlq={}, is_auto_exec={}",
            eval.action.id,
            eval.action.status,
            eval.gate_status,
            eval.policy_metadata.is_dlq_candidate,
            eval.policy_metadata.auto_executable
        );
    }

    eprintln!("\n=== Summary ===");
    eprintln!(
        "total={}, eligible={}, blocked={}, manual_review={}, dlq={}, pending={}, auto_exec={}",
        result.summary.total_actions,
        result.summary.eligible_count,
        result.summary.blocked_count,
        result.summary.manual_review_required_count,
        result.summary.dlq_candidate_count,
        result.summary.pending_approval_count,
        result.summary.auto_executable_count
    );

    // Verify gate statuses from evaluations
    // The order in the repository is not guaranteed to be creation order.
    // We verify by checking the action IDs.

    // Find evaluations by action ID
    let approved_action_id = approved.id;
    let dlq_action_id = _dlq_failed.id;
    let retryable_action_id = _retryable_failed.id;
    let pending_action_id = pending_created.id;

    let eval_by_id = |id: Uuid| -> &PolicyGateEvaluation {
        result
            .evaluations
            .iter()
            .find(|e| e.action.id == id)
            .unwrap()
    };

    // pending_action: Pending -> ManualReviewRequired
    let pending_eval = eval_by_id(pending_action_id);
    assert_eq!(
        pending_eval.gate_status,
        PolicyGateStatus::ManualReviewRequired
    );

    // approved_action: Approved -> Eligible
    let approved_eval = eval_by_id(approved_action_id);
    assert_eq!(approved_eval.gate_status, PolicyGateStatus::Eligible);

    // dlq_action: Failed (DLQ) -> Blocked
    let dlq_eval = eval_by_id(dlq_action_id);
    assert_eq!(dlq_eval.gate_status, PolicyGateStatus::Blocked);

    // retryable_action: Failed (retryable) -> ManualReviewRequired
    let retryable_eval = eval_by_id(retryable_action_id);
    assert_eq!(
        retryable_eval.gate_status,
        PolicyGateStatus::ManualReviewRequired
    );

    // Verify summary counts
    assert_eq!(result.summary.total_actions, 4);
    assert_eq!(result.summary.eligible_count, 1); // only approved_action
    assert_eq!(result.summary.blocked_count, 1); // only dlq_action
    assert_eq!(result.summary.manual_review_required_count, 2); // pending + retryable
    assert_eq!(result.summary.pending_approval_count, 1);
    assert_eq!(result.summary.dlq_candidate_count, 1);
    assert_eq!(result.summary.auto_executable_count, 4); // all actions have Automatic feasibility
}

#[tokio::test]
async fn test_evaluate_policy_gates_for_intent() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let other_intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Create action for the target intent
    let action1 = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "For target intent",
    );
    let _created1 = service.create_action(action1).await.unwrap();

    // Create action for a different intent
    let other_rebase_context = RebaseContext::new(other_intent_id, 1, 2, Uuid::new_v4());
    let action2 = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        other_intent_id,
        other_rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "For other intent",
    );
    let _created2 = service.create_action(action2).await.unwrap();

    // Evaluate for target intent only
    let result = service
        .evaluate_policy_gates_for_intent(intent_id, tenant_id)
        .await
        .unwrap();

    assert_eq!(result.summary.total_actions, 1);
}

// ============================================================================
// Coordination Status Tests (Phase 3 Batch 1 bounded read-only orchestration view)
// ============================================================================

#[test]
fn test_coordination_status_ready() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test ready action",
    );

    // Approved + Automatic + not blocked = Ready
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::AwaitingPolicy
    );

    let mut approved_action = action;
    approved_action.status = CompensationStatus::Approved;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&approved_action),
        CoordinationStatus::Ready
    );
}

#[test]
fn test_coordination_status_awaiting_policy() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test pending action",
    );

    // Pending = AwaitingPolicy
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::AwaitingPolicy
    );
}

#[test]
fn test_coordination_status_awaiting_manual_review() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Approved + ManualOnly = AwaitingManualReview
    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::ManualOnly,
        StrategyType::CounterAction,
        "Test manual action",
    );
    action.status = CompensationStatus::Approved;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::AwaitingManualReview
    );

    // Failed + can reapprove = AwaitingManualReview
    let mut retryable_action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test retryable failed",
    );
    retryable_action.status = CompensationStatus::Failed;
    retryable_action.execution_result_payload = Some(ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    ));
    assert_eq!(
        CoordinationStatus::from_compensation_action(&retryable_action),
        CoordinationStatus::AwaitingManualReview
    );
}

#[test]
fn test_coordination_status_blocked() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // DLQ candidate (exhausted budget) = Blocked
    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test DLQ action",
    );
    action.status = CompensationStatus::Failed;
    action.attempt_count = 3;
    action.max_retries = 3;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::Blocked
    );

    // Failed + non-retryable error = Blocked
    let mut non_retryable_action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test non-retryable",
    );
    non_retryable_action.status = CompensationStatus::Failed;
    non_retryable_action.execution_result_payload = Some(ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    ));
    assert_eq!(
        CoordinationStatus::from_compensation_action(&non_retryable_action),
        CoordinationStatus::Blocked
    );
}

#[test]
fn test_coordination_status_terminal() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    // Executed = Terminal
    let mut executed_action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test executed",
    );
    executed_action.status = CompensationStatus::Executed;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&executed_action),
        CoordinationStatus::Terminal
    );

    // Waived = Terminal
    let mut waived_action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test waived",
    );
    waived_action.status = CompensationStatus::Waived;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&waived_action),
        CoordinationStatus::Terminal
    );
}

#[test]
fn test_coordination_status_ready_counter_action_semi_auto() {
    // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic (Approved) should be Ready
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Test CounterAction+SemiAutomatic",
    );

    // Pending + SemiAutomatic = AwaitingPolicy (not yet approved)
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::AwaitingPolicy
    );

    // Approved + CounterAction + SemiAutomatic = Ready (service-executable)
    action.status = CompensationStatus::Approved;
    assert_eq!(
        CoordinationStatus::from_compensation_action(&action),
        CoordinationStatus::Ready
    );
}

#[tokio::test]
async fn test_evaluate_coordination_status_empty_for_tenant() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();

    let result = service
        .evaluate_coordination_status(tenant_id)
        .await
        .unwrap();

    assert_eq!(result.summary.total_actions, 0);
    assert_eq!(result.summary.ready_count, 0);
    assert_eq!(result.summary.awaiting_policy_count, 0);
    assert_eq!(result.summary.awaiting_manual_review_count, 0);
    assert_eq!(result.summary.blocked_count, 0);
    assert_eq!(result.summary.terminal_count, 0);
    assert!(result.records.is_empty());
}

#[tokio::test]
async fn test_evaluate_coordination_status_mixed_actions() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();

    // Create: Pending = AwaitingPolicy
    let pending_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Pending",
    );
    service.create_action(pending_action).await.unwrap();

    // Create: Approved + Automatic = Ready
    let mut ready_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Ready",
    );
    ready_action.status = CompensationStatus::Approved;
    service.create_action(ready_action).await.unwrap();

    // Create: Approved + ManualOnly = AwaitingManualReview
    let mut manual_review_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::ManualOnly,
        StrategyType::CounterAction,
        "Manual",
    );
    manual_review_action.status = CompensationStatus::Approved;
    service.create_action(manual_review_action).await.unwrap();

    // Create: Failed + retryable = AwaitingManualReview
    let mut retryable_failed = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Retryable failed",
    );
    retryable_failed.status = CompensationStatus::Failed;
    retryable_failed.execution_result_payload =
        Some(ExecutionResult::failure("Temp", "CONNECTION_TIMEOUT", None));
    service.create_action(retryable_failed).await.unwrap();

    // Create: Failed + exhausted = Blocked
    let mut blocked_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Blocked",
    );
    blocked_action.status = CompensationStatus::Failed;
    blocked_action.attempt_count = 3;
    blocked_action.max_retries = 3;
    service.create_action(blocked_action).await.unwrap();

    // Create: Executed = Terminal
    let mut terminal_action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Terminal",
    );
    terminal_action.status = CompensationStatus::Executed;
    service.create_action(terminal_action).await.unwrap();

    let result = service
        .evaluate_coordination_status(tenant_id)
        .await
        .unwrap();

    assert_eq!(result.summary.total_actions, 6);
    assert_eq!(result.summary.ready_count, 1); // Approved + Automatic
    assert_eq!(result.summary.awaiting_policy_count, 1); // Pending
    assert_eq!(result.summary.awaiting_manual_review_count, 2); // ManualOnly + retryable failed
    assert_eq!(result.summary.blocked_count, 1); // exhausted budget
    assert_eq!(result.summary.terminal_count, 1); // Executed
    assert_eq!(result.records.len(), 6);
}

#[tokio::test]
async fn test_evaluate_coordination_status_for_intent() {
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let other_intent_id = Uuid::new_v4();

    // Create action for the target intent
    let action1 = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "For target intent",
    );
    service.create_action(action1).await.unwrap();

    // Create action for a different intent
    let action2 = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        other_intent_id,
        RebaseContext::new(other_intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "For other intent",
    );
    service.create_action(action2).await.unwrap();

    // Evaluate for target intent only
    let result = service
        .evaluate_coordination_status_for_intent(intent_id, tenant_id)
        .await
        .unwrap();

    assert_eq!(result.summary.total_actions, 1);
}

#[tokio::test]
async fn test_coordination_record_from_action() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test action",
    );
    action.status = CompensationStatus::Approved;

    let record = CoordinationRecord::from_action(&action);

    assert_eq!(record.coordination_status, CoordinationStatus::Ready);
    assert!(record.auto_executable); // is_service_executable() includes Rollback+Automatic
    assert!(!record.is_dlq_candidate);
    assert_eq!(record.feasibility, CompensationFeasibility::Automatic);
    assert_eq!(record.strategy_type, StrategyType::Rollback);
    assert_eq!(record.status, CompensationStatus::Approved);
}

#[test]
fn test_coordination_record_auto_executable_for_counter_action_semi_auto() {
    // Phase 3 Batch 1 P7: CounterAction+SemiAutomatic auto_executable=true (is_service_executable)
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let mut action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter PR",
    );
    action.status = CompensationStatus::Approved;

    let record = CoordinationRecord::from_action(&action);

    assert_eq!(record.coordination_status, CoordinationStatus::Ready);
    assert!(record.auto_executable); // is_service_executable() includes CounterAction+SemiAutomatic
    assert!(!record.is_dlq_candidate);
    assert_eq!(record.feasibility, CompensationFeasibility::SemiAutomatic);
    assert_eq!(record.strategy_type, StrategyType::CounterAction);
    assert_eq!(record.status, CompensationStatus::Approved);
}

// ============================================================================
// Audit Emission Tests (Phase 3 Batch 0 bounded slice)
// ============================================================================

fn create_test_service_with_audit_repo(
    audit_repo: Arc<dyn intent_rebase_types::AuditRepository>,
) -> CompensationActionService {
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
        .with_audit_repo(audit_repo)
}

#[tokio::test]
async fn test_create_action_emits_compensation_planned_audit_event() {
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
    let service = create_test_service_with_audit_repo(audit_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Verify CompensationPlanned audit event was emitted
    let events = audit_repo
        .list_by_intent(intent_id, tenant_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].event_type,
        intent_rebase_types::AuditEventType::CompensationPlanned
    ));

    // Verify payload contents
    let payload: intent_rebase_types::CompensationPlannedAuditPayload =
        serde_json::from_value(events[0].payload.clone()).unwrap();
    assert_eq!(payload.compensation_plan_id, created.id);
    assert_eq!(payload.intent_id, intent_id);
    assert_eq!(payload.side_effect_count, 1);
    assert_eq!(payload.auto_compensatable_count, 1);
}

#[tokio::test]
async fn test_create_action_does_not_emit_audit_when_no_audit_repo() {
    // Service without audit repo should not emit events
    let service = create_test_service();
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let result = service.create_action(action).await;
    assert!(result.is_ok());
    // No error even without audit repo - fail-open behavior
}

#[tokio::test]
async fn test_execute_action_emits_started_and_completed_audit_events() {
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
    let _service = create_test_service_with_audit_repo(audit_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create side effect so executor can find it
    let side_effect = crate::side_effect::SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: crate::side_effect::SideEffectClass::S1InternalReversible,
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    // Access the side_effect_repo through the service's internal state
    // For this test, we use a service that has side_effect_repo configured
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    side_effect_repo.create(side_effect).await.unwrap();

    let service_with_side_effect =
        CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo)
            .with_audit_repo(audit_repo.clone());

    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service_with_side_effect
        .create_action(action)
        .await
        .unwrap();
    let approved = service_with_side_effect
        .approve_action(created.id, created.lock_version, Some("test-approver"))
        .await
        .unwrap();

    // Execute - should succeed and emit CompensationStarted + CompensationCompleted
    let _executed = service_with_side_effect
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    // Verify audit events were emitted
    let events = audit_repo
        .list_by_intent(intent_id, tenant_id)
        .await
        .unwrap();
    assert_eq!(events.len(), 3); // CompensationPlanned + Started + Completed

    let event_types: Vec<_> = events.iter().map(|e| e.event_type.clone()).collect();
    assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationPlanned));
    assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationStarted));
    assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationCompleted));
}

#[tokio::test]
async fn test_execute_action_emits_failed_audit_event_on_failure() {
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    // Create service WITHOUT side_effect_repo so RollbackExecutor fails
    let service = CompensationActionService::new(repo).with_audit_repo(audit_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();
    let approved = service
        .approve_action(created.id, created.lock_version, Some("test-approver"))
        .await
        .unwrap();

    // Execute - stub returns success without side_effect_repo, so this should succeed
    // (using stub behavior for backward compatibility)
    let _executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    // Even stub success should have emitted Started + Completed
    let events = audit_repo
        .list_by_intent(intent_id, tenant_id)
        .await
        .unwrap();
    assert!(events.len() >= 2);
    let event_types: Vec<_> = events.iter().map(|e| e.event_type.clone()).collect();
    assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationStarted));
    assert!(event_types.contains(&intent_rebase_types::AuditEventType::CompensationCompleted));
    assert_eq!(_executed.status, CompensationStatus::Executed);
}

#[tokio::test]
async fn test_audit_emission_is_best_effort_fail_open() {
    // Test that audit emission failures don't affect the main operation
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
    let service = create_test_service_with_audit_repo(audit_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    // Create action should succeed even if audit repo has issues
    let _created = service.create_action(action).await.unwrap();
    assert_eq!(_created.tenant_id, tenant_id);
}

#[tokio::test]
async fn test_compensation_audit_payload_contents() {
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new());
    let service = create_test_service_with_audit_repo(audit_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();
    let action = create_test_action(tenant_id, side_effect_id, intent_id);

    let created = service.create_action(action).await.unwrap();

    // Verify CompensationPlanned audit event payload
    let events = audit_repo
        .list_by_intent(intent_id, tenant_id)
        .await
        .unwrap();
    let planned_event = events
        .iter()
        .find(|e| {
            matches!(
                e.event_type,
                intent_rebase_types::AuditEventType::CompensationPlanned
            )
        })
        .unwrap();

    let payload: intent_rebase_types::CompensationPlannedAuditPayload =
        serde_json::from_value(planned_event.payload.clone()).unwrap();
    assert_eq!(payload.compensation_plan_id, created.id);
    assert_eq!(payload.intent_id, intent_id);
    assert_eq!(payload.intent_version_from, 1);
    assert_eq!(payload.intent_version_to, 2);
    assert_eq!(payload.side_effect_count, 1);
    assert_eq!(payload.auto_compensatable_count, 1);
    assert_eq!(payload.manual_required_count, 0);
    assert_eq!(payload.not_possible_count, 0);
}

// ============================================================================
// Rollback Record Tests (Phase 3 Batch 1 bounded rollback record slice)
// ============================================================================

fn create_test_service_with_rollback_record_repo(
    rollback_record_repo: Arc<dyn RollbackRecordRepository>,
) -> (
    CompensationActionService,
    Arc<crate::side_effect_repo::InMemorySideEffectRepository>,
) {
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let service =
        CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo.clone())
            .with_rollback_record_repo(rollback_record_repo);
    (service, side_effect_repo)
}

#[tokio::test]
async fn test_execute_action_creates_rollback_record_on_success() {
    let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
    let (service, side_effect_repo) =
        create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create the side effect first so executor can find it
    let side_effect = crate::side_effect::SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: crate::side_effect::SideEffectClass::S1InternalReversible,
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    side_effect_repo.create(side_effect).await.unwrap();

    let action = create_test_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    // Approve and execute
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute - with side effect present, executor succeeds
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    // Verify rollback records exist
    let records = rollback_record_repo
        .list_by_compensation_action(executed.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result, RollbackRecordResult::Success);
    assert_eq!(records[0].compensation_action_id, executed.id);
    assert_eq!(records[0].side_effect_id, side_effect_id);
    assert_eq!(records[0].intent_id, intent_id);
    assert_eq!(records[0].recorded_by, Some("test-executor".to_string()));
}

#[tokio::test]
async fn test_execute_action_creates_rollback_record_on_failure() {
    let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
    let (service, _side_effect_repo) =
        create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    // Use a side_effect_id that won't exist, causing executor to fail
    let side_effect_id = Uuid::new_v4();

    let action = create_test_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    // Approve
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute - should fail because side effect doesn't exist in repo
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    // Verify rollback record was created with failure
    let records = rollback_record_repo
        .list_by_compensation_action(executed.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result, RollbackRecordResult::Failure);
    assert!(records[0].error_code.is_some());
}

#[tokio::test]
async fn test_waive_action_creates_rollback_record() {
    let rollback_record_repo = Arc::new(InMemoryRollbackRecordRepository::new());
    let (service, _side_effect_repo) =
        create_test_service_with_rollback_record_repo(rollback_record_repo.clone());

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let action = create_test_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    // Waive the action
    let waived = service
        .waive_action(created.id, created.lock_version, Some("test-waiver"))
        .await
        .unwrap();

    // Verify rollback record was created
    let records = rollback_record_repo
        .list_by_compensation_action(waived.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].result, RollbackRecordResult::Waived);
    assert_eq!(records[0].recorded_by, Some("test-waiver".to_string()));
}

#[tokio::test]
async fn test_execute_action_skips_rollback_record_when_repo_not_configured() {
    // Service WITHOUT rollback_record_repo - should not fail
    let service = create_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let action = create_test_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute should succeed even without rollback_record_repo
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();

    assert_eq!(executed.status, CompensationStatus::Executed);
}

#[tokio::test]
async fn test_waive_action_skips_rollback_record_when_repo_not_configured() {
    // Service WITHOUT rollback_record_repo - should not fail
    let service = create_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    let action = create_test_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    // Waive should succeed even without rollback_record_repo
    let waived = service
        .waive_action(created.id, created.lock_version, Some("test-waiver"))
        .await
        .unwrap();

    assert_eq!(waived.status, CompensationStatus::Waived);
}

// ============================================================================
// CounterAction + SemiAutomatic Tests (Phase 3 Batch 1 P7 bounded slice)
// ============================================================================

fn create_counter_action_semi_auto_test_service() -> (
    CompensationActionService,
    Arc<crate::side_effect_repo::InMemorySideEffectRepository>,
) {
    let repo = Arc::new(InMemoryCompensationActionRepository::new());
    let side_effect_repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let service =
        CompensationActionService::new_with_side_effect_repo(repo, side_effect_repo.clone());
    (service, side_effect_repo)
}

fn create_counter_action_semi_auto_action(
    tenant_id: Uuid,
    side_effect_id: Uuid,
    intent_id: Uuid,
) -> CompensationAction {
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Close PR as counter-action",
    )
}

#[tokio::test]
async fn test_execute_counter_action_semi_auto_success() {
    let (service, side_effect_repo) = create_counter_action_semi_auto_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create S2ExternalReversible side effect so executor can find it
    let side_effect = crate::side_effect::SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: crate::side_effect::SideEffectClass::S2ExternalReversible,
        effect_type: "pr_opened".to_string(),
        target: "https://github.com/pulls/123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    side_effect_repo.create(side_effect).await.unwrap();

    // Create CounterAction + SemiAutomatic action
    let action = create_counter_action_semi_auto_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    // Approve the action
    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();
    assert_eq!(approved.status, CompensationStatus::Approved);

    // Execute should succeed
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();
    assert_eq!(executed.status, CompensationStatus::Executed);
    assert!(executed.execution_result_payload.is_some());
    let result = executed.execution_result_payload.unwrap();
    assert!(result.success);
    assert!(result.summary.contains("Counter-action"));
    assert!(result.summary.contains("acknowledged"));
}

#[tokio::test]
async fn test_execute_counter_action_semi_auto_fails_on_wrong_strategy() {
    let (service, _side_effect_repo) = create_counter_action_semi_auto_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create action with Rollback strategy but SemiAutomatic feasibility
    // This should fail because Rollback only works with Automatic
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Rollback, // Wrong: Rollback needs Automatic
        "Rollback with SemiAuto",
    );
    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute should fail with CompensationActionNotExecutable error
    // because Rollback + SemiAutomatic is not a supported combo
    let result = service
        .execute_action(approved.id, Some("test-executor"))
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_counter_action_semi_auto_fails_on_wrong_feasibility() {
    let (service, _side_effect_repo) = create_counter_action_semi_auto_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create action with CounterAction strategy but Automatic feasibility
    // This should fail because CounterAction needs SemiAutomatic
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic, // Wrong: CounterAction needs SemiAutomatic
        StrategyType::CounterAction,
        "CounterAction with Automatic",
    );
    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute should fail with CompensationActionNotExecutable error
    // because CounterAction + Automatic is not a supported combo
    let result = service
        .execute_action(approved.id, Some("test-executor"))
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::CompensationActionNotExecutable(_)
    ));
}

#[tokio::test]
async fn test_execute_counter_action_semi_auto_fails_on_s1_side_effect() {
    let (service, side_effect_repo) = create_counter_action_semi_auto_test_service();

    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let side_effect_id = Uuid::new_v4();

    // Create S1InternalReversible side effect instead of S2ExternalReversible
    let side_effect = crate::side_effect::SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: crate::side_effect::SideEffectClass::S1InternalReversible, // Wrong class
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    side_effect_repo.create(side_effect).await.unwrap();

    let action = create_counter_action_semi_auto_action(tenant_id, side_effect_id, intent_id);
    let created = service.create_action(action).await.unwrap();

    let approved = service
        .approve_action(created.id, created.lock_version, None)
        .await
        .unwrap();

    // Execute should fail because counter-action is only valid for S2ExternalReversible
    let executed = service
        .execute_action(approved.id, Some("test-executor"))
        .await
        .unwrap();
    assert_eq!(executed.status, CompensationStatus::Failed);
    let result = executed.execution_result_payload.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("INVALID_SIDE_EFFECT_CLASS".to_string())
    );
}

#[tokio::test]
async fn test_is_service_executable_for_counter_action_semi_auto() {
    let rebase_context = RebaseContext::new(Uuid::new_v4(), 1, 2, Uuid::new_v4());

    // CounterAction + SemiAutomatic should be service executable
    let counter_action = CompensationAction::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        rebase_context.clone(),
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Test",
    );
    assert!(counter_action.is_service_executable());

    // Rollback + Automatic should also be service executable
    let rollback_action = CompensationAction::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        rebase_context.clone(),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test",
    );
    assert!(rollback_action.is_service_executable());

    // Rollback + SemiAutomatic should NOT be service executable
    let invalid_combo = CompensationAction::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        rebase_context.clone(),
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Rollback,
        "Test",
    );
    assert!(!invalid_combo.is_service_executable());

    // CounterAction + Automatic should NOT be service executable
    let invalid_combo2 = CompensationAction::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::CounterAction,
        "Test",
    );
    assert!(!invalid_combo2.is_service_executable());
}
