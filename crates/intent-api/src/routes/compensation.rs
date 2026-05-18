use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Compensation actions query endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/:intent_id/compensation-actions",
            get(crate::compensation_query_handlers::list_compensation_actions),
        )
        // Compensation action mutation endpoints (Phase 3 Batch 1 bounded execution slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/:action_id/approve",
            post(crate::compensation_mutation_handlers::approve_compensation_action),
        )
        .route(
            "/compensation-actions/:action_id/waive",
            post(crate::compensation_mutation_handlers::waive_compensation_action),
        )
        .route(
            "/compensation-actions/:action_id/execute",
            post(crate::compensation_mutation_handlers::execute_compensation_action),
        )
        // Compensation action manual retry and DLQ endpoints (Phase 3 Batch 1 bounded manual retry slice)
        .route(
            "/compensation-actions/:action_id/reapprove",
            post(crate::compensation_mutation_handlers::reapprove_compensation_action),
        )
        // Bounded compensation planner endpoint (Phase 3 bounded planner slice)
        .route(
            "/compensation-actions/plan",
            post(crate::compensation_planner_handlers::plan_compensation_actions),
        )
        .route(
            "/compensation-actions/dlq",
            get(crate::compensation_query_handlers::list_dlq_candidates),
        )
        // Batch candidates query endpoint (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
        .route(
            "/compensation-actions/batch-candidates",
            get(crate::compensation_query_handlers::list_batch_candidates),
        )
        // Policy gate evaluation endpoints (Phase 3 Batch 1 bounded read-only slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/policy-gate",
            get(crate::compensation_query_handlers::get_compensation_policy_gate),
        )
        .route(
            "/intents/:intent_id/compensation-policy-gate",
            get(crate::compensation_query_handlers::get_intent_compensation_policy_gate),
        )
        // Orchestration coordination status endpoints (Phase 3 Batch 1 bounded read-only orchestration view)
        .route(
            "/compensation-actions/orchestration-coordination",
            get(crate::compensation_query_handlers::get_orchestration_coordination),
        )
        .route(
            "/intents/:intent_id/orchestration-coordination",
            get(crate::compensation_query_handlers::get_intent_orchestration_coordination),
        )
        // Manual orchestration & dry-run planner endpoints (Phase 3 Batch 1 bounded slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/orchestration-dry-run",
            post(crate::compensation_planner_handlers::orchestration_dry_run),
        )
        .route(
            "/compensation-actions/batch-approve",
            post(crate::batch_handlers::batch_approve_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-reapprove",
            post(crate::batch_handlers::batch_reapprove_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-execute",
            post(crate::batch_handlers::batch_execute_compensation_actions),
        )
        // Orchestration run endpoints (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
        .route(
            "/compensation-actions/runs",
            post(crate::orchestration_run_handlers::create_orchestration_run),
        )
        .route(
            "/compensation-actions/runs/:run_id",
            get(crate::orchestration_run_handlers::get_orchestration_run),
        )
}
