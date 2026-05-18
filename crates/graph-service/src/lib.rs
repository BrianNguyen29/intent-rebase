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
pub mod traversal;

mod classification;
mod type_mappings;

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ApprovalIngestRequest, ArtifactIngestRequest, ClassificationResult, ClassifyRequest,
    CreateGraphEdgeRequest, CreateGraphNodeRequest, EdgeType, GraphEdge, GraphEdgeFilter,
    GraphNode, GraphNodeFilter, IngestorResult, IntentRebaseError, NodeState, NodeType,
    SideEffectIngestRequest,
};
#[allow(unused_imports)]
use intent_rebase_types::{CycleDetectionResult, GraphPath, ReachabilityResult, TraversalOptions};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
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

/// Unified graph state to prevent lock-order inversion deadlocks.
/// All locks are consolidated into a single RwLock to ensure
/// consistent lock ordering across all operations.
#[derive(Default)]
pub struct GraphState {
    nodes: HashMap<Uuid, GraphNode>,
    edges: HashMap<Uuid, GraphEdge>,
    edges_by_from: HashMap<Uuid, Vec<Uuid>>,
    edges_by_to: HashMap<Uuid, Vec<Uuid>>,
}

/// In-memory implementation for testing and Phase 1
pub struct InMemoryGraphRepository {
    state: RwLock<GraphState>,
}

impl InMemoryGraphRepository {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(GraphState::default()),
        }
    }
}

impl Default for InMemoryGraphRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GraphRepository for InMemoryGraphRepository {
    async fn create_node(
        &self,
        request: CreateGraphNodeRequest,
    ) -> Result<GraphNode, IntentRebaseError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let node = GraphNode {
            id,
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: request.node_type,
            external_ref: request.external_ref,
            label: request.label,
            state: NodeState::Active,
            properties: request.properties.unwrap_or(serde_json::json!({})),
            created_at: now,
        };

        let mut state = self.state.write().await;
        state.nodes.insert(id, node.clone());

        Ok(node)
    }

    async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError> {
        let state = self.state.read().await;
        state
            .nodes
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::GraphNodeNotFound(id))
    }

    async fn list_nodes(
        &self,
        filter: GraphNodeFilter,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        let state = self.state.read().await;
        let mut result: Vec<GraphNode> = state.nodes.values().cloned().collect();

        if let Some(tenant_id) = filter.tenant_id {
            result.retain(|n| n.tenant_id == tenant_id);
        }
        if let Some(workflow_id) = filter.workflow_id {
            result.retain(|n| n.workflow_id == workflow_id);
        }
        if let Some(node_type) = filter.node_type {
            result.retain(|n| n.node_type == node_type);
        }
        if let Some(state) = filter.state {
            result.retain(|n| n.state == state);
        }

        Ok(result)
    }

    async fn update_node_state(
        &self,
        id: Uuid,
        state: NodeState,
    ) -> Result<GraphNode, IntentRebaseError> {
        let mut state_guard = self.state.write().await;
        let node = state_guard
            .nodes
            .get_mut(&id)
            .ok_or(IntentRebaseError::GraphNodeNotFound(id))?;

        node.state = state;
        Ok(node.clone())
    }

    async fn create_edge(
        &self,
        request: CreateGraphEdgeRequest,
    ) -> Result<GraphEdge, IntentRebaseError> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        // Validate node existence and tenant/workflow consistency under single lock
        // to prevent lock-order inversion deadlocks.
        let edge = {
            let mut state = self.state.write().await;

            // Verify from_node exists and matches tenant/workflow
            let from_node = state
                .nodes
                .get(&request.from_node_id)
                .ok_or(IntentRebaseError::GraphNodeNotFound(request.from_node_id))?;

            if from_node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::GraphIntegrityError(format!(
                    "from_node {} belongs to tenant {} but edge has tenant {}",
                    request.from_node_id, from_node.tenant_id, request.tenant_id
                )));
            }

            if from_node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::GraphIntegrityError(format!(
                    "from_node {} belongs to workflow {} but edge has workflow {}",
                    request.from_node_id, from_node.workflow_id, request.workflow_id
                )));
            }

            // Verify to_node exists and matches tenant/workflow
            let to_node = state
                .nodes
                .get(&request.to_node_id)
                .ok_or(IntentRebaseError::GraphNodeNotFound(request.to_node_id))?;

            if to_node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::GraphIntegrityError(format!(
                    "to_node {} belongs to tenant {} but edge has tenant {}",
                    request.to_node_id, to_node.tenant_id, request.tenant_id
                )));
            }

            if to_node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::GraphIntegrityError(format!(
                    "to_node {} belongs to workflow {} but edge has workflow {}",
                    request.to_node_id, to_node.workflow_id, request.workflow_id
                )));
            }

            let edge = GraphEdge {
                id,
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: request.from_node_id,
                to_node_id: request.to_node_id,
                edge_type: request.edge_type,
                properties: request.properties.unwrap_or(serde_json::json!({})),
                created_at: now,
            };

            state.edges.insert(id, edge.clone());

            // Update indices
            state
                .edges_by_from
                .entry(edge.from_node_id)
                .or_insert_with(Vec::new)
                .push(id);

            state
                .edges_by_to
                .entry(edge.to_node_id)
                .or_insert_with(Vec::new)
                .push(id);

            edge
        };

        Ok(edge)
    }

    async fn get_edge(&self, id: Uuid) -> Result<GraphEdge, IntentRebaseError> {
        let state = self.state.read().await;
        state
            .edges
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::GraphEdgeNotFound(id))
    }

    async fn list_edges(
        &self,
        filter: GraphEdgeFilter,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        let state = self.state.read().await;
        let mut result: Vec<GraphEdge> = state.edges.values().cloned().collect();

        if let Some(tenant_id) = filter.tenant_id {
            result.retain(|e| e.tenant_id == tenant_id);
        }
        if let Some(workflow_id) = filter.workflow_id {
            result.retain(|e| e.workflow_id == workflow_id);
        }
        if let Some(from_node_id) = filter.from_node_id {
            result.retain(|e| e.from_node_id == from_node_id);
        }
        if let Some(to_node_id) = filter.to_node_id {
            result.retain(|e| e.to_node_id == to_node_id);
        }
        if let Some(edge_type) = filter.edge_type {
            result.retain(|e| e.edge_type == edge_type);
        }

        Ok(result)
    }

    async fn list_edges_from(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        let state = self.state.read().await;

        let edge_ids = state
            .edges_by_from
            .get(&node_id)
            .cloned()
            .unwrap_or_default();

        let mut result = Vec::new();
        for id in edge_ids {
            if let Some(edge) = state.edges.get(&id) {
                result.push(edge.clone());
            }
        }

        Ok(result)
    }

    async fn list_edges_to(&self, node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        let state = self.state.read().await;

        let edge_ids = state.edges_by_to.get(&node_id).cloned().unwrap_or_default();

        let mut result = Vec::new();
        for id in edge_ids {
            if let Some(edge) = state.edges.get(&id) {
                result.push(edge.clone());
            }
        }

        Ok(result)
    }

    async fn delete_edge(&self, id: Uuid) -> Result<(), IntentRebaseError> {
        let mut state = self.state.write().await;
        let edge = state
            .edges
            .remove(&id)
            .ok_or(IntentRebaseError::GraphEdgeNotFound(id))?;

        // Update indices
        if let Some(ids) = state.edges_by_from.get_mut(&edge.from_node_id) {
            ids.retain(|eid| *eid != id);
        }

        if let Some(ids) = state.edges_by_to.get_mut(&edge.to_node_id) {
            ids.retain(|eid| *eid != id);
        }

        Ok(())
    }
}

// =============================================================================
// SqlxGraphRepository — SQL-backed graph storage
// =============================================================================

/// SQL-backed implementation of GraphRepository
///
/// Phase 2b bounded slice: Core CRUD operations against existing graph_nodes/graph_edges
/// tables. Does NOT implement traversal operations (find_reachable, find_path, detect_cycles)
/// which require application-level graph algorithms; those remain on GraphService using
/// the repository's list_* methods.
///
/// Bounded gaps:
/// - No transaction-based consistency checks (DB trigger enforces node existence)
/// - No bulk operations
/// - No pagination on list operations
pub struct SqlxGraphRepository {
    pool: PgPool,
}

impl SqlxGraphRepository {
    /// Create a new SqlxGraphRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to a GraphNode domain object
    fn row_to_node(&self, row: PgRow) -> Result<GraphNode, IntentRebaseError> {
        let external_ref_type: Option<String> = row.get("external_ref_type");
        let external_ref_id: Option<Uuid> = row.get("external_ref_id");
        let node_type_str: String = row.get("node_type");
        let state_str: String = row.get("state");
        let properties: serde_json::Value = row.get("properties");

        let external_ref = match (&external_ref_type, &external_ref_id) {
            (Some(ref_type), Some(ref_id)) => {
                let rt = external_ref_type_from_string(ref_type)?;
                Some(ExternalRef {
                    ref_type: rt,
                    ref_id: *ref_id,
                })
            }
            (None, None) => None,
            _ => {
                return Err(IntentRebaseError::Internal(
                    "graph node has partial external ref: both type and id must be set or both unset"
                        .to_string(),
                ));
            }
        };

        Ok(GraphNode {
            id: row.get("node_id"),
            tenant_id: row.get("tenant_id"),
            workflow_id: row.get("workflow_id"),
            node_type: node_type_from_string(&node_type_str)?,
            external_ref,
            label: row.get("label"),
            state: node_state_from_string(&state_str)?,
            properties,
            created_at: row.get("created_at"),
        })
    }

    /// Convert a database row to a GraphEdge domain object
    fn row_to_edge(&self, row: PgRow) -> Result<GraphEdge, IntentRebaseError> {
        let edge_type_str: String = row.get("edge_type");
        let properties: serde_json::Value = row.get("properties");

        Ok(GraphEdge {
            id: row.get("edge_id"),
            tenant_id: row.get("tenant_id"),
            workflow_id: row.get("workflow_id"),
            from_node_id: row.get("from_node_id"),
            to_node_id: row.get("to_node_id"),
            edge_type: edge_type_from_string(&edge_type_str)?,
            properties,
            created_at: row.get("created_at"),
        })
    }

    /// Create a graph node within an external transaction.
    ///
    /// This method is used for RLS-wrapped operations where the transaction
    /// is created by `RlsAwarePool::begin_with_tenant` which sets the RLS
    /// tenant context before any operations.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `request` - The create node request
    ///
    /// # Errors
    ///
    /// Returns an error if the insert fails or if the transaction is invalid.
    pub async fn create_node_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .execute(&mut **tx)
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

    /// Create a new graph edge within an existing RLS-aware transaction.
    ///
    /// This method inserts a new edge into the graph_edges table using a transaction
    /// that has already been configured with RLS tenant context.
    ///
    /// The caller is responsible for:
    /// - Beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// - Committing or rolling back the transaction after this call
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `request` - The create edge request
    ///
    /// # Errors
    ///
    /// Returns an error if the insert fails or if the transaction is invalid.
    pub async fn create_edge_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .execute(&mut **tx)
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

    /// Ingest an artifact into the graph within an existing RLS-aware transaction.
    ///
    /// This method creates an artifact node and wires DependsOn edges to the
    /// referenced IntentVersion nodes, all within a single transaction that has
    /// already been configured with RLS tenant context.
    ///
    /// The caller is responsible for:
    /// - Beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// - Committing or rolling back the transaction after this call
    ///
    /// # Prevalidation
    /// - `depends_on_intent_versions` MUST contain at least one IntentVersion node ID
    /// - All referenced IntentVersion nodes MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the artifact
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `request` - The artifact ingest request
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails, insert fails, or if the transaction is invalid.
    pub async fn ingest_artifact_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        request: ArtifactIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // PREVALIDATION: Enforce artifact traceability contract
        if request.depends_on_intent_versions.is_empty() {
            return Err(IntentRebaseError::ArtifactTraceabilityEmpty);
        }

        // Validate all referenced IntentVersion nodes exist, have correct type, and match scope
        // We query within the transaction to maintain consistency
        for intent_version_id in &request.depends_on_intent_versions {
            let row = sqlx::query(
                r#"
                SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
                FROM graph_nodes
                WHERE node_id = $1
                "#,
            )
            .bind(intent_version_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| IntentRebaseError::StorageError(format!("fetch intent version node: {}", e)))?;

            let node = match row {
                Some(r) => self.row_to_node(r)?,
                None => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
            };

            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }

            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but artifact has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }

            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but artifact has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the artifact node within the transaction
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::Artifact,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.create_node_with_tx(tx, node_request).await?;
        let mut edges = Vec::new();

        // Wire DependsOn edges to each IntentVersion within the transaction
        for intent_version_id in &request.depends_on_intent_versions {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: *intent_version_id,
                edge_type: EdgeType::DependsOn,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let edge = self.create_edge_with_tx(tx, edge_request).await?;
            edges.push(edge);
        }

        Ok(IngestorResult { node, edges })
    }

    /// Update a graph node's state within an existing transaction.
    ///
    /// This method is used for transaction-aware state updates where the transaction
    /// is managed externally (e.g., by `RlsAwarePool::begin_with_tenant`).
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `id` - The UUID of the node to update
    /// * `state` - The new `NodeState` to set
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found or the update fails.
    pub async fn update_node_state_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("update node state: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(IntentRebaseError::GraphNodeNotFound(id));
        }

        let row = sqlx::query(
            r#"
            SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
            FROM graph_nodes
            WHERE node_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch graph node: {}", e)))?;

        match row {
            Some(r) => self.row_to_node(r),
            None => Err(IntentRebaseError::GraphNodeNotFound(id)),
        }
    }

    /// Get a graph node by ID within an existing transaction.
    ///
    /// Phase 4 D2: Transaction-aware read for graph updater validation.
    pub async fn get_node_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: Uuid,
    ) -> Result<GraphNode, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT node_id, tenant_id, workflow_id, node_type, external_ref_type, external_ref_id, label, state, properties, created_at
            FROM graph_nodes
            WHERE node_id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch graph node: {}", e)))?;

        match row {
            Some(r) => self.row_to_node(r),
            None => Err(IntentRebaseError::GraphNodeNotFound(id)),
        }
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

    // ============================================================================
    // Ingestor Methods
    // ============================================================================

    /// Ingest an artifact into the graph.
    ///
    /// Creates an Artifact node and wires DependsOn edges to the specified IntentVersion nodes.
    /// This enforces the graph invariant that every artifact traces to at least one intent version.
    ///
    /// # Phase 3 Batch 1 (groundwork): Side Effect Capture Context
    /// When `request.side_effect_context` is provided with sufficient fields, the caller
    /// (typically intent-api) should record a side effect to the compensation ledger
    /// after successful ingest. This enables capture-on-write for artifact-producing
    /// operations that have proper intent/version context.
    ///
    /// **Note:** This method consumes the `side_effect_context` but does NOT automatically
    /// record the side effect. The caller is responsible for checking `request.side_effect_context`
    /// and recording to compensation-service if provided. This separation keeps graph-service
    /// free of compensation-service dependency.
    ///
    /// # Prevalidation
    /// - `depends_on_intent_versions` MUST contain at least one IntentVersion node ID
    /// - All referenced IntentVersion nodes MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the artifact
    pub async fn ingest_artifact(
        &self,
        request: ArtifactIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // Extract side effect context before consuming request
        // Note: The context is consumed but not used by graph-service itself.
        // The caller (e.g., intent-api) should check if context was provided
        // and record the side effect to compensation-service after successful ingest.
        let _side_effect_context = request.side_effect_context.clone();

        // PREVALIDATION: Enforce artifact traceability contract
        if request.depends_on_intent_versions.is_empty() {
            return Err(IntentRebaseError::ArtifactTraceabilityEmpty);
        }

        // Validate all referenced IntentVersion nodes exist, have correct type, and match scope
        for intent_version_id in &request.depends_on_intent_versions {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(*intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but artifact has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but artifact has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the artifact node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::Artifact,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire DependsOn edges to each IntentVersion
        for intent_version_id in &request.depends_on_intent_versions {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: *intent_version_id,
                edge_type: EdgeType::DependsOn,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        // Note: If side_effect_context was provided, the caller should record the side effect
        // after successful ingest. The context is available via the consumed request's
        // side_effect_context field. This method does not auto-record to keep graph-service
        // free of compensation-service dependency.

        Ok(IngestorResult { node, edges })
    }

    /// Ingest an approval into the graph.
    ///
    /// Creates an Approval node and optionally wires:
    /// - A GovernedBy edge to the PolicySnapshot that governs this approval
    /// - A ValidatedBy edge to the IntentVersion this approval is associated with
    ///
    /// # Prevalidation
    /// - If `governed_by_policy_snapshot` is provided, the node MUST exist, be of type `NodeType::PolicySnapshot`,
    ///   AND belong to the same tenant_id and workflow_id as the approval
    /// - If `intent_version_id` is provided, the node MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the approval
    pub async fn ingest_approval(
        &self,
        request: ApprovalIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // PREVALIDATION: Validate PolicySnapshot reference if provided
        if let Some(policy_snapshot_id) = request.governed_by_policy_snapshot {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(policy_snapshot_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "PolicySnapshot node {} does not exist",
                        policy_snapshot_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::PolicySnapshot {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not a PolicySnapshot (found {:?})",
                    policy_snapshot_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "PolicySnapshot node {} belongs to tenant {} but approval has tenant {}",
                    policy_snapshot_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "PolicySnapshot node {} belongs to workflow {} but approval has workflow {}",
                    policy_snapshot_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // PREVALIDATION: Validate IntentVersion reference if provided
        if let Some(intent_version_id) = request.intent_version_id {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but approval has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but approval has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the approval node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::Approval,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire GovernedBy edge to PolicySnapshot if provided
        if let Some(policy_snapshot_id) = request.governed_by_policy_snapshot {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: policy_snapshot_id,
                edge_type: EdgeType::GovernedBy,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "PolicySnapshot"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        // Wire ValidatedBy edge to IntentVersion if provided
        if let Some(intent_version_id) = request.intent_version_id {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: intent_version_id,
                edge_type: EdgeType::ValidatedBy,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        Ok(IngestorResult { node, edges })
    }

    /// Ingest a side effect into the graph.
    ///
    /// Creates a SideEffect node and wires appropriate edges to:
    /// - The initiating node that triggered this side effect (Triggers edge, from trigger node to SideEffect)
    /// - The IntentVersion (DerivedFrom edge, from SideEffect to IntentVersion)
    /// - The Approval snapshot if applicable (GeneratedFrom edge, from SideEffect to Approval)
    ///
    /// # Prevalidation
    /// - `triggered_by_task` MUST exist in the graph AND belong to the same tenant_id and workflow_id
    /// - If `derived_from_intent_version` is provided, the node MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the side effect
    /// - If `approval_snapshot_id` is provided, the node MUST exist, be of type `NodeType::Approval`,
    ///   AND belong to the same tenant_id and workflow_id as the side effect
    pub async fn ingest_side_effect(
        &self,
        request: SideEffectIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // PREVALIDATION: Validate triggered_by_task exists and matches scope
        // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
        let triggered_node = match self.repo.get_node(request.triggered_by_task).await {
            Ok(n) => n,
            Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Triggering node {} does not exist",
                    request.triggered_by_task
                )));
            }
            Err(e) => return Err(e), // Preserve truthful error classification
        };
        // Validate scope: tenant_id must match
        if triggered_node.tenant_id != request.tenant_id {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "Triggering node {} belongs to tenant {} but side effect has tenant {}",
                request.triggered_by_task, triggered_node.tenant_id, request.tenant_id
            )));
        }
        // Validate scope: workflow_id must match
        if triggered_node.workflow_id != request.workflow_id {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "Triggering node {} belongs to workflow {} but side effect has workflow {}",
                request.triggered_by_task, triggered_node.workflow_id, request.workflow_id
            )));
        }

        // PREVALIDATION: Validate IntentVersion reference if provided
        if let Some(intent_version_id) = request.derived_from_intent_version {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but side effect has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but side effect has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // PREVALIDATION: Validate Approval reference if provided
        if let Some(approval_snapshot_id) = request.approval_snapshot_id {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(approval_snapshot_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "Approval node {} does not exist",
                        approval_snapshot_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::Approval {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an Approval (found {:?})",
                    approval_snapshot_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Approval node {} belongs to tenant {} but side effect has tenant {}",
                    approval_snapshot_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Approval node {} belongs to workflow {} but side effect has workflow {}",
                    approval_snapshot_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the side effect node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::SideEffect,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire Triggers edge: triggering node -> SideEffect
        let triggers_edge = CreateGraphEdgeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            from_node_id: request.triggered_by_task,
            to_node_id: node.id,
            edge_type: EdgeType::Triggers,
            properties: Some(serde_json::json!({
                "direction": "downstream",
                "target_type": "SideEffect"
            })),
        };
        let triggers_created = self.repo.create_edge(triggers_edge).await?;
        edges.push(triggers_created);

        // Wire DerivedFrom edge: SideEffect -> IntentVersion
        if let Some(intent_version_id) = request.derived_from_intent_version {
            let derived_edge = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: intent_version_id,
                edge_type: EdgeType::DerivedFrom,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let derived_created = self.repo.create_edge(derived_edge).await?;
            edges.push(derived_created);
        }

        // Wire GeneratedFrom edge: SideEffect -> Approval (if under approval)
        if let Some(approval_snapshot_id) = request.approval_snapshot_id {
            let generated_edge = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: approval_snapshot_id,
                edge_type: EdgeType::GeneratedFrom,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "Approval"
                })),
            };

            let generated_created = self.repo.create_edge(generated_edge).await?;
            edges.push(generated_created);
        }

        Ok(IngestorResult { node, edges })
    }
}

#[cfg(test)]
mod tests;

// =============================================================================
// SqlxGraphRepository tests (require live Postgres)
// =============================================================================

#[cfg(test)]
mod sqlx_graph_repository_tests;
