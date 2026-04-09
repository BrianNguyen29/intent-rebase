//! Compensation action service facade for creating and querying compensation actions.
//!
//! Phase 3 Batch 1: Compensation action persistence service.
//! Provides a convenient API for recording and querying compensation actions
//! with proper tenant isolation.

use std::sync::Arc;
use uuid::Uuid;

use crate::compensation_action::{CompensationAction, CompensationStatus, ExecutionResult};
use crate::compensation_action_repo::CompensationActionRepository;
use intent_rebase_types::IntentRebaseError;

/// Service facade for compensation action operations.
///
/// Provides a convenient API for creating and querying compensation actions
/// with proper tenant isolation.
#[derive(Clone)]
pub struct CompensationActionService {
    repo: Arc<dyn CompensationActionRepository>,
}

impl CompensationActionService {
    /// Create a new CompensationActionService with the given repository.
    pub fn new(repo: Arc<dyn CompensationActionRepository>) -> Self {
        Self { repo }
    }

    /// Create a new compensation action.
    ///
    /// Returns the created action with its generated ID.
    pub async fn create_action(
        &self,
        action: CompensationAction,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo.create(action).await
    }

    /// Get a compensation action by its ID.
    pub async fn get_action(
        &self,
        action_id: Uuid,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo.get(action_id).await
    }

    /// List compensation actions for a given tenant.
    ///
    /// Returns up to `limit` actions (default 100), ordered by generated_at descending.
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_tenant(tenant_id, limit).await
    }

    /// List compensation actions for a given side effect.
    pub async fn list_by_side_effect(
        &self,
        side_effect_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_side_effect(side_effect_id, tenant_id)
            .await
    }

    /// List compensation actions for a given intent.
    ///
    /// Enables direct intent-scoped queries without joining through side_effects.
    pub async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_intent(intent_id, tenant_id).await
    }

    /// List compensation actions by status for a given tenant.
    pub async fn list_by_status(
        &self,
        tenant_id: Uuid,
        status: CompensationStatus,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo.list_by_status(tenant_id, status).await
    }

    /// Update the status of a compensation action.
    ///
    /// Uses optimistic locking via lock_version to prevent concurrent updates.
    pub async fn update_status(
        &self,
        action_id: Uuid,
        new_status: CompensationStatus,
        lock_version: i32,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo
            .update_status(action_id, new_status, lock_version)
            .await
    }

    /// Record the execution result of a compensation action.
    ///
    /// Updates status to Executed or Failed based on the result,
    /// and increments the attempt counter.
    pub async fn record_result(
        &self,
        action_id: Uuid,
        result: &ExecutionResult,
    ) -> Result<CompensationAction, IntentRebaseError> {
        self.repo.record_result(action_id, result).await
    }

    /// Get all pending compensation actions for a tenant.
    ///
    /// Useful for batch processing of pending compensations.
    pub async fn get_pending_actions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_status(tenant_id, CompensationStatus::Pending)
            .await
    }

    /// Get all failed compensation actions for a tenant (for retry review).
    pub async fn get_failed_actions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CompensationAction>, IntentRebaseError> {
        self.repo
            .list_by_status(tenant_id, CompensationStatus::Failed)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use crate::compensation_action_repo::InMemoryCompensationActionRepository;
    use std::sync::Arc;

    fn create_test_service() -> CompensationActionService {
        let repo = Arc::new(InMemoryCompensationActionRepository::new());
        CompensationActionService::new(repo)
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
        let updated = service.record_result(created.id, &result).await.unwrap();

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
        let updated = service.record_result(created.id, &result).await.unwrap();

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
}
