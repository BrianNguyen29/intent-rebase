//! RLS Integration Test - validates tenant isolation under PostgreSQL Row-Level Security.
//!
//! This test verifies that `SET LOCAL app.current_tenant_id` and migration 013's RLS
//! policies correctly enforce tenant isolation in live PostgreSQL.
//!
//! ## Running the Test
//!
//! ### Prerequisites
//! - PostgreSQL 16+ running (via docker-compose or local installation)
//! - Database pre-migrated with all migrations including 013 (see Migration Note below)
//!
//! ### Environment Variables
//!
//! | Variable | Required | Default | Description |
//! |----------|----------|---------|-------------|
//! | `DATABASE_URL` | Yes | - | PostgreSQL connection string. Example: `postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase` |
//! | `RLS_TEST_RUN_MIGRATIONS` | No | `false` | Set to `true` to run migrations before tests (not recommended - see Migration Note) |
//!
//! ### Run Commands
//!
//! ```bash
//! # Normal test run (skips live DB tests)
//! cargo test -p intent-api
//!
//! # Run RLS integration test explicitly (requires pre-migrated Postgres)
//! cargo test -p intent-api --test rls_integration -- --ignored
//!
//! # Run with custom DATABASE_URL
//! DATABASE_URL="postgres://user:pass@host:5432/db" cargo test -p intent-api --test rls_integration -- --ignored
//! ```
//!
//! ### Docker Compose (Local Postgres)
//!
//! From repo root:
//! ```bash
//! docker compose -f infrastructure/local/docker-compose.yml up -d postgres
//! ```
//!
//! ## Migration Note
//!
//! **Migration 009 Consolidation**: The original duplicate 009 migrations
//! (`009_create_policy_snapshot.sql` and `009_add_rebase_apply_blocked_audit_event.sql`)
//! have been consolidated into a single `009_add_rebase_apply_blocked_audit_event.sql` file.
//! This was done to resolve the duplicate version error in `_sqlx_migrations`.
//!
//! **Local DB Note**: If your local DB was migrated when 009 existed as two separate files,
//! you may have a checksum mismatch in `_sqlx_migrations` for version 009. The consolidated
//! migration is idempotent (uses `CREATE TABLE IF NOT EXISTS`), so re-running will not cause
//! issues, but the checksum warning is expected in this specific case.
//!
//! **Default behavior**: This test assumes the database is already pre-migrated.
//! Do NOT set `RLS_TEST_RUN_MIGRATIONS=true` unless you have a clean migration history.
//!
//! ## Test Strategy
//!
//! 1. **Setup**: Connect to live DB (migrations assumed already applied)
//! 2. **Tenant A context**: Begin transaction, set `app.current_tenant_id = tenant_a_uuid`
//! 3. **Tenant B context**: Begin transaction, set `app.current_tenant_id = tenant_b_uuid`
//! 4. **Verification**: Query from each tenant context and confirm cross-tenant visibility is blocked
//!
//! ## What This Test Validates
//!
//! - RLS policies are created correctly by migration 013
//! - `SET LOCAL app.current_tenant_id` sets the session variable within transactions
//! - Tenant-scoped tables (intents, etc.) filter rows based on current_tenant_id()
//! - No cross-tenant data leakage under RLS enforcement
//!
//! ## Owner-Bypass Protection (RLC-3)
//!
//! Migration 013 applies `FORCE ROW LEVEL SECURITY` to prevent table owners from
//! bypassing RLS. Without FORCE RLS, the table owner (e.g., `intent_rebase` user)
//! can bypass RLS even when `relrowsecurity=true`, allowing cross-tenant data
//! leakage. The test verifies `relforcerowsecurity=true` via
//! `verify_force_rls_enabled_on_tables()` to ensure this protection is active.

use intent_rebase_types::rls::{rls_set_tenant_context_sql, RlsTenantContext};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
use uuid::Uuid;

/// Name of the dedicated non-bypass test role for RLS isolation testing.
/// This role is created at test start and dropped at test end to ensure
/// RLS policies are actually enforced (intent_rebase is superuser/bypass).
const TEST_ROLE_NAME: &str = "intent_rebase_rls_test_user";
/// Password for the test role (not production credentials).
const TEST_ROLE_PASSWORD: &str = "intent_rebase_rls_test_user_dev_password";

/// Serializes migration application across all tests in this file.
/// Uses OnceCell so migrations are applied exactly once per test process,
/// even when multiple ignored tests run concurrently.
static MIGRATION_GUARD: OnceCell<()> = OnceCell::const_new();

/// Used to serialize migration initialization across concurrent tests.
/// Only used as a fallback if OnceCell initialization races.
static MIGRATION_MUTEX: Mutex<()> = Mutex::const_new(());

/// Tenant A UUID for RLS isolation testing
const TENANT_A_UUID: &str = "550e8400-e29b-41d4-a716-446655440001";
/// Tenant B UUID for RLS isolation testing
const TENANT_B_UUID: &str = "550e8400-e29b-41d4-a716-446655440002";
/// Test workflow UUID (constant for deterministic testing)
const TEST_WORKFLOW_UUID: &str = "550e8400-e29b-41d4-a716-446655440099";

/// Key tables to verify RLS isolation on.
/// These are selected from migration 013 as the primary tenant-scoped targets.
const RLS_SCOPED_TABLES: &[&str] = &["intents", "audit_events", "checkpoints"];

/// Test result type for clearer error handling
type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Parse a UUID constant string, panicking on failure (only for test constants).
fn parse_test_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_else(|_| panic!("Invalid test UUID: {}", s))
}

/// Skip reason when DATABASE_URL is not configured
const SKIP_REASON_NO_DATABASE: &str = "DATABASE_URL not set - cannot run live RLS integration test";

/// Check if the DATABASE_URL environment variable is configured and non-empty.
fn get_database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

/// Check if we should skip running migrations.
/// Defaults to true (skip) unless explicitly requested. Set RLS_TEST_RUN_MIGRATIONS=true
/// only if your database has a clean migration history without checksum mismatches.
fn should_skip_migrations() -> bool {
    // Default to skip migrations (true) to avoid checksum mismatch issues
    // Only run migrations if explicitly requested via RLS_TEST_RUN_MIGRATIONS=true
    std::env::var("RLS_TEST_RUN_MIGRATIONS")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(true) // Default: skip migrations
}

/// Verify that RLS policies exist on key tables by checking pg_policies.
/// Returns Ok if policies are found, Err with message if not.
async fn verify_rls_policies_exist(pool: &sqlx::PgPool) -> TestResult<()> {
    for table_name in RLS_SCOPED_TABLES {
        let policy_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM pg_policies
                WHERE schemaname = 'public'
                  AND tablename = $1
                  AND policyname = 'tenant_isolation'
            )
            "#,
        )
        .bind(table_name)
        .fetch_one(pool)
        .await?;

        if !policy_exists {
            return Err(format!(
                "RLS policy 'tenant_isolation' not found on table '{}' - migration 013 may not have been applied",
                table_name
            )
            .into());
        }
    }
    Ok(())
}

/// Run database migrations from infrastructure/migrations.
/// This applies all migrations including 013_enable_rls_policies.sql.
async fn run_migrations(pool: &sqlx::PgPool) -> TestResult<()> {
    // Migration path: from crates/intent-api/, ../../infrastructure/migrations
    // sqlx::migrate! expects path relative to crate root (manifest directory)
    sqlx::migrate!("../../infrastructure/migrations")
        .run(pool)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;
    Ok(())
}

/// Ensures migrations are applied exactly once per test process.
///
/// Uses a static OnceCell to serialize migration application across all tests.
/// When multiple tests call this concurrently, they all wait for the same
/// migration to complete (no duplicate key errors).
///
/// By default (should_skip_migrations=true), this function does nothing to avoid
/// checksum mismatch issues. Set RLS_TEST_RUN_MIGRATIONS=true only if your DB
/// has a clean migration history.
async fn ensure_migrations(pool: &sqlx::PgPool) -> TestResult<()> {
    if should_skip_migrations() {
        // Default: skip migrations due to duplicate 009 migration blocker
        return Ok(());
    }

    // Fast path: if already initialized, skip
    if MIGRATION_GUARD.get().is_some() {
        return Ok(());
    }

    // Slow path: acquire mutex and initialize
    let _guard = MIGRATION_MUTEX.lock().await;

    // Double-check after acquiring lock
    if MIGRATION_GUARD.get().is_some() {
        return Ok(());
    }

    println!("Running migrations (first test to run in this process)...");
    run_migrations(pool).await?;
    println!("Migrations applied successfully.");

    // Mark as initialized
    let _ = MIGRATION_GUARD.set(());

    Ok(())
}

/// Create a test intent row for the given tenant within an RLS-scoped transaction.
///
/// The RLS context must be set BEFORE calling this function via `SET LOCAL app.current_tenant_id`.
async fn create_test_intent_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO intents (intent_id, tenant_id, workflow_id, current_version, status,
                             created_by_actor_type, created_by_actor_id, source_refs, tags)
        VALUES ($1, $2, $3, 1, 'active', 'test', 'rls-integration-test', '[]', '{}')
        "#,
    )
    .bind(intent_id)
    .bind(tenant_id)
    .bind(Uuid::parse_str(TEST_WORKFLOW_UUID).unwrap())
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test intent: {}", e))?;

    Ok(())
}

/// Count intents visible to the current tenant context, filtered by a list of specific intent_ids.
/// This is robust against pre-existing data because we only count test-created rows.
async fn count_test_intents_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    intent_ids: &[Uuid],
) -> TestResult<i64> {
    if intent_ids.is_empty() {
        return Ok(0);
    }
    // Build a query that counts only the specific intent_ids we created
    let placeholders: Vec<String> = intent_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM intents WHERE intent_id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in intent_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test intents: {}", e))?;
    Ok(count)
}

/// Verify RLS is enforced by checking pg_tables relrowsecurity flag.
async fn verify_rls_enabled_on_tables(pool: &sqlx::PgPool) -> TestResult<()> {
    for table_name in RLS_SCOPED_TABLES {
        let rls_enabled: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT relrowsecurity::bool
            FROM pg_tables
            JOIN pg_class ON pg_tables.tablename = pg_class.relname
            WHERE schemaname = 'public' AND tablename = $1
            "#,
        )
        .bind(table_name)
        .fetch_one(pool)
        .await?;

        if !rls_enabled.unwrap_or(false) {
            return Err(format!(
                "RLS is not enabled on table '{}' - migration 013 may not have been applied",
                table_name
            )
            .into());
        }
    }
    Ok(())
}

/// Verify FORCE ROW LEVEL SECURITY is enabled on tenant-scoped tables.
///
/// This prevents table owners from bypassing RLS policies. Without FORCE RLS,
/// the table owner (e.g., intent_rebase user) can bypass RLS even when
/// relrowsecurity=true, allowing cross-tenant data leakage (RLC-3 failure).
async fn verify_force_rls_enabled_on_tables(pool: &sqlx::PgPool) -> TestResult<()> {
    for table_name in RLS_SCOPED_TABLES {
        let force_rls_enabled: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT relforcerowsecurity::bool
            FROM pg_tables
            JOIN pg_class ON pg_tables.tablename = pg_class.relname
            WHERE schemaname = 'public' AND tablename = $1
            "#,
        )
        .bind(table_name)
        .fetch_one(pool)
        .await?;

        if !force_rls_enabled.unwrap_or(false) {
            return Err(format!(
                "FORCE ROW LEVEL SECURITY is not enabled on table '{}' - \
                table owner can bypass RLS policies (RLC-3 owner-bypass vulnerability). \
                Migration 013 must set FORCE ROW LEVEL SECURITY on all tenant-scoped tables.",
                table_name
            )
            .into());
        }
    }
    Ok(())
}

/// Check if the current connection role has BYPASSRLS or is a superuser.
///
/// This is the root cause of RLC-3 test failures when run against the
/// local docker `intent_rebase` role which has rolsuper=true and rolbypassrls=true.
/// Even with FORCE RLS enabled, a superuser/bypass role bypasses all RLS policies.
async fn check_current_role_is_bypass(pool: &sqlx::PgPool) -> TestResult<(bool, String)> {
    let row: (bool, bool, String) = sqlx::query_as(
        r#"
        SELECT rolsuper, rolbypassrls, rolname::text
        FROM pg_roles
        WHERE rolname = current_user
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to check current role attributes: {}", e))?;

    let (is_superuser, is_bypass, role_name) = row;
    let is_problematic = is_superuser || is_bypass;
    Ok((is_problematic, role_name))
}

/// Create the dedicated non-bypass test role for RLS isolation testing.
///
/// This role is explicitly NOT a superuser and does NOT have BYPASSRLS,
/// so RLS policies will be enforced. Returns the connection string for
/// the new role.
async fn create_test_role(pool: &sqlx::PgPool) -> TestResult<String> {
    // First check if role already exists and drop it
    drop_test_role(pool).await?;

    // Create the test role with a password
    // Use $ quoting for password to avoid escaping issues
    sqlx::query(&format!(
        "CREATE ROLE {} LOGIN PASSWORD $${}$$",
        TEST_ROLE_NAME, TEST_ROLE_PASSWORD
    ))
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create test role {}: {}", TEST_ROLE_NAME, e))?;

    // Grant schema usage and table privileges
    // The test role needs to be able to insert/select from tenant tables
    let grants = [
        "GRANT USAGE ON SCHEMA public TO",
        "GRANT INSERT, SELECT ON intents TO",
        "GRANT INSERT, SELECT ON audit_events TO",
        "GRANT INSERT, SELECT ON checkpoints TO",
        "GRANT INSERT, SELECT ON approval_requests TO",
        "GRANT INSERT, SELECT ON graph_nodes TO",
        "GRANT INSERT, SELECT ON graph_edges TO",
        "GRANT INSERT, SELECT ON side_effects TO",
        "GRANT INSERT, SELECT ON compensation_actions TO",
        "GRANT INSERT, SELECT ON side_effect_rollback_records TO",
        "GRANT INSERT, SELECT ON policy_snapshot TO",
    ];

    for grant in &grants {
        sqlx::query(&format!("{} {}", grant, TEST_ROLE_NAME))
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to grant privileges: {}", e))?;
    }

    // Build connection URL for the test role
    // Parse the original DATABASE_URL and replace credentials
    let original_url = std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL not set")?;
    // Simple parse: postgres://user:pass@host:port/db
    // Replace user portion
    let test_url = if original_url.contains('@') {
        // Extract host portion and rebuild
        if let Some(at_pos) = original_url.find('@') {
            let host_part = &original_url[at_pos + 1..];
            format!(
                "postgres://{}:{}@{}",
                TEST_ROLE_NAME, TEST_ROLE_PASSWORD, host_part
            )
        } else {
            return Err("Invalid DATABASE_URL format".into());
        }
    } else {
        return Err("Invalid DATABASE_URL format - no @ found".into());
    };

    println!(
        "Created test role '{}' for RLS isolation testing",
        TEST_ROLE_NAME
    );
    Ok(test_url)
}

/// Drop the test role if it exists.
async fn drop_test_role(pool: &sqlx::PgPool) -> TestResult<()> {
    // Check if role exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
            .bind(TEST_ROLE_NAME)
            .fetch_one(pool)
            .await
            .map_err(|e| format!("Failed to check if test role exists: {}", e))?;

    if exists {
        // Terminate any existing connections first
        sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1")
            .bind(TEST_ROLE_NAME)
            .execute(pool)
            .await
            .ok(); // Ignore errors - connection might already be gone

        // Revoke all privileges and drop owned objects first
        // DROP OWNED BY revokes privileges granted to the role and drops objects owned by the role
        sqlx::query(&format!("DROP OWNED BY {} CASCADE", TEST_ROLE_NAME))
            .execute(pool)
            .await
            .ok(); // Ignore errors if no owned objects

        // Now drop the role (should succeed since dependencies are cleared)
        sqlx::query(&format!("DROP ROLE IF EXISTS {}", TEST_ROLE_NAME))
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to drop test role: {}", e))?;
        println!("Dropped test role '{}'", TEST_ROLE_NAME);
    }
    Ok(())
}

// =============================================================================
// Integration Tests
// =============================================================================

/// Test: RLS policies are correctly configured on tenant-scoped tables.
///
/// This test verifies:
/// - Migration 013 was applied successfully
/// - RLS is enabled on key tenant-scoped tables
/// - The tenant_isolation policy exists on each table
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rls_policies_exist() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            eprintln!("Set DATABASE_URL to run this test locally.");
            return;
        }
    };

    // Connect to the database
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database - check DATABASE_URL and postgres service");

    // Ensure migrations are applied once per test process
    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Verify RLS is enabled on tables
    verify_rls_enabled_on_tables(&pool)
        .await
        .expect("RLS verification failed - ensure migration 013 was applied");

    // Verify FORCE ROW LEVEL SECURITY is enabled (prevents owner bypass)
    verify_force_rls_enabled_on_tables(&pool).await.expect(
        "FORCE RLS verification failed - ensure migration 013 applies FORCE ROW LEVEL SECURITY",
    );

    // Verify tenant_isolation policies exist
    verify_rls_policies_exist(&pool)
        .await
        .expect("RLS policy verification failed - ensure migration 013 was applied");

    pool.close().await;
    println!("test_rls_policies_exist PASSED - RLS policies correctly configured");
}

/// Test: SET LOCAL app.current_tenant_id enforces tenant isolation.
///
/// This is the core RLC-3 test. It:
/// 1. Creates intent rows for Tenant A and Tenant B in separate transactions
/// 2. Verifies each tenant can only see their own rows when querying with RLS context set
/// 3. Confirms cross-tenant visibility is blocked by RLS policies
///
/// **IMPORTANT**: This test requires a non-bypass RLS role to validate isolation.
/// If the DATABASE_URL role is a superuser or has BYPASSRLS, this test will:
/// - Detect this condition
/// - Create a dedicated `intent_rebase_rls_test_user` role without bypass privileges
/// - Use that role for isolation verification
/// - Clean up the test role at the end
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_tenant_isolation_under_rls() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            eprintln!("Set DATABASE_URL to run this test locally.");
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    // ===========================================================================
    // Step 1: Connect as admin and verify RLS configuration
    // ===========================================================================
    let admin_pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database - check DATABASE_URL and postgres service");

    // Ensure migrations are applied once per test process
    ensure_migrations(&admin_pool)
        .await
        .expect("Failed to ensure migrations");

    // Pre-flight: verify RLS is configured
    verify_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("RLS not enabled - cannot run isolation test");
    verify_force_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("FORCE RLS not enabled - table owner can bypass RLS (RLC-3 owner-bypass)");
    verify_rls_policies_exist(&admin_pool)
        .await
        .expect("RLS policies missing - cannot run isolation test");

    // ===========================================================================
    // Step 2: Check if we need a non-bypass role for testing
    // ===========================================================================
    let (is_bypass, current_role) = check_current_role_is_bypass(&admin_pool)
        .await
        .expect("Failed to check current role bypass status");

    let test_pool: sqlx::PgPool;
    let test_role_name: &str;

    if is_bypass {
        println!(
            "WARNING: Current role '{}' is superuser/bypass - RLS policies are bypassed!",
            current_role
        );
        println!("Creating dedicated non-bypass test role for RLS isolation verification...");

        // Create a non-bypass test role
        let test_url = create_test_role(&admin_pool)
            .await
            .expect("Failed to create non-bypass test role - cannot run RLS isolation test");

        // Connect as the non-bypass test role
        test_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(15))
            .connect(&test_url)
            .await
            .expect("Failed to connect with non-bypass test role");
        test_role_name = TEST_ROLE_NAME;

        println!(
            "Using non-bypass role '{}' for RLS isolation test (role '{}' is bypass)",
            TEST_ROLE_NAME, current_role
        );
    } else {
        println!(
            "Using current role '{}' for RLS isolation test (not a bypass role)",
            current_role
        );
        test_pool = admin_pool.clone();
        test_role_name = &current_role;
    }

    // ===========================================================================
    // Phase 1: Create test data for Tenant A
    // ===========================================================================
    println!("Setting up Tenant A data...");

    let tenant_a_intent_id = Uuid::new_v4();

    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");

        // Set RLS context for Tenant A
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        // Create intent for Tenant A
        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create test intent for Tenant A");

        // Verify Tenant A can see their own intent (using specific intent_id filter)
        let count = count_test_intents_for_current_tenant(&mut tx, &[tenant_a_intent_id])
            .await
            .expect("Failed to count Tenant A intents");
        assert_eq!(
            count, 1,
            "Tenant A should see exactly 1 intent after inserting their own"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
        println!(
            "Tenant A setup complete - created intent {}",
            tenant_a_intent_id
        );
    }

    // ===========================================================================
    // Phase 2: Create test data for Tenant B
    // ===========================================================================
    println!("Setting up Tenant B data...");

    let tenant_b_intent_id = Uuid::new_v4();

    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");

        // Set RLS context for Tenant B
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        // Create intent for Tenant B
        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create test intent for Tenant B");

        // Verify Tenant B can see their own intent (using specific intent_id filter)
        let count = count_test_intents_for_current_tenant(&mut tx, &[tenant_b_intent_id])
            .await
            .expect("Failed to count Tenant B intents");
        assert_eq!(
            count, 1,
            "Tenant B should see exactly 1 intent after inserting their own"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
        println!(
            "Tenant B setup complete - created intent {}",
            tenant_b_intent_id
        );
    }

    // ===========================================================================
    // Phase 3: Verify Tenant Isolation (using non-bypass role)
    // ===========================================================================
    println!(
        "Verifying tenant isolation under RLS using role '{}'...",
        test_role_name
    );

    // Tenant A context: should see Tenant A's intent, NOT Tenant B's
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        // CRITICAL ASSERTION: Tenant A must see their own intent
        let count_own = count_test_intents_for_current_tenant(&mut tx, &[tenant_a_intent_id])
            .await
            .expect("Failed to count Tenant A's own intents");
        assert_eq!(
            count_own, 1,
            "Tenant A should see exactly 1 intent (their own) - RLS isolation may be broken!"
        );

        // CRITICAL ASSERTION: Tenant A must NOT see Tenant B's intent
        let count_other = count_test_intents_for_current_tenant(&mut tx, &[tenant_b_intent_id])
            .await
            .expect("Failed to count Tenant B's intents from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should see 0 intents from Tenant B - RLS isolation may be broken! \
            If you see this failure with a non-bypass role, the RLS policy is not working. \
            If you see this with a bypass/superuser role, the test environment is misconfigured."
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
        println!(
            "Tenant A isolation verified - only sees intent {} (own), not Tenant B's {}",
            tenant_a_intent_id, tenant_b_intent_id
        );
    }

    // Tenant B context: should see Tenant B's intent, NOT Tenant A's
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        // CRITICAL ASSERTION: Tenant B must see their own intent
        let count_own = count_test_intents_for_current_tenant(&mut tx, &[tenant_b_intent_id])
            .await
            .expect("Failed to count Tenant B's own intents");
        assert_eq!(
            count_own, 1,
            "Tenant B should see exactly 1 intent (their own) - RLS isolation may be broken!"
        );

        // CRITICAL ASSERTION: Tenant B must NOT see Tenant A's intent
        let count_other = count_test_intents_for_current_tenant(&mut tx, &[tenant_a_intent_id])
            .await
            .expect("Failed to count Tenant A's intents from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should see 0 intents from Tenant A - RLS isolation may be broken! \
            If you see this failure with a non-bypass role, the RLS policy is not working. \
            If you see this with a bypass/superuser role, the test environment is misconfigured."
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
        println!(
            "Tenant B isolation verified - only sees intent {} (own), not Tenant A's {}",
            tenant_b_intent_id, tenant_a_intent_id
        );
    }

    // ===========================================================================
    // Phase 4: Verify Superuser/NULL context can see the test rows (admin pool)
    // ===========================================================================
    println!("Verifying superuser bypass (NULL tenant context)...");

    {
        let mut tx = admin_pool
            .begin()
            .await
            .expect("Failed to begin superuser transaction");

        // Without setting tenant context (NULL), should see both test intents
        // Use specific intent_id filtering to avoid interference from pre-existing data
        let count = count_test_intents_for_current_tenant(
            &mut tx,
            &[tenant_a_intent_id, tenant_b_intent_id],
        )
        .await
        .expect("Failed to count test intents in superuser context");

        assert_eq!(
            count, 2,
            "Superuser (NULL tenant context) should see both test intents (2)"
        );

        tx.commit()
            .await
            .expect("Failed to commit superuser verification");
        println!(
            "Superuser bypass verified - sees both test intents (tenant A: {}, tenant B: {})",
            tenant_a_intent_id, tenant_b_intent_id
        );
    }

    // ===========================================================================
    // Cleanup: Drop test role if we created one
    // ===========================================================================
    if is_bypass {
        println!("Cleaning up test role '{}'...", TEST_ROLE_NAME);
        drop_test_role(&admin_pool)
            .await
            .expect("Failed to drop test role");
    }

    admin_pool.close().await;
    if is_bypass {
        test_pool.close().await;
    }

    println!();
    println!("========================================");
    println!("test_tenant_isolation_under_rls PASSED");
    println!("========================================");
    println!("RLS tenant isolation is correctly enforced:");
    println!("  - Tenant A cannot see Tenant B's data");
    println!("  - Tenant B cannot see Tenant A's data");
    println!("  - Superuser (NULL context) bypasses RLS for migrations");
    println!(
        "  - Test used role '{}' for isolation verification",
        test_role_name
    );
}

/// Test: RlsTenantContext helper works correctly with SET LOCAL.
///
/// This validates that the RlsTenantContext helper from intent-rebase-types
/// correctly generates and executes the SET LOCAL SQL.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rls_tenant_context_helper() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_id = parse_test_uuid(TENANT_A_UUID);

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Ensure migrations are applied once per test process
    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Test RlsTenantContext::set_rls_context
    let ctx = RlsTenantContext::new(tenant_id).expect("Failed to create RlsTenantContext");

    let mut tx = pool.begin().await.expect("Failed to begin transaction");

    // Use the RlsTenantContext helper to set RLS
    ctx.set_rls_context(&mut tx)
        .await
        .expect("Failed to set RLS context via helper");

    // Verify we can query with the context set
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM intents")
        .fetch_one(&mut *tx)
        .await
        .expect("Failed to query intents");

    // Reset context
    ctx.reset_rls_context(&mut tx)
        .await
        .expect("Failed to reset RLS context");

    tx.commit().await.expect("Failed to commit");

    pool.close().await;
    println!(
        "test_rls_tenant_context_helper PASSED - helper works correctly (saw {} intents)",
        count
    );
}

/// Test: Verify RLS_TENANT_SETTING constant matches actual PostgreSQL setting name.
///
/// This is a basic sanity check that the hardcoded setting name matches what
/// migration 013 uses.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rls_tenant_setting_name() {
    use intent_rebase_types::rls::RLS_TENANT_SETTING;

    // Verify the constant matches what migration 013 uses
    assert_eq!(
        RLS_TENANT_SETTING, "app.current_tenant_id",
        "RLS_TENANT_SETTING must match the PostgreSQL setting name used in migration 013"
    );
    println!(
        "test_rls_tenant_setting_name PASSED - setting name: {}",
        RLS_TENANT_SETTING
    );
}
