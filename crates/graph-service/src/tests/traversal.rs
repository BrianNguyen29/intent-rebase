use super::*;
// ===== Traversal Tests =====

#[tokio::test]
async fn test_bfs_reachable_simple_chain() {
    // Graph: A -> B -> C -> D
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // Create edges
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Find all reachable from A (unlimited depth)
    let result = service
        .find_reachable(node_a.id, TraversalOptions::default())
        .await
        .unwrap();

    assert!(result.reachable_nodes.contains(&node_a.id)); // include_start is true by default
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert!(result.reachable_nodes.contains(&node_d.id));
    assert_eq!(result.reachable_nodes.len(), 4);
}

#[tokio::test]
async fn test_bfs_reachable_with_max_depth() {
    // Graph: A -> B -> C -> D
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // Create edges A->B, B->C, C->D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Depth 1: only B
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                max_depth: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(!result.reachable_nodes.contains(&node_c.id));
    assert!(!result.reachable_nodes.contains(&node_d.id));

    // Depth 2: B and C
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                max_depth: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert!(!result.reachable_nodes.contains(&node_d.id));
}

#[tokio::test]
async fn test_bfs_reachable_diamond_graph() {
    // Diamond graph: A -> B, A -> C, B -> D, C -> D
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // A -> B
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    // A -> C
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_c.id,
        ))
        .await
        .unwrap();
    // B -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_d.id,
        ))
        .await
        .unwrap();
    // C -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // From A, should reach B, C, D (D only once despite two paths)
    let result = service
        .find_reachable(node_a.id, TraversalOptions::default())
        .await
        .unwrap();
    assert_eq!(result.reachable_nodes.len(), 4); // A, B, C, D
    assert!(result.reachable_nodes.contains(&node_a.id));
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert!(result.reachable_nodes.contains(&node_d.id));
}

#[tokio::test]
async fn test_find_path_simple() {
    // Graph: A -> B -> C
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();

    // Find path A -> C
    let path = service
        .find_path(node_a.id, node_c.id, TraversalOptions::default())
        .await
        .unwrap();
    assert_eq!(path.node_ids, vec![node_a.id, node_b.id, node_c.id]);
    assert_eq!(path.edge_ids.len(), 2);
    assert_eq!(path.len(), 2);
}

#[tokio::test]
async fn test_find_path_no_path() {
    // Two disconnected graphs: A -> B and C -> D
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();

    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // A -> B
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    // C -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Try to find path B -> C (no connection)
    let path = service
        .find_path(node_b.id, node_c.id, TraversalOptions::default())
        .await
        .unwrap();
    assert!(path.is_empty());
    assert_eq!(path.node_ids.len(), 0);
}

#[tokio::test]
async fn test_find_path_diamond_shortest() {
    // Diamond: A -> B -> D, A -> C -> D
    // Shortest path should be 2 hops
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // A -> B -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_d.id,
        ))
        .await
        .unwrap();
    // A -> C -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Path A -> D should be 2 hops (A to either B or C to D)
    let path = service
        .find_path(node_a.id, node_d.id, TraversalOptions::default())
        .await
        .unwrap();
    assert_eq!(path.len(), 2);
    assert!(path.node_ids.first() == Some(&node_a.id));
    assert!(path.node_ids.last() == Some(&node_d.id));
}

#[tokio::test]
async fn test_cycle_detection_no_cycle() {
    // Simple chain: A -> B -> C (no cycle)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();

    let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
    assert!(!result.has_cycle);
    assert!(result.cycle_path.is_none());
}

#[tokio::test]
async fn test_cycle_detection_simple_cycle() {
    // Cycle: A -> B -> C -> A
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A -> B
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    // B -> C
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    // C -> A (creates cycle)
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_a.id,
        ))
        .await
        .unwrap();

    let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
    assert!(result.has_cycle);
    assert!(result.cycle_path.is_some());
    let cycle = result.cycle_path.unwrap();
    // The cycle should form a loop
    assert!(cycle.len() >= 3);
    assert_eq!(cycle.first(), cycle.last()); // Loop back to start
}

#[tokio::test]
async fn test_cycle_detection_self_loop() {
    // Self-loop: A -> A
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();

    // A -> A (self-loop)
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_a.id,
        ))
        .await
        .unwrap();

    let result = service.detect_cycles(node_a.workflow_id).await.unwrap();
    assert!(result.has_cycle);
}

#[tokio::test]
async fn test_cycle_detection_empty_workflow() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    // Use a workflow ID that has no nodes
    let result = service.detect_cycles(Uuid::new_v4()).await.unwrap();
    assert!(!result.has_cycle);
    assert!(result.cycle_path.is_none());
}

#[tokio::test]
async fn test_are_connected() {
    // Graph: A -> B -> C
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();

    // A connected to C
    assert!(service
        .are_connected(node_a.id, node_c.id, None)
        .await
        .unwrap());
    // C not connected to A (reverse direction)
    assert!(!service
        .are_connected(node_c.id, node_a.id, None)
        .await
        .unwrap());
    // A connected to B
    assert!(service
        .are_connected(node_a.id, node_b.id, None)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_list_reachable_nodes() {
    // Graph: A -> B -> C -> D
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();
    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    let node_d = service.add_node(node_d_req).await.unwrap();

    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Unlimited depth
    let reachable = service.list_reachable_nodes(node_a.id, None).await.unwrap();
    assert_eq!(reachable.len(), 4);

    // Depth 2
    let reachable = service
        .list_reachable_nodes(node_a.id, Some(2))
        .await
        .unwrap();
    assert!(reachable.contains(&node_a.id));
    assert!(reachable.contains(&node_b.id));
    assert!(reachable.contains(&node_c.id));
    assert!(!reachable.contains(&node_d.id));
}

#[tokio::test]
async fn test_edge_type_filter() {
    // Graph with mixed edge types
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A --DependsOn--> B
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    // B --Triggers--> C (different edge type)
    service
        .add_edge(CreateGraphEdgeRequest {
            tenant_id: node_a.tenant_id,
            workflow_id: node_a.workflow_id,
            from_node_id: node_b.id,
            to_node_id: node_c.id,
            edge_type: EdgeType::Triggers,
            properties: None,
        })
        .await
        .unwrap();

    // Find path filtering by DependsOn only
    let path = service
        .find_path(
            node_a.id,
            node_c.id,
            TraversalOptions {
                edge_types: Some(vec![EdgeType::DependsOn]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // No path exists if we can only use DependsOn edges
    assert!(path.is_empty());

    // Find path filtering by Triggers only
    let path = service
        .find_path(
            node_a.id,
            node_c.id,
            TraversalOptions {
                edge_types: Some(vec![EdgeType::Triggers]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // No path exists (A doesn't have Triggers to C)
    assert!(path.is_empty());

    // Using both edge types should find the path
    let path = service
        .find_path(
            node_a.id,
            node_c.id,
            TraversalOptions {
                edge_types: Some(vec![EdgeType::DependsOn, EdgeType::Triggers]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(path.len(), 2);
}

#[tokio::test]
async fn test_reachable_nonexistent_node() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let result = service
        .find_reachable(Uuid::new_v4(), TraversalOptions::default())
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::GraphNodeNotFound(_)
    ));
}

#[tokio::test]
async fn test_path_to_self() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();

    // Path to self with include_start=true
    let path = service
        .find_path(
            node_a.id,
            node_a.id,
            TraversalOptions {
                include_start: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(path.node_ids, vec![node_a.id]);
    assert!(path.edge_ids.is_empty());

    // Path to self with include_start=false
    let path = service
        .find_path(
            node_a.id,
            node_a.id,
            TraversalOptions {
                include_start: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(path.is_empty());
}

// ===== Issue #2 Fix: include_start=false should not re-include start node through cycles =====

#[tokio::test]
async fn test_reachable_include_start_false_no_cycle() {
    // Graph: A -> B -> C (no cycle back to A)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A -> B -> C
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();

    // include_start=false should exclude A, include B and C
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                include_start: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!result.reachable_nodes.contains(&node_a.id));
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert_eq!(result.reachable_nodes.len(), 2);
}

#[tokio::test]
async fn test_reachable_include_start_false_with_cycle() {
    // Graph: A -> B -> C -> A (cycle back to start)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A -> B -> C -> A (cycle)
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_a.id,
        ))
        .await
        .unwrap();

    // include_start=false should STILL exclude A even though there's a cycle back to it
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                include_start: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        !result.reachable_nodes.contains(&node_a.id),
        "Start node should NOT be re-included via cycle when include_start=false"
    );
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert_eq!(result.reachable_nodes.len(), 2);
}

#[tokio::test]
async fn test_reachable_include_start_true_with_cycle() {
    // Graph: A -> B -> C -> A (cycle back to start)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A -> B -> C -> A (cycle)
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_a.id,
        ))
        .await
        .unwrap();

    // include_start=true should include A (once) along with B and C
    let result = service
        .find_reachable(node_a.id, TraversalOptions::default())
        .await
        .unwrap();
    assert!(result.reachable_nodes.contains(&node_a.id));
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(result.reachable_nodes.contains(&node_c.id));
    assert_eq!(result.reachable_nodes.len(), 3);
}

// ===== Issue #1 Fix: node_types filtering =====

#[tokio::test]
async fn test_reachable_node_type_filter() {
    // Graph: A (Intent) -> B (Intent) -> C (Artifact) -> D (Intent)
    // When filtering to Intent, we only traverse through Intent nodes
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let mut node_a_req = create_test_node_request();
    node_a_req.node_type = NodeType::Intent;
    let node_a = service.add_node(node_a_req).await.unwrap();

    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    node_b_req.node_type = NodeType::Intent;
    let node_b = service.add_node(node_b_req).await.unwrap();

    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    node_c_req.node_type = NodeType::Artifact;
    let node_c = service.add_node(node_c_req).await.unwrap();

    let mut node_d_req = create_test_node_request();
    node_d_req.tenant_id = node_a.tenant_id;
    node_d_req.workflow_id = node_a.workflow_id;
    node_d_req.node_type = NodeType::Intent;
    let node_d = service.add_node(node_d_req).await.unwrap();

    // A -> B -> C -> D
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_b.id,
            node_c.id,
        ))
        .await
        .unwrap();
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_c.id,
            node_d.id,
        ))
        .await
        .unwrap();

    // Filter to only Intent nodes
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                node_types: Some(vec![NodeType::Intent]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Only traverse through Intent nodes
    // A (Intent) - matches, expand from it
    // B (Intent) - matches, expand from it
    // C (Artifact) - filtered out, don't expand from it, so D is never discovered
    assert!(result.reachable_nodes.contains(&node_a.id));
    assert!(result.reachable_nodes.contains(&node_b.id));
    assert!(
        !result.reachable_nodes.contains(&node_c.id),
        "Artifact node should not be traversed through"
    );
    assert!(
        !result.reachable_nodes.contains(&node_d.id),
        "D should not be discovered since we don't traverse through C"
    );
    assert_eq!(result.reachable_nodes.len(), 2);
}

#[tokio::test]
async fn test_reachable_node_type_filter_no_match() {
    // Graph: A (Intent) -> B (Artifact)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let mut node_a_req = create_test_node_request();
    node_a_req.node_type = NodeType::Intent;
    let node_a = service.add_node(node_a_req).await.unwrap();

    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    node_b_req.node_type = NodeType::Artifact;
    let node_b = service.add_node(node_b_req).await.unwrap();

    // A -> B
    service
        .add_edge(create_test_edge_request_with_ids(
            node_a.tenant_id,
            node_a.workflow_id,
            node_a.id,
            node_b.id,
        ))
        .await
        .unwrap();

    // Filter to only SideEffect nodes (none exist in graph)
    let result = service
        .find_reachable(
            node_a.id,
            TraversalOptions {
                node_types: Some(vec![NodeType::SideEffect]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Only A (Intent) is reachable since B (Artifact) is filtered
    assert!(result.reachable_nodes.contains(&node_a.id));
    assert!(!result.reachable_nodes.contains(&node_b.id));
    assert_eq!(result.reachable_nodes.len(), 1);
}
