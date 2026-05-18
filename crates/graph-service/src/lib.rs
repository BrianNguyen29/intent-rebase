//! Graph Service — manages dependency graph state
//!
//! Phase 1 PR #10: Graph traversal baseline (BFS, path-finding, cycle detection) with in-memory repository.
//! Phase 1 PR #11: Graph ingestors baseline for artifact, approval, and side-effect nodes.
//! Phase 2b: Edge re-evaluation and orphan detection bounded slices.
//! Provides persisted graph nodes/edges CRUD for future traversal/classification work.
//!
//! Architecture: Repository trait allows swapping to SQL-backed implementation.
//! See: docs/03-spec/03-dependency-graph.md (storage strategy)

pub mod edge_reevaluation;
pub mod in_memory;
pub mod sqlx_repository;
pub mod traversal;

mod classification;
mod ingestion;
mod type_mappings;

pub use in_memory::{GraphState, InMemoryGraphRepository};
pub use sqlx_repository::SqlxGraphRepository;

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ClassificationResult, ClassifyRequest, CreateGraphEdgeRequest, CreateGraphNodeRequest,
    GraphEdge, GraphEdgeFilter, GraphNode, GraphNodeFilter, IntentRebaseError, NodeState, NodeType,
};
#[allow(unused_imports)]
use intent_rebase_types::{CycleDetectionResult, GraphPath, ReachabilityResult, TraversalOptions};
use std::sync::Arc;
use uuid::Uuid;

use crate::type_mappings::*;

// Re-export types for convenience
pub use intent_rebase_types::{ExternalRef, ExternalRefType};

// Re-export RlsAwarePool from intent-rebase_types for backward compatibility
pub use intent_rebase_types::rls::RlsAwarePool;

/// Repository trait for graph storage
/// Allows for in-memory (tests) or SQL-backed implementations
#[async_trait]
pub trait GraphRepository: Send + Sync {
    // Node operations
    async fn create_node(
        &self,
        request: CreateGraphNodeRequest,
    ) -> Result<GraphNode, IntentRebaseError>;
    async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError>;
    async fn list_nodes(
        &self,
        filter: GraphNodeFilter,
    ) -> Result<Vec<GraphNode>, IntentRebaseError>;
    async fn update_node_state(
        &self,
        id: Uuid,
        state: NodeState,
    ) -> Result<GraphNode, IntentRebaseError>;

    // Edge operations
    async fn create_edge(
        &self,
        request: CreateGraphEdgeRequest,
    ) -> Result<GraphEdge, IntentRebaseError>;
    async fn get_edge(&self, id: Uuid) -> Result<GraphEdge, IntentRebaseError>;
    async fn list_edges(
        &self,
        filter: GraphEdgeFilter,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError>;
    async fn list_edges_from(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError>;
    async fn list_edges_to(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError>;
    async fn delete_edge(&self, id: Uuid) -> Result<(), IntentRebaseError>;

    /// Returns a reference to the underlying `SqlxGraphRepository` if this is a SQL-backed repository.
    ///
    /// Returns `None` for in-memory or other non-SQL implementations.
    ///
    /// This method is used for RLS-aware operations that require direct access to the
    /// SQL repository and its transaction capabilities.
    fn as_sqlx_repo(&self) -> Option<&SqlxGraphRepository> {
        None
    }
}

#[async_trait]
impl GraphRepository for SqlxGraphRepository {
    async fn create_node(
        &self,
        request: CreateGraphNodeRequest,
    ) -> Result<GraphNode, IntentRebaseError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let (external_ref_type, external_ref_id) = match &request.external_ref {
            Some(eref) => {
                let rt = external_ref_type_to_string(&eref.ref_type);
                (Some(rt), Some(eref.ref_id))
            }
            None => (None, None),
        };

        let node_type_str = node_type_to_string(&request.node_type);
        let state_str = node_state_to_string(&NodeState::Active);
        let properties = serde_json::to_value(request.properties.unwrap_or(serde_json::json!({})))
            .map_err(|e| {
                IntentRebaseError::SerializationError(format!("node properties: {}", e))
            })?;

        let node_properties = properties.clone();

        sqlx::query(
            r#"
            INSERT INTO graph_nodes (node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(request.tenant_id)
        .bind(request.workflow_id)
        .bind(node_type_str)
        .bind(external_ref_type)
        .bind(external_ref_id)
        .bind(&request.label)
        .bind(state_str)
        .bind(properties)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert graph node: {}", e)))?;

        let node = GraphNode {
            id,
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: request.node_type,
            external_ref: request.external_ref,
            label: request.label,
            state: NodeState::Active,
            properties: node_properties,
            created_at: now,
        };

        Ok(node)
    }

    async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
            FROM graph_nodes
            WHERE node_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch graph node: {}", e)))?;

        match row {
            Some(r) => self.row_to_node(r),
            None => Err(IntentRebaseError::GraphNodeNotFound(id)),
        }
    }

    async fn list_nodes(
        &self,
        filter: GraphNodeFilter,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        // Extract filter values upfront to avoid move issues
        let tenant_id = filter.tenant_id;
        let workflow_id = filter.workflow_id;
        let node_type = filter.node_type;
        let state = filter.state;

        // Build query with optional filters
        let rows = if let (Some(tid), Some(wid), Some(nt), Some(st)) = (&tenant_id, &workflow_id, &node_type, &state) {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE tenant_id = $1 AND workflow_id = $2 AND node_type = $3 AND state = $4
                ORDER BY created_at DESC
                "#,
            )
            .bind(*tid)
            .bind(*wid)
            .bind(node_type_to_string(nt))
            .bind(node_state_to_string(st))
            .fetch_all(&self.pool)
            .await
        } else if let (Some(tid), Some(wid), Some(nt)) = (&tenant_id, &workflow_id, &node_type) {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE tenant_id = $1 AND workflow_id = $2 AND node_type = $3
                ORDER BY created_at DESC
                "#,
            )
            .bind(*tid)
            .bind(*wid)
            .bind(node_type_to_string(nt))
            .fetch_all(&self.pool)
            .await
        } else if let (Some(tid), Some(wid)) = (&tenant_id, &workflow_id) {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE tenant_id = $1 AND workflow_id = $2
                ORDER BY created_at DESC
                "#,
            )
            .bind(*tid)
            .bind(*wid)
            .fetch_all(&self.pool)
            .await
        } else if let Some(tid) = &tenant_id {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE tenant_id = $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(*tid)
            .fetch_all(&self.pool)
            .await
        } else if let Some(wid) = &workflow_id {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE workflow_id = $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(*wid)
            .fetch_all(&self.pool)
            .await
        } else if let Some(nt) = &node_type {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE node_type = $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(node_type_to_string(nt))
            .fetch_all(&self.pool)
            .await
        } else if let Some(st) = &state {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE state = $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(node_state_to_string(st))
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                ORDER BY created_at DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| IntentRebaseError::StorageError(format!("list graph nodes: {}", e)))?;

        rows.into_iter().map(|r| self.row_to_node(r)).collect()
    }

    async fn update_node_state(
        &self,
        id: Uuid,
        state: NodeState,
    ) -> Result<GraphNode, IntentRebaseError> {
        let now = Utc::now();
        let state_str = node_state_to_string(&state);

        let result = sqlx::query(
            r#"
            UPDATE graph_nodes SET state = $1, updated_at = $2 WHERE node_id = $3
            "#,
        )
        .bind(state_str)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("update node state: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(IntentRebaseError::GraphNodeNotFound(id));
        }

        self.get_node(id).await
    }

    async fn create_edge(
        &self,
        request: CreateGraphEdgeRequest,
    ) -> Result<GraphEdge, IntentRebaseError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        // DB trigger will validate node existence and tenant/workflow consistency
        let edge_type_str = edge_type_to_string(&request.edge_type);
        let properties = serde_json::to_value(request.properties.unwrap_or(serde_json::json!({})))
            .map_err(|e| {
                IntentRebaseError::SerializationError(format!("edge properties: {}", e))
            })?;

        let edge_properties = properties.clone();

        sqlx::query(
            r#"
            INSERT INTO graph_edges (edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(request.tenant_id)
        .bind(request.workflow_id)
        .bind(request.from_node_id)
        .bind(request.to_node_id)
        .bind(edge_type_str)
        .bind(properties)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert graph edge: {}", e)))?;

        Ok(GraphEdge {
            id,
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            from_node_id: request.from_node_id,
            to_node_id: request.to_node_id,
            edge_type: request.edge_type,
            properties: edge_properties,
            created_at: now,
        })
    }

    async fn get_edge(&self, id: Uuid) -> Result<GraphEdge, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
            FROM graph_edges
            WHERE edge_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch graph edge: {}", e)))?;

        match row {
            Some(r) => self.row_to_edge(r),
            None => Err(IntentRebaseError::GraphEdgeNotFound(id)),
        }
    }

    async fn list_edges(
        &self,
        filter: GraphEdgeFilter,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        // Build query with optional filters - simplified approach for type inference
        // Extract filter values upfront to avoid move issues
        let tenant_id = filter.tenant_id;
        let workflow_id = filter.workflow_id;
        let from_node_id = filter.from_node_id;
        let to_node_id = filter.to_node_id;
        let edge_type = filter.edge_type;

        let rows =
            if let (Some(tid), Some(wid), Some(fnid), Some(tnid), Some(et)) =
                (&tenant_id, &workflow_id, &from_node_id, &to_node_id, &edge_type)
            {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE tenant_id = $1 AND workflow_id = $2 AND from_node_id = $3 AND to_node_id = $4 AND edge_type = $5
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tid)
                .bind(*wid)
                .bind(*fnid)
                .bind(*tnid)
                .bind(edge_type_to_string(et))
                .fetch_all(&self.pool)
                .await
            } else if let (Some(tid), Some(wid), Some(fnid), Some(tnid)) =
                (&tenant_id, &workflow_id, &from_node_id, &to_node_id)
            {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE tenant_id = $1 AND workflow_id = $2 AND from_node_id = $3 AND to_node_id = $4
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tid)
                .bind(*wid)
                .bind(*fnid)
                .bind(*tnid)
                .fetch_all(&self.pool)
                .await
            } else if let (Some(tid), Some(wid), Some(fnid)) =
                (&tenant_id, &workflow_id, &from_node_id)
            {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE tenant_id = $1 AND workflow_id = $2 AND from_node_id = $3
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tid)
                .bind(*wid)
                .bind(*fnid)
                .fetch_all(&self.pool)
                .await
            } else if let (Some(tid), Some(wid)) = (&tenant_id, &workflow_id) {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE tenant_id = $1 AND workflow_id = $2
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tid)
                .bind(*wid)
                .fetch_all(&self.pool)
                .await
            } else if let Some(tid) = &tenant_id {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE tenant_id = $1
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tid)
                .fetch_all(&self.pool)
                .await
            } else if let Some(fnid) = &from_node_id {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE from_node_id = $1
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*fnid)
                .fetch_all(&self.pool)
                .await
            } else if let Some(tnid) = &to_node_id {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE to_node_id = $1
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(*tnid)
                .fetch_all(&self.pool)
                .await
            } else if let Some(et) = &edge_type {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    WHERE edge_type = $1
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(edge_type_to_string(et))
                .fetch_all(&self.pool)
                .await
            } else {
                sqlx::query(
                    r#"
                    SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
                    FROM graph_edges
                    ORDER BY created_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await
            }
            .map_err(|e| IntentRebaseError::StorageError(format!("list graph edges: {}", e)))?;

        rows.into_iter().map(|r| self.row_to_edge(r)).collect()
    }

    async fn list_edges_from(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
            FROM graph_edges
            WHERE from_node_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("list edges from: {}", e)))?;

        rows.into_iter().map(|r| self.row_to_edge(r)).collect()
    }

    async fn list_edges_to(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT edge_id, tenant_id, workflow_id, from_node_id, to_node_id, edge_type, properties, created_at
            FROM graph_edges
            WHERE to_node_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("list edges to: {}", e)))?;

        rows.into_iter().map(|r| self.row_to_edge(r)).collect()
    }

    async fn delete_edge(&self, id: Uuid) -> Result<(), IntentRebaseError> {
        let result = sqlx::query("DELETE FROM graph_edges WHERE edge_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| IntentRebaseError::StorageError(format!("delete edge: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(IntentRebaseError::GraphEdgeNotFound(id));
        }

        Ok(())
    }

    fn as_sqlx_repo(&self) -> Option<&SqlxGraphRepository> {
        Some(self)
    }
}

/// GraphService handles graph lifecycle operations
#[derive(Clone)]
pub struct GraphService {
    repo: Arc<dyn GraphRepository>,
}

impl GraphService {
    pub fn new(repo: Arc<dyn GraphRepository>) -> Self {
        Self { repo }
    }

    /// Returns a reference to the underlying repository.
    ///
    /// This is used for RLS-aware operations that require direct access
    /// to the underlying SQL repository.
    pub fn repo(&self) -> &Arc<dyn GraphRepository> {
        &self.repo
    }

    /// Add a node to the graph
    pub async fn add_node(
        &self,
        request: CreateGraphNodeRequest,
    ) -> Result<GraphNode, IntentRebaseError> {
        self.repo.create_node(request).await
    }

    /// Get a node by ID
    pub async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError> {
        self.repo.get_node(id).await
    }

    /// List nodes with optional filters
    pub async fn list_nodes(
        &self,
        filter: GraphNodeFilter,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        self.repo.list_nodes(filter).await
    }

    /// List nodes scoped by intent (via external_ref filter)
    pub async fn get_intent_nodes(
        &self,
        intent_id: Uuid,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        let filter = GraphNodeFilter {
            node_type: Some(NodeType::Intent),
            ..Default::default()
        };
        let nodes = self.repo.list_nodes(filter).await?;

        // Filter by external_ref if it matches the intent_id
        Ok(nodes.into_iter().filter(|n| {
            matches!(n.external_ref, Some(ref r) if r.ref_type == ExternalRefType::Intent && r.ref_id == intent_id)
        }).collect())
    }

    /// Find the IntentVersion graph node by its IntentVersion UUID.
    ///
    /// This is used during rebase preview to locate the target IntentVersion node
    /// in the dependency graph for impact classification.
    ///
    /// Returns `Ok(None)` if the IntentVersion node is not found in the graph
    /// (graph coverage may be incomplete for this intent version).
    pub async fn find_intent_version_node(
        &self,
        intent_version_id: Uuid,
    ) -> Result<Option<GraphNode>, IntentRebaseError> {
        let filter = GraphNodeFilter {
            node_type: Some(NodeType::IntentVersion),
            ..Default::default()
        };
        let nodes = self.repo.list_nodes(filter).await?;

        // Find the node with matching external_ref ref_id
        Ok(nodes
            .into_iter()
            .find(|n| matches!(n.external_ref, Some(ref r) if r.ref_id == intent_version_id)))
    }

    /// Classify affected items starting from a target IntentVersion node.
    ///
    /// This is a convenience method that combines finding the IntentVersion node
    /// and running impact classification. Returns `Ok(None)` if the IntentVersion
    /// node is not found in the graph.
    ///
    /// # Parameters
    /// - `intent_version_id`: The UUID of the IntentVersion to classify from
    /// - `max_depth`: Maximum traversal depth (defaults to 3)
    ///
    /// # Returns
    /// - `Ok(Some(ClassificationResult))` if the node was found and classified
    /// - `Ok(None)` if the IntentVersion node was not found in the graph
    pub async fn classify_affected_items_from_intent_version(
        &self,
        intent_version_id: Uuid,
        max_depth: Option<usize>,
    ) -> Result<Option<ClassificationResult>, IntentRebaseError> {
        let node = self.find_intent_version_node(intent_version_id).await?;

        match node {
            Some(start_node) => {
                let request = ClassifyRequest {
                    start_node_id: start_node.id,
                    max_depth,
                    target_node_types: None,
                    propagation_config: None,
                };
                let result = self.classify_impact(request).await?;
                Ok(Some(result))
            }
            None => Ok(None),
        }
    }

    /// Update node state
    pub async fn update_node_state(
        &self,
        id: Uuid,
        state: NodeState,
    ) -> Result<GraphNode, IntentRebaseError> {
        self.repo.update_node_state(id, state).await
    }

    /// Add an edge to the graph
    pub async fn add_edge(
        &self,
        request: CreateGraphEdgeRequest,
    ) -> Result<GraphEdge, IntentRebaseError> {
        self.repo.create_edge(request).await
    }

    /// Get an edge by ID
    pub async fn get_edge(&self, id: Uuid) -> Result<GraphEdge, IntentRebaseError> {
        self.repo.get_edge(id).await
    }

    /// List edges with optional filters
    pub async fn list_edges(
        &self,
        filter: GraphEdgeFilter,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        self.repo.list_edges(filter).await
    }

    /// List edges outgoing from a node
    pub async fn list_edges_from(
        &self,
        node_id: Uuid,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        self.repo.list_edges_from(node_id).await
    }

    /// List edges incoming to a node
    pub async fn list_edges_to(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        self.repo.list_edges_to(node_id).await
    }

    /// Delete an edge
    pub async fn delete_edge(&self, id: Uuid) -> Result<(), IntentRebaseError> {
        self.repo.delete_edge(id).await
    }
}

#[cfg(test)]
mod tests;

// =============================================================================
// SqlxGraphRepository tests (require live Postgres)
// =============================================================================

#[cfg(test)]
mod sqlx_graph_repository_tests;
