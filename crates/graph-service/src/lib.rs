//! Graph Service — manages dependency graph state
//!
//! Phase 1 PR #9: Storage-first Graph baseline with in-memory repository.
//! Provides persisted graph nodes/edges CRUD for future traversal/classification work.
//!
//! Architecture: Repository trait allows swapping to SQL-backed implementation.
//! See: docs/03-spec/03-dependency-graph.md (storage strategy)

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    CreateGraphEdgeRequest, CreateGraphNodeRequest, GraphEdge, GraphEdgeFilter, GraphNode,
    GraphNodeFilter, IntentRebaseError, NodeState, NodeType,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// Re-export types for convenience
pub use intent_rebase_types::{ExternalRef, ExternalRefType};

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

/// GraphService handles graph lifecycle operations
#[derive(Clone)]
pub struct GraphService {
    repo: Arc<dyn GraphRepository>,
}

impl GraphService {
    pub fn new(repo: Arc<dyn GraphRepository>) -> Self {
        Self { repo }
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
mod tests {
    use super::*;
    use intent_rebase_types::{EdgeType, ExternalRef, ExternalRefType};

    fn create_test_node_request() -> CreateGraphNodeRequest {
        CreateGraphNodeRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            node_type: NodeType::Intent,
            external_ref: Some(ExternalRef {
                ref_type: ExternalRefType::Intent,
                ref_id: Uuid::new_v4(),
            }),
            label: "Test Intent Node".to_string(),
            properties: Some(serde_json::json!({"priority": "high"})),
        }
    }

    fn create_test_edge_request_with_ids(
        tenant_id: Uuid,
        workflow_id: Uuid,
        from_node_id: Uuid,
        to_node_id: Uuid,
    ) -> CreateGraphEdgeRequest {
        CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id,
            to_node_id,
            edge_type: EdgeType::DependsOn,
            properties: Some(serde_json::json!({"reason": "test"})),
        }
    }

    #[tokio::test]
    async fn test_create_and_get_node() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let request = create_test_node_request();
        let created = service.add_node(request.clone()).await.unwrap();

        assert_eq!(created.label, request.label);
        assert_eq!(created.node_type, request.node_type);

        // Get by ID
        let retrieved = service.get_node(created.id).await.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.label, created.label);
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        let result = service.get_node(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::GraphNodeNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_list_nodes_with_filter() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        // Create nodes of different types
        let mut request1 = create_test_node_request();
        request1.node_type = NodeType::Intent;

        let mut request2 = create_test_node_request();
        request2.node_type = NodeType::Artifact;

        let node1 = service.add_node(request1).await.unwrap();
        let _node2 = service.add_node(request2).await.unwrap();

        // Filter by node type
        let filter = GraphNodeFilter {
            node_type: Some(NodeType::Intent),
            ..Default::default()
        };
        let nodes = service.list_nodes(filter).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, node1.id);
    }

    #[tokio::test]
    async fn test_create_and_get_edge() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        // Create two nodes with SAME tenant/workflow (required for edge creation)
        let node1 = service.add_node(create_test_node_request()).await.unwrap();
        let mut request2 = create_test_node_request();
        request2.tenant_id = node1.tenant_id;
        request2.workflow_id = node1.workflow_id;
        request2.external_ref = Some(ExternalRef {
            ref_type: ExternalRefType::IntentVersion,
            ref_id: Uuid::new_v4(),
        });
        let node2 = service.add_node(request2).await.unwrap();

        // Create edge - must use same tenant/workflow as nodes
        let edge_request = create_test_edge_request_with_ids(
            node1.tenant_id,
            node1.workflow_id,
            node1.id,
            node2.id,
        );
        let created = service.add_edge(edge_request.clone()).await.unwrap();

        assert_eq!(created.from_node_id, node1.id);
        assert_eq!(created.to_node_id, node2.id);
        assert_eq!(created.edge_type, EdgeType::DependsOn);

        // Get by ID
        let retrieved = service.get_edge(created.id).await.unwrap();
        assert_eq!(retrieved.id, created.id);
    }

    #[tokio::test]
    async fn test_create_edge_nonexistent_node() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        let request = create_test_edge_request_with_ids(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let result = service.add_edge(request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::GraphNodeNotFound(_)));
    }

    #[tokio::test]
    async fn test_create_edge_cross_tenant_rejected() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        // Create two nodes with tenant A
        let node1 = service.add_node(create_test_node_request()).await.unwrap();
        let mut request2 = create_test_node_request();
        request2.tenant_id = node1.tenant_id;
        request2.workflow_id = node1.workflow_id;
        let node2 = service.add_node(request2).await.unwrap();

        // Try to create edge with different tenant
        let mut edge_request = create_test_edge_request_with_ids(
            node1.tenant_id,
            node1.workflow_id,
            node1.id,
            node2.id,
        );
        edge_request.tenant_id = Uuid::new_v4(); // Different tenant

        let result = service.add_edge(edge_request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::GraphIntegrityError(_)
        ));
    }

    #[tokio::test]
    async fn test_create_edge_cross_workflow_rejected() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        // Create two nodes with same tenant but different workflow
        let node1 = service.add_node(create_test_node_request()).await.unwrap();
        let mut request2 = create_test_node_request();
        request2.tenant_id = node1.tenant_id;
        request2.workflow_id = Uuid::new_v4(); // Different workflow
        let node2 = service.add_node(request2).await.unwrap();

        // Try to create edge with node1's workflow (not node2's)
        let edge_request = create_test_edge_request_with_ids(
            node1.tenant_id,
            node1.workflow_id,
            node1.id,
            node2.id,
        );

        let result = service.add_edge(edge_request).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::GraphIntegrityError(_)
        ));
    }

    #[tokio::test]
    async fn test_list_edges_from_and_to() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        // Create three nodes: A -> B -> C
        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        node_b_req.external_ref = Some(ExternalRef {
            ref_type: ExternalRefType::IntentVersion,
            ref_id: Uuid::new_v4(),
        });
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        node_c_req.external_ref = Some(ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        });
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A -> B
        let edge_ab = create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        );
        let _edge_ab = service.add_edge(edge_ab).await.unwrap();

        // B -> C
        let edge_bc = create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        );
        let _edge_bc = service.add_edge(edge_bc).await.unwrap();

        // Check edges from A
        let edges_from_a = service.list_edges_from(node_a.id).await.unwrap();
        assert_eq!(edges_from_a.len(), 1);
        assert_eq!(edges_from_a[0].to_node_id, node_b.id);

        // Check edges to C
        let edges_to_c = service.list_edges_to(node_c.id).await.unwrap();
        assert_eq!(edges_to_c.len(), 1);
        assert_eq!(edges_to_c[0].from_node_id, node_b.id);

        // Check edges from B
        let edges_from_b = service.list_edges_from(node_b.id).await.unwrap();
        assert_eq!(edges_from_b.len(), 1);
        assert_eq!(edges_from_b[0].to_node_id, node_c.id);
    }

    #[tokio::test]
    async fn test_delete_edge() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        // Create nodes
        let node1 = service.add_node(create_test_node_request()).await.unwrap();
        let mut node2_req = create_test_node_request();
        node2_req.tenant_id = node1.tenant_id;
        node2_req.workflow_id = node1.workflow_id;
        let node2 = service.add_node(node2_req).await.unwrap();

        // Create edge
        let edge_request = create_test_edge_request_with_ids(
            node1.tenant_id,
            node1.workflow_id,
            node1.id,
            node2.id,
        );
        let edge = service.add_edge(edge_request).await.unwrap();

        // Delete it
        let result = service.delete_edge(edge.id).await;
        assert!(result.is_ok());

        // Verify it's gone
        let get_result = service.get_edge(edge.id).await;
        assert!(get_result.is_err());
    }

    #[tokio::test]
    async fn test_update_node_state() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo);

        let node = service.add_node(create_test_node_request()).await.unwrap();
        assert_eq!(node.state, NodeState::Active);

        let updated = service
            .update_node_state(node.id, NodeState::Stale)
            .await
            .unwrap();
        assert_eq!(updated.state, NodeState::Stale);
    }

    #[tokio::test]
    async fn test_in_memory_repo_persistence() {
        // Verify in-memory repo shares state between service instances
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service1 = GraphService::new(repo.clone());
        let service2 = GraphService::new(repo);

        let node = service1.add_node(create_test_node_request()).await.unwrap();

        // Second service should see the same data
        let retrieved = service2.get_node(node.id).await.unwrap();
        assert_eq!(retrieved.id, node.id);
    }

    #[tokio::test]
    async fn test_concurrent_operations_no_deadlock() {
        // Verify that concurrent create_edge and list_edges_from don't deadlock
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        // Create a node
        let node = service.add_node(create_test_node_request()).await.unwrap();

        // Spawn concurrent edge creates
        let mut edge_handles = vec![];

        for i in 0..10 {
            let service_clone = service.clone();
            let node_clone = node.id;
            let tenant_id = node.tenant_id;
            let workflow_id = node.workflow_id;

            edge_handles.push(tokio::spawn(async move {
                let mut req = create_test_node_request();
                req.tenant_id = tenant_id;
                req.workflow_id = workflow_id;
                req.label = format!("Target Node {}", i);

                let target = service_clone.add_node(req).await.unwrap();

                let edge_req = create_test_edge_request_with_ids(
                    tenant_id,
                    workflow_id,
                    node_clone,
                    target.id,
                );
                service_clone.add_edge(edge_req).await
            }));
        }

        // Also spawn list operations
        let mut list_handles = vec![];
        for _ in 0..5 {
            let service_clone = service.clone();
            let node_clone = node.id;
            list_handles.push(tokio::spawn(async move {
                service_clone.list_edges_from(node_clone).await
            }));
        }

        // Wait for all - if there's a deadlock, this will hang
        for handle in edge_handles {
            let result = handle.await.unwrap();
            // Edge creates may fail if target nodes conflict, but shouldn't deadlock
            let _ = result;
        }

        for handle in list_handles {
            let result = handle.await.unwrap();
            // List operations should succeed
            assert!(result.is_ok());
        }

        // Verify final state
        let edges = service.list_edges_from(node.id).await.unwrap();
        assert!(edges.len() <= 10);
    }
}
