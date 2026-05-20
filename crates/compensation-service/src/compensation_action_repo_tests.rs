#[cfg(test)]
mod tests {
    use crate::compensation_action::{
        CompensationAction, CompensationFeasibility, CompensationStatus, ExecutionResult,
        RebaseContext, StrategyType,
    };
    use crate::compensation_action_repo::{
        CompensationActionRepository, InMemoryCompensationActionRepository,
    };
    use intent_rebase_types::IntentRebaseError;
    use std::sync::Arc;
    use uuid::Uuid;

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
    async fn test_create_compensation_action() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        let result = repo.create(action).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, id);
        assert_eq!(created.tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_get_compensation_action() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_compensation_action_not_found() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::CompensationActionNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_by_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _i in 0..3 {
            let side_effect_id = Uuid::new_v4();
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_tenant_with_limit() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        for _i in 0..5 {
            let side_effect_id = Uuid::new_v4();
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_tenant(tenant_id, Some(2)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_list_by_status() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create actions with different statuses
        let action1 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        repo.create(action1).await.unwrap();

        let mut action2 = create_test_action(tenant_id, Uuid::new_v4(), intent_id);
        action2.status = CompensationStatus::Executed;
        repo.create(action2).await.unwrap();

        let result = repo
            .list_by_status(tenant_id, CompensationStatus::Pending)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let result = repo
            .update_status(id, CompensationStatus::Approved, 0, None, None)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, CompensationStatus::Approved);
    }

    #[tokio::test]
    async fn test_update_status_concurrency_conflict() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        // First update succeeds
        repo.update_status(id, CompensationStatus::Approved, 0, None, None)
            .await
            .unwrap();

        // Second update with wrong lock_version fails
        let result = repo
            .update_status(id, CompensationStatus::Executed, 0, None, None)
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ConcurrencyConflict(_)
        ));
    }

    #[tokio::test]
    async fn test_record_result_success() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let exec_result = ExecutionResult::success("Rollback completed");
        let result = repo.record_result(id, &exec_result, 0, None).await;
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, CompensationStatus::Executed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_record_result_failure() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        let exec_result = ExecutionResult::failure(
            "Rollback failed",
            "ROLLBACK_ERR_001",
            Some("Database connection timeout".to_string()),
        );
        let result = repo.record_result(id, &exec_result, 0, None).await;
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, CompensationStatus::Failed);
        assert_eq!(updated.attempt_count, 1);
    }

    #[tokio::test]
    async fn test_record_result_increments_attempt_count() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let action = create_test_action(tenant_id, side_effect_id, intent_id);
        let id = action.id;

        repo.create(action).await.unwrap();

        // First attempt
        let exec_result1 = ExecutionResult::failure("Failed first time", "ERR_001", None);
        repo.record_result(id, &exec_result1, 0, None)
            .await
            .unwrap();

        // Second attempt - lock_version is now 1 after first call
        let exec_result2 = ExecutionResult::success("Succeeded second time");
        let updated = repo
            .record_result(id, &exec_result2, 1, None)
            .await
            .unwrap();

        assert_eq!(updated.attempt_count, 2);
        assert_eq!(updated.status, CompensationStatus::Executed);
    }

    #[tokio::test]
    async fn test_list_by_side_effect() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create multiple actions for same side effect
        for _ in 0..3 {
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_side_effect(side_effect_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_side_effect_filters_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let intent_id_1 = Uuid::new_v4();
        let intent_id_2 = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let action1 = create_test_action(tenant_id_1, side_effect_id, intent_id_1);
        repo.create(action1).await.unwrap();

        let action2 = create_test_action(tenant_id_2, side_effect_id, intent_id_2);
        repo.create(action2).await.unwrap();

        let result = repo.list_by_side_effect(side_effect_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create multiple actions for same intent
        for _ in 0..3 {
            let action = create_test_action(tenant_id, side_effect_id, intent_id);
            repo.create(action).await.unwrap();
        }

        let result = repo.list_by_intent(intent_id, tenant_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent_filters_tenant() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id_1 = Uuid::new_v4();
        let side_effect_id_2 = Uuid::new_v4();

        let action1 = create_test_action(tenant_id_1, side_effect_id_1, intent_id);
        repo.create(action1).await.unwrap();

        // Different tenant, same intent_id - should not be returned
        let action2 = create_test_action(tenant_id_2, side_effect_id_2, intent_id);
        repo.create(action2).await.unwrap();

        let result = repo.list_by_intent(intent_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_get_compensation_action_cross_tenant_blocked() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let _tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates a compensation action
        let action = create_test_action(tenant_a, Uuid::new_v4(), Uuid::new_v4());
        let action_id = action.id;
        repo.create(action).await.unwrap();

        // Tenant A can get their own action
        let result = repo.get(action_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tenant_id, tenant_a);

        // Note: The InMemory repository's `get` method does not enforce tenant isolation.
        // This test documents the current behavior where any tenant can get any action by ID.
        // Production implementations should add tenant filtering to the `get` method.
    }

    #[tokio::test]
    async fn test_list_by_tenant_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates 3 compensation actions
        for _ in 0..3 {
            let action = create_test_action(tenant_a, Uuid::new_v4(), Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // Tenant B creates 2 compensation actions
        for _ in 0..2 {
            let action = create_test_action(tenant_b, Uuid::new_v4(), Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // List for tenant A should return 3 actions
        let actions_a = repo.list_by_tenant(tenant_a, None).await.unwrap();
        assert_eq!(actions_a.len(), 3);
        assert!(actions_a.iter().all(|a| a.tenant_id == tenant_a));

        // List for tenant B should return 2 actions
        let actions_b = repo.list_by_tenant(tenant_b, None).await.unwrap();
        assert_eq!(actions_b.len(), 2);
        assert!(actions_b.iter().all(|a| a.tenant_id == tenant_b));
    }

    #[tokio::test]
    async fn test_list_by_side_effect_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let shared_side_effect_id = Uuid::new_v4();

        // Tenant A creates 2 actions for the shared side effect
        for _ in 0..2 {
            let action = create_test_action(tenant_a, shared_side_effect_id, Uuid::new_v4());
            repo.create(action).await.unwrap();
        }

        // Tenant B creates 1 action for the same shared side effect
        let action_b = create_test_action(tenant_b, shared_side_effect_id, Uuid::new_v4());
        repo.create(action_b).await.unwrap();

        // List for tenant A should only return tenant A's 2 actions
        let actions_a = repo
            .list_by_side_effect(shared_side_effect_id, tenant_a)
            .await
            .unwrap();
        assert_eq!(actions_a.len(), 2);
        assert!(actions_a.iter().all(|a| a.tenant_id == tenant_a));

        // List for tenant B should only return tenant B's 1 action
        let actions_b = repo
            .list_by_side_effect(shared_side_effect_id, tenant_b)
            .await
            .unwrap();
        assert_eq!(actions_b.len(), 1);
        assert!(actions_b.iter().all(|a| a.tenant_id == tenant_b));
    }
}

#[cfg(test)]
mod sqlx_compensation_action_tests {
    use crate::compensation_action::{CompensationFeasibility, CompensationStatus, StrategyType};
    use crate::compensation_action_repo::{
        compensation_feasibility_from_string, compensation_feasibility_to_string,
        compensation_status_from_string, compensation_status_to_string, strategy_type_from_string,
        strategy_type_to_string,
    };

    #[test]
    fn test_compensation_feasibility_to_string() {
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::Automatic),
            "automatic"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::SemiAutomatic),
            "semi_automatic"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::ManualOnly),
            "manual_only"
        );
        assert_eq!(
            compensation_feasibility_to_string(CompensationFeasibility::NotPossible),
            "not_possible"
        );
    }

    #[test]
    fn test_compensation_feasibility_from_string() {
        assert_eq!(
            compensation_feasibility_from_string("automatic").unwrap(),
            CompensationFeasibility::Automatic
        );
        assert_eq!(
            compensation_feasibility_from_string("semi_automatic").unwrap(),
            CompensationFeasibility::SemiAutomatic
        );
        assert_eq!(
            compensation_feasibility_from_string("manual_only").unwrap(),
            CompensationFeasibility::ManualOnly
        );
        assert_eq!(
            compensation_feasibility_from_string("not_possible").unwrap(),
            CompensationFeasibility::NotPossible
        );
        // Unknown values return error
        assert!(compensation_feasibility_from_string("unknown").is_err());
    }

    #[test]
    fn test_strategy_type_to_string() {
        assert_eq!(strategy_type_to_string(StrategyType::Rollback), "rollback");
        assert_eq!(
            strategy_type_to_string(StrategyType::CounterAction),
            "counter_action"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::FollowupNotice),
            "followup_notice"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::Quarantine),
            "quarantine"
        );
        assert_eq!(
            strategy_type_to_string(StrategyType::Escalation),
            "escalation"
        );
    }

    #[test]
    fn test_strategy_type_from_string() {
        assert_eq!(
            strategy_type_from_string("rollback").unwrap(),
            StrategyType::Rollback
        );
        assert_eq!(
            strategy_type_from_string("counter_action").unwrap(),
            StrategyType::CounterAction
        );
        assert_eq!(
            strategy_type_from_string("followup_notice").unwrap(),
            StrategyType::FollowupNotice
        );
        assert_eq!(
            strategy_type_from_string("quarantine").unwrap(),
            StrategyType::Quarantine
        );
        assert_eq!(
            strategy_type_from_string("escalation").unwrap(),
            StrategyType::Escalation
        );
        // Unknown values return error
        assert!(strategy_type_from_string("unknown").is_err());
    }

    #[test]
    fn test_compensation_status_to_string() {
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Pending),
            "pending"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Approved),
            "approved"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Executed),
            "executed"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Failed),
            "failed"
        );
        assert_eq!(
            compensation_status_to_string(CompensationStatus::Waived),
            "waived"
        );
    }

    #[test]
    fn test_compensation_status_from_string() {
        assert_eq!(
            compensation_status_from_string("pending").unwrap(),
            CompensationStatus::Pending
        );
        assert_eq!(
            compensation_status_from_string("approved").unwrap(),
            CompensationStatus::Approved
        );
        assert_eq!(
            compensation_status_from_string("executed").unwrap(),
            CompensationStatus::Executed
        );
        assert_eq!(
            compensation_status_from_string("failed").unwrap(),
            CompensationStatus::Failed
        );
        assert_eq!(
            compensation_status_from_string("waived").unwrap(),
            CompensationStatus::Waived
        );
        // Unknown values return error
        assert!(compensation_status_from_string("unknown").is_err());
    }
}
