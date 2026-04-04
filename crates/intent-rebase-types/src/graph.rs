//! Dependency graph domain types
//!
//! Phase 1 baseline: storage-first graph with Postgres-backed relational edge tables.
//! Provides graph traversal primitives (BFS reachability, path-finding, cycle detection)
//! for impact classification work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A node in the dependency graph
///
/// Phase 1: Nodes are scoped to a tenant+workflow and reference external entities
/// (intent_id, artifact_id) when applicable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub node_type: NodeType,
    pub external_ref: Option<ExternalRef>,
    pub label: String,
    pub state: NodeState,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Reference to an external entity that this node represents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalRef {
    pub ref_type: ExternalRefType,
    pub ref_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExternalRefType {
    Intent,
    IntentVersion,
    Artifact,
    Approval,
    PolicySnapshot,
    SideEffect,
    Checkpoint,
}

/// The type of a graph node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Intent,
    IntentVersion,
    Artifact,
    Approval,
    PolicySnapshot,
    SideEffect,
    Checkpoint,
    Workflow,
    Generic,
}

/// State of a graph node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    Active,
    Stale,
    Invalid,
    Archived,
}

impl Default for NodeState {
    fn default() -> Self {
        NodeState::Active
    }
}

/// An edge in the dependency graph
///
/// Phase 1: Edges represent typed relationships between nodes with optional properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub from_node_id: Uuid,
    pub to_node_id: Uuid,
    pub edge_type: EdgeType,
    pub properties: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Edge type labels for the dependency graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    DependsOn,
    Produces,
    Approves,
    Triggers,
    Defines,
    GeneratedFrom,
    ValidatedBy,
    GovernedBy,
    DerivedFrom,
    StoredIn,
    Supersedes,
    Blocks,
    Compensates,
}

/// Request to create a new graph node
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGraphNodeRequest {
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub node_type: NodeType,
    pub external_ref: Option<ExternalRef>,
    pub label: String,
    pub properties: Option<serde_json::Value>,
}

/// Request to create a new graph edge
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGraphEdgeRequest {
    pub tenant_id: Uuid,
    pub workflow_id: Uuid,
    pub from_node_id: Uuid,
    pub to_node_id: Uuid,
    pub edge_type: EdgeType,
    pub properties: Option<serde_json::Value>,
}

/// Query filter for listing graph nodes
#[derive(Debug, Clone, Default)]
pub struct GraphNodeFilter {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub node_type: Option<NodeType>,
    pub state: Option<NodeState>,
}

/// Query filter for listing graph edges
#[derive(Debug, Clone, Default)]
pub struct GraphEdgeFilter {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub from_node_id: Option<Uuid>,
    pub to_node_id: Option<Uuid>,
    pub edge_type: Option<EdgeType>,
}

/// A path through the graph from one node to another
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPath {
    /// Sequence of node IDs in the path (including start and end)
    pub node_ids: Vec<Uuid>,
    /// Sequence of edge IDs that connect the nodes
    pub edge_ids: Vec<Uuid>,
}

impl GraphPath {
    /// Returns the length of the path (number of hops)
    pub fn len(&self) -> usize {
        self.node_ids.len().saturating_sub(1)
    }

    /// Returns true if the path is empty (no path exists)
    pub fn is_empty(&self) -> bool {
        self.node_ids.is_empty()
    }
}

/// Result of a reachability query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachabilityResult {
    /// All nodes reachable from the source
    pub reachable_nodes: Vec<Uuid>,
    /// Edge IDs used to reach each node (in traversal order)
    pub incoming_edges: Vec<Uuid>,
}

/// Result of a cycle detection query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleDetectionResult {
    /// True if a cycle was detected
    pub has_cycle: bool,
    /// If a cycle exists, one cycle path (node IDs)
    pub cycle_path: Option<Vec<Uuid>>,
}

/// Traversal options for graph queries
#[derive(Debug, Clone)]
pub struct TraversalOptions {
    /// Maximum depth to traverse (None = unlimited)
    pub max_depth: Option<usize>,
    /// Edge types to include in traversal
    pub edge_types: Option<Vec<EdgeType>>,
    /// Node types to include in traversal
    pub node_types: Option<Vec<NodeType>>,
    /// Whether to include the starting node in results
    pub include_start: bool,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            edge_types: None,
            node_types: None,
            include_start: true,
        }
    }
}
