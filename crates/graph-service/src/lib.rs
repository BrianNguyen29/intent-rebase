//! Graph Service — manages dependency graph state
//!
//! Phase 1 PR #10: Graph traversal baseline (BFS, path-finding, cycle detection) with in-memory repository.
//! Phase 1 PR #11: Graph ingestors baseline for artifact, approval, and side-effect nodes.
//! Provides persisted graph nodes/edges CRUD for future traversal/classification work.
//!
//! Architecture: Repository trait allows swapping to SQL-backed implementation.
//! See: docs/03-spec/03-dependency-graph.md (storage strategy)

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::{
    ApprovalIngestRequest, ArtifactIngestRequest, ClassificationImpact, ClassificationResult,
    ClassifiedNode, ClassifyRequest, CreateGraphEdgeRequest, CreateGraphNodeRequest,
    CycleDetectionResult, EdgeType, GraphEdge, GraphEdgeFilter, GraphNode, GraphNodeFilter,
    GraphPath, IngestorResult, IntentRebaseError, NodeState, NodeType, PropagationConfig,
    ReachabilityResult, SideEffectIngestRequest, TraversalOptions, DEFAULT_PROPAGATION_CONFIG,
};
use std::collections::{HashMap, VecDeque};
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

    // ============================================================================
    // Ingestor Methods
    // ============================================================================

    /// Ingest an artifact into the graph.
    ///
    /// Creates an Artifact node and wires DependsOn edges to the specified IntentVersion nodes.
    /// This enforces the graph invariant that every artifact traces to at least one intent version.
    ///
    /// # Prevalidation
    /// - `depends_on_intent_versions` MUST contain at least one IntentVersion node ID
    /// - All referenced IntentVersion nodes MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the artifact
    pub async fn ingest_artifact(
        &self,
        request: ArtifactIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
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

    // ============================================================================
    // Classification Methods
    // ============================================================================

    /// Classify the impact of a change originating from a starting node.
    ///
    /// This is a baseline classification implementation that uses deterministic,
    /// explicit propagation rules with bounded depth.
    ///
    /// # Graph Edge Direction Semantics
    /// The dependency graph uses edges that point UPSTREAM (from dependent to dependency):
    /// - `DependsOn`: Artifact -> IntentVersion (artifact depends on intent)
    /// - `Triggers`: TaskNode -> SideEffect (task triggers side effect)
    /// - `GeneratedFrom`: SideEffect -> Approval (side effect generated from approval)
    /// - `ValidatedBy`: Approval -> IntentVersion (approval validates intent)
    ///
    /// # Propagation Configuration (PR #13 Rule-Pack Baseline)
    /// When `request.propagation_config` is `Some`, the provided `PropagationConfig` drives:
    /// - `max_depth`: Maximum traversal depth (default: 3)
    /// - `traversable_edge_types`: Which edge types to follow (default: DependsOn, Triggers, GeneratedFrom)
    /// - `target_node_types`: Which node types to classify as affected (default: Artifact, Approval, SideEffect, Generic)
    ///
    /// When `request.propagation_config` is `None`, uses `DEFAULT_PROPAGATION_CONFIG`.
    ///
    /// Note: `traversable_directions` is accepted for future extension but Phase 1 baseline
    /// uses hardcoded direction logic per edge type (DependsOn=incoming, Triggers/GeneratedFrom=outgoing).
    ///
    /// # Classification Rules (Baseline)
    /// - **Direct**: Nodes at depth 1 from the starting node (e.g., Artifacts that directly
    ///   depend on the changed IntentVersion via DependsOn edge)
    /// - **Transitive**: Nodes at depth 2+ from the starting node (e.g., SideEffects
    ///   triggered downstream from affected Artifacts)
    /// - **Unchanged**: Nodes not reachable from the starting node
    ///
    /// # Traversal Semantics
    /// - For DependsOn: traverse INCOMING edges to find dependents
    /// - For Triggers/GeneratedFrom: traverse OUTGOING edges to find downstream
    /// - Depth is bounded by `max_depth` (default 3) to keep baseline bounded
    /// - Only classifies nodes matching `target_node_types` filter (if provided in config)
    ///
    /// # Example
    /// ```ignore
    /// // Given: IntentVersion IV1 <-(DependsOn)- Artifact A1 <-(Triggers)- SideEffect SE1
    /// // Graph edges: Artifact A1 -> IV1, SE1 -> A1 (via Triggers)
    /// let result = service.classify_impact(ClassifyRequest {
    ///     start_node_id: iv1.id,
    ///     max_depth: Some(3),
    ///     target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]),
    ///     propagation_config: None, // Uses DEFAULT_PROPAGATION_CONFIG
    /// }).await?;
    ///
    /// // Result: A1 classified as Direct (via incoming DependsOn), SE1 classified as Transitive (via outgoing Triggers)
    /// ```
    pub async fn classify_impact(
        &self,
        request: ClassifyRequest,
    ) -> Result<ClassificationResult, IntentRebaseError> {
        use std::collections::{HashSet, VecDeque};

        // PR #13: Use propagation_config if provided, otherwise use default
        let config = request
            .propagation_config
            .as_ref()
            .unwrap_or(&DEFAULT_PROPAGATION_CONFIG);

        // Validate start node exists
        let start_node = self.repo.get_node(request.start_node_id).await?;

        // PR #13: For backward compat, when propagation_config is None, prefer request.max_depth.
        // When config is explicitly provided, use config.max_depth but fall back to request.max_depth.
        let use_config_max_depth = request.propagation_config.is_some();
        let max_depth = if use_config_max_depth {
            config.max_depth.or(request.max_depth).unwrap_or(3)
        } else {
            request.max_depth.unwrap_or(3)
        };

        // BFS traversal to classify nodes by impact
        // We track (node_id, depth, path_reason) for each discovered node
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut classified: Vec<ClassifiedNode> = Vec::new();
        let mut queue: VecDeque<(Uuid, usize, String)> = VecDeque::new();

        // Phase 1: Seed with start node's INCOMING DependsOn edges at depth 1
        // (Artifacts that depend on this IntentVersion)
        // Note: Direction is hardcoded per edge type for Phase 1 baseline
        let incoming_edges = self.repo.list_edges_to(request.start_node_id).await?;
        for edge in incoming_edges {
            if edge.edge_type == EdgeType::DependsOn
                && config.traversable_edge_types.contains(&edge.edge_type)
            {
                queue.push_back((edge.from_node_id, 1, "directly depends on".to_string()));
            }
        }

        while let Some((node_id, depth, reason)) = queue.pop_front() {
            // Check max depth
            if depth > max_depth {
                continue;
            }

            // Skip if already visited
            if visited.contains(&node_id) {
                continue;
            }

            // Get the node to classify it
            let node = match self.repo.get_node(node_id).await {
                Ok(n) => n,
                Err(_) => continue, // Node not found, skip
            };

            // Check if this node type is a target.
            // PR #13 backward compat: When propagation_config is None AND request.target_node_types
            // is Some, prefer request.target_node_types (legacy behavior for existing callers).
            // When propagation_config is Some, use config.target_node_types (may be empty to override).
            let use_config = request.propagation_config.is_some();
            let target_types = if use_config {
                // Explicit config provided - use it (may be empty to target all types)
                Some(&config.target_node_types)
            } else {
                // No config - use request.target_node_types if provided, else config defaults
                request
                    .target_node_types
                    .as_ref()
                    .or(Some(&config.target_node_types))
            };

            if let Some(ref allowed_types) = target_types {
                if !allowed_types.contains(&node.node_type) {
                    // Skip this node but still explore its outgoing edges for propagation
                    if depth < max_depth {
                        Self::enqueue_propagation_edges_with_config(
                            &self.repo, node_id, depth, &mut queue, config,
                        )
                        .await?;
                    }
                    continue;
                }
            }

            // Classify based on depth from start
            let impact = if depth == 1 {
                ClassificationImpact::Direct
            } else {
                ClassificationImpact::Transitive
            };

            visited.insert(node_id);
            classified.push(ClassifiedNode {
                node: node.clone(),
                impact,
                reason,
            });

            // Continue traversal if within depth limit
            if depth < max_depth {
                Self::enqueue_propagation_edges_with_config(
                    &self.repo, node_id, depth, &mut queue, config,
                )
                .await?;
            }
        }

        Ok(ClassificationResult {
            classified_nodes: classified,
            start_node_id: start_node.id,
            max_depth,
        })
    }

    /// Helper: Enqueue edges for impact propagation from a node (PR #13 with PropagationConfig).
    ///
    /// Impact propagation follows the dependency chain downstream:
    /// - For DependsOn (artifact -> intent): downstream is the artifact (incoming edges)
    /// - For Triggers (task -> side_effect): downstream is the side_effect (outgoing edges)
    /// - For GeneratedFrom (se -> approval): downstream is the approval (outgoing edges)
    ///
    /// Note: Direction is hardcoded per edge type for Phase 1 baseline.
    async fn enqueue_propagation_edges_with_config(
        repo: &Arc<dyn GraphRepository>,
        node_id: Uuid,
        current_depth: usize,
        queue: &mut VecDeque<(Uuid, usize, String)>,
        config: &PropagationConfig,
    ) -> Result<(), IntentRebaseError> {
        // For DependsOn: look at INCOMING edges to find dependents (downstream artifacts)
        // Direction is hardcoded for Phase 1 - DependsOn always uses incoming
        let incoming_edges = repo.list_edges_to(node_id).await?;
        for edge in incoming_edges {
            if edge.edge_type == EdgeType::DependsOn
                && config.traversable_edge_types.contains(&edge.edge_type)
            {
                queue.push_back((
                    edge.from_node_id,
                    current_depth + 1,
                    "downstream via dependency chain".to_string(),
                ));
            }
        }

        // For Triggers/GeneratedFrom: follow OUTGOING edges to find downstream
        // Direction is hardcoded for Phase 1 - Triggers/GeneratedFrom always use outgoing
        let outgoing_edges = repo.list_edges_from(node_id).await?;
        for edge in outgoing_edges {
            match edge.edge_type {
                EdgeType::Triggers
                    if config.traversable_edge_types.contains(&EdgeType::Triggers) =>
                {
                    queue.push_back((
                        edge.to_node_id,
                        current_depth + 1,
                        "downstream via triggered".to_string(),
                    ));
                }
                EdgeType::GeneratedFrom
                    if config
                        .traversable_edge_types
                        .contains(&EdgeType::GeneratedFrom) =>
                {
                    // SideEffect -> Approval (side effect generated from approval)
                    queue.push_back((
                        edge.to_node_id,
                        current_depth + 1,
                        "downstream via generated from".to_string(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::{EdgeDirection, EdgeType, ExternalRef, ExternalRefType};

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

    // ===== Ingestor Tests =====

    #[tokio::test]
    async fn test_ingest_artifact_creates_node_and_edges() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node first (artifact will depend on it)
        let mut intent_version_req = create_test_node_request();
        intent_version_req.tenant_id = tenant_id;
        intent_version_req.workflow_id = workflow_id;
        intent_version_req.node_type = NodeType::IntentVersion;
        let intent_version = service.add_node(intent_version_req).await.unwrap();

        // Ingest an artifact that depends on the IntentVersion
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "patch-42".to_string(),
            depends_on_intent_versions: vec![intent_version.id],
            properties: Some(serde_json::json!({"artifact_type": "patch"})),
        };

        let result = service.ingest_artifact(artifact_req).await.unwrap();

        // Verify node
        assert_eq!(result.node.node_type, NodeType::Artifact);
        assert_eq!(result.node.label, "patch-42");
        assert_eq!(result.node.tenant_id, tenant_id);
        assert_eq!(result.node.workflow_id, workflow_id);

        // Verify edge: Artifact depends on IntentVersion (DependsOn from artifact to intent_version)
        assert_eq!(result.edges.len(), 1);
        let edge = &result.edges[0];
        assert_eq!(edge.edge_type, EdgeType::DependsOn);
        assert_eq!(edge.from_node_id, result.node.id);
        assert_eq!(edge.to_node_id, intent_version.id);

        // Verify the node can be retrieved
        let retrieved = service.get_node(result.node.id).await.unwrap();
        assert_eq!(retrieved.id, result.node.id);
    }

    #[tokio::test]
    async fn test_ingest_artifact_with_multiple_dependencies() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create two IntentVersion nodes
        let mut iv1_req = create_test_node_request();
        iv1_req.tenant_id = tenant_id;
        iv1_req.workflow_id = workflow_id;
        iv1_req.node_type = NodeType::IntentVersion;
        let iv1 = service.add_node(iv1_req).await.unwrap();

        let mut iv2_req = create_test_node_request();
        iv2_req.tenant_id = tenant_id;
        iv2_req.workflow_id = workflow_id;
        iv2_req.node_type = NodeType::IntentVersion;
        let iv2 = service.add_node(iv2_req).await.unwrap();

        // Ingest artifact with two dependencies
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "multi-dep-artifact".to_string(),
            depends_on_intent_versions: vec![iv1.id, iv2.id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await.unwrap();

        assert_eq!(result.node.node_type, NodeType::Artifact);
        assert_eq!(result.edges.len(), 2);

        // Verify both edges exist
        let edge_ids: Vec<_> = result.edges.iter().map(|e| e.to_node_id).collect();
        assert!(edge_ids.contains(&iv1.id));
        assert!(edge_ids.contains(&iv2.id));
    }

    #[tokio::test]
    async fn test_ingest_approval_creates_node_with_governed_by() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a PolicySnapshot node
        let mut policy_req = create_test_node_request();
        policy_req.tenant_id = tenant_id;
        policy_req.workflow_id = workflow_id;
        policy_req.node_type = NodeType::PolicySnapshot;
        let policy_snapshot = service.add_node(policy_req).await.unwrap();

        // Create an IntentVersion node
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let intent_version = service.add_node(iv_req).await.unwrap();

        // Ingest approval governed by policy snapshot and associated with intent version
        let approval_req = ApprovalIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Approval,
                ref_id: Uuid::new_v4(),
            },
            label: "approval-7".to_string(),
            governed_by_policy_snapshot: Some(policy_snapshot.id),
            intent_version_id: Some(intent_version.id),
            properties: Some(serde_json::json!({"scope": "production-deploy"})),
        };

        let result = service.ingest_approval(approval_req).await.unwrap();

        // Verify node
        assert_eq!(result.node.node_type, NodeType::Approval);
        assert_eq!(result.node.label, "approval-7");
        assert_eq!(result.node.tenant_id, tenant_id);

        // Verify two edges: GovernedBy -> PolicySnapshot, ValidatedBy -> IntentVersion
        assert_eq!(result.edges.len(), 2);

        let governed_by_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::GovernedBy)
            .unwrap();
        assert_eq!(governed_by_edge.from_node_id, result.node.id);
        assert_eq!(governed_by_edge.to_node_id, policy_snapshot.id);

        let validated_by_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::ValidatedBy)
            .unwrap();
        assert_eq!(validated_by_edge.from_node_id, result.node.id);
        assert_eq!(validated_by_edge.to_node_id, intent_version.id);
    }

    #[tokio::test]
    async fn test_ingest_approval_without_optional_edges() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Ingest approval without policy snapshot or intent version
        let approval_req = ApprovalIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Approval,
                ref_id: Uuid::new_v4(),
            },
            label: "minimal-approval".to_string(),
            governed_by_policy_snapshot: None,
            intent_version_id: None,
            properties: None,
        };

        let result = service.ingest_approval(approval_req).await.unwrap();

        // Verify node created but no edges
        assert_eq!(result.node.node_type, NodeType::Approval);
        assert!(result.edges.is_empty());
    }

    #[tokio::test]
    async fn test_ingest_side_effect_creates_node_and_edges() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a TaskNode (triggering task)
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic; // Using Generic as proxy for TaskNode in baseline
        task_req.label = "deploy-task".to_string();
        let task_node = service.add_node(task_req).await.unwrap();

        // Create an IntentVersion
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let intent_version = service.add_node(iv_req).await.unwrap();

        // Create an Approval
        let mut approval_req = create_test_node_request();
        approval_req.tenant_id = tenant_id;
        approval_req.workflow_id = workflow_id;
        approval_req.node_type = NodeType::Approval;
        let approval = service.add_node(approval_req).await.unwrap();

        // Ingest side effect with full trace
        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "deploy-1".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: Some(intent_version.id),
            approval_snapshot_id: Some(approval.id),
            properties: Some(serde_json::json!({"action": "kubectl-apply"})),
        };

        let result = service.ingest_side_effect(side_effect_req).await.unwrap();

        // Verify node
        assert_eq!(result.node.node_type, NodeType::SideEffect);
        assert_eq!(result.node.label, "deploy-1");
        assert_eq!(result.node.tenant_id, tenant_id);

        // Verify 3 edges:
        // 1. Triggers: TaskNode -> SideEffect
        // 2. DerivedFrom: SideEffect -> IntentVersion
        // 3. GeneratedFrom: SideEffect -> Approval
        assert_eq!(result.edges.len(), 3);

        let triggers_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::Triggers)
            .unwrap();
        assert_eq!(triggers_edge.from_node_id, task_node.id);
        assert_eq!(triggers_edge.to_node_id, result.node.id);

        let derived_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::DerivedFrom)
            .unwrap();
        assert_eq!(derived_edge.from_node_id, result.node.id);
        assert_eq!(derived_edge.to_node_id, intent_version.id);

        let generated_edge = result
            .edges
            .iter()
            .find(|e| e.edge_type == EdgeType::GeneratedFrom)
            .unwrap();
        assert_eq!(generated_edge.from_node_id, result.node.id);
        assert_eq!(generated_edge.to_node_id, approval.id);
    }

    #[tokio::test]
    async fn test_ingest_side_effect_minimal() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create only a TaskNode (triggering task)
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic;
        task_req.label = "minimal-task".to_string();
        let task_node = service.add_node(task_req).await.unwrap();

        // Ingest side effect with only required fields
        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "minimal-side-effect".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: None,
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await.unwrap();

        // Verify node created
        assert_eq!(result.node.node_type, NodeType::SideEffect);

        // Only 1 edge (Triggers) - the required one
        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].edge_type, EdgeType::Triggers);
        assert_eq!(result.edges[0].from_node_id, task_node.id);
        assert_eq!(result.edges[0].to_node_id, result.node.id);
    }

    #[tokio::test]
    async fn test_ingest_artifact_traces_to_intent_version() {
        // This test verifies the graph invariant: every artifact must trace to at least one IntentVersion
        // Artifact --DependsOn--> IntentVersion (edge flows from artifact to intent version)
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create multiple IntentVersion nodes (representing different versions of an intent)
        let mut iv1_req = create_test_node_request();
        iv1_req.tenant_id = tenant_id;
        iv1_req.workflow_id = workflow_id;
        iv1_req.node_type = NodeType::IntentVersion;
        let iv1 = service.add_node(iv1_req).await.unwrap();

        let mut iv2_req = create_test_node_request();
        iv2_req.tenant_id = tenant_id;
        iv2_req.workflow_id = workflow_id;
        iv2_req.node_type = NodeType::IntentVersion;
        let iv2 = service.add_node(iv2_req).await.unwrap();

        // Ingest artifact that depends on both versions
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "traceable-artifact".to_string(),
            depends_on_intent_versions: vec![iv1.id, iv2.id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await.unwrap();

        // Verify artifact can reach both IntentVersions via DependsOn edges
        // Artifact --DependsOn--> IntentVersion (edge flows from artifact to intent)
        let path1 = service
            .find_path(result.node.id, iv1.id, TraversalOptions::default())
            .await
            .unwrap();
        assert!(
            !path1.is_empty(),
            "Artifact should be able to reach IntentVersion iv1 via DependsOn edge"
        );

        let path2 = service
            .find_path(result.node.id, iv2.id, TraversalOptions::default())
            .await
            .unwrap();
        assert!(
            !path2.is_empty(),
            "Artifact should be able to reach IntentVersion iv2 via DependsOn edge"
        );

        // Also verify the edges are created with correct direction
        let edges_from_artifact = service.list_edges_from(result.node.id).await.unwrap();
        assert_eq!(edges_from_artifact.len(), 2);
        let edge_targets: Vec<_> = edges_from_artifact.iter().map(|e| e.to_node_id).collect();
        assert!(edge_targets.contains(&iv1.id));
        assert!(edge_targets.contains(&iv2.id));
    }

    // ===== Negative Tests for Ingestor Failure Paths =====

    #[tokio::test]
    async fn test_ingest_artifact_empty_dependencies_rejected() {
        // Contract: every artifact must trace to at least one IntentVersion
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Attempt to ingest artifact with NO dependencies - should fail
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "orphan-artifact".to_string(),
            depends_on_intent_versions: vec![], // EMPTY - violates contract!
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ArtifactTraceabilityEmpty
        ));
    }

    #[tokio::test]
    async fn test_ingest_artifact_nonexistent_intent_version_rejected() {
        // Prevalidation: referenced IntentVersion nodes must exist
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "artifact-with-bad-ref".to_string(),
            depends_on_intent_versions: vec![nonexistent_id], // Does not exist!
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_artifact_wrong_node_type_rejected() {
        // Prevalidation: referenced nodes must be of type IntentVersion
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an Artifact node (wrong type - should be IntentVersion)
        let mut wrong_node_req = create_test_node_request();
        wrong_node_req.tenant_id = tenant_id;
        wrong_node_req.workflow_id = workflow_id;
        wrong_node_req.node_type = NodeType::Artifact; // Wrong type!
        let wrong_node = service.add_node(wrong_node_req).await.unwrap();

        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "artifact-with-wrong-type".to_string(),
            depends_on_intent_versions: vec![wrong_node.id], // Not an IntentVersion!
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        // Verify no artifact node was created with this specific label (prevalidation prevents partial state)
        let nodes = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                workflow_id: Some(workflow_id),
                node_type: Some(NodeType::Artifact),
                ..Default::default()
            })
            .await
            .unwrap();
        // Only the wrong_node exists (which is not an ingested artifact), not a new artifact
        assert_eq!(
            nodes.len(),
            1,
            "Only wrong_node should exist, not a newly ingested artifact"
        );
        assert_eq!(
            nodes[0].id, wrong_node.id,
            "The only artifact should be wrong_node"
        );
    }

    #[tokio::test]
    async fn test_ingest_approval_nonexistent_policy_snapshot_rejected() {
        // Prevalidation: PolicySnapshot reference must exist
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let approval_req = ApprovalIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Approval,
                ref_id: Uuid::new_v4(),
            },
            label: "approval-with-bad-policy".to_string(),
            governed_by_policy_snapshot: Some(nonexistent_id), // Does not exist!
            intent_version_id: None,
            properties: None,
        };

        let result = service.ingest_approval(approval_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_approval_nonexistent_intent_version_rejected() {
        // Prevalidation: IntentVersion reference must exist
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let approval_req = ApprovalIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Approval,
                ref_id: Uuid::new_v4(),
            },
            label: "approval-with-bad-iv".to_string(),
            governed_by_policy_snapshot: None,
            intent_version_id: Some(nonexistent_id), // Does not exist!
            properties: None,
        };

        let result = service.ingest_approval(approval_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_side_effect_nonexistent_trigger_rejected() {
        // Prevalidation: triggered_by_task must exist
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "side-effect-with-bad-trigger".to_string(),
            triggered_by_task: nonexistent_id, // Does not exist!
            derived_from_intent_version: None,
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_side_effect_nonexistent_intent_version_rejected() {
        // Prevalidation: derived_from_intent_version must exist and be IntentVersion
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a valid triggering node
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic;
        let task_node = service.add_node(task_req).await.unwrap();

        let nonexistent_id = Uuid::new_v4();

        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "side-effect-with-bad-iv".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: Some(nonexistent_id), // Does not exist!
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_side_effect_nonexistent_approval_rejected() {
        // Prevalidation: approval_snapshot_id must exist and be Approval
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a valid triggering node
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic;
        let task_node = service.add_node(task_req).await.unwrap();

        let nonexistent_id = Uuid::new_v4();

        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "side-effect-with-bad-approval".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: None,
            approval_snapshot_id: Some(nonexistent_id), // Does not exist!
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));
    }

    #[tokio::test]
    async fn test_ingest_side_effect_wrong_node_type_for_intent_version_rejected() {
        // Prevalidation: derived_from_intent_version must be of type IntentVersion
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a valid triggering node
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic;
        let task_node = service.add_node(task_req).await.unwrap();

        // Create a node of wrong type (Artifact instead of IntentVersion)
        let mut wrong_node_req = create_test_node_request();
        wrong_node_req.tenant_id = tenant_id;
        wrong_node_req.workflow_id = workflow_id;
        wrong_node_req.node_type = NodeType::Artifact; // Wrong type!
        let wrong_node = service.add_node(wrong_node_req).await.unwrap();

        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "side-effect-with-wrong-iv-type".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: Some(wrong_node.id), // Not an IntentVersion!
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        // Verify no side effect node was created (prevalidation prevents partial state)
        let nodes = service
            .list_nodes(GraphNodeFilter {
                node_type: Some(NodeType::SideEffect),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            nodes.is_empty(),
            "No side effect should be created when prevalidation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_artifact_no_partial_state_on_failure() {
        // Verify that when prevalidation fails, no artifact node is created
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest artifact with nonexistent IntentVersion
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "should-not-be-created".to_string(),
            depends_on_intent_versions: vec![nonexistent_id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());

        // Count nodes after - should be same as before (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when ingest fails prevalidation"
        );
    }

    #[tokio::test]
    async fn test_ingest_side_effect_no_partial_state_on_failure() {
        // Verify that when prevalidation fails, no side effect node is created
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a valid triggering node
        let mut task_req = create_test_node_request();
        task_req.tenant_id = tenant_id;
        task_req.workflow_id = workflow_id;
        task_req.node_type = NodeType::Generic;
        let task_node = service.add_node(task_req).await.unwrap();

        // Count nodes before (should have 1 - the task node)
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest side effect with nonexistent IntentVersion
        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "should-not-be-created".to_string(),
            triggered_by_task: task_node.id,
            derived_from_intent_version: Some(Uuid::new_v4()), // Nonexistent IntentVersion
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());

        // Count nodes after - should be same as before (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when ingest fails prevalidation"
        );
    }

    // ===== Cross-Tenant/Workflow Scope Validation Tests =====

    #[tokio::test]
    async fn test_ingest_artifact_cross_tenant_rejected_no_partial_state() {
        // Artifact with tenant A cannot depend on IntentVersion with tenant B
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion in tenant B
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_b;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest artifact in tenant A depending on IntentVersion in tenant B
        let artifact_req = ArtifactIngestRequest {
            tenant_id: tenant_a,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "cross-tenant-artifact".to_string(),
            depends_on_intent_versions: vec![iv.id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));

        // Verify no artifact node was created (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when cross-tenant scope validation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_artifact_cross_workflow_rejected_no_partial_state() {
        // Artifact with workflow A cannot depend on IntentVersion with workflow B
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_a = Uuid::new_v4();
        let workflow_b = Uuid::new_v4();

        // Create IntentVersion in workflow B
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_b;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest artifact in workflow A depending on IntentVersion in workflow B
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id: workflow_a,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "cross-workflow-artifact".to_string(),
            depends_on_intent_versions: vec![iv.id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));

        // Verify no artifact node was created (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when cross-workflow scope validation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_approval_cross_tenant_rejected_no_partial_state() {
        // Approval with tenant A cannot reference PolicySnapshot with tenant B
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create PolicySnapshot in tenant B
        let mut ps_req = create_test_node_request();
        ps_req.tenant_id = tenant_b;
        ps_req.workflow_id = workflow_id;
        ps_req.node_type = NodeType::PolicySnapshot;
        let ps = service.add_node(ps_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest approval in tenant A referencing PolicySnapshot in tenant B
        let approval_req = ApprovalIngestRequest {
            tenant_id: tenant_a,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Approval,
                ref_id: Uuid::new_v4(),
            },
            label: "cross-tenant-approval".to_string(),
            governed_by_policy_snapshot: Some(ps.id),
            intent_version_id: None,
            properties: None,
        };

        let result = service.ingest_approval(approval_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));

        // Verify no approval node was created (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when cross-tenant scope validation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_side_effect_cross_tenant_trigger_rejected_no_partial_state() {
        // SideEffect with tenant A cannot be triggered by node with tenant B
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create trigger node in tenant B
        let mut trigger_req = create_test_node_request();
        trigger_req.tenant_id = tenant_b;
        trigger_req.workflow_id = workflow_id;
        trigger_req.node_type = NodeType::Generic;
        let trigger = service.add_node(trigger_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest side effect in tenant A triggered by node in tenant B
        let side_effect_req = SideEffectIngestRequest {
            tenant_id: tenant_a,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "cross-tenant-side-effect".to_string(),
            triggered_by_task: trigger.id,
            derived_from_intent_version: None,
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));

        // Verify no side effect node was created (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when cross-tenant scope validation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_side_effect_cross_workflow_trigger_rejected_no_partial_state() {
        // SideEffect with workflow A cannot be triggered by node with workflow B
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_a = Uuid::new_v4();
        let workflow_b = Uuid::new_v4();

        // Create trigger node in workflow B
        let mut trigger_req = create_test_node_request();
        trigger_req.tenant_id = tenant_id;
        trigger_req.workflow_id = workflow_b;
        trigger_req.node_type = NodeType::Generic;
        let trigger = service.add_node(trigger_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Attempt to ingest side effect in workflow A triggered by node in workflow B
        let side_effect_req = SideEffectIngestRequest {
            tenant_id,
            workflow_id: workflow_a,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::SideEffect,
                ref_id: Uuid::new_v4(),
            },
            label: "cross-workflow-side-effect".to_string(),
            triggered_by_task: trigger.id,
            derived_from_intent_version: None,
            approval_snapshot_id: None,
            properties: None,
        };

        let result = service.ingest_side_effect(side_effect_req).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::InvalidIngestRequest(_)
        ));

        // Verify no side effect node was created (no partial state)
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_a),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before, nodes_after,
            "No nodes should be created when cross-workflow scope validation fails"
        );
    }

    #[tokio::test]
    async fn test_ingest_artifact_same_scope_succeeds() {
        // Verify that artifacts CAN depend on IntentVersions in the same tenant/workflow
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion in same tenant/workflow
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Count nodes before
        let nodes_before = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        // Ingest artifact in same tenant/workflow
        let artifact_req = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "same-scope-artifact".to_string(),
            depends_on_intent_versions: vec![iv.id],
            properties: None,
        };

        let result = service.ingest_artifact(artifact_req).await;
        assert!(result.is_ok());

        // Verify artifact node was created
        let nodes_after = service
            .list_nodes(GraphNodeFilter {
                tenant_id: Some(tenant_id),
                ..Default::default()
            })
            .await
            .unwrap()
            .len();

        assert_eq!(
            nodes_before + 1,
            nodes_after,
            "Artifact node should be created"
        );
    }

    // ===== Classification Tests =====

    #[tokio::test]
    async fn test_classify_direct_impact_single_hop() {
        // Graph: IntentVersion IV1 -> (DependsOn) -> Artifact A1
        // When we classify from IV1, A1 should be Direct impact
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Create Artifact that depends on it
        let mut artifact_req = create_test_node_request();
        artifact_req.tenant_id = tenant_id;
        artifact_req.workflow_id = workflow_id;
        artifact_req.node_type = NodeType::Artifact;
        let artifact = service.add_node(artifact_req).await.unwrap();

        // Create DependsOn edge: Artifact -> IntentVersion
        let edge_req = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(edge_req).await.unwrap();

        // Classify from IntentVersion
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact]),
                propagation_config: None,
            })
            .await
            .unwrap();

        assert_eq!(result.start_node_id, iv.id);
        assert_eq!(result.max_depth, 3);
        assert_eq!(result.classified_nodes.len(), 1);

        let classified = &result.classified_nodes[0];
        assert_eq!(classified.node.id, artifact.id);
        assert_eq!(classified.impact, ClassificationImpact::Direct);
        assert!(classified.reason.contains("depends on"));
    }

    #[tokio::test]
    async fn test_classify_transitive_impact_two_hops() {
        // Graph: IntentVersion IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
        // When we classify from IV1, A1 should be Direct and SE1 should be Transitive
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Create Artifact
        let mut artifact_req = create_test_node_request();
        artifact_req.tenant_id = tenant_id;
        artifact_req.workflow_id = workflow_id;
        artifact_req.node_type = NodeType::Artifact;
        let artifact = service.add_node(artifact_req).await.unwrap();

        // Create Generic trigger node
        let mut trigger_req = create_test_node_request();
        trigger_req.tenant_id = tenant_id;
        trigger_req.workflow_id = workflow_id;
        trigger_req.node_type = NodeType::Generic;
        let trigger = service.add_node(trigger_req).await.unwrap();

        // Create SideEffect
        let mut side_effect_req = create_test_node_request();
        side_effect_req.tenant_id = tenant_id;
        side_effect_req.workflow_id = workflow_id;
        side_effect_req.node_type = NodeType::SideEffect;
        let side_effect = service.add_node(side_effect_req).await.unwrap();

        // Create edges: Artifact -> IntentVersion, SideEffect -> Artifact (via Triggers)
        let edge1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(edge1).await.unwrap();

        let edge2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: trigger.id,
            to_node_id: side_effect.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(edge2).await.unwrap();

        // Wire: artifact triggers trigger node (so we get a chain)
        let edge3 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: trigger.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(edge3).await.unwrap();

        // Classify from IntentVersion
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // Should find: Artifact (Direct), Trigger (Direct), SideEffect (Transitive)
        assert_eq!(result.start_node_id, iv.id);

        // Find artifact and side_effect in classified
        let artifact_classified = result
            .classified_nodes
            .iter()
            .find(|c| c.node.id == artifact.id);
        let side_effect_classified = result
            .classified_nodes
            .iter()
            .find(|c| c.node.id == side_effect.id);

        assert!(artifact_classified.is_some());
        assert_eq!(
            artifact_classified.unwrap().impact,
            ClassificationImpact::Direct
        );

        assert!(side_effect_classified.is_some());
        assert_eq!(
            side_effect_classified.unwrap().impact,
            ClassificationImpact::Transitive
        );
    }

    #[tokio::test]
    async fn test_classify_no_impact_unreachable_node() {
        // Graph: IntentVersion IV1 -> Artifact A1
        //                    (separate) IV2 -> Artifact A2
        // IV1 classify should only find A1, not A2
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IV1 and A1
        let mut iv1_req = create_test_node_request();
        iv1_req.tenant_id = tenant_id;
        iv1_req.workflow_id = workflow_id;
        iv1_req.node_type = NodeType::IntentVersion;
        let iv1 = service.add_node(iv1_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        // Create IV2 and A2 (not connected to IV1)
        let mut iv2_req = create_test_node_request();
        iv2_req.tenant_id = tenant_id;
        iv2_req.workflow_id = workflow_id;
        iv2_req.node_type = NodeType::IntentVersion;
        let _iv2 = service.add_node(iv2_req).await.unwrap();

        let mut a2_req = create_test_node_request();
        a2_req.tenant_id = tenant_id;
        a2_req.workflow_id = workflow_id;
        a2_req.node_type = NodeType::Artifact;
        let _a2 = service.add_node(a2_req).await.unwrap();

        // Connect IV1 -> A1 only
        let edge = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv1.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(edge).await.unwrap();

        // Classify from IV1
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv1.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // Only A1 should be classified
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(result.classified_nodes[0].node.id, a1.id);
    }

    #[tokio::test]
    async fn test_classify_max_depth_bounds_traversal() {
        // Graph: IV1 -> A1 -> A2 -> A3
        // With max_depth=2, only A1 and A2 should be found
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create nodes
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut a2_req = create_test_node_request();
        a2_req.tenant_id = tenant_id;
        a2_req.workflow_id = workflow_id;
        a2_req.node_type = NodeType::Artifact;
        let a2 = service.add_node(a2_req).await.unwrap();

        let mut a3_req = create_test_node_request();
        a3_req.tenant_id = tenant_id;
        a3_req.workflow_id = workflow_id;
        a3_req.node_type = NodeType::Artifact;
        let a3 = service.add_node(a3_req).await.unwrap();

        // Create chain: A1->IV1, A2->A1, A3->A2
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a2.id,
            to_node_id: a1.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        let e3 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a3.id,
            to_node_id: a2.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e3).await.unwrap();

        // Classify with max_depth=2
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(2),
                target_node_types: Some(vec![NodeType::Artifact]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // A1 (depth 1) and A2 (depth 2) should be found
        assert_eq!(result.classified_nodes.len(), 2);
        let ids: Vec<_> = result.classified_nodes.iter().map(|c| c.node.id).collect();
        assert!(ids.contains(&a1.id));
        assert!(ids.contains(&a2.id));
        assert!(!ids.contains(&a3.id));
    }

    #[tokio::test]
    async fn test_classify_start_node_not_found() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: Uuid::new_v4(),
                max_depth: Some(3),
                target_node_types: None,
                propagation_config: None,
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::GraphNodeNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_classify_empty_graph() {
        // Start node exists but no outgoing edges
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: None,
                propagation_config: None,
            })
            .await
            .unwrap();

        assert_eq!(result.classified_nodes.len(), 0);
        assert_eq!(result.start_node_id, iv.id);
    }

    #[tokio::test]
    async fn test_classify_diamond_graph_reaches_node_once() {
        // Diamond: IV1 -> A1, IV1 -> A2, A1 -> A3, A2 -> A3
        // A3 should appear once with the shortest path reason
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create nodes
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut a2_req = create_test_node_request();
        a2_req.tenant_id = tenant_id;
        a2_req.workflow_id = workflow_id;
        a2_req.node_type = NodeType::Artifact;
        let a2 = service.add_node(a2_req).await.unwrap();

        let mut a3_req = create_test_node_request();
        a3_req.tenant_id = tenant_id;
        a3_req.workflow_id = workflow_id;
        a3_req.node_type = NodeType::Artifact;
        let a3 = service.add_node(a3_req).await.unwrap();

        // Create edges: A1->IV1, A2->IV1, A3->A1, A3->A2
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a2.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        let e3 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a3.id,
            to_node_id: a1.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e3).await.unwrap();

        let e4 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a3.id,
            to_node_id: a2.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e4).await.unwrap();

        // Classify from IV1
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // A3 should appear exactly once (visited once despite two paths)
        let a3_classified: Vec<_> = result
            .classified_nodes
            .iter()
            .filter(|c| c.node.id == a3.id)
            .collect();
        assert_eq!(a3_classified.len(), 1);
        // A3 should be transitive (depth 2)
        assert_eq!(a3_classified[0].impact, ClassificationImpact::Transitive);
    }

    // ===== PR #13 Rule-Pack Propagation Config Tests =====

    #[tokio::test]
    async fn test_classify_propagation_config_default_backward_compat() {
        // When propagation_config is None, should use DEFAULT_PROPAGATION_CONFIG
        // This test verifies backward compatibility - existing behavior preserved
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Create Artifact
        let mut artifact_req = create_test_node_request();
        artifact_req.tenant_id = tenant_id;
        artifact_req.workflow_id = workflow_id;
        artifact_req.node_type = NodeType::Artifact;
        let artifact = service.add_node(artifact_req).await.unwrap();

        // Create DependsOn edge
        let edge_req = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(edge_req).await.unwrap();

        // Classify with propagation_config = None (should use default)
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // Should find the artifact as Direct
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(
            result.classified_nodes[0].impact,
            ClassificationImpact::Direct
        );
    }

    #[tokio::test]
    async fn test_classify_propagation_config_custom_max_depth() {
        // Custom propagation config with max_depth=1 should not find transitive nodes
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create chain: IV1 -> A1 -> A2
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut a2_req = create_test_node_request();
        a2_req.tenant_id = tenant_id;
        a2_req.workflow_id = workflow_id;
        a2_req.node_type = NodeType::Artifact;
        let a2 = service.add_node(a2_req).await.unwrap();

        // A1 -> IV1, A2 -> A1
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a2.id,
            to_node_id: a1.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        // Custom config with max_depth=1
        let custom_config = PropagationConfig {
            max_depth: Some(1),
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![NodeType::Artifact],
        };

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: None,         // Should be overridden by config
                target_node_types: None, // Should be overridden by config
                propagation_config: Some(custom_config),
            })
            .await
            .unwrap();

        // With max_depth=1, only A1 (Direct) should be found, not A2 (Transitive)
        assert_eq!(result.max_depth, 1);
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(result.classified_nodes[0].node.id, a1.id);
        assert_eq!(
            result.classified_nodes[0].impact,
            ClassificationImpact::Direct
        );
    }

    #[tokio::test]
    async fn test_classify_propagation_config_custom_target_types() {
        // Custom config targeting only SideEffect should not classify Artifacts
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        // Create Artifact
        let mut artifact_req = create_test_node_request();
        artifact_req.tenant_id = tenant_id;
        artifact_req.workflow_id = workflow_id;
        artifact_req.node_type = NodeType::Artifact;
        let artifact = service.add_node(artifact_req).await.unwrap();

        // Create SideEffect
        let mut se_req = create_test_node_request();
        se_req.tenant_id = tenant_id;
        se_req.workflow_id = workflow_id;
        se_req.node_type = NodeType::SideEffect;
        let side_effect = service.add_node(se_req).await.unwrap();

        // Create edges: Artifact -> IV1 (DependsOn), Artifact -> SideEffect (Triggers)
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: side_effect.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        // Custom config targeting only SideEffect
        let custom_config = PropagationConfig {
            max_depth: Some(3),
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![NodeType::SideEffect], // Only SideEffect!
        };

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: None,
                propagation_config: Some(custom_config),
            })
            .await
            .unwrap();

        // Only SideEffect should be classified, not Artifact
        // Note: The propagation still goes through Artifact to reach SideEffect,
        // but Artifact itself is not classified.
        // SideEffect is at depth 2 (transitive via Artifact -> SideEffect),
        // so it should be classified as Transitive, not Direct.
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(result.classified_nodes[0].node.id, side_effect.id);
        assert_eq!(
            result.classified_nodes[0].impact,
            ClassificationImpact::Transitive
        );
    }

    #[tokio::test]
    async fn test_classify_propagation_config_empty_edge_types() {
        // With empty traversable_edge_types, no nodes should be reached
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion and Artifact
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut artifact_req = create_test_node_request();
        artifact_req.tenant_id = tenant_id;
        artifact_req.workflow_id = workflow_id;
        artifact_req.node_type = NodeType::Artifact;
        let artifact = service.add_node(artifact_req).await.unwrap();

        // Create DependsOn edge
        let edge_req = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(edge_req).await.unwrap();

        // Custom config with EMPTY traversable_edge_types
        let custom_config = PropagationConfig {
            max_depth: Some(3),
            traversable_edge_types: vec![], // Nothing traversable!
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![NodeType::Artifact],
        };

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: None,
                propagation_config: Some(custom_config),
            })
            .await
            .unwrap();

        // No edges should be traversed, so no nodes classified
        assert!(result.classified_nodes.is_empty());
    }

    #[tokio::test]
    async fn test_classify_propagation_config_max_depth_from_request() {
        // When config.max_depth is None, should fall back to request.max_depth
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create chain: IV1 -> A1 -> A2
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut a2_req = create_test_node_request();
        a2_req.tenant_id = tenant_id;
        a2_req.workflow_id = workflow_id;
        a2_req.node_type = NodeType::Artifact;
        let a2 = service.add_node(a2_req).await.unwrap();

        // A1 -> IV1, A2 -> A1
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a2.id,
            to_node_id: a1.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        // Config with max_depth=None, but request with max_depth=2
        let custom_config = PropagationConfig {
            max_depth: None, // Fall back to request
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![NodeType::Artifact],
        };

        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(2), // Should be used since config.max_depth is None
                target_node_types: None,
                propagation_config: Some(custom_config),
            })
            .await
            .unwrap();

        // max_depth should be 2 from request
        assert_eq!(result.max_depth, 2);
        // A1 (Direct) and A2 (Transitive) should be found
        assert_eq!(result.classified_nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_classify_propagation_config_reaches_approval_via_generated_from() {
        // Graph: IV1 -> A1 (DependsOn), A1 -> SE1 (Triggers), SE1 -> AP1 (GeneratedFrom)
        // Starting from IV1, we should reach AP1 via the chain
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create nodes
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut se_req = create_test_node_request();
        se_req.tenant_id = tenant_id;
        se_req.workflow_id = workflow_id;
        se_req.node_type = NodeType::SideEffect;
        let se1 = service.add_node(se_req).await.unwrap();

        let mut ap_req = create_test_node_request();
        ap_req.tenant_id = tenant_id;
        ap_req.workflow_id = workflow_id;
        ap_req.node_type = NodeType::Approval;
        let ap1 = service.add_node(ap_req).await.unwrap();

        // Create edges: A1->IV1, A1->SE1, SE1->AP1
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: se1.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        let e3 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: se1.id,
            to_node_id: ap1.id,
            edge_type: EdgeType::GeneratedFrom,
            properties: None,
        };
        service.add_edge(e3).await.unwrap();

        // Default config should reach all node types
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![
                    NodeType::Artifact,
                    NodeType::SideEffect,
                    NodeType::Approval,
                ]),
                propagation_config: None,
            })
            .await
            .unwrap();

        // Should find all three: A1 (Direct), SE1 (Transitive), AP1 (Transitive)
        assert_eq!(result.classified_nodes.len(), 3);
        let ids: Vec<_> = result.classified_nodes.iter().map(|c| c.node.id).collect();
        assert!(ids.contains(&a1.id));
        assert!(ids.contains(&se1.id));
        assert!(ids.contains(&ap1.id));
    }

    // ===== PR #13 Backward Compat: target_node_types fallback Tests =====

    #[tokio::test]
    async fn test_classify_backward_compat_request_target_types_when_config_none() {
        // PR #13 fix: When propagation_config is None AND request.target_node_types is Some,
        // the request's target_node_types should be used (backward compat for existing callers).
        //
        // Graph: IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
        // With request.target_node_types = Some([SideEffect]) and propagation_config = None,
        // only SE1 should be classified, NOT A1.
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create nodes
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut se_req = create_test_node_request();
        se_req.tenant_id = tenant_id;
        se_req.workflow_id = workflow_id;
        se_req.node_type = NodeType::SideEffect;
        let se1 = service.add_node(se_req).await.unwrap();

        // Create edges: Artifact -> IV1, Artifact -> SE1
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: se1.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        // Classify with propagation_config=None but request.target_node_types=Some([SideEffect])
        // This should classify ONLY SideEffect, not Artifact
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::SideEffect]), // Only SideEffect!
                propagation_config: None, // Uses default config, but should fall back to request types
            })
            .await
            .unwrap();

        // Should classify only SideEffect (at depth 2, transitive)
        // Artifact should NOT be classified because it's not in request.target_node_types
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(result.classified_nodes[0].node.id, se1.id);
        assert_eq!(
            result.classified_nodes[0].impact,
            ClassificationImpact::Transitive
        );
        // Verify Artifact A1 is NOT in the results
        let artifact_in_results = result.classified_nodes.iter().any(|c| c.node.id == a1.id);
        assert!(
            !artifact_in_results,
            "Artifact should NOT be classified when only SideEffect is in target_node_types"
        );
    }

    #[tokio::test]
    async fn test_classify_request_target_types_ignored_when_config_provided() {
        // When propagation_config is Some, config.target_node_types takes precedence
        // over request.target_node_types (new behavior with explicit config).
        //
        // Graph: IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
        // With config.target_node_types = [SideEffect], only SE1 should be classified.
        let repo = Arc::new(InMemoryGraphRepository::new());
        let service = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create nodes
        let mut iv_req = create_test_node_request();
        iv_req.tenant_id = tenant_id;
        iv_req.workflow_id = workflow_id;
        iv_req.node_type = NodeType::IntentVersion;
        let iv = service.add_node(iv_req).await.unwrap();

        let mut a1_req = create_test_node_request();
        a1_req.tenant_id = tenant_id;
        a1_req.workflow_id = workflow_id;
        a1_req.node_type = NodeType::Artifact;
        let a1 = service.add_node(a1_req).await.unwrap();

        let mut se_req = create_test_node_request();
        se_req.tenant_id = tenant_id;
        se_req.workflow_id = workflow_id;
        se_req.node_type = NodeType::SideEffect;
        let se1 = service.add_node(se_req).await.unwrap();

        // Create edges: Artifact -> IV1, Artifact -> SE1
        let e1 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: iv.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        };
        service.add_edge(e1).await.unwrap();

        let e2 = CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: a1.id,
            to_node_id: se1.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        };
        service.add_edge(e2).await.unwrap();

        // Config that only targets SideEffect
        let config = PropagationConfig {
            max_depth: Some(3),
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![intent_rebase_types::EdgeDirection::Both],
            target_node_types: vec![NodeType::SideEffect], // Only SideEffect in config
        };

        // Classify with explicit config (request.target_node_types is ignored)
        let result = service
            .classify_impact(ClassifyRequest {
                start_node_id: iv.id,
                max_depth: Some(3),
                target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]), // Ignored
                propagation_config: Some(config),
            })
            .await
            .unwrap();

        // Should classify only SideEffect (config takes precedence)
        assert_eq!(result.classified_nodes.len(), 1);
        assert_eq!(result.classified_nodes[0].node.id, se1.id);
    }
}
