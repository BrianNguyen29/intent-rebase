use axum::{
    routing::{delete, get, patch, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Webhook subscription CRUD endpoints (Slice 4b — bounded local-dev subscription CRUD)
        .route(
            "/webhooks/subscriptions",
            post(crate::webhook_subscription_handlers::create_subscription),
        )
        .route(
            "/webhooks/subscriptions",
            get(crate::webhook_subscription_handlers::list_subscriptions),
        )
        .route(
            "/webhooks/subscriptions/:id",
            get(crate::webhook_subscription_handlers::get_subscription),
        )
        .route(
            "/webhooks/subscriptions/:id",
            patch(crate::webhook_subscription_handlers::update_subscription),
        )
        .route(
            "/webhooks/subscriptions/:id",
            delete(crate::webhook_subscription_handlers::delete_subscription),
        )
        // Webhook outbox DLQ endpoints (Slice 5b — bounded local-dev failed-status DLQ)
        .route(
            "/webhooks/outbox/dlq",
            get(crate::webhook_outbox_dlq_handlers::list_dlq),
        )
        .route(
            "/webhooks/outbox/dlq/:id/replay",
            post(crate::webhook_outbox_dlq_handlers::replay_dlq),
        )
        // Webhook outbox replayed audit query endpoint (Phase 1.3 — bounded local-dev)
        .route(
            "/webhooks/outbox/dlq/replayed",
            get(crate::webhook_outbox_dlq_handlers::list_replayed),
        )
        // Webhook outbox bulk replay endpoint (Phase 2.2 — bounded local-dev)
        .route(
            "/webhooks/outbox/dlq/bulk-replay",
            post(crate::webhook_outbox_dlq_handlers::bulk_replay_dlq),
        )
        // Webhook outbox DLQ stats endpoint (Phase 2.3 — bounded local-dev)
        .route(
            "/webhooks/outbox/dlq/stats",
            get(crate::webhook_outbox_dlq_handlers::dlq_stats),
        )
}
