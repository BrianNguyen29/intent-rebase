//! Graph updater — bounded state mutations driven by classification results
//!
//! This module provides bounded graph state update orchestration:
//! - Only node state transitions (Active → Stale/Invalid/Archived)
//! - NO structural mutations (no create/delete nodes or edges)
//! - Driven by classification results from graph-service
//!
//! ## Design Constraints
//!
//! - **State-only mutations**: Only updates `NodeState`, no structural changes
//! - **Bounded depth**: Only processes directly affected items from classification
//! - **Audit trail**: Every mutation is recorded with rationale
//!
//! ## State Transition Rules
//!
//! | From State  | To State  | Reason                          |
//! |-------------|-----------|----------------------------------|
//! | Active      | Stale     | Affected by intent change        |
//! | Active      | Invalid   | Error during rebase processing   |
//! | Stale       | Active    | Re-validated after rebase       |
//! | Stale       | Archived  | Superseded by new version        |
//! | Any         | Invalid   | Critical error (terminal state)  |

use intent_rebase_types::{GraphNode, IntentRebaseError, NodeState, NodeType};
use std::sync::Arc;
use uuid::Uuid;

/// A single graph update action
#[derive(Debug, Clone)]
pub struct GraphUpdateAction {
    /// Node ID that was updated
    pub node_id: Uuid,
    /// Node type
    pub node_type: NodeType,
    /// Node label (for human readability)
    pub label: String,
    /// Previous state before the update
    pub previous_state: NodeState,
    /// New state after the update
    pub new_state: NodeState,
    /// Human-readable reason for the update
    pub reason: String,
}

/// Result of a graph state update operation
#[derive(Debug, Clone)]
pub struct GraphUpdateResult {
    /// Whether the update was successful
    pub success: bool,
    /// The action that was performed
    pub action: Option<GraphUpdateAction>,
    /// Error message if failed
    pub error: Option<String>,
}

impl GraphUpdateResult {
    /// Create a successful result
    pub fn success(action: GraphUpdateAction) -> Self {
        Self {
            success: true,
            action: Some(action),
            error: None,
        }
    }

    /// Create a failed result
    pub fn failure(_node_id: Uuid, error: String) -> Self {
        Self {
            success: false,
            action: None,
            error: Some(error),
        }
    }
}

/// Phase 2b: Signal generated when an artifact is invalidated due to intent change.
///
/// This is metadata only - real S3 quarantine move is Phase 3.
/// The signal can be used to emit ArtifactInvalidated audit events.
#[derive(Debug, Clone)]
pub struct ArtifactInvalidationSignal {
    /// Graph node ID of the invalidated artifact
    pub node_id: Uuid,
    /// Intent ID that caused the invalidation
    pub intent_id: Uuid,
    /// Version range of the intent change
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    /// Reason for invalidation
    pub reason: String,
    /// Who initiated the invalidation
    pub initiated_by: String,
    /// When the invalidation was signaled
    pub signaled_at: chrono::DateTime<chrono::Utc>,
    /// Current quarantine status
    pub quarantine_status: intent_rebase_types::QuarantineStatus,
}

/// Graph updater for bounded state mutations
///
/// This service applies state transitions to graph nodes based on
/// classification results from the graph-service. It only updates
/// node states and does not perform structural mutations.
pub struct GraphUpdater {
    graph_service: Arc<graph_service::GraphService>,
}

impl GraphUpdater {
    /// Create a new GraphUpdater
    pub fn new(graph_service: Arc<graph_service::GraphService>) -> Self {
        Self { graph_service }
    }

    /// Update a node's state if it exists and is in an applicable state.
    ///
    /// This is the primary method for applying state mutations based on
    /// classification results. It will:
    /// 1. Verify the node exists
    /// 2. Check if the transition is valid
    /// 3. Apply the state change
    ///
    /// Returns `GraphUpdateResult` with the action taken or error.
    pub async fn update_node_state_if_affected(
        &self,
        node_id: Uuid,
        new_state: NodeState,
        reason: String,
    ) -> Result<GraphUpdateResult, IntentRebaseError> {
        // Fetch current node state
        let node = match self.graph_service.get_node(node_id).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Node {} not found in graph: {}", node_id, e);
                return Ok(GraphUpdateResult::failure(
                    node_id,
                    format!("Node not found: {}", e),
                ));
            }
        };

        // Validate state transition
        let previous_state = node.state.clone();

        // Check if transition is valid
        if !is_valid_transition(&previous_state, &new_state) {
            tracing::debug!(
                "Skipping invalid transition for node {}: {:?} -> {:?}",
                node_id,
                previous_state,
                new_state
            );
            return Ok(GraphUpdateResult::failure(
                node_id,
                format!(
                    "Invalid transition: {:?} -> {:?}",
                    previous_state, new_state
                ),
            ));
        }

        // Skip if already in target state
        if previous_state == new_state {
            tracing::debug!("Node {} already in state {:?}", node_id, new_state);
            return Ok(GraphUpdateResult::success(GraphUpdateAction {
                node_id,
                node_type: node.node_type.clone(),
                label: node.label.clone(),
                previous_state: previous_state.clone(),
                new_state: new_state.clone(),
                reason: format!("No change needed: already in {:?}", new_state),
            }));
        }

        // Apply the state change
        match self
            .graph_service
            .update_node_state(node_id, new_state.clone())
            .await
        {
            Ok(updated_node) => {
                tracing::info!(
                    "Updated node {} state: {:?} -> {:?} ({})",
                    node_id,
                    previous_state,
                    new_state,
                    reason
                );
                Ok(GraphUpdateResult::success(GraphUpdateAction {
                    node_id,
                    node_type: updated_node.node_type,
                    label: updated_node.label,
                    previous_state,
                    new_state,
                    reason,
                }))
            }
            Err(e) => {
                tracing::error!("Failed to update node {} state: {}", node_id, e);
                Ok(GraphUpdateResult::failure(
                    node_id,
                    format!("Update failed: {}", e),
                ))
            }
        }
    }

    /// Mark all affected artifacts as stale.
    ///
    /// Convenience method for processing affected artifacts from classification.
    pub async fn mark_artifacts_stale(
        &self,
        node_ids: &[Uuid],
        intent_id: Uuid,
        intent_version: i32,
    ) -> Vec<GraphUpdateResult> {
        let reason = format!("Affected by intent {} v{}", intent_id, intent_version);
        let mut results = Vec::new();

        for node_id in node_ids {
            let result = self
                .update_node_state_if_affected(*node_id, NodeState::Stale, reason.clone())
                .await
                .unwrap_or_else(|e| {
                    GraphUpdateResult::failure(*node_id, format!("Internal error: {}", e))
                });
            results.push(result);
        }

        results
    }

    /// Mark all affected approvals as stale.
    ///
    /// Convenience method for processing affected approvals from classification.
    pub async fn mark_approvals_stale(
        &self,
        node_ids: &[Uuid],
        intent_id: Uuid,
        intent_version: i32,
    ) -> Vec<GraphUpdateResult> {
        let reason = format!(
            "Approval revalidation needed for intent {} v{}",
            intent_id, intent_version
        );
        let mut results = Vec::new();

        for node_id in node_ids {
            let result = self
                .update_node_state_if_affected(*node_id, NodeState::Stale, reason.clone())
                .await
                .unwrap_or_else(|e| {
                    GraphUpdateResult::failure(*node_id, format!("Internal error: {}", e))
                });
            results.push(result);
        }

        results
    }

    /// Re-validate nodes after successful rebase.
    ///
    /// This is used when a rebase completes successfully and previously
    /// stale nodes should be marked as active again (or archived if superseded).
    pub async fn revalidate_nodes(
        &self,
        node_ids: &[Uuid],
        new_intent_version: i32,
    ) -> Vec<GraphUpdateResult> {
        let reason = format!(
            "Re-validated after successful rebase to v{}",
            new_intent_version
        );
        let mut results = Vec::new();

        for node_id in node_ids {
            let result = self
                .update_node_state_if_affected(*node_id, NodeState::Active, reason.clone())
                .await
                .unwrap_or_else(|e| {
                    GraphUpdateResult::failure(*node_id, format!("Internal error: {}", e))
                });
            results.push(result);
        }

        results
    }

    /// Archive nodes that are no longer needed.
    ///
    /// This is a terminal state transition - archived nodes should not
    /// be processed further.
    pub async fn archive_nodes(&self, node_ids: &[Uuid], reason: String) -> Vec<GraphUpdateResult> {
        let mut results = Vec::new();

        for node_id in node_ids {
            let result = self
                .update_node_state_if_affected(*node_id, NodeState::Archived, reason.clone())
                .await
                .unwrap_or_else(|e| {
                    GraphUpdateResult::failure(*node_id, format!("Internal error: {}", e))
                });
            results.push(result);
        }

        results
    }

    /// Phase 2b: Generate artifact invalidation signals for affected artifacts.
    ///
    /// This is a bounded slice - it only marks artifacts as Stale in the graph
    /// and generates metadata for audit. Real S3 quarantine move is Phase 3.
    ///
    /// Returns a list of ArtifactInvalidationSignal for each artifact that was invalidated.
    /// The signal can be used to emit ArtifactInvalidated audit events and to prepare
    /// for Phase 3 S3 quarantine move (when artifact service is implemented).
    pub async fn invalidate_artifacts(
        &self,
        node_ids: &[Uuid],
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
        initiated_by: &str,
    ) -> Vec<ArtifactInvalidationSignal> {
        use chrono::Utc;
        use intent_rebase_types::QuarantineStatus;

        let reason = format!(
            "Artifact invalidated due to intent {} change from v{} to v{}",
            intent_id, from_version, to_version
        );
        let signaled_at = Utc::now();

        let mut signals = Vec::new();

        for node_id in node_ids {
            // Update the graph node state to Stale
            let result = self
                .update_node_state_if_affected(*node_id, NodeState::Stale, reason.clone())
                .await;

            if let Ok(ok_result) = result {
                if ok_result.success {
                    // Create the invalidation signal
                    let signal = ArtifactInvalidationSignal {
                        node_id: *node_id,
                        intent_id,
                        intent_version_from: from_version,
                        intent_version_to: to_version,
                        reason: reason.clone(),
                        initiated_by: initiated_by.to_string(),
                        signaled_at,
                        quarantine_status: QuarantineStatus::Signaled,
                    };
                    signals.push(signal);
                }
            }
        }

        signals
    }

    /// Get a summary of graph state for an intent.
    ///
    /// Returns counts of nodes by state and type, useful for debugging
    /// and audit purposes.
    pub async fn get_state_summary(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<GraphStateSummary, IntentRebaseError> {
        use intent_rebase_types::ExternalRefType;

        let filter = intent_rebase_types::GraphNodeFilter {
            tenant_id: Some(tenant_id),
            workflow_id: None,
            node_type: None,
            state: None,
        };

        let nodes = self.graph_service.list_nodes(filter).await?;

        // Filter to nodes related to this intent
        let intent_nodes: Vec<&GraphNode> = nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.external_ref,
                    Some(ref r) if r.ref_type == ExternalRefType::Intent && r.ref_id == intent_id
                )
            })
            .collect();

        let mut summary = GraphStateSummary::default();

        for node in intent_nodes {
            summary.total_nodes += 1;
            match node.state {
                NodeState::Active => summary.active += 1,
                NodeState::Stale => summary.stale += 1,
                NodeState::Invalid => summary.invalid += 1,
                NodeState::Archived => summary.archived += 1,
            }
            match node.node_type {
                NodeType::Artifact => summary.artifacts += 1,
                NodeType::Approval => summary.approvals += 1,
                NodeType::SideEffect => summary.side_effects += 1,
                _ => summary.other += 1,
            }
        }

        Ok(summary)
    }
}

/// State transition validation
fn is_valid_transition(from: &NodeState, to: &NodeState) -> bool {
    // Same state is not really a transition
    if from == to {
        return true; // Allow but will be short-circuited
    }

    match (from, to) {
        // Active can go to Stale, Invalid
        (NodeState::Active, NodeState::Stale) => true,
        (NodeState::Active, NodeState::Invalid) => true,

        // Stale can go to Active (re-validated), Archived, or Invalid
        (NodeState::Stale, NodeState::Active) => true,
        (NodeState::Stale, NodeState::Archived) => true,
        (NodeState::Stale, NodeState::Invalid) => true,

        // Invalid is terminal - should not transition
        (NodeState::Invalid, _) => false,

        // Archived is terminal - should not transition
        (NodeState::Archived, _) => false,

        // Default case
        _ => false,
    }
}

/// Summary of graph node states
#[derive(Debug, Clone, Default)]
pub struct GraphStateSummary {
    pub total_nodes: usize,
    pub active: usize,
    pub stale: usize,
    pub invalid: usize,
    pub archived: usize,
    pub artifacts: usize,
    pub approvals: usize,
    pub side_effects: usize,
    pub other: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::GraphEdge;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Mock graph repository for testing with thread-safe interior mutability
    struct MockGraphRepo {
        nodes: Arc<RwLock<std::collections::HashMap<Uuid, GraphNode>>>,
    }

    impl MockGraphRepo {
        fn new() -> Self {
            Self {
                nodes: Arc::new(RwLock::new(std::collections::HashMap::new())),
            }
        }

        async fn add_node(&self, node: GraphNode) {
            self.nodes.write().await.insert(node.id, node);
        }
    }

    #[async_trait::async_trait]
    impl graph_service::GraphRepository for MockGraphRepo {
        async fn create_node(
            &self,
            _request: intent_rebase_types::CreateGraphNodeRequest,
        ) -> Result<GraphNode, IntentRebaseError> {
            unimplemented!()
        }

        async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError> {
            self.nodes
                .read()
                .await
                .get(&id)
                .cloned()
                .ok_or(IntentRebaseError::GraphNodeNotFound(id))
        }

        async fn list_nodes(
            &self,
            filter: intent_rebase_types::GraphNodeFilter,
        ) -> Result<Vec<GraphNode>, IntentRebaseError> {
            let nodes: Vec<GraphNode> = self.nodes.read().await.values().cloned().collect();
            let mut result = nodes;
            if let Some(tenant_id) = filter.tenant_id {
                result.retain(|n| n.tenant_id == tenant_id);
            }
            Ok(result)
        }

        async fn update_node_state(
            &self,
            id: Uuid,
            state: NodeState,
        ) -> Result<GraphNode, IntentRebaseError> {
            let mut nodes = self.nodes.write().await;
            let node = nodes
                .get_mut(&id)
                .ok_or(IntentRebaseError::GraphNodeNotFound(id))?;
            node.state = state;
            Ok(node.clone())
        }

        async fn create_edge(
            &self,
            _request: intent_rebase_types::CreateGraphEdgeRequest,
        ) -> Result<GraphEdge, IntentRebaseError> {
            unimplemented!()
        }

        async fn get_edge(&self, _id: Uuid) -> Result<GraphEdge, IntentRebaseError> {
            unimplemented!()
        }

        async fn list_edges(
            &self,
            _filter: intent_rebase_types::GraphEdgeFilter,
        ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
            Ok(vec![])
        }

        async fn list_edges_from(
            &self,
            _node_id: Uuid,
        ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
            Ok(vec![])
        }

        async fn list_edges_to(&self, _node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
            Ok(vec![])
        }

        async fn delete_edge(&self, _id: Uuid) -> Result<(), IntentRebaseError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_valid_transition_active_to_stale() {
        let repo = MockGraphRepo::new();

        let node_id = Uuid::new_v4();
        let node = GraphNode {
            id: node_id,
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            node_type: NodeType::Artifact,
            external_ref: None,
            label: "Test".to_string(),
            state: NodeState::Active,
            properties: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        repo.add_node(node).await;

        let graph_service = Arc::new(graph_service::GraphService::new(Arc::new(repo)));
        let updater = GraphUpdater::new(graph_service);

        let result = updater
            .update_node_state_if_affected(node_id, NodeState::Stale, "Test transition".to_string())
            .await
            .unwrap();

        assert!(result.success);
        let action = result.action.unwrap();
        assert_eq!(action.previous_state, NodeState::Active);
        assert_eq!(action.new_state, NodeState::Stale);
    }

    #[tokio::test]
    async fn test_invalid_transition_invalid_to_active() {
        let repo = MockGraphRepo::new();

        let node_id = Uuid::new_v4();
        let node = GraphNode {
            id: node_id,
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            node_type: NodeType::Artifact,
            external_ref: None,
            label: "Test".to_string(),
            state: NodeState::Invalid,
            properties: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        repo.add_node(node).await;

        let graph_service = Arc::new(graph_service::GraphService::new(Arc::new(repo)));
        let updater = GraphUpdater::new(graph_service);

        let result = updater
            .update_node_state_if_affected(
                node_id,
                NodeState::Active,
                "Test transition".to_string(),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid transition"));
    }

    #[tokio::test]
    async fn test_archive_terminal_state() {
        let repo = MockGraphRepo::new();

        let node_id = Uuid::new_v4();
        let node = GraphNode {
            id: node_id,
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            node_type: NodeType::Artifact,
            external_ref: None,
            label: "Test".to_string(),
            state: NodeState::Stale,
            properties: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        repo.add_node(node).await;

        let graph_service = Arc::new(graph_service::GraphService::new(Arc::new(repo)));
        let updater = GraphUpdater::new(graph_service);

        let result = updater
            .update_node_state_if_affected(
                node_id,
                NodeState::Archived,
                "Archive stale node".to_string(),
            )
            .await
            .unwrap();

        assert!(result.success);
        let action = result.action.unwrap();
        assert_eq!(action.new_state, NodeState::Archived);
    }

    #[tokio::test]
    async fn test_node_not_found() {
        let repo = MockGraphRepo::new();
        let graph_service = Arc::new(graph_service::GraphService::new(Arc::new(repo)));
        let updater = GraphUpdater::new(graph_service);

        let result = updater
            .update_node_state_if_affected(Uuid::new_v4(), NodeState::Stale, "Test".to_string())
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_invalidate_artifacts_generates_signals() {
        // Test Phase 2b bounded artifact invalidation signal generation
        let repo = Arc::new(MockGraphRepo::new());

        let node_id = Uuid::new_v4();
        let node = GraphNode {
            id: node_id,
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            node_type: NodeType::Artifact,
            external_ref: None,
            label: "Test Artifact".to_string(),
            state: NodeState::Active,
            properties: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        repo.add_node(node).await;

        let graph_service = Arc::new(graph_service::GraphService::new(repo.clone()));
        let updater = GraphUpdater::new(graph_service);

        let intent_id = Uuid::new_v4();
        let signals = updater
            .invalidate_artifacts(&[node_id], intent_id, 1, 2, "test-actor")
            .await;

        // Phase 2b: Should generate exactly one invalidation signal
        assert_eq!(signals.len(), 1);
        let signal = &signals[0];
        assert_eq!(signal.node_id, node_id);
        assert_eq!(signal.intent_id, intent_id);
        assert_eq!(signal.intent_version_from, 1);
        assert_eq!(signal.intent_version_to, 2);
        assert_eq!(signal.initiated_by, "test-actor");

        // Verify the graph node was also marked Stale
        let nodes = repo.nodes.read().await;
        let node_after = nodes.get(&node_id).unwrap();
        assert_eq!(node_after.state, NodeState::Stale);
    }
}
