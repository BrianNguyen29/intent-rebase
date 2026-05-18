use axum::{routing::get, Router};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        .route("/health", get(crate::health_routes::health_handler))
        .route("/ready", get(crate::health_routes::ready_handler))
        .route("/metrics", get(crate::health_routes::metrics_handler))
}
