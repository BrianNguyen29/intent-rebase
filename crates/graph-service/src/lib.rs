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
use intent_rebase_types::{
    ClassificationResult, ClassifyRequest, CreateGraphEdgeRequest, CreateGraphNodeRequest,
    GraphEdge, GraphEdgeFilter, GraphNode, GraphNodeFilter, IntentRebaseError, NodeState, NodeType,
};
#[allow(unused_imports)]
use intent_rebase_types::{CycleDetectionResult, GraphPath, ReachabilityResult, TraversalOptions};
use std::sync::Arc;
use uuid::Uuid;

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
