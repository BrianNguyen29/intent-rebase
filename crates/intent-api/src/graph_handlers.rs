//! Graph handlers (Phase 1 - Internal CRUD only)
//!
//! Extracted from lib.rs as a bounded handler decomposition slice.

#[cfg(feature = "jwt-auth")]
use crate::auth;
use crate::{ApiErrorResponse, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
#[allow(unused_imports)]
use intent_rebase_types::IntentRebaseError;
use intent_rebase_types::{CreateGraphEdgeRequest, CreateGraphNodeRequest, GraphEdge, GraphNode};
use uuid::Uuid;

/// POST /v1/graph/nodes - Create a new graph node
///
/// Phase 3 P3-S5 bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// When jwt-auth feature is disabled, this handler uses the non-RLS path only.
#[cfg(feature = "jwt-auth")]
pub async fn create_graph_node(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<CreateGraphNodeRequest>,
) -> Result<(StatusCode, Json<GraphNode>), ApiErrorResponse> {
    // Check if RLS path is available (pool exists AND JWT claims present)
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: JWT tenant must match request tenant
        if rls_claims.tenant_id != request.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("create_graph_node: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and create node within the transaction
        if let Some(sql_repo) = state.graph_service.repo().as_sqlx_repo() {
            let node_result = sql_repo.create_node_with_tx(&mut tx, request).await;
            let node = match node_result {
                Ok(node) => node,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS node creation failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "create_graph_node: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );
            return Ok((StatusCode::CREATED, Json(node)));
        } else {
            // Fallback to non-RLS if repo doesn't support SQL
            tracing::warn!(
                "create_graph_node: rls_pool set but repo doesn't support SQL, falling back"
            );
        }
    }

    // Non-RLS path (no JWT claims or rls_pool is None)
    state
        .graph_service
        .add_node(request)
        .await
        .map(|node| (StatusCode::CREATED, Json(node)))
        .map_err(ApiErrorResponse)
}

/// POST /v1/graph/nodes - Create a new graph node (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_graph_node(
    State(state): State<AppState>,
    Json(request): Json<CreateGraphNodeRequest>,
) -> Result<(StatusCode, Json<GraphNode>), ApiErrorResponse> {
    state
        .graph_service
        .add_node(request)
        .await
        .map(|node| (StatusCode::CREATED, Json(node)))
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes - List graph nodes with optional filters
pub async fn list_graph_nodes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<super::ListGraphNodesQuery>,
) -> Result<Json<Vec<GraphNode>>, ApiErrorResponse> {
    use intent_rebase_types::GraphNodeFilter;

    let filter = GraphNodeFilter {
        tenant_id: query.tenant_id,
        workflow_id: query.workflow_id,
        node_type: query.node_type,
        state: None,
    };

    state
        .graph_service
        .list_nodes(filter)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes/{node_id} - Get a single graph node by ID
pub async fn get_graph_node(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<GraphNode>, ApiErrorResponse> {
    state
        .graph_service
        .get_node(node_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// POST /v1/graph/edges - Create a new graph edge
///
/// Phase 1 P1-S4 bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// When jwt-auth feature is disabled, this handler uses the non-RLS path only.
#[cfg(feature = "jwt-auth")]
pub async fn create_graph_edge(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<CreateGraphEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdge>), ApiErrorResponse> {
    // Check if RLS path is available (pool exists AND JWT claims present)
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Tenant mismatch rejection: JWT tenant must match request tenant
        if rls_claims.tenant_id != request.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("create_graph_edge: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and create edge within the transaction
        if let Some(sql_repo) = state.graph_service.repo().as_sqlx_repo() {
            let edge_result = sql_repo.create_edge_with_tx(&mut tx, request).await;
            let edge = match edge_result {
                Ok(edge) => edge,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS edge creation failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "create_graph_edge: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );
            return Ok((StatusCode::CREATED, Json(edge)));
        } else {
            // Fallback to non-RLS if repo doesn't support SQL
            tracing::warn!(
                "create_graph_edge: rls_pool set but repo doesn't support SQL, falling back"
            );
        }
    }

    // Non-RLS path (no JWT claims or rls_pool is None)
    state
        .graph_service
        .add_edge(request)
        .await
        .map(|edge| (StatusCode::CREATED, Json(edge)))
        .map_err(ApiErrorResponse)
}

/// POST /v1/graph/edges - Create a new graph edge (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_graph_edge(
    State(state): State<AppState>,
    Json(request): Json<CreateGraphEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdge>), ApiErrorResponse> {
    state
        .graph_service
        .add_edge(request)
        .await
        .map(|edge| (StatusCode::CREATED, Json(edge)))
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/edges - List graph edges with optional filters
pub async fn list_graph_edges(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<super::ListGraphEdgesQuery>,
) -> Result<Json<Vec<GraphEdge>>, ApiErrorResponse> {
    use intent_rebase_types::GraphEdgeFilter;

    let filter = GraphEdgeFilter {
        tenant_id: query.tenant_id,
        workflow_id: query.workflow_id,
        from_node_id: query.from_node_id,
        to_node_id: None,
        edge_type: query.edge_type,
    };

    state
        .graph_service
        .list_edges(filter)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes/{node_id}/edges - List edges outgoing from a node
pub async fn list_edges_from_node(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Vec<GraphEdge>>, ApiErrorResponse> {
    state
        .graph_service
        .list_edges_from(node_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}
