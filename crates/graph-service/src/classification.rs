//! Graph classification primitives
//!
//! Extracted into a dedicated module to keep `lib.rs` bounded.

use crate::{GraphRepository, GraphService};
use intent_rebase_types::{
    ClassificationImpact, ClassificationResult, ClassifiedNode, ClassifyRequest, EdgeType,
    IntentRebaseError, PropagationConfig, DEFAULT_PROPAGATION_CONFIG,
};
use std::collections::VecDeque;
use std::sync::Arc;
use uuid::Uuid;

impl GraphService {
    /// Classify the impact of a change originating from a starting node.
    ///
    /// This is a baseline classification implementation that uses deterministic,
    /// explicit propagation rules with bounded depth.
    ///
    /// # Graph Edge Direction Semantics
    /// The dependency graph uses edges that point UPSTREAM (from dependent to dependency):
    /// - `DependsOn`: Artifact -> IntentVersion (artifact depends on intent)
    /// - `Triggers`: TaskNode -> SideEffect (task triggers side effect)
    /// - `GeneratedFrom`: SideEffect -> Approval (side effect generated from approval)
    /// - `ValidatedBy`: Approval -> IntentVersion (approval validates intent)
    ///
    /// # Propagation Configuration (PR #13 Rule-Pack Baseline)
    /// When `request.propagation_config` is `Some`, the provided `PropagationConfig` drives:
    /// - `max_depth`: Maximum traversal depth (default: 3)
    /// - `traversable_edge_types`: Which edge types to follow (default: DependsOn, Triggers, GeneratedFrom)
    /// - `target_node_types`: Which node types to classify as affected (default: Artifact, Approval, SideEffect, Generic)
    ///
    /// When `request.propagation_config` is `None`, uses `DEFAULT_PROPAGATION_CONFIG`.
    ///
    /// Note: `traversable_directions` is accepted for future extension but Phase 1 baseline
    /// uses hardcoded direction logic per edge type (DependsOn=incoming, Triggers/GeneratedFrom=outgoing).
    ///
    /// # Classification Rules (Baseline)
    /// - **Direct**: Nodes at depth 1 from the starting node (e.g., Artifacts that directly
    ///   depend on the changed IntentVersion via DependsOn edge)
    /// - **Transitive**: Nodes at depth 2+ from the starting node (e.g., SideEffects
    ///   triggered downstream from affected Artifacts)
    /// - **Unchanged**: Nodes not reachable from the starting node
    ///
    /// # Traversal Semantics
    /// - For DependsOn: traverse INCOMING edges to find dependents
    /// - For Triggers/GeneratedFrom: traverse OUTGOING edges to find downstream
    /// - Depth is bounded by `max_depth` (default 3) to keep baseline bounded
    /// - Only classifies nodes matching `target_node_types` filter (if provided in config)
    ///
    /// # Example
    /// ```ignore
    /// // Given: IntentVersion IV1 <-(DependsOn)- Artifact A1 <-(Triggers)- SideEffect SE1
    /// // Graph edges: Artifact A1 -> IV1, SE1 -> A1 (via Triggers)
    /// let result = service.classify_impact(ClassifyRequest {
    ///     start_node_id: iv1.id,
    ///     max_depth: Some(3),
    ///     target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]),
    ///     propagation_config: None, // Uses DEFAULT_PROPAGATION_CONFIG
    /// }).await?;
    ///
    /// // Result: A1 classified as Direct (via incoming DependsOn), SE1 classified as Transitive (via outgoing Triggers)
    /// ```
    pub async fn classify_impact(
        &self,
        request: ClassifyRequest,
    ) -> Result<ClassificationResult, IntentRebaseError> {
        use std::collections::{HashSet, VecDeque};

        // PR #13: Use propagation_config if provided, otherwise use default
        let config = request
            .propagation_config
            .as_ref()
            .unwrap_or(&DEFAULT_PROPAGATION_CONFIG);

        // Validate start node exists
        let start_node = self.repo.get_node(request.start_node_id).await?;

        // PR #13: For backward compat, when propagation_config is None, prefer request.max_depth.
        // When config is explicitly provided, use config.max_depth but fall back to request.max_depth.
        let use_config_max_depth = request.propagation_config.is_some();
        let max_depth = if use_config_max_depth {
            config.max_depth.or(request.max_depth).unwrap_or(3)
        } else {
            request.max_depth.unwrap_or(3)
        };

        // BFS traversal to classify nodes by impact
        // We track (node_id, depth, path_reason) for each discovered node
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut classified: Vec<ClassifiedNode> = Vec::new();
        let mut queue: VecDeque<(Uuid, usize, String)> = VecDeque::new();

        // Phase 1: Seed with start node's INCOMING edges at depth 1
        // - DependsOn edges: Artifacts that depend on this IntentVersion
        // - ValidatedBy edges: Approvals that validate this IntentVersion
        // Note: Direction is hardcoded per edge type for Phase 1 baseline
        let incoming_edges = self.repo.list_edges_to(request.start_node_id).await?;
        for edge in incoming_edges {
            if edge.edge_type == EdgeType::DependsOn
                && config.traversable_edge_types.contains(&edge.edge_type)
            {
                queue.push_back((edge.from_node_id, 1, "directly depends on".to_string()));
            }
            if edge.edge_type == EdgeType::ValidatedBy
                && config.traversable_edge_types.contains(&edge.edge_type)
            {
                queue.push_back((
                    edge.from_node_id,
                    1,
                    "directly validates this version".to_string(),
                ));
            }
        }

        while let Some((node_id, depth, reason)) = queue.pop_front() {
            // Check max depth
            if depth > max_depth {
                continue;
            }

            // Skip if already visited
            if visited.contains(&node_id) {
                continue;
            }

            // Get the node to classify it
            let node = match self.repo.get_node(node_id).await {
                Ok(n) => n,
                Err(_) => continue, // Node not found, skip
            };

            // Check if this node type is a target.
            // PR #13 backward compat: When propagation_config is None AND request.target_node_types
            // is Some, prefer request.target_node_types (legacy behavior for existing callers).
            // When propagation_config is Some, use config.target_node_types (may be empty to override).
            let use_config = request.propagation_config.is_some();
            let target_types = if use_config {
                // Explicit config provided - use it (may be empty to target all types)
                Some(&config.target_node_types)
            } else {
                // No config - use request.target_node_types if provided, else config defaults
                request
                    .target_node_types
                    .as_ref()
                    .or(Some(&config.target_node_types))
            };

            if let Some(allowed_types) = target_types {
                if !allowed_types.contains(&node.node_type) {
                    // Skip this node but still explore its outgoing edges for propagation
                    if depth < max_depth {
                        Self::enqueue_propagation_edges_with_config(
                            &self.repo, node_id, depth, &mut queue, config,
                        )
                        .await?;
                    }
                    continue;
                }
            }

            // Classify based on depth from start
            let impact = if depth == 1 {
                ClassificationImpact::Direct
            } else {
                ClassificationImpact::Transitive
            };

            visited.insert(node_id);
            classified.push(ClassifiedNode {
                node: node.clone(),
                impact,
                reason,
            });

            // Continue traversal if within depth limit
            if depth < max_depth {
                Self::enqueue_propagation_edges_with_config(
                    &self.repo, node_id, depth, &mut queue, config,
                )
                .await?;
            }
        }

        Ok(ClassificationResult {
            classified_nodes: classified,
            start_node_id: start_node.id,
            max_depth,
        })
    }

    /// Helper: Enqueue edges for impact propagation from a node (PR #13 with PropagationConfig).
    ///
    /// Impact propagation follows the dependency chain downstream:
    /// - For DependsOn (artifact -> intent): downstream is the artifact (incoming edges)
    /// - For ValidatedBy (approval -> intent): ValidatedBy is handled in seed phase only;
    ///   when an IntentVersion is the start node, approvals validating it are found via
    ///   incoming ValidatedBy edges. Propagation from an Approval continues via Triggers/GeneratedFrom.
    /// - For Triggers (task -> side_effect): downstream is the side_effect (outgoing edges)
    /// - For GeneratedFrom (se -> approval): downstream is the approval (outgoing edges)
    ///
    /// Note: Direction is hardcoded per edge type for Phase 1 baseline.
    async fn enqueue_propagation_edges_with_config(
        repo: &Arc<dyn GraphRepository>,
        node_id: Uuid,
        current_depth: usize,
        queue: &mut VecDeque<(Uuid, usize, String)>,
        config: &PropagationConfig,
    ) -> Result<(), IntentRebaseError> {
        // For DependsOn: look at INCOMING edges to find dependents (downstream artifacts)
        // Direction is hardcoded for Phase 1 - DependsOn always uses incoming
        let incoming_edges = repo.list_edges_to(node_id).await?;
        for edge in incoming_edges {
            if edge.edge_type == EdgeType::DependsOn
                && config.traversable_edge_types.contains(&edge.edge_type)
            {
                queue.push_back((
                    edge.from_node_id,
                    current_depth + 1,
                    "downstream via dependency chain".to_string(),
                ));
            }
        }

        // For Triggers/GeneratedFrom: follow OUTGOING edges to find downstream
        // Direction is hardcoded for Phase 1 - Triggers/GeneratedFrom always use outgoing
        let outgoing_edges = repo.list_edges_from(node_id).await?;
        for edge in outgoing_edges {
            match edge.edge_type {
                EdgeType::Triggers
                    if config.traversable_edge_types.contains(&EdgeType::Triggers) =>
                {
                    queue.push_back((
                        edge.to_node_id,
                        current_depth + 1,
                        "downstream via triggered".to_string(),
                    ));
                }
                EdgeType::GeneratedFrom
                    if config
                        .traversable_edge_types
                        .contains(&EdgeType::GeneratedFrom) =>
                {
                    // SideEffect -> Approval (side effect generated from approval)
                    queue.push_back((
                        edge.to_node_id,
                        current_depth + 1,
                        "downstream via generated from".to_string(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
