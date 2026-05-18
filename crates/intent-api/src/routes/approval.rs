use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Approval request endpoints (Phase 2b bounded slice)
        .route(
            "/approval-requests/pending",
            get(crate::approval_handlers_readonly::list_pending_approval_requests),
        )
        .route(
            "/approval-requests/:approval_request_id/approve",
            post(crate::approval_mutation_handlers::approve_approval_request),
        )
        .route(
            "/approval-requests/:approval_request_id/reject",
            post(crate::approval_mutation_handlers::reject_approval_request),
        )
        // POST expire - bounded manual expiry transition (Phase 2b)
        .route(
            "/approval-requests/:approval_request_id/expire",
            post(crate::approval_mutation_handlers::expire_approval_request),
        )
        // GET revalidate - bounded read-only scope comparison (Phase 2b)
        .route(
            "/approval-requests/:approval_request_id/revalidate",
            get(crate::approval_handlers_readonly::revalidate_approval_request),
        )
        // ADR-07: POST trigger-reapproval - bounded re-approval trigger (Phase 2b)
        .route(
            "/approval-requests/trigger-reapproval",
            post(crate::trigger_reapproval_handlers::trigger_reapproval),
        )
}
