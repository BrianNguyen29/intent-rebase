use super::*;
use intent_rebase_types::{
    ApprovalIngestRequest, ArtifactIngestRequest, ClassificationImpact, EdgeDirection, EdgeType,
    ExternalRef, ExternalRefType, PropagationConfig, SideEffectIngestRequest,
};

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

mod classification;
mod core;
mod ingestor;
mod traversal;
