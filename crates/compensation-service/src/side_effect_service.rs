//! Side effect service facade for recording and querying side effects.
//!
//! Phase 3 Batch 1 (groundwork): This service provides a facade over the
//! SideEffectRepository, enabling capture-on-write from artifact-producing
//! operations and query-by-intent for compensation planning.
//!
//! **Bounded scope (this slice):**
//! - Record side effects with tenant/intent/version context
//! - List side effects by intent and tenant
//! - Idempotency key support for duplicate prevention
//!
//! **Not included in this slice:**
//! - Compensation planner/executor/retry (Batch 1+)
//! - Rollback record schema/work (Batch 1+)
//! - Full capture across every artifact-producing operation (requires artifact-service integration)

use std::sync::Arc;
use uuid::Uuid;

use crate::side_effect::{SideEffect, SideEffectClass};
use crate::side_effect_repo::SideEffectRepository;
use intent_rebase_types::IntentRebaseError;

/// Service facade for side effect operations.
///
/// Provides a convenient API for recording and querying side effects
/// with proper tenant isolation and idempotency support.
#[derive(Clone)]
pub struct SideEffectService {
    repo: Arc<dyn SideEffectRepository>,
}

impl SideEffectService {
    /// Create a new SideEffectService with the given repository.
    pub fn new(repo: Arc<dyn SideEffectRepository>) -> Self {
        Self { repo }
    }

    /// Record a new side effect from an artifact-producing operation.
    ///
    /// Returns the recorded side effect with its generated ID.
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant scope for the side effect
    /// * `intent_id` - Intent that produced the artifact
    /// * `intent_version` - Version of the intent at time of effect emission
    /// * `effect_class` - Severity class (S0-S4)
    /// * `effect_type` - Type identifier (e.g., "email_sent", "pr_opened")
    /// * `target` - Target of the effect (e.g., email address, PR URL)
    pub async fn record_side_effect(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        effect_class: SideEffectClass,
        effect_type: &str,
        target: &str,
    ) -> Result<SideEffect, intent_rebase_types::IntentRebaseError> {
        let side_effect = SideEffect::new(
            tenant_id,
            intent_id,
            intent_version,
            effect_class,
            effect_type,
            target,
        );
        self.repo.create(side_effect).await
    }

    /// Record a new side effect with an idempotency key.
    ///
    /// If a side effect with the same idempotency key already exists for this tenant,
    /// returns the existing side effect instead of creating a duplicate.
    ///
    /// This enables safe retry of artifact-producing operations without
    /// creating duplicate side effect records.
    ///
    /// # Implementation Note
    /// Uses atomic get-or-create under the hood to avoid TOCTOU races between
    /// checking for existing entries and creating new ones.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_side_effect_with_idempotency(
        &self,
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        effect_class: SideEffectClass,
        effect_type: &str,
        target: &str,
        idempotency_key: &str,
    ) -> Result<SideEffect, IntentRebaseError> {
        let side_effect = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            intent_version,
            effect_class,
            effect_type,
            target,
            idempotency_key,
        );
        // Atomically get or create - avoids TOCTOU race of check-then-create
        self.repo.get_or_create_idempotent(side_effect).await
    }

    /// List all side effects for a given intent, ordered by occurred_at descending.
    ///
    /// Returns side effects scoped to the specified tenant, newest first.
    pub async fn list_side_effects_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffect>, intent_rebase_types::IntentRebaseError> {
        self.repo.list_by_intent(intent_id, tenant_id).await
    }

    /// Get a side effect by its ID.
    pub async fn get_side_effect(
        &self,
        side_effect_id: Uuid,
    ) -> Result<SideEffect, intent_rebase_types::IntentRebaseError> {
        self.repo.get(side_effect_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::side_effect_repo::InMemorySideEffectRepository;
    use std::sync::Arc;

    fn create_test_service() -> SideEffectService {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        SideEffectService::new(repo)
    }

    #[tokio::test]
    async fn test_record_side_effect() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let effect = service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "pr_opened",
                "https://github.com/example/pull/123",
            )
            .await
            .unwrap();

        assert_eq!(effect.tenant_id, tenant_id);
        assert_eq!(effect.intent_id, intent_id);
        assert_eq!(effect.intent_version, 1);
        assert_eq!(effect.effect_type, "pr_opened");
    }

    #[tokio::test]
    async fn test_list_side_effects_by_intent() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Record multiple side effects
        for i in 0..3 {
            service
                .record_side_effect(
                    tenant_id,
                    intent_id,
                    1,
                    SideEffectClass::S2ExternalReversible,
                    &format!("effect_type_{}", i),
                    "target",
                )
                .await
                .unwrap();
        }

        let effects = service
            .list_side_effects_by_intent(intent_id, tenant_id)
            .await
            .unwrap();

        assert_eq!(effects.len(), 3);
        // Should be sorted by occurred_at descending (newest first)
        assert!(effects
            .windows(2)
            .all(|w| w[0].occurred_at >= w[1].occurred_at));
    }

    #[tokio::test]
    async fn test_list_side_effects_filters_tenant() {
        let service = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Record side effect for tenant 1
        service
            .record_side_effect(
                tenant_id_1,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "effect_1",
                "target",
            )
            .await
            .unwrap();

        // Record side effect for tenant 2
        service
            .record_side_effect(
                tenant_id_2,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "effect_2",
                "target",
            )
            .await
            .unwrap();

        // Query for tenant 1 should only return tenant 1's side effect
        let effects_1 = service
            .list_side_effects_by_intent(intent_id, tenant_id_1)
            .await
            .unwrap();
        assert_eq!(effects_1.len(), 1);
        assert_eq!(effects_1[0].effect_type, "effect_1");

        // Query for tenant 2 should only return tenant 2's side effect
        let effects_2 = service
            .list_side_effects_by_intent(intent_id, tenant_id_2)
            .await
            .unwrap();
        assert_eq!(effects_2.len(), 1);
        assert_eq!(effects_2[0].effect_type, "effect_2");
    }

    #[tokio::test]
    async fn test_record_with_idempotency_key_duplicate() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let idempotency_key = "test-idempotency-key-123";

        // First call creates
        let effect_1 = service
            .record_side_effect_with_idempotency(
                tenant_id,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "payment_initiated",
                "txn-12345",
                idempotency_key,
            )
            .await
            .unwrap();

        // Second call with same key returns existing
        let effect_2 = service
            .record_side_effect_with_idempotency(
                tenant_id,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "payment_initiated",
                "txn-12345",
                idempotency_key,
            )
            .await
            .unwrap();

        assert_eq!(effect_1.id, effect_2.id);
    }

    #[tokio::test]
    async fn test_get_side_effect() {
        let service = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let recorded = service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                "pr_opened",
                "https://github.com/example/pull/123",
            )
            .await
            .unwrap();

        let retrieved = service.get_side_effect(recorded.id).await.unwrap();
        assert_eq!(retrieved.id, recorded.id);
    }

    #[tokio::test]
    async fn test_get_side_effect_not_found() {
        let service = create_test_service();
        let result = service.get_side_effect(Uuid::new_v4()).await;
        assert!(result.is_err());
    }
}
