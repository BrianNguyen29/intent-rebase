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

// ============================================================================
// Ingestor Request Types
// ============================================================================

/// Request to ingest an artifact into the graph.
///
/// Creates an Artifact node and wires DependsOn edges to the IntentVersion
/// nodes that this artifact depends on.
///
/// # Contract
/// The `depends_on_intent_versions` field MUST contain at least one IntentVersion node ID.
/// An artifact without any IntentVersion dependency violates the traceability invariant:
/// every artifact must trace upstream to at least one IntentVersion.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the artifact (e.g., from artifact service)
    pub external_ref: ExternalRef,
    /// Human-readable label for the artifact
    pub label: String,
    /// IntentVersion node IDs this artifact depends on
    pub depends_on_intent_versions: Vec<Uuid>,
    /// Optional properties to attach to the artifact node
    pub properties: Option<serde_json::Value>,
}

/// Request to ingest an approval into the graph.
///
/// Creates an Approval node and wires a GovernedBy edge to the PolicySnapshot
/// node that governs this approval.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the approval (e.g., from approval service)
    pub external_ref: ExternalRef,
    /// Human-readable label for the approval
    pub label: String,
    /// PolicySnapshot node ID that governs this approval
    pub governed_by_policy_snapshot: Option<Uuid>,
    /// IntentVersion node ID this approval is associated with
    pub intent_version_id: Option<Uuid>,
    /// Optional properties to attach to the approval node
    pub properties: Option<serde_json::Value>,
}

/// Request to ingest a side effect into the graph.
///
/// Creates a SideEffect node and wires appropriate edges to:
/// - The initiating node that triggered this side effect (Triggers edge)
/// - The IntentVersion (DerivedFrom edge)
/// - The Approval snapshot if applicable (GeneratedFrom edge)
///
/// Note: The `triggered_by_task` field references any graph node (typically a Workflow
/// or Generic node) that initiates the side effect. The node type must exist in the graph.
#[derive(Debug, Clone, Deserialize)]
pub struct SideEffectIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the side effect (e.g., from runtime)
    pub external_ref: ExternalRef,
    /// Human-readable label for the side effect
    pub label: String,
    /// Node ID that triggered this side effect (typically a Workflow or Generic node)
    pub triggered_by_task: Uuid,
    /// IntentVersion this side effect is derived from
    pub derived_from_intent_version: Option<Uuid>,
    /// Approval snapshot if this side effect was taken under an approval
    pub approval_snapshot_id: Option<Uuid>,
    /// Optional properties to attach to the side effect node
    pub properties: Option<serde_json::Value>,
}

/// Result of an ingestor operation, containing created nodes and edges
#[derive(Debug, Clone, Serialize)]
pub struct IngestorResult {
    /// The created graph node
    pub node: GraphNode,
    /// Any edges created during ingestion
    pub edges: Vec<GraphEdge>,
}

// ============================================================================
// Propagation Configuration (Rule Pack Driven)
// ============================================================================

/// Direction for edge traversal during impact propagation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EdgeDirection {
    /// Traverse incoming edges (from dependent to dependency)
    Incoming,
    /// Traverse outgoing edges (from dependency to dependent)
    Outgoing,
    /// Traverse both directions
    Both,
}

impl Default for EdgeDirection {
    fn default() -> Self {
        EdgeDirection::Both
    }
}

/// Configuration for impact propagation through the dependency graph.
///
/// This drives the `classify_impact` behavior and enables rule-pack-driven
/// propagation rules in future PRs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropagationConfig {
    /// Maximum traversal depth for impact propagation (default: 3)
    pub max_depth: Option<usize>,
    /// Edge types to traverse during propagation (default: DependsOn, Triggers, GeneratedFrom)
    pub traversable_edge_types: Vec<EdgeType>,
    /// Directions to traverse for each edge type (default: Both)
    pub traversable_directions: Vec<EdgeDirection>,
    /// Node types that can be affected targets (default: all relevant types)
    pub target_node_types: Vec<NodeType>,
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self {
            max_depth: Some(3),
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![
                NodeType::Artifact,
                NodeType::Approval,
                NodeType::SideEffect,
                NodeType::Generic,
            ],
        }
    }
}

/// Phase 1 default propagation configuration.
///
/// This matches the hardcoded behavior in `classify_impact`:
/// - max_depth: 3
/// - Edge types: DependsOn (incoming), Triggers (outgoing), GeneratedFrom (outgoing)
/// - Target node types: Artifact, Approval, SideEffect, Generic
pub static DEFAULT_PROPAGATION_CONFIG: once_cell::sync::Lazy<PropagationConfig> =
    once_cell::sync::Lazy::new(|| PropagationConfig::default());

// ============================================================================
// Classification Types
// ============================================================================

/// Impact classification level for a node in the dependency graph
///
/// - `Direct`: Node is directly affected by the starting change (1 hop)
/// - `Transitive`: Node is affected through a chain of dependencies (2+ hops)
/// - `Unchanged`: Node exists in the graph but is not affected by the change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClassificationImpact {
    Direct,
    Transitive,
    Unchanged,
}

/// A node classified with its impact level and a human-readable reason
#[derive(Debug, Clone, Serialize)]
pub struct ClassifiedNode {
    /// The graph node that was classified
    pub node: GraphNode,
    /// The impact classification
    pub impact: ClassificationImpact,
    /// Human-readable explanation of why this node was classified this way
    pub reason: String,
}

/// Request to classify the impact of a change originating from a specific node.
///
/// The starting node is typically an IntentVersion that has been modified.
/// The classification traverses downstream to find affected Artifacts, Approvals,
/// and SideEffects within the bounded depth.
#[derive(Debug, Clone)]
pub struct ClassifyRequest {
    /// The node where the change originates (typically an IntentVersion)
    pub start_node_id: Uuid,
    /// Maximum traversal depth for impact propagation (default: 3)
    /// Note: This is used when propagation_config is None; ignored if propagation_config is Some
    pub max_depth: Option<usize>,
    /// Optional filter: only consider these node types as affected targets
    /// Note: This is used when propagation_config is None; ignored if propagation_config is Some
    pub target_node_types: Option<Vec<NodeType>>,
    /// Optional propagation configuration (PR #13 rule-pack-driven baseline).
    /// When Some, this config drives max_depth, traversable edge types/directions,
    /// and target node types. When None, uses DEFAULT_PROPAGATION_CONFIG.
    pub propagation_config: Option<PropagationConfig>,
}

impl Default for ClassifyRequest {
    fn default() -> Self {
        Self {
            start_node_id: Uuid::nil(), // Caller must set this
            max_depth: Some(3),
            target_node_types: None,
            propagation_config: None,
        }
    }
}

/// Result of impact classification, containing all classified nodes
#[derive(Debug, Clone, Serialize)]
pub struct ClassificationResult {
    /// All nodes reachable from the starting node, classified by impact level
    pub classified_nodes: Vec<ClassifiedNode>,
    /// The starting node ID that was used for classification
    pub start_node_id: Uuid,
    /// Maximum depth used in the traversal
    pub max_depth: usize,
}
