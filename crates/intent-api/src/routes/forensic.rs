use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Forensic verification endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/verify",
            post(crate::forensic_handlers::verify_forensic_bundle),
        )
        // Forensic archive export endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/export",
            post(crate::forensic_handlers::export_forensic_archive),
        )
        // Forensic bundle generation endpoint (P4 bounded slice)
        .route(
            "/forensic/bundle",
            post(crate::forensic_handlers::create_forensic_bundle),
        )
        // Forensic bundle listing endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles",
            get(crate::forensic_handlers::list_forensic_bundles),
        )
        // Forensic bundle download endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles/:bundle_id/download",
            get(crate::forensic_handlers::download_forensic_bundle),
        )
        // Forensic bundle replay verification endpoint (bounded replay evidence slice)
        .route(
            "/forensic/bundles/:bundle_id/replay-verify",
            post(crate::forensic_handlers::replay_verify_forensic_bundle),
        )
}
