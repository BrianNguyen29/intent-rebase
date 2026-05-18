use axum::{routing::get, Router};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Policy snapshot endpoints (Phase 2 bounded read-only slice)
        .route(
            "/policy-snapshots/:snapshot_id",
            get(crate::policy_snapshot_handlers::get_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/:intent_id/latest",
            get(crate::policy_snapshot_handlers::get_latest_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/:intent_id/versions/:version",
            get(crate::policy_snapshot_handlers::get_policy_snapshot_by_version),
        )
        .route(
            "/policy-snapshots/intent/:intent_id",
            get(crate::policy_snapshot_handlers::list_policy_snapshots),
        )
        // Policy snapshot impact report endpoint (ADR-11 bounded MVP)
        .route(
            "/policy-snapshots/:snapshot_id/impact-report",
            get(crate::policy_snapshot_handlers::get_policy_snapshot_impact_report),
        )
}
