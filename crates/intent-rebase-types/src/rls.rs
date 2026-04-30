//! RLS (Row-Level Security) SQL helpers and types for tenant-scoped SQL transactions.
//!
//! This module provides shared RLS helpers that can be used across crates to:
//! - Generate SQL for setting/resetting tenant context in PostgreSQL sessions
//! - Validate tenant IDs are safe for RLS use
//! - Manage transaction-scoped RLS context
//!
//! # Security Notes
//!
//! - Uses parameterized UUID to prevent SQL injection
//! - The UUID is validated before being embedded in the SQL
//! - RLS policies check `NULL` tenant_id as bypass (superuser/migration access)
//! - Always use `SET LOCAL` for transaction-scoped context

/// PostgreSQL session setting name for tenant context
pub const RLS_TENANT_SETTING: &str = "app.current_tenant_id";

/// Generates a SQL statement to safely set the RLS tenant context for a session.
///
/// This helper constructs the proper `SET LOCAL` or `SET` command to configure
/// the `app.current_tenant_id` session variable used by RLS policies.
///
/// # Security Notes
///
/// - Uses parameterized UUID to prevent SQL injection
/// - The UUID is validated before being embedded in the SQL
/// - RLS policies check `NULL` tenant_id as bypass (superuser/migration access)
/// - Always use `SET LOCAL` for transaction-scoped context
///
/// # Example
///
/// ```sql
/// -- Set tenant context for current session (transaction-scoped with SET LOCAL)
/// SET LOCAL app.current_tenant_id = '550e8400-e29b-41d4-a716-446655440000';
///
/// -- Then subsequent queries in the same transaction will be tenant-scoped
/// SELECT * FROM intents WHERE tenant_id = current_tenant_id();
/// ```
pub fn rls_set_tenant_context_sql(tenant_id: uuid::Uuid) -> String {
    format!("SET LOCAL {} = '{}'", RLS_TENANT_SETTING, tenant_id)
}

/// Generates a SQL statement to reset the RLS tenant context.
///
/// Use this at the end of a transaction or when switching tenants.
/// The `RESET` command clears the session variable.
pub fn rls_reset_tenant_context_sql() -> String {
    format!("RESET {}", RLS_TENANT_SETTING)
}

/// Validates that a tenant_id UUID is safe to use in RLS context.
///
/// Returns `Err` with explanation if the UUID is not valid for RLS use.
pub fn validate_tenant_id_for_rls(tenant_id: uuid::Uuid) -> Result<(), String> {
    // Check for nil UUID which is used as sentinel/default
    if tenant_id == uuid::Uuid::nil() {
        return Err(
            "Nil UUID (00000000-0000-0000-0000-000000000000) cannot be used as tenant_id \
             for RLS context; it is reserved as the default/sentinel value"
                .into(),
        );
    }

    // Additional validation could go here (e.g., format checks, range checks)
    Ok(())
}

/// Bounded RLS tenant context helper for transaction-scoped SQL RLS enforcement.
///
/// This struct encapsulates a validated tenant_id and provides methods to
/// set/reset the PostgreSQL `app.current_tenant_id` session variable within
/// a SQL transaction.
///
/// **Bounded scope:** This helper sets the RLS context but does NOT
/// automatically wrap all repository operations. Full repository transaction
/// wrapping remains pending (see Phase 3 P3-S5 pending items).
///
/// # Usage
///
/// ```ignore
/// // Create context from validated tenant_id
/// let ctx = RlsTenantContext::new(validated_tenant_id)?;
///
/// // Execute within a transaction
/// let mut tx = pool.begin().await?;
/// ctx.set_rls_context(&mut tx).await?;
/// // ... run tenant-scoped queries ...
/// ctx.reset_rls_context(&mut tx).await?;
/// tx.commit().await?;
/// ```
#[derive(Debug, Clone)]
pub struct RlsTenantContext {
    tenant_id: uuid::Uuid,
}

impl RlsTenantContext {
    /// Creates a new RLS tenant context from a validated tenant UUID.
    ///
    /// Returns an error if the tenant_id is not valid for RLS use.
    /// Use `validate_tenant_id_for_rls()` to validate before calling this.
    pub fn new(tenant_id: uuid::Uuid) -> Result<Self, String> {
        validate_tenant_id_for_rls(tenant_id)?;
        Ok(Self { tenant_id })
    }

    /// Returns the validated tenant ID.
    pub fn tenant_id(&self) -> uuid::Uuid {
        self.tenant_id
    }

    /// Sets the RLS tenant context in the provided transaction.
    ///
    /// Executes `SET LOCAL app.current_tenant_id = '<tenant_id>'` to configure
    /// the PostgreSQL session variable used by RLS policies.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQL execution fails.
    pub async fn set_rls_context(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::Error> {
        let sql = rls_set_tenant_context_sql(self.tenant_id);
        sqlx::query(&sql).execute(&mut **tx).await?;
        Ok(())
    }

    /// Resets the RLS tenant context in the provided transaction.
    ///
    /// Executes `RESET app.current_tenant_id` to clear the session variable.
    /// Call this at the end of a tenant-scoped transaction or when switching tenants.
    ///
    /// # Errors
    ///
    /// Returns an error if the SQL execution fails.
    pub async fn reset_rls_context(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::Error> {
        let sql = rls_reset_tenant_context_sql();
        sqlx::query(&sql).execute(&mut **tx).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rls_set_tenant_context_sql() {
        let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let sql = rls_set_tenant_context_sql(tenant_id);
        assert_eq!(
            sql,
            "SET LOCAL app.current_tenant_id = '550e8400-e29b-41d4-a716-446655440000'"
        );
    }

    #[test]
    fn test_rls_reset_tenant_context_sql() {
        let sql = rls_reset_tenant_context_sql();
        assert_eq!(sql, "RESET app.current_tenant_id");
    }

    #[test]
    fn test_validate_tenant_id_for_rls_nil_rejected() {
        let nil_uuid = uuid::Uuid::nil();
        let result = validate_tenant_id_for_rls(nil_uuid);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Nil UUID"));
    }

    #[test]
    fn test_validate_tenant_id_for_rls_valid() {
        let valid_uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let result = validate_tenant_id_for_rls(valid_uuid);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rls_tenant_context_new_valid() {
        let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let ctx = RlsTenantContext::new(tenant_id);
        assert!(ctx.is_ok());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.tenant_id(), tenant_id);
    }

    #[test]
    fn test_rls_tenant_context_new_nil_rejected() {
        let nil_uuid = uuid::Uuid::nil();
        let ctx = RlsTenantContext::new(nil_uuid);
        assert!(ctx.is_err());
        assert!(ctx.unwrap_err().contains("Nil UUID"));
    }

    #[test]
    fn test_rls_tenant_context_clone() {
        let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let ctx = RlsTenantContext::new(tenant_id).unwrap();
        let ctx_clone = ctx.clone();
        assert_eq!(ctx.tenant_id(), ctx_clone.tenant_id());
    }

    #[test]
    fn test_rls_tenant_context_debug() {
        let tenant_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let ctx = RlsTenantContext::new(tenant_id).unwrap();
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("RlsTenantContext"));
        assert!(debug_str.contains("550e8400-e29b-41d4-a716-446655440000"));
    }
}
