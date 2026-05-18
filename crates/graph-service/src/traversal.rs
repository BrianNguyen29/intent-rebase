//! Graph traversal primitives
//!
//! Phase 1 PR #10: BFS reachability, shortest-path finding, and cycle detection.
//! Extracted into a dedicated module to keep `lib.rs` bounded.

use crate::GraphService;
use intent_rebase_types::{
    CycleDetectionResult, GraphEdge, GraphEdgeFilter, GraphNodeFilter, GraphPath,
    IntentRebaseError, ReachabilityResult, TraversalOptions,
};
use std::collections::HashMap;
use uuid::Uuid;

impl GraphService {
    /// Find all nodes reachable from a starting node using BFS
    ///
    /// Returns all nodes reachable via outgoing edges, optionally filtered by depth and edge type.
    pub async fn find_reachable(
        &self,
        start_node_id: Uuid,
        options: TraversalOptions,
    ) -> Result<ReachabilityResult, IntentRebaseError> {
        use std::collections::{HashSet, VecDeque};

        // First verify start node exists
        let _ = self.repo.get_node(start_node_id).await?;

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut incoming_edges: Vec<Uuid> = Vec::new();
        let mut queue: VecDeque<(Uuid, Option<Uuid>, usize)> = VecDeque::new();

        // (node_id, edge_id_used_to_reach, depth)
        if options.include_start {
            visited.insert(start_node_id);
        }

        // Seed with start node's outgoing edges
        let start_edges = self.repo.list_edges_from(start_node_id).await?;
        for edge in start_edges {
            if Self::edge_passes_filter(&edge, &options) {
                queue.push_back((edge.to_node_id, Some(edge.id), 1));
            }
        }

        while let Some((node_id, edge_id, depth)) = queue.pop_front() {
            // Issue #2 fix: Skip the start node entirely if include_start=false
            // This prevents re-including the start node through cycles
            if node_id == start_node_id && !options.include_start {
                continue;
            }

            // Check max depth
            if let Some(max) = options.max_depth {
                if depth > max {
                    continue;
                }
            }

            // Issue #1 fix: Apply node_types filter BEFORE adding to visited and queue
            // This implements "only traverse through matching node types" semantics
            if let Some(ref filter_node_types) = options.node_types {
                let node = match self.repo.get_node(node_id).await {
                    Ok(n) => n,
                    Err(_) => continue, // Node not found, skip
                };
                if !filter_node_types.contains(&node.node_type) {
                    continue; // Don't visit filtered nodes - don't add to visited or expand from them
                }
            }

            if visited.insert(node_id) {
                if let Some(eid) = edge_id {
                    incoming_edges.push(eid);
                }

                // Only explore further if within depth limit
                let at_max_depth = options.max_depth.map(|max| depth >= max).unwrap_or(false);

                if !at_max_depth {
                    let edges = self.repo.list_edges_from(node_id).await?;
                    for edge in edges {
                        if Self::edge_passes_filter(&edge, &options) {
                            queue.push_back((edge.to_node_id, Some(edge.id), depth + 1));
                        }
                    }
                }
            }
        }

        Ok(ReachabilityResult {
            reachable_nodes: visited.into_iter().collect(),
            incoming_edges,
        })
    }

    /// Find a path between two nodes using BFS
    ///
    /// Returns the shortest path (fewest hops) from source to target, if one exists.
    pub async fn find_path(
        &self,
        source_id: Uuid,
        target_id: Uuid,
        options: TraversalOptions,
    ) -> Result<GraphPath, IntentRebaseError> {
        use std::collections::{HashMap, HashSet, VecDeque};

        // First verify both nodes exist
        let _ = self.repo.get_node(source_id).await?;
        let _ = self.repo.get_node(target_id).await?;

        if source_id == target_id {
            if options.include_start {
                return Ok(GraphPath {
                    node_ids: vec![source_id],
                    edge_ids: vec![],
                });
            }
            return Ok(GraphPath {
                node_ids: vec![],
                edge_ids: vec![],
            });
        }

        // BFS to find shortest path
        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut parent_edge: HashMap<Uuid, (Uuid, Uuid)> = HashMap::new(); // node -> (prev_node, edge_id)

        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((source_id, 0));
        visited.insert(source_id);

        while let Some((node_id, depth)) = queue.pop_front() {
            // Check max depth
            if let Some(max) = options.max_depth {
                if depth >= max {
                    continue;
                }
            }

            let edges = self.repo.list_edges_from(node_id).await?;
            for edge in edges {
                if !Self::edge_passes_filter(&edge, &options) {
                    continue;
                }

                let neighbor = edge.to_node_id;

                if neighbor == target_id {
                    parent_edge.insert(neighbor, (node_id, edge.id));
                    // Reconstruct path
                    return Ok(Self::reconstruct_path(source_id, target_id, parent_edge));
                }

                if visited.insert(neighbor) {
                    parent_edge.insert(neighbor, (node_id, edge.id));
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        // No path found
        Ok(GraphPath {
            node_ids: vec![],
            edge_ids: vec![],
        })
    }

    /// Detect if there are any cycles in the graph (within a workflow scope)
    ///
    /// Uses DFS to detect back edges.
    pub async fn detect_cycles(
        &self,
        workflow_id: Uuid,
    ) -> Result<CycleDetectionResult, IntentRebaseError> {
        use std::collections::HashMap;

        // Get all nodes in the workflow
        let nodes = self
            .repo
            .list_nodes(GraphNodeFilter {
                workflow_id: Some(workflow_id),
                ..Default::default()
            })
            .await?;

        if nodes.is_empty() {
            return Ok(CycleDetectionResult {
                has_cycle: false,
                cycle_path: None,
            });
        }

        // Build adjacency list
        let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for node in &nodes {
            adj.entry(node.id).or_default();
        }

        let edges = self
            .repo
            .list_edges(GraphEdgeFilter {
                workflow_id: Some(workflow_id),
                ..Default::default()
            })
            .await?;

        for edge in &edges {
            adj.entry(edge.from_node_id)
                .or_default()
                .push(edge.to_node_id);
        }

        // DFS with three-color marking
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut color: HashMap<Uuid, Color> = HashMap::new();
        for node in &nodes {
            color.insert(node.id, Color::White);
        }

        let mut cycle_path: Option<Vec<Uuid>> = None;

        fn dfs(
            node: Uuid,
            adj: &HashMap<Uuid, Vec<Uuid>>,
            color: &mut HashMap<Uuid, Color>,
            path: &mut Vec<Uuid>,
            cycle_result: &mut Option<Vec<Uuid>>,
        ) -> bool {
            color.insert(node, Color::Gray);
            path.push(node);

            if let Some(neighbors) = adj.get(&node) {
                for &neighbor in neighbors {
                    // Check if we found a cycle
                    if let Some(&Color::Gray) = color.get(&neighbor) {
                        // Found back edge - cycle detected
                        if let Some(pos) = path.iter().position(|&n| n == neighbor) {
                            let mut cycle = path[pos..].to_vec();
                            cycle.push(neighbor); // Append neighbor to complete the cycle
                            *cycle_result = Some(cycle);
                            path.pop();
                            color.insert(node, Color::Black);
                            return true;
                        }
                    }
                }

                for &neighbor in neighbors {
                    if color.get(&neighbor) == Some(&Color::White)
                        && dfs(neighbor, adj, color, path, cycle_result)
                    {
                        return true;
                    }
                }
            }

            path.pop();
            color.insert(node, Color::Black);
            false
        }

        for node in &nodes {
            if color.get(&node.id) == Some(&Color::White) {
                let mut path = Vec::new();
                if dfs(node.id, &adj, &mut color, &mut path, &mut cycle_path) {
                    break;
                }
            }
        }

        Ok(CycleDetectionResult {
            has_cycle: cycle_path.is_some(),
            cycle_path,
        })
    }

    /// List nodes reachable from a starting node (alias for find_reachable with simpler options)
    pub async fn list_reachable_nodes(
        &self,
        start_node_id: Uuid,
        max_depth: Option<usize>,
    ) -> Result<Vec<Uuid>, IntentRebaseError> {
        let result = self
            .find_reachable(
                start_node_id,
                TraversalOptions {
                    max_depth,
                    ..Default::default()
                },
            )
            .await?;
        Ok(result.reachable_nodes)
    }

    /// Check if two nodes are connected (any path exists)
    pub async fn are_connected(
        &self,
        source_id: Uuid,
        target_id: Uuid,
        max_depth: Option<usize>,
    ) -> Result<bool, IntentRebaseError> {
        let path = self
            .find_path(
                source_id,
                target_id,
                TraversalOptions {
                    max_depth,
                    ..Default::default()
                },
            )
            .await?;
        Ok(!path.is_empty())
    }

    /// Helper: Check if an edge passes the traversal filter
    fn edge_passes_filter(edge: &GraphEdge, options: &TraversalOptions) -> bool {
        if let Some(ref edge_types) = options.edge_types {
            if !edge_types.contains(&edge.edge_type) {
                return false;
            }
        }
        true
    }

    /// Helper: Reconstruct path from parent map
    fn reconstruct_path(
        source: Uuid,
        target: Uuid,
        parent_edge: HashMap<Uuid, (Uuid, Uuid)>,
    ) -> GraphPath {
        let mut node_ids = Vec::new();
        let mut edge_ids = Vec::new();

        let mut current = target;
        node_ids.push(current);

        while current != source {
            if let Some((prev, edge_id)) = parent_edge.get(&current) {
                edge_ids.push(*edge_id);
                node_ids.push(*prev);
                current = *prev;
            } else {
                // Path reconstruction failed
                return GraphPath {
                    node_ids: vec![],
                    edge_ids: vec![],
                };
            }
        }

        node_ids.reverse();
        edge_ids.reverse();

        GraphPath { node_ids, edge_ids }
    }
}
