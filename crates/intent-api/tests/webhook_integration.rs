//! Webhook integration test (S4 — SQLx-backed outbox pipeline with in-process HTTP receiver)
//!
//! Bounded non-production test: exercises the full SQLx outbox → subscription
//! resolver → real HTTP receiver → DB status verification path against a live
//! Postgres instance from docker-compose.
//!
//! ## Running
//!
//! ```bash
//! # Requires local Postgres (docker-compose)
//! docker compose -f infrastructure/local/docker-compose.yml up -d postgres
//!
//! # Run the ignored test
//! DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix \
//!   cargo test -p intent-api --test webhook_integration -- --ignored
//! ```
//!
//! ## Prerequisites
//!
//! - Postgres 16+ running with migrations applied (including 018, 019, 020, 021)
//! - `DATABASE_URL` set to a pre-migrated database
//!
//! ## What this test validates
//!
//! - `SqlxWebhookSubscriptionRepository` can create and persist subscriptions
//! - `SqlxWebhookOutboxRepository` can create and persist pending outbox records
//! - `WebhookOutboxWorkerImpl::process_once` claims, dispatches, and marks delivered
//! - `WebhookDeliveryDispatcher` sends a real HTTP POST via `reqwest::Client`
//! - The in-process HTTP receiver receives the webhook payload with correct shape
//! - DB outbox status transitions from `Pending` → `Claimed` → `Delivered`
//!
//! ## Caveats
//!
//! - Local-dev / docker-compose evidence only; not production-ready validation
//! - No RLS tenant isolation verification (see `rls_integration.rs` for that)
//! - No HMAC signing verification (see `webhook_delivery_tests.rs` for that)
//! - One-shot processing; no retry, DLQ, or background loop coverage

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::{routing::post, Router};
use http::StatusCode;
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use intent_api::webhook_delivery::build_webhook_client;
use intent_api::webhook_dispatcher::WebhookDeliveryDispatcher;
use intent_api::webhook_outbox_repo::{
    SqlxWebhookOutboxRepository, WebhookOutboxRecord, WebhookOutboxRepository, WebhookOutboxStatus,
};
use intent_api::webhook_outbox_worker::{WebhookOutboxWorker, WebhookOutboxWorkerImpl};
use intent_api::webhook_subscription_repo::{
    SqlxWebhookSubscriptionRepository, WebhookSubscriptionRecord, WebhookSubscriptionRepository,
};

// =============================================================================
// In-process HTTP receiver
// =============================================================================

#[derive(Clone)]
struct CaptureState {
    requests: Arc<Mutex<Vec<(Value, http::HeaderMap)>>>,
}

async fn webhook_receiver(
    State(state): State<CaptureState>,
    headers: http::HeaderMap,
    axum::extract::Json(body): axum::extract::Json<Value>,
) -> StatusCode {
    state.requests.lock().await.push((body, headers));
    StatusCode::OK
}

// =============================================================================
// Helpers
// =============================================================================

fn get_database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

/// Bind an ephemeral TCP port and return the address.
async fn start_http_receiver(capture: CaptureState) -> SocketAddr {
    let app = Router::new()
        .route("/webhook", post(webhook_receiver))
        .with_state(capture);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind ephemeral port for HTTP receiver");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start accepting
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    addr
}

// =============================================================================
// Integration Test
// =============================================================================

#[tokio::test]
#[ignore = "requires live Postgres (set DATABASE_URL to run)"]
async fn test_webhook_sqlx_outbox_pipeline_success() {
    let database_url = match get_database_url() {
        Some(url) => url,
        None => {
            eprintln!("Skipping webhook integration test: DATABASE_URL not set");
            eprintln!("Set DATABASE_URL to run this test locally.");
            return;
        }
    };

    // Connect to live Postgres
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to Postgres — check DATABASE_URL and postgres service");

    // Start in-process HTTP receiver
    let capture = CaptureState {
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let addr = start_http_receiver(capture.clone()).await;
    let webhook_url = format!("http://{}/webhook", addr);

    // Repositories under test
    let outbox_repo = Arc::new(SqlxWebhookOutboxRepository::new(pool.clone()));
    let subscription_repo = SqlxWebhookSubscriptionRepository::new(pool.clone());

    // Seed a matching subscription + outbox record
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    let subscription = WebhookSubscriptionRecord::new(
        tenant_id,
        intent_id,
        subscription_id,
        webhook_url.clone(),
        Some("test-system".to_string()),
        vec!["intent_changed".to_string()],
    );
    let subscription = subscription_repo
        .create(subscription)
        .await
        .expect("Failed to create subscription");

    let outbox_record = WebhookOutboxRecord::new(
        tenant_id,
        intent_id,
        subscription_id,
        "intent_changed".to_string(),
        serde_json::json!({"test": "payload"}),
        Some(webhook_url),
    );
    let outbox_record = outbox_repo
        .create(outbox_record)
        .await
        .expect("Failed to create outbox record");

    // Ensure worker env gate is enabled for this test
    let original_worker_env = std::env::var("INTENT_API_WEBHOOK_OUTBOX_WORKER").ok();
    std::env::set_var("INTENT_API_WEBHOOK_OUTBOX_WORKER", "true");

    // Build dispatcher + worker
    let client = build_webhook_client();
    let dispatcher = Arc::new(WebhookDeliveryDispatcher::new(Arc::new(client)));
    let worker = WebhookOutboxWorkerImpl::new(outbox_repo.clone(), dispatcher);

    // Process the pending outbox record
    let processed = worker
        .process_once(tenant_id, 10)
        .await
        .expect("Worker process_once failed");
    assert_eq!(processed, 1, "Expected exactly 1 record to be processed");

    // Verify DB outbox status transition: Pending → Claimed → Delivered
    let fetched = outbox_repo
        .get(outbox_record.id, tenant_id)
        .await
        .expect("Failed to fetch outbox record after processing");
    assert_eq!(
        fetched.status,
        WebhookOutboxStatus::Delivered,
        "Outbox record should be marked as delivered"
    );
    assert!(
        fetched.delivered_at.is_some(),
        "delivered_at should be set on successful delivery"
    );
    assert_eq!(
        fetched.lock_version, 2,
        "lock_version should be 2 after claim + delivered"
    );

    // Verify HTTP POST was received with correct payload shape
    let requests = capture.requests.lock().await;
    assert_eq!(
        requests.len(),
        1,
        "Expected exactly 1 HTTP request to the in-process receiver"
    );
    let (body, headers) = &requests[0];
    assert_eq!(
        body["event_type"], "intent_changed",
        "Payload event_type mismatch"
    );
    assert_eq!(
        body["intent_id"].as_str().unwrap(),
        intent_id.to_string(),
        "Payload intent_id mismatch"
    );
    assert_eq!(
        body["tenant_id"].as_str().unwrap(),
        tenant_id.to_string(),
        "Payload tenant_id mismatch"
    );
    assert_eq!(
        body["subscription_id"].as_str().unwrap(),
        subscription_id.to_string(),
        "Payload subscription_id mismatch"
    );
    assert_eq!(
        body["delivery_id"].as_str().unwrap(),
        outbox_record.id.to_string(),
        "Payload delivery_id should match outbox record id"
    );
    assert_eq!(
        body["attempt_number"], 1,
        "attempt_number should be 1 on first delivery"
    );
    assert!(
        headers.get("X-Idempotency-Key").is_some(),
        "X-Idempotency-Key header should be present"
    );
    assert!(
        headers.get("Content-Type").is_some(),
        "Content-Type header should be present"
    );

    // Restore env var
    match original_worker_env {
        Some(v) => std::env::set_var("INTENT_API_WEBHOOK_OUTBOX_WORKER", v),
        None => std::env::remove_var("INTENT_API_WEBHOOK_OUTBOX_WORKER"),
    }

    // Best-effort cleanup of seeded rows
    let _ = sqlx::query("DELETE FROM webhook_outbox WHERE id = $1")
        .bind(outbox_record.id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM webhook_subscriptions WHERE id = $1")
        .bind(subscription.id)
        .execute(&pool)
        .await;

    pool.close().await;
    println!("test_webhook_sqlx_outbox_pipeline_success PASSED");
}
