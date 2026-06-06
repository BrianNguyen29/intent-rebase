use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Propagation status endpoint (Slice 1 bounded, Slice 2 record-backed)
        .route(
            "/intents/:intent_id/propagation-status",
            get(crate::propagation_handlers::get_propagation_status),
        )
        // Propagation signal ingestion endpoint (Slice 2 bounded)
        .route(
            "/intents/:intent_id/propagation-signals",
            post(crate::propagation_handlers::ingest_propagation_signal),
        )
}
