use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        .route(
            "/v1/intents/validate",
            post(crate::intent_validation_handlers::validate_intent),
        )
        .route(
            "/intents",
            post(crate::intent_mutation_handlers::create_intent),
        )
        .route(
            "/intents/:intent_id",
            get(crate::intent_read_handlers::get_intent_head),
        )
        .route(
            "/intents/:intent_id/versions",
            post(crate::intent_mutation_handlers::create_version),
        )
        .route(
            "/intents/:intent_id/versions",
            get(crate::intent_read_handlers::list_versions),
        )
        .route(
            "/intents/:intent_id/versions/:version_number",
            get(crate::intent_read_handlers::get_version),
        )
        .route(
            "/intents/:intent_id/diff",
            post(crate::diff_handlers::compute_diff),
        )
        .route(
            "/intents/:intent_id/rebase-preview",
            post(crate::rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/:intent_id/rebase-apply",
            post(crate::rebase_apply_handlers::rebase_apply),
        )
        // Replay endpoint (Phase 2b bounded replay slice)
        .route(
            "/intents/:intent_id/replay",
            post(crate::replay_handlers::replay_intent),
        )
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route(
            "/intents/:intent_id/side-effects",
            get(crate::query_handlers::list_side_effects),
        )
        // N4-4: Rebase simulation endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/intents/:intent_id/rebase-simulation",
            get(crate::simulation_handlers::rebase_simulation),
        )
        // N4-4 POST: Compensation simulation run endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/compensation-simulation/run",
            post(crate::simulation_handlers::compensation_simulation_run),
        )
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/:intent_id/orchestration-dashboard",
            get(crate::query_handlers::get_orchestration_dashboard),
        )
        // ImpactReport endpoint (Phase 2 bounded MVP — on-demand read-only projection)
        .route(
            "/intents/:intent_id/impact-report",
            get(crate::query_handlers::get_impact_report),
        )
}
