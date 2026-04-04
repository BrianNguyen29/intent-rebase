//! Graph Service — manages dependency graph state
//!
//! Phase 1 PR #10: Graph traversal baseline (BFS, path-finding, cycle detection) with in-memory repository.
//! Provides persisted graph nodes/edges CRUD for future traversal/classification work.
//!
//! Architecture: Repository trait allows swapping to SQL-backed implementation.
//! See: docs/03-spec/03-dependency-graph.md (storage strategy)

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    CreateGraphEdgeRequest, CreateGraphNodeRequest, CycleDetectionResult, GraphEdge,
    GraphEdgeFilter, GraphNode, GraphNodeFilter, GraphPath, IntentRebaseError, NodeState, NodeType,
    ReachabilityResult, TraversalOptions,
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

    /// Find all nodes reachable from a starting node using BFS
    ///
    /// Returns all nodes reachable via outgoing edges, optionally filtered by depth and edge type.
    pub async fn find_reachable(
        &self,
        start_node_id: Uuid,
        options: TraversalOptions,
    ) -> Result<ReachabilityResult, IntentRebaseError> {
        use std::collections::{HashSet, VecDeque};

        // First verify start node exists
        let _ = self.repo.get_node(start_node_id).await?;

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut incoming_edges: Vec<Uuid> = Vec::new();
        let mut queue: VecDeque<(Uuid, Option<Uuid>, usize)> = VecDeque::new();

        // (node_id, edge_id_used_to_reach, depth)
        if options.include_start {
            visited.insert(start_node_id);
        }

        // Seed with start node's outgoing edges
        let start_edges = self.repo.list_edges_from(start_node_id).await?;
        for edge in start_edges {
            if Self::edge_passes_filter(&edge, &options) {
                queue.push_back((edge.to_node_id, Some(edge.id), 1));
            }
        }

        while let Some((node_id, edge_id, depth)) = queue.pop_front() {
            // Issue #2 fix: Skip the start node entirely if include_start=false
            // This prevents re-including the start node through cycles
            if node_id == start_node_id && !options.include_start {
                continue;
            }

            // Check max depth
            if let Some(max) = options.max_depth {
                if depth > max {
                    continue;
                }
            }

            // Issue #1 fix: Apply node_types filter BEFORE adding to visited and queue
            // This implements "only traverse through matching node types" semantics
            if let Some(ref filter_node_types) = options.node_types {
                let node = match self.repo.get_node(node_id).await {
                    Ok(n) => n,
                    Err(_) => continue, // Node not found, skip
                };
                if !filter_node_types.contains(&node.node_type) {
                    continue; // Don't visit filtered nodes - don't add to visited or expand from them
                }
            }

            if visited.insert(node_id) {
                if let Some(eid) = edge_id {
                    incoming_edges.push(eid);
                }

                // Only explore further if within depth limit
                let at_max_depth = options.max_depth.map(|max| depth >= max).unwrap_or(false);

                if !at_max_depth {
                    let edges = self.repo.list_edges_from(node_id).await?;
                    for edge in edges {
                        if Self::edge_passes_filter(&edge, &options) {
                            queue.push_back((edge.to_node_id, Some(edge.id), depth + 1));
                        }
                    }
                }
            }
        }

        Ok(ReachabilityResult {
            reachable_nodes: visited.into_iter().collect(),
            incoming_edges,
        })
    }

    /// Find a path between two nodes using BFS
    ///
    /// Returns the shortest path (fewest hops) from source to target, if one exists.
    pub async fn find_path(
        &self,
        source_id: Uuid,
        target_id: Uuid,
        options: TraversalOptions,
    ) -> Result<GraphPath, IntentRebaseError> {
        use std::collections::{HashMap, HashSet, VecDeque};

        // First verify both nodes exist
        let _ = self.repo.get_node(source_id).await?;
        let _ = self.repo.get_node(target_id).await?;

        if source_id == target_id {
            if options.include_start {
                return Ok(GraphPath {
                    node_ids: vec![source_id],
                    edge_ids: vec![],
                });
            }
            return Ok(GraphPath {
                node_ids: vec![],
                edge_ids: vec![],
            });
        }

        // BFS to find shortest path
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut parent_edge: HashMap<Uuid, (Uuid, Uuid)> = HashMap::new(); // node -> (prev_node, edge_id)

        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((source_id, 0));
        visited.insert(source_id);

        while let Some((node_id, depth)) = queue.pop_front() {
            // Check max depth
            if let Some(max) = options.max_depth {
                if depth >= max {
                    continue;
                }
            }

            let edges = self.repo.list_edges_from(node_id).await?;
            for edge in edges {
                if !Self::edge_passes_filter(&edge, &options) {
                    continue;
                }

                let neighbor = edge.to_node_id;

                if neighbor == target_id {
                    parent_edge.insert(neighbor, (node_id, edge.id));
                    // Reconstruct path
                    return Ok(Self::reconstruct_path(source_id, target_id, parent_edge));
                }

                if visited.insert(neighbor) {
                    parent_edge.insert(neighbor, (node_id, edge.id));
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        // No path found
        Ok(GraphPath {
            node_ids: vec![],
            edge_ids: vec![],
        })
    }

    /// Detect if there are any cycles in the graph (within a workflow scope)
    ///
    /// Uses DFS to detect back edges.
    pub async fn detect_cycles(
        &self,
        workflow_id: Uuid,
    ) -> Result<CycleDetectionResult, IntentRebaseError> {
        use std::collections::HashMap;

        // Get all nodes in the workflow
        let nodes = self
            .repo
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_id),
                ..Default::default()
            })
            .await?;

        if nodes.is_empty() {
            return Ok(CycleDetectionResult {
                has_cycle: false,
                cycle_path: None,
            });
        }

        // Build adjacency list
        let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for node in &nodes {
            adj.entry(node.id).or_insert_with(Vec::new);
        }

        let edges = self
            .repo
            .list_edges(GraphEdgeFilter {
                workflow_id: Some(workflow_id),
                ..Default::default()
            })
            .await?;

        for edge in &edges {
            adj.entry(edge.from_node_id)
                .or_insert_with(Vec::new)
                .push(edge.to_node_id);
        }

        // DFS with three-color marking
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut color: HashMap<Uuid, Color> = HashMap::new();
        for node in &nodes {
            color.insert(node.id, Color::White);
        }

        let mut cycle_path: Option<Vec<Uuid>> = None;

        fn dfs(
            node: Uuid,
            adj: &HashMap<Uuid, Vec<Uuid>>,
            color: &mut HashMap<Uuid, Color>,
            path: &mut Vec<Uuid>,
            cycle_result: &mut Option<Vec<Uuid>>,
        ) -> bool {
            color.insert(node, Color::Gray);
            path.push(node);

            if let Some(neighbors) = adj.get(&node) {
                for &neighbor in neighbors {
                    // Check if we found a cycle
                    if let Some(&Color::Gray) = color.get(&neighbor) {
                        // Found back edge - cycle detected
                        if let Some(pos) = path.iter().position(|&n| n == neighbor) {
                            let mut cycle = path[pos..].to_vec();
                            cycle.push(neighbor); // Append neighbor to complete the cycle
                            *cycle_result = Some(cycle);
                            path.pop();
                            color.insert(node, Color::Black);
                            return true;
                        }
                    }
                }

                for &neighbor in neighbors {
                    if color.get(&neighbor) == Some(&Color::White) {
                        if dfs(neighbor, adj, color, path, cycle_result) {
                            return true;
                        }
                    }
                }
            }

            path.pop();
            color.insert(node, Color::Black);
            false
        }

        for node in &nodes {
            if color.get(&node.id) == Some(&Color::White) {
                let mut path = Vec::new();
                if dfs(node.id, &adj, &mut color, &mut path, &mut cycle_path) {
                    break;
                }
            }
        }

        Ok(CycleDetectionResult {
            has_cycle: cycle_path.is_some(),
            cycle_path,
        })
    }

    /// List nodes reachable from a starting node (alias for find_reachable with simpler options)
    pub async fn list_reachable_nodes(
        &self,
        start_node_id: Uuid,
        max_depth: Option<usize>,
    ) -> Result<Vec<Uuid>, IntentRebaseError> {
        let result = self
            .find_reachable(
                start_node_id,
                TraversalOptions {
                    max_depth,
                    ..Default::default()
                },
            )
            .await?;
        Ok(result.reachable_nodes)
    }

    /// Check if two nodes are connected (any path exists)
    pub async fn are_connected(
        &self,
        source_id: Uuid,
        target_id: Uuid,
        max_depth: Option<usize>,
    ) -> Result<bool, IntentRebaseError> {
        let path = self
            .find_path(
                source_id,
                target_id,
                TraversalOptions {
                    max_depth,
                    ..Default::default()
                },
            )
            .await?;
        Ok(!path.is_empty())
    }

    /// Helper: Check if an edge passes the traversal filter
    fn edge_passes_filter(edge: &GraphEdge, options: &TraversalOptions) -> bool {
        if let Some(ref edge_types) = options.edge_types {
            if !edge_types.contains(&edge.edge_type) {
                return false;
            }
        }
        true
    }

    /// Helper: Reconstruct path from parent map
    fn reconstruct_path(
        source: Uuid,
        target: Uuid,
        parent_edge: HashMap<Uuid, (Uuid, Uuid)>,
    ) -> GraphPath {
        let mut node_ids = Vec::new();
        let mut edge_ids = Vec::new();

        let mut current = target;
        node_ids.push(current);

        while current != source {
            if let Some((prev, edge_id)) = parent_edge.get(&current) {
                edge_ids.push(*edge_id);
                node_ids.push(*prev);
                current = *prev;
            } else {
                // Path reconstruction failed
                return GraphPath {
                    node_ids: vec![],
                    edge_ids: vec![],
                };
            }
        }

        node_ids.reverse();
        edge_ids.reverse();

        GraphPath { node_ids, edge_ids }
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

    // ===== Traversal Tests =====

    #[tokio::test]
    async fn test_bfs_reachable_simple_chain() {
        // Graph: A -> B -> C -> D
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // Create edges
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Find all reachable from A (unlimited depth)
        let result = service
            .find_reachable(node_a.id, TraversalOptions::default())
            .await
            .unwrap();

        assert!(result.reachable_nodes.contains(&node_a.id)); // include_start is true by default
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert!(result.reachable_nodes.contains(&node_d.id));
        assert_eq!(result.reachable_nodes.len(), 4);
    }

    #[tokio::test]
    async fn test_bfs_reachable_with_max_depth() {
        // Graph: A -> B -> C -> D
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // Create edges A->B, B->C, C->D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Depth 1: only B
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    max_depth: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(!result.reachable_nodes.contains(&node_c.id));
        assert!(!result.reachable_nodes.contains(&node_d.id));

        // Depth 2: B and C
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    max_depth: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert!(!result.reachable_nodes.contains(&node_d.id));
    }

    #[tokio::test]
    async fn test_bfs_reachable_diamond_graph() {
        // Diamond graph: A -> B, A -> C, B -> D, C -> D
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // A -> B
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        // A -> C
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_c.id,
            ))
            .await
            .unwrap();
        // B -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_d.id,
            ))
            .await
            .unwrap();
        // C -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // From A, should reach B, C, D (D only once despite two paths)
        let result = service
            .find_reachable(node_a.id, TraversalOptions::default())
            .await
            .unwrap();
        assert_eq!(result.reachable_nodes.len(), 4); // A, B, C, D
        assert!(result.reachable_nodes.contains(&node_a.id));
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert!(result.reachable_nodes.contains(&node_d.id));
    }

    #[tokio::test]
    async fn test_find_path_simple() {
        // Graph: A -> B -> C
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();

        // Find path A -> C
        let path = service
            .find_path(node_a.id, node_c.id, TraversalOptions::default())
            .await
            .unwrap();
        assert_eq!(path.node_ids, vec![node_a.id, node_b.id, node_c.id]);
        assert_eq!(path.edge_ids.len(), 2);
        assert_eq!(path.len(), 2);
    }

    #[tokio::test]
    async fn test_find_path_no_path() {
        // Two disconnected graphs: A -> B and C -> D
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();

        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // A -> B
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        // C -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Try to find path B -> C (no connection)
        let path = service
            .find_path(node_b.id, node_c.id, TraversalOptions::default())
            .await
            .unwrap();
        assert!(path.is_empty());
        assert_eq!(path.node_ids.len(), 0);
    }

    #[tokio::test]
    async fn test_find_path_diamond_shortest() {
        // Diamond: A -> B -> D, A -> C -> D
        // Shortest path should be 2 hops
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // A -> B -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_d.id,
            ))
            .await
            .unwrap();
        // A -> C -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Path A -> D should be 2 hops (A to either B or C to D)
        let path = service
            .find_path(node_a.id, node_d.id, TraversalOptions::default())
            .await
            .unwrap();
        assert_eq!(path.len(), 2);
        assert!(path.node_ids.first() == Some(&node_a.id));
        assert!(path.node_ids.last() == Some(&node_d.id));
    }

    #[tokio::test]
    async fn test_cycle_detection_no_cycle() {
        // Simple chain: A -> B -> C (no cycle)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();

        let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
        assert!(!result.has_cycle);
        assert!(result.cycle_path.is_none());
    }

    #[tokio::test]
    async fn test_cycle_detection_simple_cycle() {
        // Cycle: A -> B -> C -> A
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A -> B
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        // B -> C
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        // C -> A (creates cycle)
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_a.id,
            ))
            .await
            .unwrap();

        let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
        assert!(result.has_cycle);
        assert!(result.cycle_path.is_some());
        let cycle = result.cycle_path.unwrap();
        // The cycle should form a loop
        assert!(cycle.len() >= 3);
        assert_eq!(cycle.first(), cycle.last()); // Loop back to start
    }

    #[tokio::test]
    async fn test_cycle_detection_self_loop() {
        // Self-loop: A -> A
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();

        // A -> A (self-loop)
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_a.id,
            ))
            .await
            .unwrap();

        let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
        assert!(result.has_cycle);
    }

    #[tokio::test]
    async fn test_cycle_detection_empty_workflow() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        // Use a workflow ID that has no nodes
        let result = service.detect_cycles(Uuid::new_v4()).await.unwrap();
        assert!(!result.has_cycle);
        assert!(result.cycle_path.is_none());
    }

    #[tokio::test]
    async fn test_are_connected() {
        // Graph: A -> B -> C
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();

        // A connected to C
        assert!(service
            .are_connected(node_a.id, node_c.id, None)
            .await
            .unwrap());
        // C not connected to A (reverse direction)
        assert!(!service
            .are_connected(node_c.id, node_a.id, None)
            .await
            .unwrap());
        // A connected to B
        assert!(service
            .are_connected(node_a.id, node_b.id, None)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_list_reachable_nodes() {
        // Graph: A -> B -> C -> D
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();
        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        let node_d = service.add_node(node_d_req).await.unwrap();

        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Unlimited depth
        let reachable = service.list_reachable_nodes(node_a.id, None).await.unwrap();
        assert_eq!(reachable.len(), 4);

        // Depth 2
        let reachable = service
            .list_reachable_nodes(node_a.id, Some(2))
            .await
            .unwrap();
        assert!(reachable.contains(&node_a.id));
        assert!(reachable.contains(&node_b.id));
        assert!(reachable.contains(&node_c.id));
        assert!(!reachable.contains(&node_d.id));
    }

    #[tokio::test]
    async fn test_edge_type_filter() {
        // Graph with mixed edge types
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A --DependsOn--> B
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        // B --Triggers--> C (different edge type)
        service
            .add_edge(CreateGraphEdgeRequest {
                tenant_id: node_a.tenant_id,
                workflow_id: node_a.workflow_id,
                from_node_id: node_b.id,
                to_node_id: node_c.id,
                edge_type: EdgeType::Triggers,
                properties: None,
            })
            .await
            .unwrap();

        // Find path filtering by DependsOn only
        let path = service
            .find_path(
                node_a.id,
                node_c.id,
                TraversalOptions {
                    edge_types: Some(vec![EdgeType::DependsOn]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // No path exists if we can only use DependsOn edges
        assert!(path.is_empty());

        // Find path filtering by Triggers only
        let path = service
            .find_path(
                node_a.id,
                node_c.id,
                TraversalOptions {
                    edge_types: Some(vec![EdgeType::Triggers]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // No path exists (A doesn't have Triggers to C)
        assert!(path.is_empty());

        // Using both edge types should find the path
        let path = service
            .find_path(
                node_a.id,
                node_c.id,
                TraversalOptions {
                    edge_types: Some(vec![EdgeType::DependsOn, EdgeType::Triggers]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(path.len(), 2);
    }

    #[tokio::test]
    async fn test_reachable_nonexistent_node() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let result = service
            .find_reachable(Uuid::new_v4(), TraversalOptions::default())
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::GraphNodeNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_path_to_self() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();

        // Path to self with include_start=true
        let path = service
            .find_path(
                node_a.id,
                node_a.id,
                TraversalOptions {
                    include_start: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(path.node_ids, vec![node_a.id]);
        assert!(path.edge_ids.is_empty());

        // Path to self with include_start=false
        let path = service
            .find_path(
                node_a.id,
                node_a.id,
                TraversalOptions {
                    include_start: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(path.is_empty());
    }

    // ===== Issue #2 Fix: include_start=false should not re-include start node through cycles =====

    #[tokio::test]
    async fn test_reachable_include_start_false_no_cycle() {
        // Graph: A -> B -> C (no cycle back to A)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A -> B -> C
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();

        // include_start=false should exclude A, include B and C
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    include_start: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(!result.reachable_nodes.contains(&node_a.id));
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert_eq!(result.reachable_nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_reachable_include_start_false_with_cycle() {
        // Graph: A -> B -> C -> A (cycle back to start)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A -> B -> C -> A (cycle)
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_a.id,
            ))
            .await
            .unwrap();

        // include_start=false should STILL exclude A even though there's a cycle back to it
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    include_start: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(
            !result.reachable_nodes.contains(&node_a.id),
            "Start node should NOT be re-included via cycle when include_start=false"
        );
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert_eq!(result.reachable_nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_reachable_include_start_true_with_cycle() {
        // Graph: A -> B -> C -> A (cycle back to start)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let node_a = service.add_node(create_test_node_request()).await.unwrap();
        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        let node_b = service.add_node(node_b_req).await.unwrap();
        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        let node_c = service.add_node(node_c_req).await.unwrap();

        // A -> B -> C -> A (cycle)
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_a.id,
            ))
            .await
            .unwrap();

        // include_start=true should include A (once) along with B and C
        let result = service
            .find_reachable(node_a.id, TraversalOptions::default())
            .await
            .unwrap();
        assert!(result.reachable_nodes.contains(&node_a.id));
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(result.reachable_nodes.contains(&node_c.id));
        assert_eq!(result.reachable_nodes.len(), 3);
    }

    // ===== Issue #1 Fix: node_types filtering =====

    #[tokio::test]
    async fn test_reachable_node_type_filter() {
        // Graph: A (Intent) -> B (Intent) -> C (Artifact) -> D (Intent)
        // When filtering to Intent, we only traverse through Intent nodes
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let mut node_a_req = create_test_node_request();
        node_a_req.node_type = NodeType::Intent;
        let node_a = service.add_node(node_a_req).await.unwrap();

        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        node_b_req.node_type = NodeType::Intent;
        let node_b = service.add_node(node_b_req).await.unwrap();

        let mut node_c_req = create_test_node_request();
        node_c_req.tenant_id = node_a.tenant_id;
        node_c_req.workflow_id = node_a.workflow_id;
        node_c_req.node_type = NodeType::Artifact;
        let node_c = service.add_node(node_c_req).await.unwrap();

        let mut node_d_req = create_test_node_request();
        node_d_req.tenant_id = node_a.tenant_id;
        node_d_req.workflow_id = node_a.workflow_id;
        node_d_req.node_type = NodeType::Intent;
        let node_d = service.add_node(node_d_req).await.unwrap();

        // A -> B -> C -> D
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_b.id,
                node_c.id,
            ))
            .await
            .unwrap();
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_c.id,
                node_d.id,
            ))
            .await
            .unwrap();

        // Filter to only Intent nodes
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    node_types: Some(vec![NodeType::Intent]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // Only traverse through Intent nodes
        // A (Intent) - matches, expand from it
        // B (Intent) - matches, expand from it
        // C (Artifact) - filtered out, don't expand from it, so D is never discovered
        assert!(result.reachable_nodes.contains(&node_a.id));
        assert!(result.reachable_nodes.contains(&node_b.id));
        assert!(
            !result.reachable_nodes.contains(&node_c.id),
            "Artifact node should not be traversed through"
        );
        assert!(
            !result.reachable_nodes.contains(&node_d.id),
            "D should not be discovered since we don't traverse through C"
        );
        assert_eq!(result.reachable_nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_reachable_node_type_filter_no_match() {
        // Graph: A (Intent) -> B (Artifact)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let mut node_a_req = create_test_node_request();
        node_a_req.node_type = NodeType::Intent;
        let node_a = service.add_node(node_a_req).await.unwrap();

        let mut node_b_req = create_test_node_request();
        node_b_req.tenant_id = node_a.tenant_id;
        node_b_req.workflow_id = node_a.workflow_id;
        node_b_req.node_type = NodeType::Artifact;
        let node_b = service.add_node(node_b_req).await.unwrap();

        // A -> B
        service
            .add_edge(create_test_edge_request_with_ids(
                node_a.tenant_id,
                node_a.workflow_id,
                node_a.id,
                node_b.id,
            ))
            .await
            .unwrap();

        // Filter to only SideEffect nodes (none exist in graph)
        let result = service
            .find_reachable(
                node_a.id,
                TraversalOptions {
                    node_types: Some(vec![NodeType::SideEffect]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // Only A (Intent) is reachable since B (Artifact) is filtered
        assert!(result.reachable_nodes.contains(&node_a.id));
        assert!(!result.reachable_nodes.contains(&node_b.id));
        assert_eq!(result.reachable_nodes.len(), 1);
    }
}
