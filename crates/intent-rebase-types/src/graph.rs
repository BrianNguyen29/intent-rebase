//! Dependency graph domain types
//!
//! Phase 1 baseline: storage-first graph with Postgres-backed relational edge tables.
/// Provides graph traversal primitives (BFS reachability, path-finding, cycle detection)
/// for impact classification work.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Side effect severity class for capture context.
///
/// This is a simplified representation used in cross-service communication
/// (intent-rebase-types -> compensation-service). The actual SideEffectClass
/// with full compensation semantics is defined in compensation-service.
///
/// | Class | Description |
/// |-------|-------------|
/// | s0_pure_read | Pure read, no side effect |
/// | s1_internal_reversible | Internal reversible |
/// | s2_external_reversible | External reversible (default) |
/// | s3_external_partially_reversible | External partially reversible |
/// | s4_irreversible | Irreversible |
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    #[default]
    S2ExternalReversible,
    S0PureRead,
    S1InternalReversible,
    S3ExternalPartiallyReversible,
    S4Irreversible,
}

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum NodeState {
    #[default]
    Active,
    Stale,
    Invalid,
    Archived,
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
///
/// # Phase 3 Batch 1 (groundwork): Optional Side Effect Capture
/// When `side_effect_context` is provided with sufficient fields, the artifact ingest
/// path can optionally record a side effect to the compensation ledger. This enables
/// capture-on-write for artifact-producing operations that have proper intent/version context.
/// Not all artifact-producing operations will have this context available — this is
/// acceptable as partial capture groundwork.
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
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    /// Phase 3 Batch 1 (groundwork): Optional context for side effect capture.
    /// When provided with sufficient fields, enables capture-on-write to the
    /// compensation ledger. This is optional and may not be available for all
    /// artifact-producing operations.
    #[serde(default)]
    pub side_effect_context: Option<SideEffectCaptureContext>,
}

impl Default for ArtifactIngestRequest {
    fn default() -> Self {
        Self {
            // Note: tenant_id, workflow_id, external_ref, label, and depends_on_intent_versions
            // have no sensible defaults and must be provided by callers.
            // This Default impl exists primarily to allow struct update syntax
            // (e.g., `reqeust.with_side_effect_context()` pattern) in the future.
            tenant_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::nil(),
            },
            label: String::new(),
            depends_on_intent_versions: Vec::new(),
            properties: None,
            side_effect_context: None,
        }
    }
}

impl ArtifactIngestRequest {
    /// Create an ArtifactIngestRequest with only the required fields.
    /// Useful for tests that don't care about side effect capture.
    pub fn new_minimal(
        tenant_id: Uuid,
        workflow_id: Uuid,
        external_ref: ExternalRef,
        label: String,
        depends_on_intent_versions: Vec<Uuid>,
    ) -> Self {
        Self {
            tenant_id,
            workflow_id,
            external_ref,
            label,
            depends_on_intent_versions,
            properties: None,
            side_effect_context: None,
        }
    }
}

/// Phase 3 Batch 1 (groundwork): Context for optional side effect capture.
///
/// When an artifact-producing operation has proper intent/version context,
/// this struct carries the information needed to record a side effect.
/// All fields are optional to maintain backward compatibility with callers
/// that don't have this context available.
#[derive(Debug, Clone, Deserialize)]
pub struct SideEffectCaptureContext {
    /// The intent ID that produced this artifact
    pub source_intent_id: Uuid,
    /// The intent version at time of artifact production
    pub source_intent_version: i32,
    /// The effect type (e.g., "artifact_created", "deployment_triggered")
    pub effect_type: String,
    /// The effect target (e.g., artifact URL, deployment ID)
    pub target: String,
    /// The side effect severity class (S0-S4)
    /// Defaults to S2ExternalReversible if not specified
    #[serde(default)]
    pub effect_class: Option<SideEffectClass>,
    /// Optional idempotency key to prevent duplicate side effect records
    /// If not provided, a new side effect will be created for each ingest
    #[serde(default)]
    pub idempotency_key: Option<String>,
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
#[derive(Default)]
pub enum EdgeDirection {
    /// Traverse incoming edges (from dependent to dependency)
    Incoming,
    /// Traverse outgoing edges (from dependency to dependent)
    Outgoing,
    /// Traverse both directions
    #[default]
    Both,
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
                EdgeType::ValidatedBy,
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
/// - Edge types: DependsOn (incoming), Triggers (outgoing), GeneratedFrom (outgoing), ValidatedBy (incoming)
/// - Target node types: Artifact, Approval, SideEffect, Generic
pub static DEFAULT_PROPAGATION_CONFIG: once_cell::sync::Lazy<PropagationConfig> =
    once_cell::sync::Lazy::new(PropagationConfig::default);

// ============================================================================
// Rebase Preview Integration Types
// ============================================================================

/// Status indicating whether graph-integrated affected items could be computed
///
/// Used in rebase preview responses to honestly communicate data availability
/// without failing the endpoint when graph coverage is incomplete.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AffectedItemsStatus {
    /// Graph data was available and affected items were successfully classified
    Available,
    /// Graph data was unavailable or the IntentVersion node was not found in the graph.
    /// The affected items arrays may be incomplete or empty.
    #[default]
    Unavailable,
}

/// Preview of affected items for rebase planning (Phase 1 PR #16).
///
/// Replaces the Phase 1 baseline TODO structure with graph-integrated classification.
/// The `status` field indicates whether graph data was available for accurate classification.
///
/// When `status` is `Available`, the classified arrays contain real graph-derived affected items.
/// When `status` is `Unavailable`, the arrays may be incomplete - this is NOT an error condition
/// for the rebase preview endpoint, which remains functional even without graph coverage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedItemsPreview {
    /// Whether graph data was available and classification succeeded
    pub status: AffectedItemsStatus,
    /// List of affected artifact IDs with their impact classification
    #[serde(default)]
    pub affected_artifacts: Vec<AffectedItem>,
    /// List of affected approval IDs requiring revalidation
    #[serde(default)]
    pub affected_approvals: Vec<AffectedItem>,
    /// List of side effects downstream from the changed intent version.
    /// Note: Side effects are classified but compensation actions are NOT generated here
    /// (Phase 2 feature) - this field identifies what MAY need compensation review.
    #[serde(default)]
    pub side_effects: Vec<AffectedItem>,
}

impl AffectedItemsPreview {
    /// Create a preview indicating graph data was unavailable
    pub fn unavailable() -> Self {
        Self {
            status: AffectedItemsStatus::Unavailable,
            affected_artifacts: vec![],
            affected_approvals: vec![],
            side_effects: vec![],
        }
    }

    /// Create a preview from a classification result
    pub fn from_classification(
        artifacts: Vec<AffectedItem>,
        approvals: Vec<AffectedItem>,
        side_effects: Vec<AffectedItem>,
    ) -> Self {
        Self {
            status: AffectedItemsStatus::Available,
            affected_artifacts: artifacts,
            affected_approvals: approvals,
            side_effects,
        }
    }
}

/// A single affected item from graph classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedItem {
    /// The graph node ID of the affected item
    pub node_id: Uuid,
    /// Human-readable label from the graph node
    pub label: String,
    /// The impact classification level
    pub impact: ClassificationImpact,
    /// Human-readable reason for the classification
    pub reason: String,
    /// External reference type if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<ExternalRef>,
}

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
