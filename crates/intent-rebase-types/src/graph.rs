//! Dependency graph domain types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A node in the dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub node_type: NodeType,
    pub intent_id: Option<Uuid>,
    pub artifact_id: Option<Uuid>,
    pub label: String,
    pub properties: serde_json::Value,
}

/// The type of a graph node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Intent,
    Artifact,
    Approval,
    Workflow,
}

/// An edge in the dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub from_node_id: Uuid,
    pub to_node_id: Uuid,
    pub edge_type: EdgeType,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    DependsOn,
    Produces,
    Approves,
    Triggers,
}
