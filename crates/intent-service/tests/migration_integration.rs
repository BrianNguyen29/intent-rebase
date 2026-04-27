//! Migration integration test - validates migrations run against live Postgres.
//!
//! This test is skipped when DATABASE_URL is not set, allowing local development
//! without a database while still running in CI with the postgres service.
//!
//! Expected DATABASE_URL: postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase

use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Key tables that must exist after migrations run.
/// These are selected from the current migration set as stable, high-value validation targets.
const KEY_TABLES: &[&str] = &["intents", "intent_versions", "audit_events"];

#[tokio::test]
#[ignore] // Skip by default; CI runs with --ignored or DATABASE_URL set
async fn test_migrations_run_successfully() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            eprintln!("DATABASE_URL not set - skipping migration integration test");
            eprintln!(
                "Set DATABASE_URL to run this test locally, or rely on CI with postgres service"
            );
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

    // Run migrations from the infrastructure/migrations directory.
    // sqlx::migrate! expects a migrations folder relative to the crate root.
    // The path ../../infrastructure/migrations is valid from crates/intent-service/.
    sqlx::migrate!("../../infrastructure/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Validate that key tables exist
    for table_name in KEY_TABLES {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = $1)",
        )
        .bind(table_name)
        .fetch_one(&pool)
        .await
        .expect("Failed to check table existence");

        assert!(
            exists,
            "Key table '{}' does not exist after migrations - migration may have failed or table was not created",
            table_name
        );
    }

    // Additional validation: verify intents table has expected columns
    let column_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'intents'",
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to count columns in intents table");

    // The intents table should have at least 10 columns based on 001_create_intents.sql
    assert!(
        column_count >= 10,
        "intents table has fewer columns than expected (expected >= 10, got {})",
        column_count
    );

    // Verify audit_events table has the audit_event_type enum
    let enum_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT FROM pg_type WHERE typname = 'audit_event_type')",
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to check enum existence");

    assert!(
        enum_exists,
        "audit_event_type enum does not exist - migration 006 may have failed"
    );

    pool.close().await;
    println!(
        "Migration integration test passed - all {} key tables exist",
        KEY_TABLES.len()
    );
}
