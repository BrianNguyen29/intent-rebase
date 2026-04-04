//! Dependency graph domain types
//!
//! Phase 1 baseline: storage-first graph with Postgres-backed relational edge tables.
//! This provides the persisted graph baseline for future traversal/classification work.

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
