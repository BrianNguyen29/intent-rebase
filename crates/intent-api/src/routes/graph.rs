use axum::{
    routing::{get, post},
    Router,
};

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route(
            "/v1/graph/nodes",
            post(crate::graph_handlers::create_graph_node),
        )
        .route(
            "/v1/graph/nodes",
            get(crate::graph_handlers::list_graph_nodes),
        )
        .route(
            "/v1/graph/nodes/:node_id",
            get(crate::graph_handlers::get_graph_node),
        )
        .route(
            "/v1/graph/edges",
            post(crate::graph_handlers::create_graph_edge),
        )
        .route(
            "/v1/graph/edges",
            get(crate::graph_handlers::list_graph_edges),
        )
        .route(
            "/v1/graph/nodes/:node_id/edges",
            get(crate::graph_handlers::list_edges_from_node),
        )
        // Artifact ingest with optional side effect capture (Phase 3 Batch 1 groundwork)
        .route(
            "/v1/graph/artifacts",
            post(crate::ingest_handlers::ingest_artifact),
        )
}
