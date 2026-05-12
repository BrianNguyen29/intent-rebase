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

use graph_service::{GraphRepository, GraphService, RlsAwarePool, SqlxGraphRepository};
use intent_rebase_types::rls::{rls_set_tenant_context_sql, RlsTenantContext};
use intent_rebase_types::{IntentRebaseError, NodeState};
use rebase_orchestrator::GraphUpdater;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
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
/// All 11 tenant-scoped tables (migration 013 + 015).
const RLS_SCOPED_TABLES: &[&str] = &[
    "intents",
    "audit_events",
    "checkpoints",
    "approval_requests",
    "graph_nodes",
    "graph_edges",
    "side_effects",
    "compensation_actions",
    "side_effect_rollback_records",
    "policy_snapshot",
    "orchestration_runs",
    "forensic_bundles",
];

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
        .map(|v| v.to_lowercase() != "true")
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

// =============================================================================
// Helper functions for additional RLS-scoped tables (RLC-4 through RLC-9)
// =============================================================================

/// Create a test approval_request for the given tenant within an RLS-scoped transaction.
async fn create_test_approval_request_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    request_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO approval_requests (id, intent_id, intent_version_from, intent_version_to,
                                     workflow_id, tenant_id, requestor_id, requestor_type,
                                     decision_class, reason, metadata, status)
        VALUES ($1, $2, 1, 2, $3, $4, 'rls-test', 'test', 'D',
                'RLS integration test approval request', '{}', 'pending')
        "#,
    )
    .bind(request_id)
    .bind(intent_id)
    .bind(Uuid::parse_str(TEST_WORKFLOW_UUID).unwrap())
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test approval_request: {}", e))?;

    Ok(())
}

/// Count approval_requests visible to the current tenant context.
async fn count_test_approval_requests_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request_ids: &[Uuid],
) -> TestResult<i64> {
    if request_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = request_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM approval_requests WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in request_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test approval_requests: {}", e))?;
    Ok(count)
}

/// Create a test checkpoint for the given tenant within an RLS-scoped transaction.
async fn create_test_checkpoint_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    checkpoint_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO checkpoints (checkpoint_id, intent_id, intent_version, workflow_id,
                                tenant_id, workflow_state, checkpoint_type, status, metadata)
        VALUES ($1, $2, 1, $3, $4, '{}', 'initial', 'pending', '{}')
        "#,
    )
    .bind(checkpoint_id)
    .bind(intent_id)
    .bind(Uuid::parse_str(TEST_WORKFLOW_UUID).unwrap())
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test checkpoint: {}", e))?;

    Ok(())
}

/// Count checkpoints visible to the current tenant context.
async fn count_test_checkpoints_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    checkpoint_ids: &[Uuid],
) -> TestResult<i64> {
    if checkpoint_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = checkpoint_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM checkpoints WHERE checkpoint_id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in checkpoint_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test checkpoints: {}", e))?;
    Ok(count)
}

/// Create a test graph_node for the given tenant within an RLS-scoped transaction.
async fn create_test_graph_node_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    node_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO graph_nodes (node_id, tenant_id, workflow_id, node_type,
                                external_ref_type, external_ref_id, label, state, properties)
        VALUES ($1, $2, $3, 'intent', 'intent', $4, 'RLS test node', 'active', '{}')
        "#,
    )
    .bind(node_id)
    .bind(tenant_id)
    .bind(Uuid::parse_str(TEST_WORKFLOW_UUID).unwrap())
    .bind(node_id) // use same ID as external_ref_id
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test graph_node: {}", e))?;

    Ok(())
}

/// Count graph_nodes visible to the current tenant context.
async fn count_test_graph_nodes_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    node_ids: &[Uuid],
) -> TestResult<i64> {
    if node_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = node_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM graph_nodes WHERE node_id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in node_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test graph_nodes: {}", e))?;
    Ok(count)
}

/// Create a test graph_edge for the given tenant within an RLS-scoped transaction.
/// Requires that the from_node_id and to_node_id already exist in graph_nodes.
async fn create_test_graph_edge_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    edge_id: Uuid,
    from_node_id: Uuid,
    to_node_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO graph_edges (edge_id, tenant_id, workflow_id, from_node_id, to_node_id,
                                edge_type, properties)
        VALUES ($1, $2, $3, $4, $5, 'depends_on', '{}')
        "#,
    )
    .bind(edge_id)
    .bind(tenant_id)
    .bind(Uuid::parse_str(TEST_WORKFLOW_UUID).unwrap())
    .bind(from_node_id)
    .bind(to_node_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test graph_edge: {}", e))?;

    Ok(())
}

/// Count graph_edges visible to the current tenant context.
async fn count_test_graph_edges_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    edge_ids: &[Uuid],
) -> TestResult<i64> {
    if edge_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = edge_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM graph_edges WHERE edge_id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in edge_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test graph_edges: {}", e))?;
    Ok(count)
}

/// Create a test side_effect for the given tenant within an RLS-scoped transaction.
async fn create_test_side_effect_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    side_effect_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO side_effects (id, tenant_id, intent_id, intent_version, effect_class,
                                  effect_type, target, idempotency_key)
        VALUES ($1, $2, $3, 1, 's2_external_reversible', 'rls_test_effect',
                'test-target-uuid', $4)
        "#,
    )
    .bind(side_effect_id)
    .bind(tenant_id)
    .bind(intent_id)
    .bind(side_effect_id) // use as idempotency_key
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test side_effect: {}", e))?;

    Ok(())
}

/// Count side_effects visible to the current tenant context.
async fn count_test_side_effects_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    side_effect_ids: &[Uuid],
) -> TestResult<i64> {
    if side_effect_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = side_effect_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM side_effects WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in side_effect_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test side_effects: {}", e))?;
    Ok(count)
}

/// Create a test compensation_action for the given tenant within an RLS-scoped transaction.
async fn create_test_compensation_action_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    compensation_id: Uuid,
    side_effect_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO compensation_actions (id, tenant_id, side_effect_id, intent_id,
                                        trigger_context, execution_result_payload,
                                        feasibility, strategy_type, rationale, status)
        VALUES ($1, $2, $3, $4, '{"intent_id": $4}', NULL,
                'semi_automatic', 'rollback', 'RLS integration test', 'pending')
        "#,
    )
    .bind(compensation_id)
    .bind(tenant_id)
    .bind(side_effect_id)
    .bind(intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test compensation_action: {}", e))?;

    Ok(())
}

/// Count compensation_actions visible to the current tenant context.
async fn count_test_compensation_actions_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    compensation_ids: &[Uuid],
) -> TestResult<i64> {
    if compensation_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = compensation_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM compensation_actions WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in compensation_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test compensation_actions: {}", e))?;
    Ok(count)
}

/// Create a test side_effect_rollback_record for the given tenant within an RLS-scoped transaction.
async fn create_test_side_effect_rollback_record_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    rollback_id: Uuid,
    compensation_action_id: Uuid,
    side_effect_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO side_effect_rollback_records (id, tenant_id, compensation_action_id,
                                                side_effect_id, intent_id, result, summary,
                                                recorded_by, lock_version)
        VALUES ($1, $2, $3, $4, $5, 'success', 'RLS integration test rollback',
                'rls-test-user', 0)
        "#,
    )
    .bind(rollback_id)
    .bind(tenant_id)
    .bind(compensation_action_id)
    .bind(side_effect_id)
    .bind(intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test side_effect_rollback_record: {}", e))?;

    Ok(())
}

/// Count side_effect_rollback_records visible to the current tenant context.
async fn count_test_side_effect_rollback_records_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rollback_ids: &[Uuid],
) -> TestResult<i64> {
    if rollback_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = rollback_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM side_effect_rollback_records WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in rollback_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test side_effect_rollback_records: {}", e))?;
    Ok(count)
}

/// Create a test policy_snapshot for the given tenant within an RLS-scoped transaction.
async fn create_test_policy_snapshot_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    snapshot_id: Uuid,
    intent_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO policy_snapshot (id, tenant_id, intent_id, intent_version,
                                    rule_pack_version, scope_type, affected_resources,
                                    required_approvers, min_approvals, scope_hash, snapshot_uri)
        VALUES ($1, $2, $3, 1, 'test-version', 'full', '[]', '[]', 1,
                'test-scope-hash-sha256', 'memory://test-snapshot')
        "#,
    )
    .bind(snapshot_id)
    .bind(tenant_id)
    .bind(intent_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test policy_snapshot: {}", e))?;

    Ok(())
}

/// Count policy_snapshots visible to the current tenant context.
async fn count_test_policy_snapshots_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    snapshot_ids: &[Uuid],
) -> TestResult<i64> {
    if snapshot_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = snapshot_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM policy_snapshot WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in snapshot_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test policy_snapshots: {}", e))?;
    Ok(count)
}

/// Create a test orchestration_run for the given tenant within an RLS-scoped transaction.
async fn create_test_orchestration_run_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    run_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO orchestration_runs (id, tenant_id, intent_id, action_ids, status,
                                      initiated_by, created_at, succeeded_count, failed_count,
                                      skipped_count, not_found_count, total_count, item_results)
        VALUES ($1, $2, NULL, '[]'::jsonb, 'pending', 'rls-integration-test', NOW(),
                0, 0, 0, 0, 0, '[]'::jsonb)
        "#,
    )
    .bind(run_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test orchestration_run: {}", e))?;

    Ok(())
}

/// Count orchestration_runs visible to the current tenant context.
async fn count_test_orchestration_runs_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_ids: &[Uuid],
) -> TestResult<i64> {
    if run_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = run_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM orchestration_runs WHERE id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in run_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test orchestration_runs: {}", e))?;
    Ok(count)
}

/// Create a test forensic_bundle for the given tenant within an RLS-scoped transaction.
async fn create_test_forensic_bundle_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    bundle_id: Uuid,
) -> TestResult<()> {
    sqlx::query(
        r#"
        INSERT INTO forensic_bundles (
            bundle_id, tenant_id, bundle_version, created_at, created_by,
            time_range_start, time_range_end, purpose, status,
            contents, integrity, retention
        )
        VALUES ($1, $2, 'v1', NOW(), 'rls-test', NOW(), NOW(),
                'incident_investigation', 'pending',
                '{"intent_versions":0,"artifacts":0,"approvals":0,"audit_events":0,"policy_snapshots":0}',
                '{"manifest_hash":"test","chain_verified":false,"verification_timestamp":"2024-01-01T00:00:00Z"}',
                NULL)
        "#,
    )
    .bind(bundle_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("Failed to insert test forensic_bundle: {}", e))?;

    Ok(())
}

/// Count forensic_bundles visible to the current tenant context.
async fn count_test_forensic_bundles_for_current_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    bundle_ids: &[Uuid],
) -> TestResult<i64> {
    if bundle_ids.is_empty() {
        return Ok(0);
    }
    let placeholders: Vec<String> = bundle_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect();
    let query = format!(
        "SELECT COUNT(*) FROM forensic_bundles WHERE bundle_id IN ({})",
        placeholders.join(", ")
    );
    let mut query_builder = sqlx::query_scalar::<_, i64>(&query);
    for id in bundle_ids {
        query_builder = query_builder.bind(id);
    }
    let count = query_builder
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| format!("Failed to count test forensic_bundles: {}", e))?;
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
        "GRANT INSERT, SELECT, UPDATE ON graph_nodes TO",
        "GRANT INSERT, SELECT ON graph_edges TO",
        "GRANT INSERT, SELECT ON side_effects TO",
        "GRANT INSERT, SELECT ON compensation_actions TO",
        "GRANT INSERT, SELECT ON side_effect_rollback_records TO",
        "GRANT INSERT, SELECT ON policy_snapshot TO",
        "GRANT INSERT, SELECT ON forensic_bundles TO",
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

// =============================================================================
// RLC-4 through RLC-9: Tenant isolation tests for additional RLS tables
// =============================================================================

/// Test: RLC-4 - Tenant isolation on approval_requests table.
///
/// Verifies that approval_requests rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc4_tenant_isolation_approval_requests() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create test data for Tenant A
    let tenant_a_request_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        // Create intent (prerequisite for approval_request via FK)
        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create test intent for Tenant A");

        // Create approval_request for Tenant A
        create_test_approval_request_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_request_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create test approval_request for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
    }

    // Create test data for Tenant B
    let tenant_b_request_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        // Create intent for Tenant B
        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create test intent for Tenant B");

        // Create approval_request for Tenant B
        create_test_approval_request_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_request_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create test approval_request for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own =
            count_test_approval_requests_for_current_tenant(&mut tx, &[tenant_a_request_id])
                .await
                .expect("Failed to count Tenant A approval_requests");
        assert_eq!(
            count_own, 1,
            "Tenant A should see their own approval_request"
        );

        let count_other =
            count_test_approval_requests_for_current_tenant(&mut tx, &[tenant_b_request_id])
                .await
                .expect("Failed to count Tenant B approval_requests from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's approval_request"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own =
            count_test_approval_requests_for_current_tenant(&mut tx, &[tenant_b_request_id])
                .await
                .expect("Failed to count Tenant B approval_requests");
        assert_eq!(
            count_own, 1,
            "Tenant B should see their own approval_request"
        );

        let count_other =
            count_test_approval_requests_for_current_tenant(&mut tx, &[tenant_a_request_id])
                .await
                .expect("Failed to count Tenant A approval_requests from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's approval_request"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc4_tenant_isolation_approval_requests PASSED");
}

/// Test: RLC-5 - Tenant isolation on graph_nodes table.
///
/// Verifies that graph_nodes rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc5_tenant_isolation_graph_nodes() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create test data for Tenant A
    let tenant_a_node_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_graph_node_for_current_tenant(&mut tx, tenant_a_id, tenant_a_node_id)
            .await
            .expect("Failed to create test graph_node for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
    }

    // Create test data for Tenant B
    let tenant_b_node_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_graph_node_for_current_tenant(&mut tx, tenant_b_id, tenant_b_node_id)
            .await
            .expect("Failed to create test graph_node for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own = count_test_graph_nodes_for_current_tenant(&mut tx, &[tenant_a_node_id])
            .await
            .expect("Failed to count Tenant A graph_nodes");
        assert_eq!(count_own, 1, "Tenant A should see their own graph_node");

        let count_other = count_test_graph_nodes_for_current_tenant(&mut tx, &[tenant_b_node_id])
            .await
            .expect("Failed to count Tenant B graph_nodes from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's graph_node"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own = count_test_graph_nodes_for_current_tenant(&mut tx, &[tenant_b_node_id])
            .await
            .expect("Failed to count Tenant B graph_nodes");
        assert_eq!(count_own, 1, "Tenant B should see their own graph_node");

        let count_other = count_test_graph_nodes_for_current_tenant(&mut tx, &[tenant_a_node_id])
            .await
            .expect("Failed to count Tenant A graph_nodes from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's graph_node"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc5_tenant_isolation_graph_nodes PASSED");
}

/// Test: RLC-6 - Tenant isolation on graph_edges table.
///
/// Verifies that graph_edges rows are correctly isolated between tenants.
/// Note: graph_edges has FK dependency on graph_nodes, so we create nodes first.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc6_tenant_isolation_graph_edges() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create graph_nodes first (prerequisite for graph_edges)
    let tenant_a_from_node = Uuid::new_v4();
    let tenant_a_to_node = Uuid::new_v4();
    let tenant_b_from_node = Uuid::new_v4();
    let tenant_b_to_node = Uuid::new_v4();

    // Tenant A nodes
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_graph_node_for_current_tenant(&mut tx, tenant_a_id, tenant_a_from_node)
            .await
            .expect("Failed to create Tenant A from_node");
        create_test_graph_node_for_current_tenant(&mut tx, tenant_a_id, tenant_a_to_node)
            .await
            .expect("Failed to create Tenant A to_node");

        tx.commit().await.expect("Failed to commit Tenant A nodes");
    }

    // Tenant B nodes
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_graph_node_for_current_tenant(&mut tx, tenant_b_id, tenant_b_from_node)
            .await
            .expect("Failed to create Tenant B from_node");
        create_test_graph_node_for_current_tenant(&mut tx, tenant_b_id, tenant_b_to_node)
            .await
            .expect("Failed to create Tenant B to_node");

        tx.commit().await.expect("Failed to commit Tenant B nodes");
    }

    // Create test edges for Tenant A
    let tenant_a_edge_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A edge");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_graph_edge_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_edge_id,
            tenant_a_from_node,
            tenant_a_to_node,
        )
        .await
        .expect("Failed to create test graph_edge for Tenant A");

        tx.commit().await.expect("Failed to commit Tenant A edge");
    }

    // Create test edges for Tenant B
    let tenant_b_edge_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B edge");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_graph_edge_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_edge_id,
            tenant_b_from_node,
            tenant_b_to_node,
        )
        .await
        .expect("Failed to create test graph_edge for Tenant B");

        tx.commit().await.expect("Failed to commit Tenant B edge");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own = count_test_graph_edges_for_current_tenant(&mut tx, &[tenant_a_edge_id])
            .await
            .expect("Failed to count Tenant A graph_edges");
        assert_eq!(count_own, 1, "Tenant A should see their own graph_edge");

        let count_other = count_test_graph_edges_for_current_tenant(&mut tx, &[tenant_b_edge_id])
            .await
            .expect("Failed to count Tenant B graph_edges from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's graph_edge"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own = count_test_graph_edges_for_current_tenant(&mut tx, &[tenant_b_edge_id])
            .await
            .expect("Failed to count Tenant B graph_edges");
        assert_eq!(count_own, 1, "Tenant B should see their own graph_edge");

        let count_other = count_test_graph_edges_for_current_tenant(&mut tx, &[tenant_a_edge_id])
            .await
            .expect("Failed to count Tenant A graph_edges from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's graph_edge"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc6_tenant_isolation_graph_edges PASSED");
}

/// Test: RLC-7 - Tenant isolation on side_effects table.
///
/// Verifies that side_effects rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc7_tenant_isolation_side_effects() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create test data for Tenant A
    let tenant_a_side_effect_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create test intent for Tenant A");

        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create test side_effect for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
    }

    // Create test data for Tenant B
    let tenant_b_side_effect_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create test intent for Tenant B");

        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create test side_effect for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own =
            count_test_side_effects_for_current_tenant(&mut tx, &[tenant_a_side_effect_id])
                .await
                .expect("Failed to count Tenant A side_effects");
        assert_eq!(count_own, 1, "Tenant A should see their own side_effect");

        let count_other =
            count_test_side_effects_for_current_tenant(&mut tx, &[tenant_b_side_effect_id])
                .await
                .expect("Failed to count Tenant B side_effects from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's side_effect"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own =
            count_test_side_effects_for_current_tenant(&mut tx, &[tenant_b_side_effect_id])
                .await
                .expect("Failed to count Tenant B side_effects");
        assert_eq!(count_own, 1, "Tenant B should see their own side_effect");

        let count_other =
            count_test_side_effects_for_current_tenant(&mut tx, &[tenant_a_side_effect_id])
                .await
                .expect("Failed to count Tenant A side_effects from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's side_effect"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc7_tenant_isolation_side_effects PASSED");
}

/// Test: RLC-8 - Tenant isolation on compensation_actions table.
///
/// Verifies that compensation_actions rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc8_tenant_isolation_compensation_actions() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();
    let tenant_a_side_effect_id = Uuid::new_v4();
    let tenant_b_side_effect_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create prerequisites: intents and side_effects for both tenants
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A prerequisites");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create Tenant A intent");
        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create Tenant A side_effect");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A prerequisites");
    }

    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B prerequisites");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create Tenant B intent");
        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create Tenant B side_effect");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B prerequisites");
    }

    // Create test data for Tenant A
    let tenant_a_compensation_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A compensation");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_compensation_action_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_compensation_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create test compensation_action for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A compensation");
    }

    // Create test data for Tenant B
    let tenant_b_compensation_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B compensation");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_compensation_action_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_compensation_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create test compensation_action for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B compensation");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own = count_test_compensation_actions_for_current_tenant(
            &mut tx,
            &[tenant_a_compensation_id],
        )
        .await
        .expect("Failed to count Tenant A compensation_actions");
        assert_eq!(
            count_own, 1,
            "Tenant A should see their own compensation_action"
        );

        let count_other = count_test_compensation_actions_for_current_tenant(
            &mut tx,
            &[tenant_b_compensation_id],
        )
        .await
        .expect("Failed to count Tenant B compensation_actions from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's compensation_action"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own = count_test_compensation_actions_for_current_tenant(
            &mut tx,
            &[tenant_b_compensation_id],
        )
        .await
        .expect("Failed to count Tenant B compensation_actions");
        assert_eq!(
            count_own, 1,
            "Tenant B should see their own compensation_action"
        );

        let count_other = count_test_compensation_actions_for_current_tenant(
            &mut tx,
            &[tenant_a_compensation_id],
        )
        .await
        .expect("Failed to count Tenant A compensation_actions from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's compensation_action"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc8_tenant_isolation_compensation_actions PASSED");
}

/// Test: RLC-9 - Tenant isolation on side_effect_rollback_records table.
///
/// Verifies that side_effect_rollback_records rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc9_tenant_isolation_side_effect_rollback_records() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();
    let tenant_a_side_effect_id = Uuid::new_v4();
    let tenant_b_side_effect_id = Uuid::new_v4();
    let tenant_a_compensation_id = Uuid::new_v4();
    let tenant_b_compensation_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create all prerequisites for both tenants
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A prerequisites");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create Tenant A intent");
        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create Tenant A side_effect");
        create_test_compensation_action_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_compensation_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create Tenant A compensation_action");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A prerequisites");
    }

    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B prerequisites");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create Tenant B intent");
        create_test_side_effect_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create Tenant B side_effect");
        create_test_compensation_action_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_compensation_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create Tenant B compensation_action");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B prerequisites");
    }

    // Create test data for Tenant A
    let tenant_a_rollback_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A rollback");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_side_effect_rollback_record_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_rollback_id,
            tenant_a_compensation_id,
            tenant_a_side_effect_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create test rollback_record for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A rollback");
    }

    // Create test data for Tenant B
    let tenant_b_rollback_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B rollback");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_side_effect_rollback_record_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_rollback_id,
            tenant_b_compensation_id,
            tenant_b_side_effect_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create test rollback_record for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B rollback");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own = count_test_side_effect_rollback_records_for_current_tenant(
            &mut tx,
            &[tenant_a_rollback_id],
        )
        .await
        .expect("Failed to count Tenant A rollback_records");
        assert_eq!(
            count_own, 1,
            "Tenant A should see their own rollback_record"
        );

        let count_other = count_test_side_effect_rollback_records_for_current_tenant(
            &mut tx,
            &[tenant_b_rollback_id],
        )
        .await
        .expect("Failed to count Tenant B rollback_records from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's rollback_record"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own = count_test_side_effect_rollback_records_for_current_tenant(
            &mut tx,
            &[tenant_b_rollback_id],
        )
        .await
        .expect("Failed to count Tenant B rollback_records");
        assert_eq!(
            count_own, 1,
            "Tenant B should see their own rollback_record"
        );

        let count_other = count_test_side_effect_rollback_records_for_current_tenant(
            &mut tx,
            &[tenant_a_rollback_id],
        )
        .await
        .expect("Failed to count Tenant A rollback_records from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's rollback_record"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc9_tenant_isolation_side_effect_rollback_records PASSED");
}

/// Test: RLC-10 - Tenant isolation on policy_snapshot table.
///
/// Verifies that policy_snapshot rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc10_tenant_isolation_policy_snapshot() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create test data for Tenant A
    let tenant_a_snapshot_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        // Create intent (prerequisite for policy_snapshot via FK)
        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create test intent for Tenant A");

        // Create policy_snapshot for Tenant A
        create_test_policy_snapshot_for_current_tenant(
            &mut tx,
            tenant_a_id,
            tenant_a_snapshot_id,
            tenant_a_intent_id,
        )
        .await
        .expect("Failed to create test policy_snapshot for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
    }

    // Create test data for Tenant B
    let tenant_b_snapshot_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        // Create intent for Tenant B
        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create test intent for Tenant B");

        // Create policy_snapshot for Tenant B
        create_test_policy_snapshot_for_current_tenant(
            &mut tx,
            tenant_b_id,
            tenant_b_snapshot_id,
            tenant_b_intent_id,
        )
        .await
        .expect("Failed to create test policy_snapshot for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own =
            count_test_policy_snapshots_for_current_tenant(&mut tx, &[tenant_a_snapshot_id])
                .await
                .expect("Failed to count Tenant A policy_snapshots");
        assert_eq!(
            count_own, 1,
            "Tenant A should see their own policy_snapshot"
        );

        let count_other =
            count_test_policy_snapshots_for_current_tenant(&mut tx, &[tenant_b_snapshot_id])
                .await
                .expect("Failed to count Tenant B policy_snapshots from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's policy_snapshot"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own =
            count_test_policy_snapshots_for_current_tenant(&mut tx, &[tenant_b_snapshot_id])
                .await
                .expect("Failed to count Tenant B policy_snapshots");
        assert_eq!(
            count_own, 1,
            "Tenant B should see their own policy_snapshot"
        );

        let count_other =
            count_test_policy_snapshots_for_current_tenant(&mut tx, &[tenant_a_snapshot_id])
                .await
                .expect("Failed to count Tenant A policy_snapshots from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's policy_snapshot"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc10_tenant_isolation_policy_snapshot PASSED");
}

/// Test: RLC-11 - Deeper checkpoints isolation verification.
///
/// Extends the basic checkpoint isolation with additional verification.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc11_deeper_checkpoints_isolation() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);
    let tenant_a_intent_id = Uuid::new_v4();
    let tenant_b_intent_id = Uuid::new_v4();

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create multiple checkpoints for each tenant
    let tenant_a_checkpoint_ids = [Uuid::new_v4(), Uuid::new_v4()];
    let tenant_b_checkpoint_ids = [Uuid::new_v4(), Uuid::new_v4()];

    // Tenant A checkpoints
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_intent_for_current_tenant(&mut tx, tenant_a_id, tenant_a_intent_id)
            .await
            .expect("Failed to create Tenant A intent");

        for cp_id in &tenant_a_checkpoint_ids {
            create_test_checkpoint_for_current_tenant(
                &mut tx,
                tenant_a_id,
                *cp_id,
                tenant_a_intent_id,
            )
            .await
            .expect("Failed to create Tenant A checkpoint");
        }

        tx.commit()
            .await
            .expect("Failed to commit Tenant A checkpoints");
    }

    // Tenant B checkpoints
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_intent_for_current_tenant(&mut tx, tenant_b_id, tenant_b_intent_id)
            .await
            .expect("Failed to create Tenant B intent");

        for cp_id in &tenant_b_checkpoint_ids {
            create_test_checkpoint_for_current_tenant(
                &mut tx,
                tenant_b_id,
                *cp_id,
                tenant_b_intent_id,
            )
            .await
            .expect("Failed to create Tenant B checkpoint");
        }

        tx.commit()
            .await
            .expect("Failed to commit Tenant B checkpoints");
    }

    // Verify Tenant A sees only their checkpoints
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        // Count all Tenant A checkpoints
        let count_own =
            count_test_checkpoints_for_current_tenant(&mut tx, &tenant_a_checkpoint_ids)
                .await
                .expect("Failed to count Tenant A checkpoints");
        assert_eq!(
            count_own,
            tenant_a_checkpoint_ids.len() as i64,
            "Tenant A should see all their own checkpoints"
        );

        // Count Tenant B checkpoints - should be 0
        let count_other =
            count_test_checkpoints_for_current_tenant(&mut tx, &tenant_b_checkpoint_ids)
                .await
                .expect("Failed to count Tenant B checkpoints from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see any Tenant B checkpoints"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B sees only their checkpoints
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        // Count all Tenant B checkpoints
        let count_own =
            count_test_checkpoints_for_current_tenant(&mut tx, &tenant_b_checkpoint_ids)
                .await
                .expect("Failed to count Tenant B checkpoints");
        assert_eq!(
            count_own,
            tenant_b_checkpoint_ids.len() as i64,
            "Tenant B should see all their own checkpoints"
        );

        // Count Tenant A checkpoints - should be 0
        let count_other =
            count_test_checkpoints_for_current_tenant(&mut tx, &tenant_a_checkpoint_ids)
                .await
                .expect("Failed to count Tenant A checkpoints from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see any Tenant A checkpoints"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc11_deeper_checkpoints_isolation PASSED");
}

/// Test: RLC-12 - Tenant isolation on orchestration_runs table.
///
/// Verifies that orchestration_runs rows are correctly isolated between tenants.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc12_tenant_isolation_orchestration_runs() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&pool)
        .await
        .expect("Failed to ensure migrations");

    // Create test data for Tenant A
    let tenant_a_run_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        create_test_orchestration_run_for_current_tenant(&mut tx, tenant_a_id, tenant_a_run_id)
            .await
            .expect("Failed to create test orchestration_run for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
    }

    // Create test data for Tenant B
    let tenant_b_run_id = Uuid::new_v4();
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        create_test_orchestration_run_for_current_tenant(&mut tx, tenant_b_id, tenant_b_run_id)
            .await
            .expect("Failed to create test orchestration_run for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
    }

    // Verify Tenant A isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own =
            count_test_orchestration_runs_for_current_tenant(&mut tx, &[tenant_a_run_id])
                .await
                .expect("Failed to count Tenant A orchestration_runs");
        assert_eq!(
            count_own, 1,
            "Tenant A should see their own orchestration_run"
        );

        let count_other =
            count_test_orchestration_runs_for_current_tenant(&mut tx, &[tenant_b_run_id])
                .await
                .expect("Failed to count Tenant B orchestration_runs from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should NOT see Tenant B's orchestration_run"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
    }

    // Verify Tenant B isolation
    {
        let mut tx = pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own =
            count_test_orchestration_runs_for_current_tenant(&mut tx, &[tenant_b_run_id])
                .await
                .expect("Failed to count Tenant B orchestration_runs");
        assert_eq!(
            count_own, 1,
            "Tenant B should see their own orchestration_run"
        );

        let count_other =
            count_test_orchestration_runs_for_current_tenant(&mut tx, &[tenant_a_run_id])
                .await
                .expect("Failed to count Tenant A orchestration_runs from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should NOT see Tenant A's orchestration_run"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
    }

    pool.close().await;
    println!("test_rlc12_tenant_isolation_orchestration_runs PASSED");
}

/// Test: RLC-13 - Tenant isolation on forensic_bundles table.
///
/// Verifies that forensic_bundles rows are correctly isolated between tenants.
/// Uses the non-bypass test role pattern from RLC-3 when the admin connection
/// is a superuser/bypass role.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc13_tenant_isolation_forensic_bundles() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    // ===========================================================================
    // Step 1: Connect as admin and ensure migrations
    // ===========================================================================
    let admin_pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&admin_pool)
        .await
        .expect("Failed to ensure migrations");

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

        let test_url = create_test_role(&admin_pool)
            .await
            .expect("Failed to create non-bypass test role - cannot run RLS isolation test");

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

    let tenant_a_bundle_id = Uuid::new_v4();

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

        create_test_forensic_bundle_for_current_tenant(&mut tx, tenant_a_id, tenant_a_bundle_id)
            .await
            .expect("Failed to create test forensic_bundle for Tenant A");

        tx.commit()
            .await
            .expect("Failed to commit Tenant A transaction");
        println!(
            "Tenant A setup complete - created forensic_bundle {}",
            tenant_a_bundle_id
        );
    }

    // ===========================================================================
    // Phase 2: Create test data for Tenant B
    // ===========================================================================
    println!("Setting up Tenant B data...");

    let tenant_b_bundle_id = Uuid::new_v4();

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

        create_test_forensic_bundle_for_current_tenant(&mut tx, tenant_b_id, tenant_b_bundle_id)
            .await
            .expect("Failed to create test forensic_bundle for Tenant B");

        tx.commit()
            .await
            .expect("Failed to commit Tenant B transaction");
        println!(
            "Tenant B setup complete - created forensic_bundle {}",
            tenant_b_bundle_id
        );
    }

    // ===========================================================================
    // Phase 3: Verify Tenant Isolation (using non-bypass role)
    // ===========================================================================
    println!(
        "Verifying tenant isolation under RLS using role '{}'...",
        test_role_name
    );

    // Tenant A context: should see Tenant A's bundle, NOT Tenant B's
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant A verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");

        let count_own =
            count_test_forensic_bundles_for_current_tenant(&mut tx, &[tenant_a_bundle_id])
                .await
                .expect("Failed to count Tenant A forensic_bundles");
        assert_eq!(
            count_own, 1,
            "Tenant A should see exactly 1 forensic_bundle (their own) - RLS isolation may be broken!"
        );

        let count_other =
            count_test_forensic_bundles_for_current_tenant(&mut tx, &[tenant_b_bundle_id])
                .await
                .expect("Failed to count Tenant B forensic_bundles from Tenant A context");
        assert_eq!(
            count_other, 0,
            "Tenant A should see 0 forensic_bundles from Tenant B - RLS isolation may be broken!"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A verification");
        println!(
            "Tenant A isolation verified - only sees bundle {} (own), not Tenant B's {}",
            tenant_a_bundle_id, tenant_b_bundle_id
        );
    }

    // Tenant B context: should see Tenant B's bundle, NOT Tenant A's
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for Tenant B verification");
        let rls_sql = rls_set_tenant_context_sql(tenant_b_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant B");

        let count_own =
            count_test_forensic_bundles_for_current_tenant(&mut tx, &[tenant_b_bundle_id])
                .await
                .expect("Failed to count Tenant B forensic_bundles");
        assert_eq!(
            count_own, 1,
            "Tenant B should see exactly 1 forensic_bundle (their own) - RLS isolation may be broken!"
        );

        let count_other =
            count_test_forensic_bundles_for_current_tenant(&mut tx, &[tenant_a_bundle_id])
                .await
                .expect("Failed to count Tenant A forensic_bundles from Tenant B context");
        assert_eq!(
            count_other, 0,
            "Tenant B should see 0 forensic_bundles from Tenant A - RLS isolation may be broken!"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B verification");
        println!(
            "Tenant B isolation verified - only sees bundle {} (own), not Tenant A's {}",
            tenant_b_bundle_id, tenant_a_bundle_id
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

    println!("test_rlc13_tenant_isolation_forensic_bundles PASSED");
}

/// Test: RLC-14 - Rebase apply graph update with RLS transaction seam.
///
/// This test covers the primary RLS graph update seam by exercising
/// `RlsAwarePool::begin_with_tenant` + `SqlxGraphRepository::update_node_state_with_tx`
/// under tenant context. It validates the bounded D1–D7 primary RLS path for
/// graph mutations, not a post-hoc helper (removed at commit `d98c7dc`).
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc14_rebase_apply_graph_update_with_rls_tx() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    // ===========================================================================
    // Step 1: Connect as admin and ensure migrations
    // ===========================================================================
    let admin_pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&admin_pool)
        .await
        .expect("Failed to ensure migrations");

    // Pre-flight: verify RLS is configured on graph_nodes
    verify_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("RLS not enabled - cannot run graph update seam test");
    verify_force_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("FORCE RLS not enabled - cannot run graph update seam test");

    // ===========================================================================
    // Step 2: Check if we need a non-bypass role for testing
    // ===========================================================================
    let (is_bypass, current_role) = check_current_role_is_bypass(&admin_pool)
        .await
        .expect("Failed to check current role bypass status");

    let test_pool: sqlx::PgPool;
    let _test_role_name: &str;

    if is_bypass {
        println!(
            "WARNING: Current role '{}' is superuser/bypass - RLS policies are bypassed!",
            current_role
        );
        println!("Creating dedicated non-bypass test role for RLS graph update seam test...");

        let test_url = create_test_role(&admin_pool).await.expect(
            "Failed to create non-bypass test role - cannot run RLS graph update seam test",
        );

        test_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(15))
            .connect(&test_url)
            .await
            .expect("Failed to connect with non-bypass test role");
        _test_role_name = TEST_ROLE_NAME;

        println!(
            "Using non-bypass role '{}' for RLS graph update seam test (role '{}' is bypass)",
            TEST_ROLE_NAME, current_role
        );
    } else {
        println!(
            "Using current role '{}' for RLS graph update seam test (not a bypass role)",
            current_role
        );
        test_pool = admin_pool.clone();
        _test_role_name = &current_role;
    }

    let node_id = Uuid::new_v4();

    // ===========================================================================
    // Phase 1: Create a graph node for Tenant A using raw SQL helper
    // ===========================================================================
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for node creation");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");
        create_test_graph_node_for_current_tenant(&mut tx, tenant_a_id, node_id)
            .await
            .expect("Failed to create test graph node for Tenant A");
        tx.commit().await.expect("Failed to commit node creation");
    }

    // ===========================================================================
    // Phase 2: Use RlsAwarePool + SqlxGraphRepository to update node state under RLS
    // ===========================================================================
    let rls_pool = RlsAwarePool::new(test_pool.clone());
    let sql_repo = SqlxGraphRepository::new(test_pool.clone());

    // Tenant A RLS tx: update should succeed
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_a_id)
            .await
            .expect("Failed to begin RLS tx for Tenant A");
        let updated = sql_repo
            .update_node_state_with_tx(&mut tx, node_id, NodeState::Stale)
            .await
            .expect("Tenant A should be able to update its own node via RLS tx");
        assert_eq!(updated.state, NodeState::Stale);
        tx.commit().await.expect("Failed to commit Tenant A RLS tx");
    }

    // ===========================================================================
    // Phase 3: Verify the update persisted
    // ===========================================================================
    {
        let state_str: String =
            sqlx::query_scalar("SELECT state FROM graph_nodes WHERE node_id = $1")
                .bind(node_id)
                .fetch_one(&admin_pool)
                .await
                .expect("Failed to query node state");
        assert_eq!(
            state_str, "stale",
            "Node state should be 'stale' after RLS update"
        );
    }

    // ===========================================================================
    // Phase 4: Tenant B RLS tx: update should fail (row not visible)
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_b_id)
            .await
            .expect("Failed to begin RLS tx for Tenant B");
        let result = sql_repo
            .update_node_state_with_tx(&mut tx, node_id, NodeState::Invalid)
            .await;
        match result {
            Err(IntentRebaseError::GraphNodeNotFound(id)) if id == node_id => {
                // Expected: Tenant B cannot see the node, so update returns NotFound
            }
            other => panic!(
                "Expected GraphNodeNotFound for cross-tenant update, got {:?}",
                other
            ),
        }
        tx.commit().await.expect("Failed to commit Tenant B RLS tx");
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

    println!("test_rlc14_rebase_apply_graph_update_with_rls_tx PASSED");
}

/// Test: D6 — Primary RLS graph update path isolation via GraphUpdater.
///
/// Validates that `GraphUpdater::update_node_state_if_affected_with_tx`
/// succeeds for same-tenant updates and is rejected for cross-tenant updates
/// under RLS enforcement. This is the same seam used by the D4 primary path.
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_d6_primary_rls_graph_update_isolation() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    let admin_pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&admin_pool)
        .await
        .expect("Failed to ensure migrations");

    verify_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("RLS not enabled - cannot run graph update seam test");
    verify_force_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("FORCE RLS not enabled - cannot run graph update seam test");

    let (is_bypass, current_role) = check_current_role_is_bypass(&admin_pool)
        .await
        .expect("Failed to check current role bypass status");

    let test_pool: sqlx::PgPool;
    let _test_role_name: &str;

    if is_bypass {
        let test_url = create_test_role(&admin_pool).await.expect(
            "Failed to create non-bypass test role - cannot run RLS graph update seam test",
        );

        test_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(15))
            .connect(&test_url)
            .await
            .expect("Failed to connect with non-bypass test role");
        _test_role_name = TEST_ROLE_NAME;
    } else {
        test_pool = admin_pool.clone();
        _test_role_name = &current_role;
    }

    let node_id = Uuid::new_v4();

    // Create a graph node for Tenant A
    {
        let mut tx = test_pool
            .begin()
            .await
            .expect("Failed to begin transaction for node creation");
        let rls_sql = rls_set_tenant_context_sql(tenant_a_id);
        sqlx::query(&rls_sql)
            .execute(&mut *tx)
            .await
            .expect("Failed to set RLS context for Tenant A");
        create_test_graph_node_for_current_tenant(&mut tx, tenant_a_id, node_id)
            .await
            .expect("Failed to create test graph node for Tenant A");
        tx.commit().await.expect("Failed to commit node creation");
    }

    let rls_pool = RlsAwarePool::new(test_pool.clone());
    let graph_repo =
        Arc::new(SqlxGraphRepository::new(test_pool.clone())) as Arc<dyn GraphRepository>;
    let graph_service = Arc::new(GraphService::new(graph_repo));
    let graph_updater = GraphUpdater::new(graph_service);

    // Tenant A: same-tenant update should succeed
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_a_id)
            .await
            .expect("Failed to begin RLS tx for Tenant A");
        let result = graph_updater
            .update_node_state_if_affected_with_tx(
                &mut tx,
                node_id,
                NodeState::Stale,
                "Test update".to_string(),
            )
            .await;
        assert!(
            result.is_ok(),
            "Tenant A update should succeed: {:?}",
            result
        );
        let update_result = result.unwrap();
        assert!(update_result.success, "Update should be successful");
        tx.commit().await.expect("Failed to commit Tenant A RLS tx");
    }

    // Verify state persisted
    {
        let state_str: String =
            sqlx::query_scalar("SELECT state FROM graph_nodes WHERE node_id = $1")
                .bind(node_id)
                .fetch_one(&admin_pool)
                .await
                .expect("Failed to query node state");
        assert_eq!(state_str, "stale", "Node state should be 'stale'");
    }

    // Tenant B: cross-tenant update should fail
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_b_id)
            .await
            .expect("Failed to begin RLS tx for Tenant B");
        let result = graph_updater
            .update_node_state_if_affected_with_tx(
                &mut tx,
                node_id,
                NodeState::Invalid,
                "Cross-tenant test".to_string(),
            )
            .await;
        assert!(
            result.is_ok(),
            "Cross-tenant update should return a GraphUpdateResult, not an Err"
        );
        let update_result = result.unwrap();
        assert!(!update_result.success, "Cross-tenant update should fail");
        assert!(
            update_result
                .error
                .unwrap_or_default()
                .contains("not found"),
            "Expected not-found error for cross-tenant update"
        );
        tx.commit().await.expect("Failed to commit Tenant B RLS tx");
    }

    // Cleanup
    if is_bypass {
        drop_test_role(&admin_pool)
            .await
            .expect("Failed to drop test role");
    }

    admin_pool.close().await;
    if is_bypass {
        test_pool.close().await;
    }

    println!("test_d6_primary_rls_graph_update_isolation PASSED");
}

/// Test: Forensic bundle application-level RLS transaction wrapping.
///
/// Validates that `SqlxBundleRepository::_with_tx` methods enforce tenant
/// isolation when executed inside an RLS-aware transaction.
///
/// **Bounded slice scope:**
/// - `create_with_tx` + `get_with_tx` + `list_by_tenant_with_tx`
/// - Uses `RlsAwarePool::begin_with_tenant` to set RLS context
/// - Same-tenant operations succeed; cross-tenant operations are blocked by RLS
#[tokio::test]
#[ignore] // Skip by default; run with `cargo test -- --ignored`
async fn test_rlc_forensic_bundle_app_level_rls() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("{}", SKIP_REASON_NO_DATABASE);
            return;
        }
    };

    let tenant_a_id = parse_test_uuid(TENANT_A_UUID);
    let tenant_b_id = parse_test_uuid(TENANT_B_UUID);

    // ===========================================================================
    // Step 1: Connect as admin and ensure migrations
    // ===========================================================================
    let admin_pool = PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    ensure_migrations(&admin_pool)
        .await
        .expect("Failed to ensure migrations");

    verify_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("RLS not enabled - cannot run forensic bundle RLS test");
    verify_force_rls_enabled_on_tables(&admin_pool)
        .await
        .expect("FORCE RLS not enabled - cannot run forensic bundle RLS test");

    // ===========================================================================
    // Step 2: Check if we need a non-bypass role for testing
    // ===========================================================================
    let (is_bypass, current_role) = check_current_role_is_bypass(&admin_pool)
        .await
        .expect("Failed to check current role bypass status");

    let test_pool: sqlx::PgPool;
    let _test_role_name: &str;

    if is_bypass {
        println!(
            "WARNING: Current role '{}' is superuser/bypass - RLS policies are bypassed!",
            current_role
        );
        println!("Creating dedicated non-bypass test role for forensic bundle RLS test...");

        let test_url = create_test_role(&admin_pool)
            .await
            .expect("Failed to create non-bypass test role - cannot run forensic bundle RLS test");

        test_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(15))
            .connect(&test_url)
            .await
            .expect("Failed to connect with non-bypass test role");
        _test_role_name = TEST_ROLE_NAME;

        println!(
            "Using non-bypass role '{}' for forensic bundle RLS test (role '{}' is bypass)",
            TEST_ROLE_NAME, current_role
        );
    } else {
        println!(
            "Using current role '{}' for forensic bundle RLS test (not a bypass role)",
            current_role
        );
        test_pool = admin_pool.clone();
        _test_role_name = &current_role;
    }

    let rls_pool = RlsAwarePool::new(test_pool.clone());
    let sql_repo = forensic_service::SqlxBundleRepository::new(test_pool.clone());

    let bundle_id = Uuid::new_v4();

    // ===========================================================================
    // Phase 1: Create a forensic bundle for Tenant A via RLS tx
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_a_id)
            .await
            .expect("Failed to begin RLS tx for Tenant A");

        let bundle = forensic_service::ForensicBundle::new(
            tenant_a_id,
            forensic_service::BundleTimeRange {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            forensic_service::BundlePurpose::IncidentInvestigation,
            forensic_service::BundleContents::default(),
            "rls-test",
            None,
        );
        // Override bundle_id for deterministic testing
        let bundle = forensic_service::ForensicBundle {
            bundle_id,
            ..bundle
        };

        sql_repo
            .create_with_tx(&mut tx, bundle)
            .await
            .expect("Tenant A should be able to create bundle via RLS tx");

        tx.commit().await.expect("Failed to commit Tenant A RLS tx");
        println!(
            "Created forensic bundle {} for Tenant A via RLS tx",
            bundle_id
        );
    }

    // ===========================================================================
    // Phase 2: Tenant A can see their bundle via RLS list
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_a_id)
            .await
            .expect("Failed to begin RLS tx for Tenant A list");

        let bundles = sql_repo
            .list_by_tenant_with_tx(&mut tx, tenant_a_id, None)
            .await
            .expect("Tenant A should be able to list bundles via RLS tx");

        assert!(
            bundles.iter().any(|b| b.bundle_id == bundle_id),
            "Tenant A should see their bundle in RLS list"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant A list RLS tx");
        println!("Tenant A can see bundle {} via RLS list", bundle_id);
    }

    // ===========================================================================
    // Phase 3: Tenant A can get their bundle via RLS get
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_a_id)
            .await
            .expect("Failed to begin RLS tx for Tenant A get");

        let bundle = sql_repo
            .get_with_tx(&mut tx, bundle_id)
            .await
            .expect("Tenant A should be able to get their bundle via RLS tx");
        assert_eq!(bundle.bundle_id, bundle_id);
        assert_eq!(bundle.tenant_id, tenant_a_id);

        tx.commit()
            .await
            .expect("Failed to commit Tenant A get RLS tx");
        println!("Tenant A can get bundle {} via RLS get", bundle_id);
    }

    // ===========================================================================
    // Phase 4: Tenant B cannot see Tenant A's bundle via RLS list
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_b_id)
            .await
            .expect("Failed to begin RLS tx for Tenant B list");

        let bundles = sql_repo
            .list_by_tenant_with_tx(&mut tx, tenant_b_id, None)
            .await
            .expect("Tenant B should be able to list bundles via RLS tx");

        assert!(
            !bundles.iter().any(|b| b.bundle_id == bundle_id),
            "Tenant B should NOT see Tenant A's bundle in RLS list"
        );

        tx.commit()
            .await
            .expect("Failed to commit Tenant B list RLS tx");
        println!(
            "Tenant B cannot see bundle {} via RLS list (PASS)",
            bundle_id
        );
    }

    // ===========================================================================
    // Phase 5: Tenant B cannot get Tenant A's bundle via RLS get
    // ===========================================================================
    {
        let mut tx = rls_pool
            .begin_with_tenant(tenant_b_id)
            .await
            .expect("Failed to begin RLS tx for Tenant B get");

        let result = sql_repo.get_with_tx(&mut tx, bundle_id).await;
        match result {
            Err(IntentRebaseError::ForensicBundleNotFound(id)) if id == bundle_id => {
                // Expected: Tenant B cannot see the bundle, so get returns NotFound
            }
            other => panic!(
                "Expected ForensicBundleNotFound for cross-tenant get, got {:?}",
                other
            ),
        }

        tx.commit()
            .await
            .expect("Failed to commit Tenant B get RLS tx");
        println!(
            "Tenant B cannot get bundle {} via RLS get (PASS)",
            bundle_id
        );
    }

    // ===========================================================================
    // Cleanup
    // ===========================================================================
    if is_bypass {
        drop_test_role(&admin_pool)
            .await
            .expect("Failed to drop test role");
    }

    admin_pool.close().await;
    if is_bypass {
        test_pool.close().await;
    }

    println!("test_rlc_forensic_bundle_app_level_rls PASSED");
}
