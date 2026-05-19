use intent_rebase_types::{EdgeType, NodeType};
use serde::Deserialize;
use uuid::Uuid;

// =============================================================================
// Graph Query Types
// =============================================================================

/// Query parameters for listing graph nodes
#[derive(Debug, Deserialize)]
pub struct ListGraphNodesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub node_type: Option<NodeType>,
}

/// Query parameters for listing graph edges
#[derive(Debug, Deserialize)]
pub struct ListGraphEdgesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub from_node_id: Option<Uuid>,
    pub edge_type: Option<EdgeType>,
}
