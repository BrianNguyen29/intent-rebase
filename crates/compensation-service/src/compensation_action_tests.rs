use crate::*;
use uuid::Uuid;

#[test]
fn test_compensation_action_from_side_effect_auto() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let effect = SideEffect::new(
        tenant_id,
        intent_id,
        1,
        SideEffectClass::S1InternalReversible,
        "metadata_write",
        "db-record",
    );
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::from_side_effect(
        tenant_id,
        &effect,
        &rebase_context,
        StrategyType::Rollback,
        "Auto rollback internal metadata",
    );

    assert_eq!(action.tenant_id, tenant_id);
    assert_eq!(action.side_effect_id, effect.id);
    assert_eq!(action.intent_id, intent_id);
    assert_eq!(action.feasibility, CompensationFeasibility::Automatic);
    assert!(action.is_auto_executable());
    assert_eq!(action.status, CompensationStatus::Pending);
    assert_eq!(action.attempt_count, 0);
    assert_eq!(action.lock_version, 0);
}

#[test]
fn test_compensation_action_from_side_effect_manual() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let effect = SideEffect::new(
        tenant_id,
        intent_id,
        1,
        SideEffectClass::S3ExternalPartiallyReversible,
        "email_sent",
        "user@example.com",
    );
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::from_side_effect(
        tenant_id,
        &effect,
        &rebase_context,
        StrategyType::FollowupNotice,
        "Send correction email",
    );

    assert_eq!(action.feasibility, CompensationFeasibility::ManualOnly);
    assert!(!action.is_auto_executable());
}

#[test]
fn test_compensation_action_not_possible_s0() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let effect = SideEffect::new(
        tenant_id,
        intent_id,
        1,
        SideEffectClass::S0PureRead,
        "read",
        "noop",
    );
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

    let action = CompensationAction::from_side_effect(
        tenant_id,
        &effect,
        &rebase_context,
        StrategyType::Quarantine,
        "N/A",
    );

    assert_eq!(action.feasibility, CompensationFeasibility::NotPossible);
}

#[test]
fn test_compensation_action_serialization_round_trip() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let effect = SideEffect::new(
        tenant_id,
        intent_id,
        1,
        SideEffectClass::S2ExternalReversible,
        "pr_opened",
        "https://github.com/pulls/123",
    );
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::from_side_effect(
        tenant_id,
        &effect,
        &rebase_context,
        StrategyType::CounterAction,
        "Close PR",
    );

    let json = serde_json::to_string(&action).unwrap();
    let deserialized: CompensationAction = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, action.id);
    assert_eq!(deserialized.tenant_id, tenant_id);
    assert_eq!(deserialized.side_effect_id, effect.id);
    assert_eq!(deserialized.intent_id, intent_id);
    assert_eq!(
        deserialized.feasibility,
        CompensationFeasibility::SemiAutomatic
    );
    assert_eq!(deserialized.strategy_type, StrategyType::CounterAction);
}

// === Retry/DLQ Model Tests ===

#[test]
fn test_classify_error_code_retryable() {
    let retryable_codes = vec![
        "CONNECTION_TIMEOUT",
        "CONNECTION_REFUSED",
        "NETWORK_UNREACHABLE",
        "READ_TIMEOUT",
        "WRITE_TIMEOUT",
        "SERVICE_UNAVAILABLE",
        "TEMPORARILY_OVERLOADED",
        "BACKEND_ERROR",
        "RESOURCE_BUSY",
        "LOCK_ACQUISITION_FAILED",
        "RATE_LIMIT_EXCEEDED",
        "QUOTA_EXCEEDED",
    ];

    for code in retryable_codes {
        let classification = CompensationAction::classify_error_code(code);
        assert_eq!(
            classification.retryable,
            RetryableErrorClass::Retryable,
            "Error code '{}' should be retryable",
            code
        );
    }
}

#[test]
fn test_classify_error_code_non_retryable() {
    let non_retryable_codes = vec![
        "INVALID_CONFIGURATION",
        "PERMISSION_DENIED",
        "RESOURCE_NOT_FOUND",
        "AUTHENTICATION_FAILED",
        "VALIDATION_ERROR",
        "UNKNOWN_ERROR",
    ];

    for code in non_retryable_codes {
        let classification = CompensationAction::classify_error_code(code);
        assert_eq!(
            classification.retryable,
            RetryableErrorClass::NonRetryable,
            "Error code '{}' should be non-retryable",
            code
        );
    }
}

#[test]
fn test_is_dlq_candidate_exhausted_budget() {
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
        "Test rollback",
    );

    // Exhaust retry budget
    action.status = CompensationStatus::Failed;
    action.attempt_count = 3; // Default max_retries is 3
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Failed",
        "CONNECTION_TIMEOUT",
        None,
    ));

    assert!(action.is_dlq_candidate());
}

#[test]
fn test_is_dlq_candidate_non_retryable_error() {
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
        "Test rollback",
    );

    // Failed with non-retryable error but still has budget
    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    ));

    assert!(action.is_dlq_candidate());
}

#[test]
fn test_is_dlq_candidate_not_failed() {
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
        "Test rollback",
    );

    // Not in Failed status
    assert!(!action.is_dlq_candidate());
}

#[test]
fn test_is_dlq_candidate_retryable_error_with_budget() {
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
        "Test rollback",
    );

    // Failed with retryable error and still has budget
    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    ));

    // Should NOT be DLQ candidate because retryable + budget remains
    assert!(!action.is_dlq_candidate());
}

#[test]
fn test_can_be_reapproved_success() {
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
        "Test rollback",
    );

    // Failed with retryable error and still has budget
    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    ));

    assert!(action.can_be_reapproved());
}

#[test]
fn test_can_be_reapproved_exhausted_budget() {
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
        "Test rollback",
    );

    // Exhausted budget
    action.status = CompensationStatus::Failed;
    action.attempt_count = 3;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Failed",
        "CONNECTION_TIMEOUT",
        None,
    ));

    assert!(!action.can_be_reapproved());
}

#[test]
fn test_can_be_reapproved_non_retryable_error() {
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
        "Test rollback",
    );

    // Non-retryable error
    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    ));

    assert!(!action.can_be_reapproved());
}

#[test]
fn test_can_be_reapproved_not_failed() {
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
        "Test rollback",
    );

    // Not in Failed status
    assert!(!action.can_be_reapproved());
}

#[test]
fn test_reapproval_denial_reason_budget_exhausted() {
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
        "Test rollback",
    );

    action.status = CompensationStatus::Failed;
    action.attempt_count = 3;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Failed",
        "CONNECTION_TIMEOUT",
        None,
    ));

    let reason = action.reapproval_denial_reason();
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("Retry budget exhausted"));
}

#[test]
fn test_reapproval_denial_reason_non_retryable_error() {
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
        "Test rollback",
    );

    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Permanent failure",
        "INVALID_CONFIGURATION",
        None,
    ));

    let reason = action.reapproval_denial_reason();
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("Non-retryable error"));
}

#[test]
fn test_reapproval_denial_reason_none_when_can_reapprove() {
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
        "Test rollback",
    );

    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    action.execution_result_payload = Some(ExecutionResult::failure(
        "Temporary failure",
        "CONNECTION_TIMEOUT",
        None,
    ));

    let reason = action.reapproval_denial_reason();
    assert!(reason.is_none());
    assert!(action.can_be_reapproved());
}

#[test]
fn test_reapproval_denial_reason_no_error_code() {
    // When there's no error code, reapproval should be allowed (assume transient)
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
        "Test rollback",
    );

    action.status = CompensationStatus::Failed;
    action.attempt_count = 1;
    action.max_retries = 3;
    // No execution_result_payload

    let reason = action.reapproval_denial_reason();
    assert!(reason.is_none());
    assert!(action.can_be_reapproved());
}

#[test]
fn test_failed_is_not_terminal() {
    // Phase 3 Batch 1: Failed is no longer terminal because manual retry exists
    assert!(!CompensationStatus::Failed.is_terminal());
    let validation = CompensationStatus::Failed.can_transition_to(CompensationStatus::Pending);
    assert!(validation.allowed);
}

#[test]
fn test_executed_and_waived_are_terminal() {
    assert!(CompensationStatus::Executed.is_terminal());
    assert!(CompensationStatus::Waived.is_terminal());
    // But Failed is not
    assert!(!CompensationStatus::Failed.is_terminal());
}

#[test]
fn test_max_retries_default() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let action = CompensationAction::new(
        tenant_id,
        Uuid::new_v4(),
        intent_id,
        RebaseContext::new(intent_id, 1, 2, Uuid::new_v4()),
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Test",
    );

    assert_eq!(action.max_retries, CompensationAction::DEFAULT_MAX_RETRIES);
    assert_eq!(action.max_retries, 3);
}
