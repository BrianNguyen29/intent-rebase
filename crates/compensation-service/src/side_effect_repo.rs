//! Side effect repository trait and implementations
//!
//! Phase 3 Batch 1: Side effect ledger storage.
//! Repository trait allows for in-memory (tests) or SQL-backed implementations.

use async_trait::async_trait;
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::side_effect::{SideEffect, SideEffectClass};

/// Repository trait for side effect storage
/// Allows for in-memory (tests) or SQL-backed implementations
#[async_trait]
pub trait SideEffectRepository: Send + Sync {
    /// Create a new side effect record
    async fn create(&self, side_effect: SideEffect) -> Result<SideEffect, IntentRebaseError>;

    /// Get a side effect by its ID
    async fn get(&self, side_effect_id: Uuid) -> Result<SideEffect, IntentRebaseError>;

    /// List side effects for a given intent, ordered by occurred_at descending
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffect>, IntentRebaseError>;

    /// Find a side effect by idempotency key
    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        tenant_id: Uuid,
    ) -> Result<Option<SideEffect>, IntentRebaseError>;

    /// Atomically get or create a side effect with idempotency key.
    ///
    /// If a side effect with the given idempotency key and tenant already exists,
    /// returns the existing one. Otherwise creates and returns the new one.
    ///
    /// This is the idiomatic path for idempotent side effect recording - use this
    /// instead of check-then-create patterns which have TOCTOU races.
    async fn get_or_create_idempotent(
        &self,
        side_effect: SideEffect,
    ) -> Result<SideEffect, IntentRebaseError>;
}

// =============================================================================
// In-memory implementation
// =============================================================================

/// In-memory implementation for testing and Phase 3 Batch 1
pub struct InMemorySideEffectRepository {
    side_effects: RwLock<HashMap<Uuid, SideEffect>>,
    /// Secondary index: intent_id -> list of side_effect_ids
    by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    /// Secondary index: (tenant_id, idempotency_key) -> side_effect_id
    by_idempotency_key: RwLock<HashMap<(Uuid, String), Uuid>>,
}

impl InMemorySideEffectRepository {
    pub fn new() -> Self {
        Self {
            side_effects: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
            by_idempotency_key: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySideEffectRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SideEffectRepository for InMemorySideEffectRepository {
    async fn create(&self, side_effect: SideEffect) -> Result<SideEffect, IntentRebaseError> {
        let mut side_effects = self.side_effects.write().await;
        let mut by_intent = self.by_intent.write().await;
        let mut by_idempotency_key = self.by_idempotency_key.write().await;

        // Store side effect
        side_effects.insert(side_effect.id, side_effect.clone());

        // Index by intent
        by_intent
            .entry(side_effect.intent_id)
            .or_insert_with(Vec::new)
            .push(side_effect.id);

        // Index by idempotency key if present
        if let Some(ref key) = side_effect.idempotency_key {
            by_idempotency_key.insert((side_effect.tenant_id, key.clone()), side_effect.id);
        }

        Ok(side_effect)
    }

    async fn get_or_create_idempotent(
        &self,
        side_effect: SideEffect,
    ) -> Result<SideEffect, IntentRebaseError> {
        // Atomically check and create under a single lock to avoid TOCTOU race.
        // If an idempotency key is present, check for existing entry first.
        // Otherwise just create a new entry.
        let mut side_effects = self.side_effects.write().await;
        let mut by_intent = self.by_intent.write().await;
        let mut by_idempotency_key = self.by_idempotency_key.write().await;

        // Check for existing entry with same idempotency key if present
        if let Some(ref key) = side_effect.idempotency_key {
            if let Some(&existing_id) =
                by_idempotency_key.get(&(side_effect.tenant_id, key.clone()))
            {
                if let Some(existing) = side_effects.get(&existing_id) {
                    return Ok(existing.clone());
                }
            }
        }

        // No existing entry found - create new one
        let id = side_effect.id;
        side_effects.insert(id, side_effect.clone());

        // Index by intent
        by_intent
            .entry(side_effect.intent_id)
            .or_insert_with(Vec::new)
            .push(id);

        // Index by idempotency key if present
        if let Some(ref key) = side_effect.idempotency_key {
            by_idempotency_key.insert((side_effect.tenant_id, key.clone()), id);
        }

        Ok(side_effect)
    }

    async fn get(&self, side_effect_id: Uuid) -> Result<SideEffect, IntentRebaseError> {
        let side_effects = self.side_effects.read().await;
        side_effects.get(&side_effect_id).cloned().ok_or_else(|| {
            IntentRebaseError::Internal(format!("side effect not found: {}", side_effect_id))
        })
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffect>, IntentRebaseError> {
        let side_effects = self.side_effects.read().await;
        let by_intent = self.by_intent.read().await;

        let side_effect_ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<SideEffect> = side_effect_ids
            .iter()
            .filter_map(|id| side_effects.get(id).cloned())
            .filter(|se| se.tenant_id == tenant_id)
            .collect();

        // Sort by occurred_at descending (newest first)
        result.sort_by_key(|b| std::cmp::Reverse(b.occurred_at));

        Ok(result)
    }

    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        tenant_id: Uuid,
    ) -> Result<Option<SideEffect>, IntentRebaseError> {
        let side_effects = self.side_effects.read().await;
        let by_idempotency_key = self.by_idempotency_key.read().await;

        let side_effect_id = by_idempotency_key
            .get(&(tenant_id, idempotency_key.to_string()))
            .cloned();

        match side_effect_id {
            Some(id) => Ok(side_effects.get(&id).cloned()),
            None => Ok(None),
        }
    }
}

// =============================================================================
// SQLx-backed Side Effect Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

/// SQL-backed repository for side effect storage using PostgreSQL.
pub struct SqlxSideEffectRepository {
    pool: PgPool,
}

impl SqlxSideEffectRepository {
    /// Create a new SqlxSideEffectRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to a SideEffect domain object
    fn row_to_side_effect(&self, row: PgRow) -> Result<SideEffect, IntentRebaseError> {
        let effect_class_str: String = row.get("effect_class");

        Ok(SideEffect {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            intent_id: row.get("intent_id"),
            intent_version: row.get("intent_version"),
            effect_class: effect_class_from_string(&effect_class_str)?,
            effect_type: row.get("effect_type"),
            target: row.get("target"),
            occurred_at: row.get("occurred_at"),
            idempotency_key: row.get("idempotency_key"),
        })
    }

    /// Insert a new side effect into the database
    async fn insert_side_effect(&self, side_effect: &SideEffect) -> Result<(), IntentRebaseError> {
        let effect_class_str = effect_class_to_string(side_effect.effect_class);

        sqlx::query(
            r#"
            INSERT INTO side_effects (
                id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(side_effect.id)
        .bind(side_effect.tenant_id)
        .bind(side_effect.intent_id)
        .bind(side_effect.intent_version)
        .bind(effect_class_str)
        .bind(&side_effect.effect_type)
        .bind(&side_effect.target)
        .bind(side_effect.occurred_at)
        .bind(&side_effect.idempotency_key)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert side effect: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl SideEffectRepository for SqlxSideEffectRepository {
    async fn create(&self, side_effect: SideEffect) -> Result<SideEffect, IntentRebaseError> {
        self.insert_side_effect(&side_effect).await?;
        Ok(side_effect)
    }

    async fn get_or_create_idempotent(
        &self,
        side_effect: SideEffect,
    ) -> Result<SideEffect, IntentRebaseError> {
        // Use INSERT ... ON CONFLICT to atomically handle idempotency.
        // If a row with the same (tenant_id, idempotency_key) already exists,
        // return the existing row instead of creating a duplicate.
        //
        // Note: This requires a UNIQUE constraint on (tenant_id, idempotency_key)
        // in the database schema, with a partial index for non-NULL idempotency_key.
        let effect_class_str = effect_class_to_string(side_effect.effect_class);

        let row = sqlx::query(
            r#"
            INSERT INTO side_effects (
                id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (tenant_id, idempotency_key)
            WHERE idempotency_key IS NOT NULL DO UPDATE SET
                id = EXCLUDED.id
            RETURNING id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            "#,
        )
        .bind(side_effect.id)
        .bind(side_effect.tenant_id)
        .bind(side_effect.intent_id)
        .bind(side_effect.intent_version)
        .bind(effect_class_str)
        .bind(&side_effect.effect_type)
        .bind(&side_effect.target)
        .bind(side_effect.occurred_at)
        .bind(&side_effect.idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("upsert side effect: {}", e)))?;

        match row {
            Some(r) => self.row_to_side_effect(r),
            None => {
                // This shouldn't happen with ON CONFLICT... but just in case, return the input
                Ok(side_effect)
            }
        }
    }

    async fn get(&self, side_effect_id: Uuid) -> Result<SideEffect, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            FROM side_effects
            WHERE id = $1
            "#,
        )
        .bind(side_effect_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch side effect: {}", e)))?;

        match row {
            Some(r) => self.row_to_side_effect(r),
            None => Err(IntentRebaseError::Internal(format!(
                "side effect not found: {}",
                side_effect_id
            ))),
        }
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<SideEffect>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            FROM side_effects
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY occurred_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list side effects by intent: {}", e))
        })?;

        rows.into_iter()
            .map(|r| self.row_to_side_effect(r))
            .collect()
    }

    async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        tenant_id: Uuid,
    ) -> Result<Option<SideEffect>, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, intent_id, intent_version, effect_class,
                effect_type, target, occurred_at, idempotency_key
            FROM side_effects
            WHERE idempotency_key = $1 AND tenant_id = $2
            "#,
        )
        .bind(idempotency_key)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("find side effect by idempotency key: {}", e))
        })?;

        match row {
            Some(r) => self.row_to_side_effect(r).map(Some),
            None => Ok(None),
        }
    }
}

// =============================================================================
// Helper functions for side effect enum conversion
// =============================================================================

fn effect_class_to_string(effect_class: SideEffectClass) -> &'static str {
    match effect_class {
        SideEffectClass::S0PureRead => "s0_pure_read",
        SideEffectClass::S1InternalReversible => "s1_internal_reversible",
        SideEffectClass::S2ExternalReversible => "s2_external_reversible",
        SideEffectClass::S3ExternalPartiallyReversible => "s3_external_partially_reversible",
        SideEffectClass::S4Irreversible => "s4_irreversible",
    }
}

/// Convert a string to SideEffectClass.
///
/// Returns an error if the string does not match any known effect class.
/// This ensures we fail loudly on data corruption rather than silently
/// defaulting to S0PureRead which could mask issues.
pub fn effect_class_from_string(s: &str) -> Result<SideEffectClass, IntentRebaseError> {
    match s {
        "s0_pure_read" => Ok(SideEffectClass::S0PureRead),
        "s1_internal_reversible" => Ok(SideEffectClass::S1InternalReversible),
        "s2_external_reversible" => Ok(SideEffectClass::S2ExternalReversible),
        "s3_external_partially_reversible" => Ok(SideEffectClass::S3ExternalPartiallyReversible),
        "s4_irreversible" => Ok(SideEffectClass::S4Irreversible),
        other => Err(IntentRebaseError::UnknownEffectClass(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_side_effect(tenant_id: Uuid, intent_id: Uuid, effect_type: &str) -> SideEffect {
        SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            effect_type,
            "https://example.com/target",
        )
    }

    #[tokio::test]
    async fn test_create_side_effect() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let side_effect = create_test_side_effect(tenant_id, intent_id, "pr_opened");
        let id = side_effect.id;

        let result = repo.create(side_effect.clone()).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, id);
        assert_eq!(created.tenant_id, tenant_id);
        assert_eq!(created.intent_id, intent_id);
    }

    #[tokio::test]
    async fn test_get_side_effect() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let side_effect = create_test_side_effect(tenant_id, intent_id, "email_sent");
        let id = side_effect.id;

        repo.create(side_effect).await.unwrap();

        let result = repo.get(id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_side_effect_not_found() {
        let repo = Arc::new(InMemorySideEffectRepository::new());

        let result = repo.get(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_by_intent() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create multiple side effects for the same intent
        for i in 0..3 {
            let side_effect = SideEffect::new(
                tenant_id,
                intent_id,
                1,
                SideEffectClass::S2ExternalReversible,
                &format!("effect_type_{}", i),
                "target",
            );
            repo.create(side_effect).await.unwrap();
        }

        let result = repo.list_by_intent(intent_id, tenant_id).await;
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.len(), 3);
        // Should be sorted by occurred_at descending
        assert!(list
            .windows(2)
            .all(|w| w[0].occurred_at >= w[1].occurred_at));
    }

    #[tokio::test]
    async fn test_list_by_intent_empty() {
        let repo = Arc::new(InMemorySideEffectRepository::new());

        let result = repo.list_by_intent(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_by_intent_filters_tenant() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Create side effect for tenant 1
        let side_effect1 = create_test_side_effect(tenant_id_1, intent_id, "effect_1");
        repo.create(side_effect1).await.unwrap();

        // Create side effect for tenant 2
        let side_effect2 = create_test_side_effect(tenant_id_2, intent_id, "effect_2");
        repo.create(side_effect2).await.unwrap();

        // Query for tenant 1 should only return tenant 1's side effect
        let result = repo.list_by_intent(intent_id, tenant_id_1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);

        // Query for tenant 2 should only return tenant 2's side effect
        let result = repo.list_by_intent(intent_id, tenant_id_2).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_find_by_idempotency_key() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let side_effect = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-idempotent-123",
        );

        repo.create(side_effect.clone()).await.unwrap();

        let result = repo
            .find_by_idempotency_key("payment-idempotent-123", tenant_id)
            .await;
        assert!(result.is_ok());
        let found = result.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, side_effect.id);
    }

    #[tokio::test]
    async fn test_find_by_idempotency_key_not_found() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();

        let result = repo
            .find_by_idempotency_key("nonexistent-key", tenant_id)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_find_by_idempotency_key_filters_tenant() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Create side effect for tenant 1 with idempotency key
        let side_effect = SideEffect::with_idempotency_key(
            tenant_id_1,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-idempotent-123",
        );
        repo.create(side_effect).await.unwrap();

        // Tenant 2 querying with same idempotency key should not find it
        let result = repo
            .find_by_idempotency_key("payment-idempotent-123", tenant_id_2)
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_or_create_idempotent_creates_new_when_not_exists() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let side_effect = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-idempotent-new",
        );

        let result = repo.get_or_create_idempotent(side_effect.clone()).await;
        assert!(result.is_ok());
        let created = result.unwrap();
        assert_eq!(created.id, side_effect.id);
        assert_eq!(created.tenant_id, tenant_id);
        assert_eq!(
            created.idempotency_key,
            Some("payment-idempotent-new".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_or_create_idempotent_returns_existing_when_exists() {
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // First call creates
        let side_effect_1 = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-idempotent-existing",
        );
        let created_1 = repo.get_or_create_idempotent(side_effect_1).await.unwrap();

        // Second call with same key returns existing
        let side_effect_2 = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-idempotent-existing",
        );
        let created_2 = repo.get_or_create_idempotent(side_effect_2).await.unwrap();

        // Should return the same record
        assert_eq!(created_1.id, created_2.id);
        assert_eq!(created_1.intent_id, created_2.intent_id);
    }

    #[tokio::test]
    async fn test_get_or_create_idempotent_concurrent_race() {
        // This test verifies that concurrent get-or-create calls with the same
        // idempotency key do not create duplicates (TOCTOU race condition fix).
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let idempotency_key = "concurrent-race-key";

        // Spawn multiple concurrent calls
        let repo_clone = repo.clone();
        let tenant_id_clone = tenant_id;
        let intent_id_clone = intent_id;

        let handle = tokio::spawn(async move {
            let side_effect = SideEffect::with_idempotency_key(
                tenant_id_clone,
                intent_id_clone,
                1,
                SideEffectClass::S2ExternalReversible,
                "concurrent_effect",
                "target-xyz",
                idempotency_key,
            );
            repo_clone.get_or_create_idempotent(side_effect).await
        });

        // Also call directly on main task
        let side_effect_main = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "concurrent_effect",
            "target-xyz",
            idempotency_key,
        );
        let result_main = repo
            .get_or_create_idempotent(side_effect_main)
            .await
            .unwrap();
        let result_handle = handle.await.unwrap().unwrap();

        // Both should return the same ID - no duplicates created
        assert_eq!(result_main.id, result_handle.id);

        // Verify only one entry exists in the repository
        let all_effects = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(all_effects.len(), 1);
    }

    #[tokio::test]
    async fn test_get_or_create_idempotent_different_tenants_same_key() {
        // Same idempotency key but different tenants should create separate entries
        let repo = Arc::new(InMemorySideEffectRepository::new());
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();
        let idempotency_key = "same-key-different-tenant";

        let side_effect_1 = SideEffect::with_idempotency_key(
            tenant_id_1,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "effect_1",
            "target-1",
            idempotency_key,
        );
        let side_effect_2 = SideEffect::with_idempotency_key(
            tenant_id_2,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "effect_2",
            "target-2",
            idempotency_key,
        );

        let result_1 = repo.get_or_create_idempotent(side_effect_1).await.unwrap();
        let result_2 = repo.get_or_create_idempotent(side_effect_2).await.unwrap();

        // Different tenants should get different records even with same idempotency key
        assert_ne!(result_1.id, result_2.id);
    }
}

// =============================================================================
// SqlxSideEffectRepository unit tests (helper function tests)
// =============================================================================

#[cfg(test)]
mod sqlx_side_effect_tests {
    use super::*;

    #[test]
    fn test_effect_class_to_string() {
        assert_eq!(
            effect_class_to_string(SideEffectClass::S0PureRead),
            "s0_pure_read"
        );
        assert_eq!(
            effect_class_to_string(SideEffectClass::S1InternalReversible),
            "s1_internal_reversible"
        );
        assert_eq!(
            effect_class_to_string(SideEffectClass::S2ExternalReversible),
            "s2_external_reversible"
        );
        assert_eq!(
            effect_class_to_string(SideEffectClass::S3ExternalPartiallyReversible),
            "s3_external_partially_reversible"
        );
        assert_eq!(
            effect_class_to_string(SideEffectClass::S4Irreversible),
            "s4_irreversible"
        );
    }

    #[test]
    fn test_effect_class_from_string() {
        assert_eq!(
            effect_class_from_string("s0_pure_read").unwrap(),
            SideEffectClass::S0PureRead
        );
        assert_eq!(
            effect_class_from_string("s1_internal_reversible").unwrap(),
            SideEffectClass::S1InternalReversible
        );
        assert_eq!(
            effect_class_from_string("s2_external_reversible").unwrap(),
            SideEffectClass::S2ExternalReversible
        );
        assert_eq!(
            effect_class_from_string("s3_external_partially_reversible").unwrap(),
            SideEffectClass::S3ExternalPartiallyReversible
        );
        assert_eq!(
            effect_class_from_string("s4_irreversible").unwrap(),
            SideEffectClass::S4Irreversible
        );
        // Unknown values return an error (not silent default to S0)
        let result = effect_class_from_string("unknown");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::UnknownEffectClass(s) if s == "unknown"));
    }
}
