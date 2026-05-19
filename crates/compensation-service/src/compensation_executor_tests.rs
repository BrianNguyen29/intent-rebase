use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, RebaseContext, StrategyType,
};
use crate::compensation_executor::CompensationExecutor;
use crate::side_effect::{SideEffect, SideEffectClass};
use crate::side_effect_repo::SideEffectRepository;
use crate::{
    CounterActionExecutor, EscalationExecutor, FollowupNoticeExecutor, RollbackExecutor,
    StubCompensationExecutor,
};
use std::sync::Arc;
use uuid::Uuid;

fn create_test_action(
    strategy_type: StrategyType,
    feasibility: CompensationFeasibility,
) -> CompensationAction {
    let intent_id = Uuid::new_v4();
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    CompensationAction::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        intent_id,
        rebase_context,
        feasibility,
        strategy_type,
        "Test compensation",
    )
}

fn create_test_side_effect(tenant_id: Uuid, intent_id: Uuid, side_effect_id: Uuid) -> SideEffect {
    SideEffect {
        id: side_effect_id, // Use the provided id so action.side_effect_id matches
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S1InternalReversible,
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    }
}

// === StubCompensationExecutor tests ===

#[tokio::test]
async fn test_stub_executor_always_succeeds() {
    let executor = StubCompensationExecutor::new();
    let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

    let result = executor.execute(&action).await.unwrap();

    assert!(result.success);
    assert!(result.error_code.is_none());
}

#[tokio::test]
async fn test_stub_executor_describes_action() {
    let executor = StubCompensationExecutor::new();
    let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

    let result = executor.execute(&action).await.unwrap();

    assert!(result.summary.contains("Rollback"));
}

// === RollbackExecutor tests ===

#[tokio::test]
async fn test_rollback_executor_success_rollback_automatic() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect.clone()).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Auto rollback internal metadata",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(
        result.success,
        "Expected success but got failure: {:?}",
        result
    );
    assert!(result.error_code.is_none());
    assert!(result.summary.contains("Rollback"));
    assert!(result.summary.contains("metadata_write"));
    assert!(result.summary.contains("db-record-123"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_non_rollback_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::CounterAction, // Not Rollback
        "Counter-action compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("CounterAction"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_semi_automatic_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic, // Not Automatic
        StrategyType::Rollback,
        "Semi-auto rollback",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("SemiAutomatic"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_manual_only_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly, // Not Automatic
        StrategyType::Rollback,
        "Manual rollback required",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("ManualOnly"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_not_possible_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible, // Not executable
        StrategyType::Rollback,
        "Cannot compensate",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("NotPossible"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_missing_side_effect() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // No side effects created in repo
    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Rollback missing side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("SIDE_EFFECT_NOT_FOUND".to_string()));
    assert!(result.summary.contains("not found"));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_tenant_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        different_tenant_id, // Different tenant than side effect
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Rollback with tenant mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("TENANT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_rollback_executor_fail_on_intent_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_intent_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(different_intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        different_intent_id, // Different intent than side effect
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Rollback with intent mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("INTENT_MISMATCH".to_string()));
}

// === Strategy type failure tests (all non-Rollback strategies) ===

#[tokio::test]
async fn test_rollback_executor_fail_on_counter_action_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::CounterAction,
        "Counter-action compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_rollback_executor_fail_on_followup_notice_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::FollowupNotice,
        "Followup notice compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_rollback_executor_fail_on_quarantine_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Quarantine,
        "Quarantine compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_rollback_executor_fail_on_escalation_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Escalation,
        "Escalation compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

// === Truthful summary tests ===

#[tokio::test]
async fn test_rollback_executor_summary_is_truthful() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_test_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = RollbackExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic,
        StrategyType::Rollback,
        "Auto rollback internal metadata",
    );

    let result = executor.execute(&action).await.unwrap();

    // Summary should contain "acknowledged" not "reversed" or "completed"
    assert!(result.summary.contains("acknowledged"));
    assert!(!result.summary.to_lowercase().contains("reversed"));
    // Should mention effect_type and target
    assert!(result.summary.contains("metadata_write"));
    assert!(result.summary.contains("db-record-123"));
}

// === CounterActionExecutor tests ===

fn create_s2_side_effect(tenant_id: Uuid, intent_id: Uuid, side_effect_id: Uuid) -> SideEffect {
    SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S2ExternalReversible,
        effect_type: "pr_opened".to_string(),
        target: "https://github.com/pulls/123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    }
}

#[tokio::test]
async fn test_counter_action_executor_success_counter_action_semi_auto() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect.clone()).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Close PR as counter-action",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(
        result.success,
        "Expected success but got failure: {:?}",
        result
    );
    assert!(result.error_code.is_none());
    assert!(result.summary.contains("Counter-action"));
    assert!(result.summary.contains("pr_opened"));
    assert!(result.summary.contains("https://github.com/pulls/123"));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_rollback_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Rollback, // Not CounterAction
        "Rollback compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("Rollback"));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_automatic_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic, // Not SemiAutomatic
        StrategyType::CounterAction,
        "Counter-action compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("Automatic"));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_manual_only_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly, // Not SemiAutomatic
        StrategyType::CounterAction,
        "Manual counter-action required",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("ManualOnly"));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_missing_side_effect() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // No side effects created in repo
    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter-action missing side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("SIDE_EFFECT_NOT_FOUND".to_string()));
    assert!(result.summary.contains("not found"));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_tenant_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        different_tenant_id, // Different tenant than side effect
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter-action with tenant mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("TENANT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_intent_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_intent_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(different_intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        different_intent_id, // Different intent than side effect
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter-action with intent mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("INTENT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_invalid_side_effect_class() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    // Create an S1InternalReversible side effect instead of S2ExternalReversible
    let side_effect = SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S1InternalReversible,
        effect_type: "metadata_write".to_string(),
        target: "db-record-123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Counter-action on S1 side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("INVALID_SIDE_EFFECT_CLASS".to_string())
    );
    assert!(result.summary.contains("S1InternalReversible"));
}

#[tokio::test]
async fn test_counter_action_executor_summary_is_truthful() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::CounterAction,
        "Close PR as counter-action",
    );

    let result = executor.execute(&action).await.unwrap();

    // Summary should contain "acknowledged" not "reversed" or "completed"
    assert!(result.summary.contains("acknowledged"));
    assert!(!result.summary.to_lowercase().contains("reversed"));
    // Should mention effect_type and target
    assert!(result.summary.contains("pr_opened"));
    assert!(result.summary.contains("https://github.com/pulls/123"));
}

// === Unsupported strategy/feasibility combo tests ===

#[tokio::test]
async fn test_counter_action_executor_fail_on_followup_notice_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::FollowupNotice, // Not CounterAction
        "Followup notice compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_quarantine_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Quarantine, // Not CounterAction
        "Quarantine compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_escalation_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::SemiAutomatic,
        StrategyType::Escalation, // Not CounterAction
        "Escalation compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
}

#[tokio::test]
async fn test_counter_action_executor_fail_on_not_possible_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s2_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = CounterActionExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible, // Not executable
        StrategyType::CounterAction,
        "Cannot counter-act",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("NotPossible"));
}

// === FollowupNoticeExecutor tests ===

fn create_s3_side_effect(tenant_id: Uuid, intent_id: Uuid, side_effect_id: Uuid) -> SideEffect {
    SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S3ExternalPartiallyReversible,
        effect_type: "email_sent".to_string(),
        target: "user@example.com".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    }
}

#[tokio::test]
async fn test_followup_notice_executor_success_followup_notice_manual_only() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect.clone()).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Followup notice for partially reversible effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(
        result.success,
        "Expected success but got failure: {:?}",
        result
    );
    assert!(result.error_code.is_none());
    assert!(result.summary.contains("FollowupNotice"));
    assert!(result.summary.contains("email_sent"));
    assert!(result.summary.contains("user@example.com"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_rollback_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Rollback, // Not FollowupNotice
        "Rollback compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("Rollback"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_automatic_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic, // Not ManualOnly
        StrategyType::FollowupNotice,
        "Followup notice compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("Automatic"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_not_possible_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible, // Not ManualOnly
        StrategyType::FollowupNotice,
        "Followup notice compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("NotPossible"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_missing_side_effect() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // No side effects created in repo
    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Followup notice missing side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("SIDE_EFFECT_NOT_FOUND".to_string()));
    assert!(result.summary.contains("not found"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_tenant_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        different_tenant_id, // Different tenant than side effect
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Followup notice with tenant mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("TENANT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_intent_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_intent_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(different_intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        different_intent_id, // Different intent than side effect
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Followup notice with intent mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("INTENT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_invalid_side_effect_class() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    // Create an S2ExternalReversible side effect instead of S3ExternalPartiallyReversible
    let side_effect = SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S2ExternalReversible,
        effect_type: "pr_opened".to_string(),
        target: "https://github.com/pulls/123".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Followup notice on S2 side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("INVALID_SIDE_EFFECT_CLASS".to_string())
    );
    assert!(result.summary.contains("S2ExternalReversible"));
}

#[tokio::test]
async fn test_followup_notice_executor_summary_is_truthful() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::FollowupNotice,
        "Send followup notice for email",
    );

    let result = executor.execute(&action).await.unwrap();

    // Summary should contain "acknowledged" not "resolved" or "completed"
    assert!(result.summary.contains("acknowledged"));
    assert!(!result.summary.to_lowercase().contains("resolved"));
    // Should mention effect_type and target
    assert!(result.summary.contains("email_sent"));
    assert!(result.summary.contains("user@example.com"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_counter_action_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::CounterAction, // Not FollowupNotice
        "Counter-action compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("CounterAction"));
}

#[tokio::test]
async fn test_followup_notice_executor_fail_on_escalation_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s3_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = FollowupNoticeExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly,
        StrategyType::Escalation, // Not FollowupNotice
        "Escalation compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("Escalation"));
}

// === EscalationExecutor tests ===

fn create_s4_side_effect(tenant_id: Uuid, intent_id: Uuid, side_effect_id: Uuid) -> SideEffect {
    SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S4Irreversible,
        effect_type: "money_transfer".to_string(),
        target: "account-xyz-amount-1000".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    }
}

#[tokio::test]
async fn test_escalation_executor_success_escalation_not_possible() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect.clone()).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalation for irreversible effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(
        result.success,
        "Expected success but got failure: {:?}",
        result
    );
    assert!(result.error_code.is_none());
    assert!(result.summary.contains("Escalation"));
    assert!(result.summary.contains("money_transfer"));
    assert!(result.summary.contains("account-xyz-amount-1000"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_rollback_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Rollback, // Not Escalation
        "Rollback compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("Rollback"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_manual_only_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::ManualOnly, // Not NotPossible
        StrategyType::Escalation,
        "Escalation compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("ManualOnly"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_automatic_feasibility() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::Automatic, // Not NotPossible
        StrategyType::Escalation,
        "Escalation compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_FEASIBILITY".to_string())
    );
    assert!(result.summary.contains("Automatic"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_missing_side_effect() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // No side effects created in repo
    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalation missing side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("SIDE_EFFECT_NOT_FOUND".to_string()));
    assert!(result.summary.contains("not found"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_tenant_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        different_tenant_id, // Different tenant than side effect
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalation with tenant mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("TENANT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_intent_mismatch() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let different_intent_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(different_intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        different_intent_id, // Different intent than side effect
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalation with intent mismatch",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.error_code, Some("INTENT_MISMATCH".to_string()));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_invalid_side_effect_class() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    // Create an S3ExternalPartiallyReversible side effect instead of S4Irreversible
    let side_effect = SideEffect {
        id: side_effect_id,
        tenant_id,
        intent_id,
        intent_version: 1,
        effect_class: SideEffectClass::S3ExternalPartiallyReversible,
        effect_type: "email_sent".to_string(),
        target: "user@example.com".to_string(),
        occurred_at: chrono::Utc::now(),
        idempotency_key: None,
    };
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalation on S3 side effect",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("INVALID_SIDE_EFFECT_CLASS".to_string())
    );
    assert!(result.summary.contains("S3ExternalPartiallyReversible"));
}

#[tokio::test]
async fn test_escalation_executor_summary_is_truthful() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::Escalation,
        "Escalate money transfer issue",
    );

    let result = executor.execute(&action).await.unwrap();

    // Summary should contain "acknowledged" not "resolved" or "completed"
    assert!(result.summary.contains("acknowledged"));
    assert!(!result.summary.to_lowercase().contains("resolved"));
    // Should mention effect_type and target
    assert!(result.summary.contains("money_transfer"));
    assert!(result.summary.contains("account-xyz-amount-1000"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_followup_notice_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::FollowupNotice, // Not Escalation
        "Followup notice compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("FollowupNotice"));
}

#[tokio::test]
async fn test_escalation_executor_fail_on_counter_action_strategy() {
    let side_effect_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let repo = Arc::new(crate::side_effect_repo::InMemorySideEffectRepository::new());
    let side_effect = create_s4_side_effect(tenant_id, intent_id, side_effect_id);
    repo.create(side_effect).await.unwrap();

    let executor = EscalationExecutor::new(repo);
    let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
    let action = CompensationAction::new(
        tenant_id,
        side_effect_id,
        intent_id,
        rebase_context,
        CompensationFeasibility::NotPossible,
        StrategyType::CounterAction, // Not Escalation
        "Counter-action compensation",
    );

    let result = executor.execute(&action).await.unwrap();

    assert!(!result.success);
    assert_eq!(
        result.error_code,
        Some("UNSUPPORTED_STRATEGY_TYPE".to_string())
    );
    assert!(result.summary.contains("CounterAction"));
}
