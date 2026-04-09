//! Edge re-evaluation for intent change scenarios
//!
//! Phase 2b bounded slice: Re-evaluates graph edges when an intent version changes.
//! Uses existing graph traversal primitives without structural mutations.
//!
//! ## Design Constraints
//!
//! - **Bounded read-only analysis**: Only examines edges, no create/delete
//! - **State-only mutations**: Detected issues marked via state changes (Stale)
//! - **Leverages existing seams**: find_reachable, find_path, list_edges

use crate::GraphService;
use intent_rebase_types::{
    GraphEdge, GraphEdgeFilter, GraphNode, IntentRebaseError, NodeState, NodeType,
};
use uuid::Uuid;

/// Result of edge re-evaluation
#[derive(Debug, Clone)]
pub struct EdgeReevaluationResult {
    /// Total edges examined
    pub edges_examined: usize,
    /// Edges that are still valid
    pub valid_edges: usize,
    /// Edges flagged for review (target node became stale)
    pub flagged_edges: usize,
    /// Edge IDs that were flagged
    pub flagged_edge_ids: Vec<Uuid>,
    /// Human-readable summary
    pub summary: String,
}

/// Edge validation status
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeValidity {
    /// Edge is still valid (both endpoints active)
    Valid,
    /// Edge target is stale (may need re-approval)
    TargetStale,
    /// Edge source is stale
    SourceStale,
}

/// Re-evaluates edges incoming to an intent version node.
///
/// Called when an intent version changes to determine if edges from
/// dependents (artifacts, approvals, side-effects) are still valid.
///
/// Graph direction: Edges point from dependent → dependency:
///   - Artifact → IntentVersion (DependsOn)
///   - Approval → IntentVersion (ValidatedBy)
///   - SideEffect → IntentVersion (DerivedFrom)
///
/// Therefore we examine INCOMING edges to the intent version (edges TO it),
/// not outgoing edges (nothing points FROM an intent version to dependents).
///
/// # Parameters
/// - `graph_service`: Graph service for querying nodes/edges
/// - `intent_version_node_id`: The IntentVersion node that changed
/// - `tenant_id`: Tenant scope
///
/// # Returns
/// - EdgeReevaluationResult with counts and flagged edge IDs
pub async fn reevaluate_edges_from_intent_version(
    graph_service: &GraphService,
    intent_version_node_id: Uuid,
    tenant_id: Uuid,
) -> Result<EdgeReevaluationResult, IntentRebaseError> {
    // Get all incoming edges to the intent version node
    // (edges from dependents: Artifact, Approval, SideEffect → IntentVersion)
    let filter = GraphEdgeFilter {
        tenant_id: Some(tenant_id),
        workflow_id: None,
        from_node_id: None,
        to_node_id: Some(intent_version_node_id),
        edge_type: None,
    };

    let edges: Vec<GraphEdge> = graph_service.list_edges(filter).await?;
    let mut flagged_edge_ids = Vec::new();

    for edge in &edges {
        let validity = evaluate_edge_validity(graph_service, edge).await?;
        if validity != EdgeValidity::Valid {
            flagged_edge_ids.push(edge.id);
        }
    }

    let valid_edges = edges.len().saturating_sub(flagged_edge_ids.len());

    Ok(EdgeReevaluationResult {
        edges_examined: edges.len(),
        valid_edges,
        flagged_edges: flagged_edge_ids.len(),
        flagged_edge_ids: flagged_edge_ids.clone(),
        summary: format!(
            "Examined {} edges from intent version: {} valid, {} flagged",
            edges.len(),
            valid_edges,
            flagged_edge_ids.len()
        ),
    })
}

/// Evaluate a single edge's validity based on endpoint node states.
async fn evaluate_edge_validity(
    graph_service: &GraphService,
    edge: &GraphEdge,
) -> Result<EdgeValidity, IntentRebaseError> {
    // Get source node state
    let source = graph_service.get_node(edge.from_node_id).await?;
    let target = graph_service.get_node(edge.to_node_id).await?;

    // Check if source is stale
    if source.state == NodeState::Stale || source.state == NodeState::Archived {
        return Ok(EdgeValidity::SourceStale);
    }

    // Check if target is stale
    if target.state == NodeState::Stale || target.state == NodeState::Archived {
        return Ok(EdgeValidity::TargetStale);
    }

    Ok(EdgeValidity::Valid)
}

/// Result of orphan detection
#[derive(Debug, Clone)]
pub struct OrphanDetectionResult {
    /// Total artifact nodes examined
    pub artifacts_examined: usize,
    /// Artifacts still reachable from active intent
    pub reachable_artifacts: usize,
    /// Artifacts no longer reachable (orphaned)
    pub orphaned_artifacts: usize,
    /// Artifact node IDs that are orphaned
    pub orphaned_artifact_ids: Vec<Uuid>,
    /// Total side effects examined
    pub side_effects_examined: usize,
    /// Side effects still reachable
    pub reachable_side_effects: usize,
    /// Side effects that are orphaned
    pub orphaned_side_effects: usize,
    /// Side effect node IDs that are orphaned
    pub orphaned_side_effect_ids: Vec<Uuid>,
    /// Human-readable summary
    pub summary: String,
}

/// Detects orphan nodes — nodes no longer reachable from the active intent version.
///
/// An artifact or side effect is "orphaned" when it can no longer trace a path
/// back to an active IntentVersion via DependsOn edges. This happens when an
/// intent version is superseded without proper migration of dependent artifacts.
///
/// Phase 2b bounded slice: Uses existing are_connected check without
/// structural mutations. Orphaned nodes are identified but NOT automatically
/// deleted or archived — state changes are left to caller.
///
/// # Parameters
/// - `graph_service`: Graph service for querying nodes/edges
/// - `intent_version_node_id`: The active IntentVersion node to check reachability from
/// - `tenant_id`: Tenant scope
///
/// # Returns
/// - OrphanDetectionResult with counts and orphaned node IDs
pub async fn detect_orphaned_nodes(
    graph_service: &GraphService,
    intent_version_node_id: Uuid,
    tenant_id: Uuid,
) -> Result<OrphanDetectionResult, IntentRebaseError> {
    // Get the intent version node to determine its workflow_id
    // This scopes orphan detection to the current workflow, not whole tenant
    let intent_version_node = graph_service.get_node(intent_version_node_id).await?;
    let workflow_id = intent_version_node.workflow_id;

    // Get all artifact nodes in this workflow scope
    let all_artifacts: Vec<GraphNode> = graph_service
        .list_nodes(intent_rebase_types::GraphNodeFilter {
            tenant_id: Some(tenant_id),
            workflow_id: Some(workflow_id),
            node_type: Some(NodeType::Artifact),
            state: None,
        })
        .await?;

    // Get all side effect nodes in this workflow scope
    let all_side_effects: Vec<GraphNode> = graph_service
        .list_nodes(intent_rebase_types::GraphNodeFilter {
            tenant_id: Some(tenant_id),
            workflow_id: Some(workflow_id),
            node_type: Some(NodeType::SideEffect),
            state: None,
        })
        .await?;

    let mut orphaned_artifact_ids = Vec::new();
    let mut reachable_artifact_count = 0;

    for artifact in &all_artifacts {
        // Check if artifact has a path to intent version
        let connected = graph_service
            .are_connected(artifact.id, intent_version_node_id, Some(10))
            .await?;
        if connected {
            reachable_artifact_count += 1;
        } else {
            orphaned_artifact_ids.push(artifact.id);
        }
    }

    let mut orphaned_side_effect_ids = Vec::new();
    let mut reachable_side_effect_count = 0;

    for side_effect in &all_side_effects {
        // Check if side effect has a path to intent version
        let connected = graph_service
            .are_connected(side_effect.id, intent_version_node_id, Some(10))
            .await?;
        if connected {
            reachable_side_effect_count += 1;
        } else {
            orphaned_side_effect_ids.push(side_effect.id);
        }
    }

    Ok(OrphanDetectionResult {
        artifacts_examined: all_artifacts.len(),
        reachable_artifacts: reachable_artifact_count,
        orphaned_artifacts: orphaned_artifact_ids.len(),
        orphaned_artifact_ids: orphaned_artifact_ids.clone(),
        side_effects_examined: all_side_effects.len(),
        reachable_side_effects: reachable_side_effect_count,
        orphaned_side_effects: orphaned_side_effect_ids.len(),
        orphaned_side_effect_ids: orphaned_side_effect_ids.clone(),
        summary: format!(
            "Orphan detection: {} artifacts ({} orphaned), {} side effects ({} orphaned)",
            all_artifacts.len(),
            orphaned_artifact_ids.len(),
            all_side_effects.len(),
            orphaned_side_effect_ids.len()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryGraphRepository;
    use intent_rebase_types::{
        CreateGraphEdgeRequest, CreateGraphNodeRequest, EdgeType, ExternalRef, ExternalRefType,
        NodeType,
    };
    use std::sync::Arc;

    fn create_test_intent_version_node(
        tenant_id: Uuid,
        workflow_id: Uuid,
        _intent_id: Uuid,
        version_id: Uuid,
    ) -> CreateGraphNodeRequest {
        CreateGraphNodeRequest {
            tenant_id,
            workflow_id,
            node_type: NodeType::IntentVersion,
            external_ref: Some(ExternalRef {
                ref_type: ExternalRefType::IntentVersion,
                ref_id: version_id,
            }),
            label: "Test IntentVersion".to_string(),
            properties: Some(serde_json::json!({})),
        }
    }

    fn create_test_artifact_node(
        tenant_id: Uuid,
        workflow_id: Uuid,
        artifact_id: Uuid,
    ) -> CreateGraphNodeRequest {
        CreateGraphNodeRequest {
            tenant_id,
            workflow_id,
            node_type: NodeType::Artifact,
            external_ref: Some(ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: artifact_id,
            }),
            label: "Test Artifact".to_string(),
            properties: Some(serde_json::json!({})),
        }
    }

    #[tokio::test]
    async fn test_evaluate_edge_validity_all_active() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent version node (Active)
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Create artifact node (Active)
        let artifact_node = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Create edge
        let edge = svc
            .add_edge(CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: artifact_node.id,
                to_node_id: iv_node.id,
                edge_type: EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();

        let validity = evaluate_edge_validity(&svc, &edge).await.unwrap();
        assert_eq!(validity, EdgeValidity::Valid);
    }

    #[tokio::test]
    async fn test_evaluate_edge_validity_target_stale() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent version node then mark it Stale
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
            ))
            .await
            .unwrap();
        svc.update_node_state(iv_node.id, NodeState::Stale)
            .await
            .unwrap();

        // Create artifact node (Active)
        let artifact_node = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Create edge
        let edge = svc
            .add_edge(CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: artifact_node.id,
                to_node_id: iv_node.id,
                edge_type: EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();

        let validity = evaluate_edge_validity(&svc, &edge).await.unwrap();
        assert_eq!(validity, EdgeValidity::TargetStale);
    }

    #[tokio::test]
    async fn test_orphan_detection_no_orphans() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create IntentVersion node
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                intent_id,
                version_id,
            ))
            .await
            .unwrap();

        // Create artifact that depends on it
        let artifact_id = Uuid::new_v4();
        let artifact_node = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                artifact_id,
            ))
            .await
            .unwrap();

        // Wire: artifact -> intent version
        svc.add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact_node.id,
            to_node_id: iv_node.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

        let result = detect_orphaned_nodes(&svc, iv_node.id, tenant_id)
            .await
            .unwrap();

        // Artifact is reachable, not orphaned
        assert_eq!(result.artifacts_examined, 1);
        assert_eq!(result.orphaned_artifacts, 0);
        assert!(result.orphaned_artifact_ids.is_empty());
    }

    #[tokio::test]
    async fn test_orphan_detection_with_orphans() {
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create IntentVersion node
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                intent_id,
                version_id,
            ))
            .await
            .unwrap();

        // Create artifact that depends on it
        let artifact_id_1 = Uuid::new_v4();
        let artifact_node_1 = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                artifact_id_1,
            ))
            .await
            .unwrap();

        svc.add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact_node_1.id,
            to_node_id: iv_node.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

        // Create orphan artifact (does NOT depend on iv_node)
        let artifact_id_2 = Uuid::new_v4();
        let _orphan_artifact = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                artifact_id_2,
            ))
            .await
            .unwrap();

        let result = detect_orphaned_nodes(&svc, iv_node.id, tenant_id)
            .await
            .unwrap();

        // One artifact reachable, one orphaned
        assert_eq!(result.artifacts_examined, 2);
        assert_eq!(result.reachable_artifacts, 1);
        assert_eq!(result.orphaned_artifacts, 1);
        assert_eq!(result.orphaned_artifact_ids.len(), 1);
    }

    #[tokio::test]
    async fn test_reevaluate_edges_examines_incoming_edges() {
        // This test verifies that reevaluate_edges_from_intent_version examines
        // INCOMING edges (from dependents to intent version), not outgoing.
        //
        // Graph direction: Artifact → IntentVersion (DependsOn)
        // So we look for edges TO the intent version node.
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent version node
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Create artifact that depends on intent version
        let artifact_node = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Edge: artifact -> intent version (incoming to iv_node)
        svc.add_edge(CreateGraphEdgeRequest {
            tenant_id,
            workflow_id,
            from_node_id: artifact_node.id,
            to_node_id: iv_node.id,
            edge_type: EdgeType::DependsOn,
            properties: None,
        })
        .await
        .unwrap();

        // Re-evaluate edges FROM the intent version
        let result = reevaluate_edges_from_intent_version(&svc, iv_node.id, tenant_id)
            .await
            .unwrap();

        // Should find 1 edge (the incoming DependsOn from artifact)
        assert_eq!(result.edges_examined, 1);
        assert_eq!(result.valid_edges, 1);
        assert_eq!(result.flagged_edges, 0);
        assert!(result.flagged_edge_ids.is_empty());
    }

    #[tokio::test]
    async fn test_reevaluate_edges_flags_incoming_when_target_stale() {
        // Verify that when intent version becomes stale, incoming edges are flagged
        let repo = Arc::new(InMemoryGraphRepository::new());
        let svc = GraphService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent version node then mark it Stale
        let iv_node = svc
            .add_node(create_test_intent_version_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
            ))
            .await
            .unwrap();
        svc.update_node_state(iv_node.id, NodeState::Stale)
            .await
            .unwrap();

        // Create artifact that depends on intent version
        let artifact_node = svc
            .add_node(create_test_artifact_node(
                tenant_id,
                workflow_id,
                Uuid::new_v4(),
            ))
            .await
            .unwrap();

        // Edge: artifact -> intent version (incoming to iv_node)
        let edge = svc
            .add_edge(CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: artifact_node.id,
                to_node_id: iv_node.id,
                edge_type: EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();

        // Re-evaluate edges FROM the intent version
        let result = reevaluate_edges_from_intent_version(&svc, iv_node.id, tenant_id)
            .await
            .unwrap();

        // Should find 1 edge and flag it because target (intent version) is stale
        assert_eq!(result.edges_examined, 1);
        assert_eq!(result.valid_edges, 0);
        assert_eq!(result.flagged_edges, 1);
        assert_eq!(result.flagged_edge_ids, vec![edge.id]);
    }
}
