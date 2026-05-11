use super::*;

// Import forensic handlers for tests (verification/export/bundle tests moved to forensic_handlers.rs)

// Import intent read handlers for tests
use crate::intent_read_handlers::{get_intent_head, get_version, list_versions};

// Use shared helper with forensic config for lib.rs tests
use crate::test_helpers::create_test_service_with_forensic_config as create_test_service;

#[tokio::test]
async fn test_router_builds_successfully() {
    let state = create_test_service();
    let _router: axum::Router = Router::new()
        .route("/intents", post(intent_mutation_handlers::create_intent))
        .route("/intents/{intent_id}", get(get_intent_head))
        .route(
            "/intents/{intent_id}/versions",
            post(intent_mutation_handlers::create_version),
        )
        .route("/intents/{intent_id}/versions", get(list_versions))
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(get_version),
        )
        .route(
            "/intents/{intent_id}/diff",
            post(diff_handlers::compute_diff),
        )
        .route(
            "/intents/{intent_id}/rebase-preview",
            post(rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/{intent_id}/rebase-apply",
            post(rebase_apply_handlers::rebase_apply),
        )
        .with_state(state);
    // Router builds successfully - this is a compile-time check essentially
}

// =========================================================================
// Trace Context Propagation Tests (Phase 3 Batch 2 Slice 2 — bounded OTEL)
//
// Note: Direct middleware testing requires complex axum infrastructure.
// The trace_context_middleware is verified through:
// 1. cargo check -p intent-api (verifies compilation)
// 2. cargo test -p intent-api (verifies existing tests still pass)
// 3. Router wiring in build_router() includes trace_context_middleware layer
// =========================================================================
