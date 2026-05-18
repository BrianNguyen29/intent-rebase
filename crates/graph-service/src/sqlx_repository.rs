use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ArtifactIngestRequest, CreateGraphEdgeRequest, CreateGraphNodeRequest, EdgeType, ExternalRef,
    GraphEdge, GraphEdgeFilter, GraphNode, GraphNodeFilter, IngestorResult, IntentRebaseError,
    NodeState, NodeType,
};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

use crate::{type_mappings::*, GraphRepository};

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
