use super::*;
use intent_rebase_types::{EdgeDirection, EdgeType, ExternalRef, ExternalRefType};

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

#[tokio::test]
async fn test_create_and_get_node() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let request = create_test_node_request();
    let created = service.add_node(request.clone()).await.unwrap();

    assert_eq!(created.label, request.label);
    assert_eq!(created.node_type, request.node_type);

    // Get by ID
    let retrieved = service.get_node(created.id).await.unwrap();
    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.label, created.label);
}

#[tokio::test]
async fn test_get_nonexistent_node() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    let result = service.get_node(Uuid::new_v4()).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::GraphNodeNotFound(_)
    ));
}

#[tokio::test]
async fn test_list_nodes_with_filter() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    // Create nodes of different types
    let mut request1 = create_test_node_request();
    request1.node_type = NodeType::Intent;

    let mut request2 = create_test_node_request();
    request2.node_type = NodeType::Artifact;

    let node1 = service.add_node(request1).await.unwrap();
    let _node2 = service.add_node(request2).await.unwrap();

    // Filter by node type
    let filter = GraphNodeFilter {
        node_type: Some(NodeType::Intent),
        ..Default::default()
    };
    let nodes = service.list_nodes(filter).await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, node1.id);
}

#[tokio::test]
async fn test_create_and_get_edge() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    // Create two nodes with SAME tenant/workflow (required for edge creation)
    let node1 = service.add_node(create_test_node_request()).await.unwrap();
    let mut request2 = create_test_node_request();
    request2.tenant_id = node1.tenant_id;
    request2.workflow_id = node1.workflow_id;
    request2.external_ref = Some(ExternalRef {
        ref_type: ExternalRefType::IntentVersion,
        ref_id: Uuid::new_v4(),
    });
    let node2 = service.add_node(request2).await.unwrap();

    // Create edge - must use same tenant/workflow as nodes
    let edge_request =
        create_test_edge_request_with_ids(node1.tenant_id, node1.workflow_id, node1.id, node2.id);
    let created = service.add_edge(edge_request.clone()).await.unwrap();

    assert_eq!(created.from_node_id, node1.id);
    assert_eq!(created.to_node_id, node2.id);
    assert_eq!(created.edge_type, EdgeType::DependsOn);

    // Get by ID
    let retrieved = service.get_edge(created.id).await.unwrap();
    assert_eq!(retrieved.id, created.id);
}

#[tokio::test]
async fn test_create_edge_nonexistent_node() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    let request = create_test_edge_request_with_ids(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let result = service.add_edge(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::GraphNodeNotFound(_)));
}

#[tokio::test]
async fn test_create_edge_cross_tenant_rejected() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    // Create two nodes with tenant A
    let node1 = service.add_node(create_test_node_request()).await.unwrap();
    let mut request2 = create_test_node_request();
    request2.tenant_id = node1.tenant_id;
    request2.workflow_id = node1.workflow_id;
    let node2 = service.add_node(request2).await.unwrap();

    // Try to create edge with different tenant
    let mut edge_request =
        create_test_edge_request_with_ids(node1.tenant_id, node1.workflow_id, node1.id, node2.id);
    edge_request.tenant_id = Uuid::new_v4(); // Different tenant

    let result = service.add_edge(edge_request).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::GraphIntegrityError(_)
    ));
}

#[tokio::test]
async fn test_create_edge_cross_workflow_rejected() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    // Create two nodes with same tenant but different workflow
    let node1 = service.add_node(create_test_node_request()).await.unwrap();
    let mut request2 = create_test_node_request();
    request2.tenant_id = node1.tenant_id;
    request2.workflow_id = Uuid::new_v4(); // Different workflow
    let node2 = service.add_node(request2).await.unwrap();

    // Try to create edge with node1's workflow (not node2's)
    let edge_request =
        create_test_edge_request_with_ids(node1.tenant_id, node1.workflow_id, node1.id, node2.id);

    let result = service.add_edge(edge_request).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::GraphIntegrityError(_)
    ));
}

#[tokio::test]
async fn test_list_edges_from_and_to() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    // Create three nodes: A -> B -> C
    let node_a = service.add_node(create_test_node_request()).await.unwrap();
    let mut node_b_req = create_test_node_request();
    node_b_req.tenant_id = node_a.tenant_id;
    node_b_req.workflow_id = node_a.workflow_id;
    node_b_req.external_ref = Some(ExternalRef {
        ref_type: ExternalRefType::IntentVersion,
        ref_id: Uuid::new_v4(),
    });
    let node_b = service.add_node(node_b_req).await.unwrap();
    let mut node_c_req = create_test_node_request();
    node_c_req.tenant_id = node_a.tenant_id;
    node_c_req.workflow_id = node_a.workflow_id;
    node_c_req.external_ref = Some(ExternalRef {
        ref_type: ExternalRefType::Artifact,
        ref_id: Uuid::new_v4(),
    });
    let node_c = service.add_node(node_c_req).await.unwrap();

    // A -> B
    let edge_ab = create_test_edge_request_with_ids(
        node_a.tenant_id,
        node_a.workflow_id,
        node_a.id,
        node_b.id,
    );
    let _edge_ab = service.add_edge(edge_ab).await.unwrap();

    // B -> C
    let edge_bc = create_test_edge_request_with_ids(
        node_a.tenant_id,
        node_a.workflow_id,
        node_b.id,
        node_c.id,
    );
    let _edge_bc = service.add_edge(edge_bc).await.unwrap();

    // Check edges from A
    let edges_from_a = service.list_edges_from(node_a.id).await.unwrap();
    assert_eq!(edges_from_a.len(), 1);
    assert_eq!(edges_from_a[0].to_node_id, node_b.id);

    // Check edges to C
    let edges_to_c = service.list_edges_to(node_c.id).await.unwrap();
    assert_eq!(edges_to_c.len(), 1);
    assert_eq!(edges_to_c[0].from_node_id, node_b.id);

    // Check edges from B
    let edges_from_b = service.list_edges_from(node_b.id).await.unwrap();
    assert_eq!(edges_from_b.len(), 1);
    assert_eq!(edges_from_b[0].to_node_id, node_c.id);
}

#[tokio::test]
async fn test_delete_edge() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    // Create nodes
    let node1 = service.add_node(create_test_node_request()).await.unwrap();
    let mut node2_req = create_test_node_request();
    node2_req.tenant_id = node1.tenant_id;
    node2_req.workflow_id = node1.workflow_id;
    let node2 = service.add_node(node2_req).await.unwrap();

    // Create edge
    let edge_request =
        create_test_edge_request_with_ids(node1.tenant_id, node1.workflow_id, node1.id, node2.id);
    let edge = service.add_edge(edge_request).await.unwrap();

    // Delete it
    let result = service.delete_edge(edge.id).await;
    assert!(result.is_ok());

    // Verify it's gone
    let get_result = service.get_edge(edge.id).await;
    assert!(get_result.is_err());
}

#[tokio::test]
async fn test_update_node_state() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo);

    let node = service.add_node(create_test_node_request()).await.unwrap();
    assert_eq!(node.state, NodeState::Active);

    let updated = service
        .update_node_state(node.id, NodeState::Stale)
        .await
        .unwrap();
    assert_eq!(updated.state, NodeState::Stale);
}

#[tokio::test]
async fn test_in_memory_repo_persistence() {
    // Verify in-memory repo shares state between service instances
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service1 = GraphService::new(repo.clone());
    let service2 = GraphService::new(repo);

    let node = service1.add_node(create_test_node_request()).await.unwrap();

    // Second service should see the same data
    let retrieved = service2.get_node(node.id).await.unwrap();
    assert_eq!(retrieved.id, node.id);
}

#[tokio::test]
async fn test_concurrent_operations_no_deadlock() {
    // Verify that concurrent create_edge and list_edges_from don't deadlock
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    // Create a node
    let node = service.add_node(create_test_node_request()).await.unwrap();

    // Spawn concurrent edge creates
    let mut edge_handles = vec![];

    for i in 0..10 {
        let service_clone = service.clone();
        let node_clone = node.id;
        let tenant_id = node.tenant_id;
        let workflow_id = node.workflow_id;

        edge_handles.push(tokio::spawn(async move {
            let mut req = create_test_node_request();
            req.tenant_id = tenant_id;
            req.workflow_id = workflow_id;
            req.label = format!("Target Node {}", i);

            let target = service_clone.add_node(req).await.unwrap();

            let edge_req =
                create_test_edge_request_with_ids(tenant_id, workflow_id, node_clone, target.id);
            service_clone.add_edge(edge_req).await
        }));
    }

    // Also spawn list operations
    let mut list_handles = vec![];
    for _ in 0..5 {
        let service_clone = service.clone();
        let node_clone = node.id;
        list_handles.push(tokio::spawn(async move {
            service_clone.list_edges_from(node_clone).await
        }));
    }

    // Wait for all - if there's a deadlock, this will hang
    for handle in edge_handles {
        let result = handle.await.unwrap();
        // Edge creates may fail if target nodes conflict, but shouldn't deadlock
        let _ = result;
    }

    for handle in list_handles {
        let result = handle.await.unwrap();
        // List operations should succeed
        assert!(result.is_ok());
    }

    // Verify final state
    let edges = service.list_edges_from(node.id).await.unwrap();
    assert!(edges.len() <= 10);
}

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

// ===== Ingestor Tests =====

#[tokio::test]
async fn test_ingest_artifact_creates_node_and_edges() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create an IntentVersion node first (artifact will depend on it)
    let mut intent_version_req = create_test_node_request();
    intent_version_req.tenant_id = tenant_id;
    intent_version_req.workflow_id = workflow_id;
    intent_version_req.node_type = NodeType::IntentVersion;
    let intent_version = service.add_node(intent_version_req).await.unwrap();

    // Ingest an artifact that depends on the IntentVersion
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "patch-42".to_string(),
        depends_on_intent_versions: vec![intent_version.id],
        properties: Some(serde_json::json!({"artifact_type": "patch"})),
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await.unwrap();

    // Verify node
    assert_eq!(result.node.node_type, NodeType::Artifact);
    assert_eq!(result.node.label, "patch-42");
    assert_eq!(result.node.tenant_id, tenant_id);
    assert_eq!(result.node.workflow_id, workflow_id);

    // Verify edge: Artifact depends on IntentVersion (DependsOn from artifact to intent_version)
    assert_eq!(result.edges.len(), 1);
    let edge = &result.edges[0];
    assert_eq!(edge.edge_type, EdgeType::DependsOn);
    assert_eq!(edge.from_node_id, result.node.id);
    assert_eq!(edge.to_node_id, intent_version.id);

    // Verify the node can be retrieved
    let retrieved = service.get_node(result.node.id).await.unwrap();
    assert_eq!(retrieved.id, result.node.id);
}

#[tokio::test]
async fn test_ingest_artifact_with_multiple_dependencies() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create two IntentVersion nodes
    let mut iv1_req = create_test_node_request();
    iv1_req.tenant_id = tenant_id;
    iv1_req.workflow_id = workflow_id;
    iv1_req.node_type = NodeType::IntentVersion;
    let iv1 = service.add_node(iv1_req).await.unwrap();

    let mut iv2_req = create_test_node_request();
    iv2_req.tenant_id = tenant_id;
    iv2_req.workflow_id = workflow_id;
    iv2_req.node_type = NodeType::IntentVersion;
    let iv2 = service.add_node(iv2_req).await.unwrap();

    // Ingest artifact with two dependencies
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "multi-dep-artifact".to_string(),
        depends_on_intent_versions: vec![iv1.id, iv2.id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await.unwrap();

    assert_eq!(result.node.node_type, NodeType::Artifact);
    assert_eq!(result.edges.len(), 2);

    // Verify both edges exist
    let edge_ids: Vec<_> = result.edges.iter().map(|e| e.to_node_id).collect();
    assert!(edge_ids.contains(&iv1.id));
    assert!(edge_ids.contains(&iv2.id));
}

#[tokio::test]
async fn test_ingest_approval_creates_node_with_governed_by() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a PolicySnapshot node
    let mut policy_req = create_test_node_request();
    policy_req.tenant_id = tenant_id;
    policy_req.workflow_id = workflow_id;
    policy_req.node_type = NodeType::PolicySnapshot;
    let policy_snapshot = service.add_node(policy_req).await.unwrap();

    // Create an IntentVersion node
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let intent_version = service.add_node(iv_req).await.unwrap();

    // Ingest approval governed by policy snapshot and associated with intent version
    let approval_req = ApprovalIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Approval,
            ref_id: Uuid::new_v4(),
        },
        label: "approval-7".to_string(),
        governed_by_policy_snapshot: Some(policy_snapshot.id),
        intent_version_id: Some(intent_version.id),
        properties: Some(serde_json::json!({"scope": "production-deploy"})),
    };

    let result = service.ingest_approval(approval_req).await.unwrap();

    // Verify node
    assert_eq!(result.node.node_type, NodeType::Approval);
    assert_eq!(result.node.label, "approval-7");
    assert_eq!(result.node.tenant_id, tenant_id);

    // Verify two edges: GovernedBy -> PolicySnapshot, ValidatedBy -> IntentVersion
    assert_eq!(result.edges.len(), 2);

    let governed_by_edge = result
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::GovernedBy)
        .unwrap();
    assert_eq!(governed_by_edge.from_node_id, result.node.id);
    assert_eq!(governed_by_edge.to_node_id, policy_snapshot.id);

    let validated_by_edge = result
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::ValidatedBy)
        .unwrap();
    assert_eq!(validated_by_edge.from_node_id, result.node.id);
    assert_eq!(validated_by_edge.to_node_id, intent_version.id);
}

#[tokio::test]
async fn test_ingest_approval_without_optional_edges() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Ingest approval without policy snapshot or intent version
    let approval_req = ApprovalIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Approval,
            ref_id: Uuid::new_v4(),
        },
        label: "minimal-approval".to_string(),
        governed_by_policy_snapshot: None,
        intent_version_id: None,
        properties: None,
    };

    let result = service.ingest_approval(approval_req).await.unwrap();

    // Verify node created but no edges
    assert_eq!(result.node.node_type, NodeType::Approval);
    assert!(result.edges.is_empty());
}

#[tokio::test]
async fn test_ingest_side_effect_creates_node_and_edges() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a TaskNode (triggering task)
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic; // Using Generic as proxy for TaskNode in baseline
    task_req.label = "deploy-task".to_string();
    let task_node = service.add_node(task_req).await.unwrap();

    // Create an IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let intent_version = service.add_node(iv_req).await.unwrap();

    // Create an Approval
    let mut approval_req = create_test_node_request();
    approval_req.tenant_id = tenant_id;
    approval_req.workflow_id = workflow_id;
    approval_req.node_type = NodeType::Approval;
    let approval = service.add_node(approval_req).await.unwrap();

    // Ingest side effect with full trace
    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "deploy-1".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: Some(intent_version.id),
        approval_snapshot_id: Some(approval.id),
        properties: Some(serde_json::json!({"action": "kubectl-apply"})),
    };

    let result = service.ingest_side_effect(side_effect_req).await.unwrap();

    // Verify node
    assert_eq!(result.node.node_type, NodeType::SideEffect);
    assert_eq!(result.node.label, "deploy-1");
    assert_eq!(result.node.tenant_id, tenant_id);

    // Verify 3 edges:
    // 1. Triggers: TaskNode -> SideEffect
    // 2. DerivedFrom: SideEffect -> IntentVersion
    // 3. GeneratedFrom: SideEffect -> Approval
    assert_eq!(result.edges.len(), 3);

    let triggers_edge = result
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::Triggers)
        .unwrap();
    assert_eq!(triggers_edge.from_node_id, task_node.id);
    assert_eq!(triggers_edge.to_node_id, result.node.id);

    let derived_edge = result
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::DerivedFrom)
        .unwrap();
    assert_eq!(derived_edge.from_node_id, result.node.id);
    assert_eq!(derived_edge.to_node_id, intent_version.id);

    let generated_edge = result
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::GeneratedFrom)
        .unwrap();
    assert_eq!(generated_edge.from_node_id, result.node.id);
    assert_eq!(generated_edge.to_node_id, approval.id);
}

#[tokio::test]
async fn test_ingest_side_effect_minimal() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create only a TaskNode (triggering task)
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic;
    task_req.label = "minimal-task".to_string();
    let task_node = service.add_node(task_req).await.unwrap();

    // Ingest side effect with only required fields
    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "minimal-side-effect".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: None,
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await.unwrap();

    // Verify node created
    assert_eq!(result.node.node_type, NodeType::SideEffect);

    // Only 1 edge (Triggers) - the required one
    assert_eq!(result.edges.len(), 1);
    assert_eq!(result.edges[0].edge_type, EdgeType::Triggers);
    assert_eq!(result.edges[0].from_node_id, task_node.id);
    assert_eq!(result.edges[0].to_node_id, result.node.id);
}

#[tokio::test]
async fn test_ingest_artifact_traces_to_intent_version() {
    // This test verifies the graph invariant: every artifact must trace to at least one IntentVersion
    // Artifact --DependsOn--> IntentVersion (edge flows from artifact to intent version)
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create multiple IntentVersion nodes (representing different versions of an intent)
    let mut iv1_req = create_test_node_request();
    iv1_req.tenant_id = tenant_id;
    iv1_req.workflow_id = workflow_id;
    iv1_req.node_type = NodeType::IntentVersion;
    let iv1 = service.add_node(iv1_req).await.unwrap();

    let mut iv2_req = create_test_node_request();
    iv2_req.tenant_id = tenant_id;
    iv2_req.workflow_id = workflow_id;
    iv2_req.node_type = NodeType::IntentVersion;
    let iv2 = service.add_node(iv2_req).await.unwrap();

    // Ingest artifact that depends on both versions
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "traceable-artifact".to_string(),
        depends_on_intent_versions: vec![iv1.id, iv2.id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await.unwrap();

    // Verify artifact can reach both IntentVersions via DependsOn edges
    // Artifact --DependsOn--> IntentVersion (edge flows from artifact to intent)
    let path1 = service
        .find_path(result.node.id, iv1.id, TraversalOptions::default())
        .await
        .unwrap();
    assert!(
        !path1.is_empty(),
        "Artifact should be able to reach IntentVersion iv1 via DependsOn edge"
    );

    let path2 = service
        .find_path(result.node.id, iv2.id, TraversalOptions::default())
        .await
        .unwrap();
    assert!(
        !path2.is_empty(),
        "Artifact should be able to reach IntentVersion iv2 via DependsOn edge"
    );

    // Also verify the edges are created with correct direction
    let edges_from_artifact = service.list_edges_from(result.node.id).await.unwrap();
    assert_eq!(edges_from_artifact.len(), 2);
    let edge_targets: Vec<_> = edges_from_artifact.iter().map(|e| e.to_node_id).collect();
    assert!(edge_targets.contains(&iv1.id));
    assert!(edge_targets.contains(&iv2.id));
}

// ===== Negative Tests for Ingestor Failure Paths =====

#[tokio::test]
async fn test_ingest_artifact_empty_dependencies_rejected() {
    // Contract: every artifact must trace to at least one IntentVersion
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Attempt to ingest artifact with NO dependencies - should fail
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "orphan-artifact".to_string(),
        depends_on_intent_versions: vec![], // EMPTY - violates contract!
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::ArtifactTraceabilityEmpty
    ));
}

#[tokio::test]
async fn test_ingest_artifact_nonexistent_intent_version_rejected() {
    // Prevalidation: referenced IntentVersion nodes must exist
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let nonexistent_id = Uuid::new_v4();

    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "artifact-with-bad-ref".to_string(),
        depends_on_intent_versions: vec![nonexistent_id], // Does not exist!
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_artifact_wrong_node_type_rejected() {
    // Prevalidation: referenced nodes must be of type IntentVersion
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create an Artifact node (wrong type - should be IntentVersion)
    let mut wrong_node_req = create_test_node_request();
    wrong_node_req.tenant_id = tenant_id;
    wrong_node_req.workflow_id = workflow_id;
    wrong_node_req.node_type = NodeType::Artifact; // Wrong type!
    let wrong_node = service.add_node(wrong_node_req).await.unwrap();

    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "artifact-with-wrong-type".to_string(),
        depends_on_intent_versions: vec![wrong_node.id], // Not an IntentVersion!
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    // Verify no artifact node was created with this specific label (prevalidation prevents partial state)
    let nodes = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            workflow_id: Some(workflow_id),
            node_type: Some(NodeType::Artifact),
            ..Default::default()
        })
        .await
        .unwrap();
    // Only the wrong_node exists (which is not an ingested artifact), not a new artifact
    assert_eq!(
        nodes.len(),
        1,
        "Only wrong_node should exist, not a newly ingested artifact"
    );
    assert_eq!(
        nodes[0].id, wrong_node.id,
        "The only artifact should be wrong_node"
    );
}

#[tokio::test]
async fn test_ingest_approval_nonexistent_policy_snapshot_rejected() {
    // Prevalidation: PolicySnapshot reference must exist
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let nonexistent_id = Uuid::new_v4();

    let approval_req = ApprovalIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Approval,
            ref_id: Uuid::new_v4(),
        },
        label: "approval-with-bad-policy".to_string(),
        governed_by_policy_snapshot: Some(nonexistent_id), // Does not exist!
        intent_version_id: None,
        properties: None,
    };

    let result = service.ingest_approval(approval_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_approval_nonexistent_intent_version_rejected() {
    // Prevalidation: IntentVersion reference must exist
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let nonexistent_id = Uuid::new_v4();

    let approval_req = ApprovalIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Approval,
            ref_id: Uuid::new_v4(),
        },
        label: "approval-with-bad-iv".to_string(),
        governed_by_policy_snapshot: None,
        intent_version_id: Some(nonexistent_id), // Does not exist!
        properties: None,
    };

    let result = service.ingest_approval(approval_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_side_effect_nonexistent_trigger_rejected() {
    // Prevalidation: triggered_by_task must exist
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let nonexistent_id = Uuid::new_v4();

    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "side-effect-with-bad-trigger".to_string(),
        triggered_by_task: nonexistent_id, // Does not exist!
        derived_from_intent_version: None,
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_side_effect_nonexistent_intent_version_rejected() {
    // Prevalidation: derived_from_intent_version must exist and be IntentVersion
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a valid triggering node
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic;
    let task_node = service.add_node(task_req).await.unwrap();

    let nonexistent_id = Uuid::new_v4();

    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "side-effect-with-bad-iv".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: Some(nonexistent_id), // Does not exist!
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_side_effect_nonexistent_approval_rejected() {
    // Prevalidation: approval_snapshot_id must exist and be Approval
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a valid triggering node
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic;
    let task_node = service.add_node(task_req).await.unwrap();

    let nonexistent_id = Uuid::new_v4();

    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "side-effect-with-bad-approval".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: None,
        approval_snapshot_id: Some(nonexistent_id), // Does not exist!
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));
}

#[tokio::test]
async fn test_ingest_side_effect_wrong_node_type_for_intent_version_rejected() {
    // Prevalidation: derived_from_intent_version must be of type IntentVersion
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a valid triggering node
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic;
    let task_node = service.add_node(task_req).await.unwrap();

    // Create a node of wrong type (Artifact instead of IntentVersion)
    let mut wrong_node_req = create_test_node_request();
    wrong_node_req.tenant_id = tenant_id;
    wrong_node_req.workflow_id = workflow_id;
    wrong_node_req.node_type = NodeType::Artifact; // Wrong type!
    let wrong_node = service.add_node(wrong_node_req).await.unwrap();

    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "side-effect-with-wrong-iv-type".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: Some(wrong_node.id), // Not an IntentVersion!
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
    // Verify no side effect node was created (prevalidation prevents partial state)
    let nodes = service
        .list_nodes(GraphNodeFilter {
            node_type: Some(NodeType::SideEffect),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        nodes.is_empty(),
        "No side effect should be created when prevalidation fails"
    );
}

#[tokio::test]
async fn test_ingest_artifact_no_partial_state_on_failure() {
    // Verify that when prevalidation fails, no artifact node is created
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let nonexistent_id = Uuid::new_v4();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest artifact with nonexistent IntentVersion
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "should-not-be-created".to_string(),
        depends_on_intent_versions: vec![nonexistent_id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());

    // Count nodes after - should be same as before (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when ingest fails prevalidation"
    );
}

#[tokio::test]
async fn test_ingest_side_effect_no_partial_state_on_failure() {
    // Verify that when prevalidation fails, no side effect node is created
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create a valid triggering node
    let mut task_req = create_test_node_request();
    task_req.tenant_id = tenant_id;
    task_req.workflow_id = workflow_id;
    task_req.node_type = NodeType::Generic;
    let task_node = service.add_node(task_req).await.unwrap();

    // Count nodes before (should have 1 - the task node)
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest side effect with nonexistent IntentVersion
    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "should-not-be-created".to_string(),
        triggered_by_task: task_node.id,
        derived_from_intent_version: Some(Uuid::new_v4()), // Nonexistent IntentVersion
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());

    // Count nodes after - should be same as before (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when ingest fails prevalidation"
    );
}

// ===== Cross-Tenant/Workflow Scope Validation Tests =====

#[tokio::test]
async fn test_ingest_artifact_cross_tenant_rejected_no_partial_state() {
    // Artifact with tenant A cannot depend on IntentVersion with tenant B
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion in tenant B
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_b;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest artifact in tenant A depending on IntentVersion in tenant B
    let artifact_req = ArtifactIngestRequest {
        tenant_id: tenant_a,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "cross-tenant-artifact".to_string(),
        depends_on_intent_versions: vec![iv.id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));

    // Verify no artifact node was created (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when cross-tenant scope validation fails"
    );
}

#[tokio::test]
async fn test_ingest_artifact_cross_workflow_rejected_no_partial_state() {
    // Artifact with workflow A cannot depend on IntentVersion with workflow B
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_a = Uuid::new_v4();
    let workflow_b = Uuid::new_v4();

    // Create IntentVersion in workflow B
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_b;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            workflow_id: Some(workflow_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest artifact in workflow A depending on IntentVersion in workflow B
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id: workflow_a,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "cross-workflow-artifact".to_string(),
        depends_on_intent_versions: vec![iv.id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));

    // Verify no artifact node was created (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            workflow_id: Some(workflow_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when cross-workflow scope validation fails"
    );
}

#[tokio::test]
async fn test_ingest_approval_cross_tenant_rejected_no_partial_state() {
    // Approval with tenant A cannot reference PolicySnapshot with tenant B
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create PolicySnapshot in tenant B
    let mut ps_req = create_test_node_request();
    ps_req.tenant_id = tenant_b;
    ps_req.workflow_id = workflow_id;
    ps_req.node_type = NodeType::PolicySnapshot;
    let ps = service.add_node(ps_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest approval in tenant A referencing PolicySnapshot in tenant B
    let approval_req = ApprovalIngestRequest {
        tenant_id: tenant_a,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Approval,
            ref_id: Uuid::new_v4(),
        },
        label: "cross-tenant-approval".to_string(),
        governed_by_policy_snapshot: Some(ps.id),
        intent_version_id: None,
        properties: None,
    };

    let result = service.ingest_approval(approval_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));

    // Verify no approval node was created (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when cross-tenant scope validation fails"
    );
}

#[tokio::test]
async fn test_ingest_side_effect_cross_tenant_trigger_rejected_no_partial_state() {
    // SideEffect with tenant A cannot be triggered by node with tenant B
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create trigger node in tenant B
    let mut trigger_req = create_test_node_request();
    trigger_req.tenant_id = tenant_b;
    trigger_req.workflow_id = workflow_id;
    trigger_req.node_type = NodeType::Generic;
    let trigger = service.add_node(trigger_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest side effect in tenant A triggered by node in tenant B
    let side_effect_req = SideEffectIngestRequest {
        tenant_id: tenant_a,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "cross-tenant-side-effect".to_string(),
        triggered_by_task: trigger.id,
        derived_from_intent_version: None,
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));

    // Verify no side effect node was created (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when cross-tenant scope validation fails"
    );
}

#[tokio::test]
async fn test_ingest_side_effect_cross_workflow_trigger_rejected_no_partial_state() {
    // SideEffect with workflow A cannot be triggered by node with workflow B
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_a = Uuid::new_v4();
    let workflow_b = Uuid::new_v4();

    // Create trigger node in workflow B
    let mut trigger_req = create_test_node_request();
    trigger_req.tenant_id = tenant_id;
    trigger_req.workflow_id = workflow_b;
    trigger_req.node_type = NodeType::Generic;
    let trigger = service.add_node(trigger_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            workflow_id: Some(workflow_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Attempt to ingest side effect in workflow A triggered by node in workflow B
    let side_effect_req = SideEffectIngestRequest {
        tenant_id,
        workflow_id: workflow_a,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::SideEffect,
            ref_id: Uuid::new_v4(),
        },
        label: "cross-workflow-side-effect".to_string(),
        triggered_by_task: trigger.id,
        derived_from_intent_version: None,
        approval_snapshot_id: None,
        properties: None,
    };

    let result = service.ingest_side_effect(side_effect_req).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidIngestRequest(_)
    ));

    // Verify no side effect node was created (no partial state)
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            workflow_id: Some(workflow_a),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before, nodes_after,
        "No nodes should be created when cross-workflow scope validation fails"
    );
}

#[tokio::test]
async fn test_ingest_artifact_same_scope_succeeds() {
    // Verify that artifacts CAN depend on IntentVersions in the same tenant/workflow
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion in same tenant/workflow
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Count nodes before
    let nodes_before = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    // Ingest artifact in same tenant/workflow
    let artifact_req = ArtifactIngestRequest {
        tenant_id,
        workflow_id,
        external_ref: ExternalRef {
            ref_type: ExternalRefType::Artifact,
            ref_id: Uuid::new_v4(),
        },
        label: "same-scope-artifact".to_string(),
        depends_on_intent_versions: vec![iv.id],
        properties: None,
        ..Default::default()
    };

    let result = service.ingest_artifact(artifact_req).await;
    assert!(result.is_ok());

    // Verify artifact node was created
    let nodes_after = service
        .list_nodes(GraphNodeFilter {
            tenant_id: Some(tenant_id),
            ..Default::default()
        })
        .await
        .unwrap()
        .len();

    assert_eq!(
        nodes_before + 1,
        nodes_after,
        "Artifact node should be created"
    );
}

// ===== Classification Tests =====

#[tokio::test]
async fn test_classify_direct_impact_single_hop() {
    // Graph: IntentVersion IV1 -> (DependsOn) -> Artifact A1
    // When we classify from IV1, A1 should be Direct impact
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Create Artifact that depends on it
    let mut artifact_req = create_test_node_request();
    artifact_req.tenant_id = tenant_id;
    artifact_req.workflow_id = workflow_id;
    artifact_req.node_type = NodeType::Artifact;
    let artifact = service.add_node(artifact_req).await.unwrap();

    // Create DependsOn edge: Artifact -> IntentVersion
    let edge_req = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(edge_req).await.unwrap();

    // Classify from IntentVersion
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact]),
            propagation_config: None,
        })
        .await
        .unwrap();

    assert_eq!(result.start_node_id, iv.id);
    assert_eq!(result.max_depth, 3);
    assert_eq!(result.classified_nodes.len(), 1);

    let classified = &result.classified_nodes[0];
    assert_eq!(classified.node.id, artifact.id);
    assert_eq!(classified.impact, ClassificationImpact::Direct);
    assert!(classified.reason.contains("depends on"));
}

#[tokio::test]
async fn test_classify_transitive_impact_two_hops() {
    // Graph: IntentVersion IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
    // When we classify from IV1, A1 should be Direct and SE1 should be Transitive
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Create Artifact
    let mut artifact_req = create_test_node_request();
    artifact_req.tenant_id = tenant_id;
    artifact_req.workflow_id = workflow_id;
    artifact_req.node_type = NodeType::Artifact;
    let artifact = service.add_node(artifact_req).await.unwrap();

    // Create Generic trigger node
    let mut trigger_req = create_test_node_request();
    trigger_req.tenant_id = tenant_id;
    trigger_req.workflow_id = workflow_id;
    trigger_req.node_type = NodeType::Generic;
    let trigger = service.add_node(trigger_req).await.unwrap();

    // Create SideEffect
    let mut side_effect_req = create_test_node_request();
    side_effect_req.tenant_id = tenant_id;
    side_effect_req.workflow_id = workflow_id;
    side_effect_req.node_type = NodeType::SideEffect;
    let side_effect = service.add_node(side_effect_req).await.unwrap();

    // Create edges: Artifact -> IntentVersion, SideEffect -> Artifact (via Triggers)
    let edge1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(edge1).await.unwrap();

    let edge2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: trigger.id,
        to_node_id: side_effect.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(edge2).await.unwrap();

    // Wire: artifact triggers trigger node (so we get a chain)
    let edge3 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: trigger.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(edge3).await.unwrap();

    // Classify from IntentVersion
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // Should find: Artifact (Direct), Trigger (Direct), SideEffect (Transitive)
    assert_eq!(result.start_node_id, iv.id);

    // Find artifact and side_effect in classified
    let artifact_classified = result
        .classified_nodes
        .iter()
        .find(|c| c.node.id == artifact.id);
    let side_effect_classified = result
        .classified_nodes
        .iter()
        .find(|c| c.node.id == side_effect.id);

    assert!(artifact_classified.is_some());
    assert_eq!(
        artifact_classified.unwrap().impact,
        ClassificationImpact::Direct
    );

    assert!(side_effect_classified.is_some());
    assert_eq!(
        side_effect_classified.unwrap().impact,
        ClassificationImpact::Transitive
    );
}

#[tokio::test]
async fn test_classify_no_impact_unreachable_node() {
    // Graph: IntentVersion IV1 -> Artifact A1
    //                    (separate) IV2 -> Artifact A2
    // IV1 classify should only find A1, not A2
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IV1 and A1
    let mut iv1_req = create_test_node_request();
    iv1_req.tenant_id = tenant_id;
    iv1_req.workflow_id = workflow_id;
    iv1_req.node_type = NodeType::IntentVersion;
    let iv1 = service.add_node(iv1_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    // Create IV2 and A2 (not connected to IV1)
    let mut iv2_req = create_test_node_request();
    iv2_req.tenant_id = tenant_id;
    iv2_req.workflow_id = workflow_id;
    iv2_req.node_type = NodeType::IntentVersion;
    let _iv2 = service.add_node(iv2_req).await.unwrap();

    let mut a2_req = create_test_node_request();
    a2_req.tenant_id = tenant_id;
    a2_req.workflow_id = workflow_id;
    a2_req.node_type = NodeType::Artifact;
    let _a2 = service.add_node(a2_req).await.unwrap();

    // Connect IV1 -> A1 only
    let edge = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv1.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(edge).await.unwrap();

    // Classify from IV1
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv1.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // Only A1 should be classified
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(result.classified_nodes[0].node.id, a1.id);
}

#[tokio::test]
async fn test_classify_max_depth_bounds_traversal() {
    // Graph: IV1 -> A1 -> A2 -> A3
    // With max_depth=2, only A1 and A2 should be found
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create nodes
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut a2_req = create_test_node_request();
    a2_req.tenant_id = tenant_id;
    a2_req.workflow_id = workflow_id;
    a2_req.node_type = NodeType::Artifact;
    let a2 = service.add_node(a2_req).await.unwrap();

    let mut a3_req = create_test_node_request();
    a3_req.tenant_id = tenant_id;
    a3_req.workflow_id = workflow_id;
    a3_req.node_type = NodeType::Artifact;
    let a3 = service.add_node(a3_req).await.unwrap();

    // Create chain: A1->IV1, A2->A1, A3->A2
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a2.id,
        to_node_id: a1.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    let e3 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a3.id,
        to_node_id: a2.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e3).await.unwrap();

    // Classify with max_depth=2
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(2),
            target_node_types: Some(vec![NodeType::Artifact]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // A1 (depth 1) and A2 (depth 2) should be found
    assert_eq!(result.classified_nodes.len(), 2);
    let ids: Vec<_> = result.classified_nodes.iter().map(|c| c.node.id).collect();
    assert!(ids.contains(&a1.id));
    assert!(ids.contains(&a2.id));
    assert!(!ids.contains(&a3.id));
}

#[tokio::test]
async fn test_classify_start_node_not_found() {
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: Uuid::new_v4(),
            max_depth: Some(3),
            target_node_types: None,
            propagation_config: None,
        })
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::GraphNodeNotFound(_)
    ));
}

#[tokio::test]
async fn test_classify_empty_graph() {
    // Start node exists but no outgoing edges
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: None,
            propagation_config: None,
        })
        .await
        .unwrap();

    assert_eq!(result.classified_nodes.len(), 0);
    assert_eq!(result.start_node_id, iv.id);
}

#[tokio::test]
async fn test_classify_diamond_graph_reaches_node_once() {
    // Diamond: IV1 -> A1, IV1 -> A2, A1 -> A3, A2 -> A3
    // A3 should appear once with the shortest path reason
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create nodes
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut a2_req = create_test_node_request();
    a2_req.tenant_id = tenant_id;
    a2_req.workflow_id = workflow_id;
    a2_req.node_type = NodeType::Artifact;
    let a2 = service.add_node(a2_req).await.unwrap();

    let mut a3_req = create_test_node_request();
    a3_req.tenant_id = tenant_id;
    a3_req.workflow_id = workflow_id;
    a3_req.node_type = NodeType::Artifact;
    let a3 = service.add_node(a3_req).await.unwrap();

    // Create edges: A1->IV1, A2->IV1, A3->A1, A3->A2
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a2.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    let e3 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a3.id,
        to_node_id: a1.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e3).await.unwrap();

    let e4 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a3.id,
        to_node_id: a2.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e4).await.unwrap();

    // Classify from IV1
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // A3 should appear exactly once (visited once despite two paths)
    let a3_classified: Vec<_> = result
        .classified_nodes
        .iter()
        .filter(|c| c.node.id == a3.id)
        .collect();
    assert_eq!(a3_classified.len(), 1);
    // A3 should be transitive (depth 2)
    assert_eq!(a3_classified[0].impact, ClassificationImpact::Transitive);
}

// ===== PR #13 Rule-Pack Propagation Config Tests =====

#[tokio::test]
async fn test_classify_propagation_config_default_backward_compat() {
    // When propagation_config is None, should use DEFAULT_PROPAGATION_CONFIG
    // This test verifies backward compatibility - existing behavior preserved
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Create Artifact
    let mut artifact_req = create_test_node_request();
    artifact_req.tenant_id = tenant_id;
    artifact_req.workflow_id = workflow_id;
    artifact_req.node_type = NodeType::Artifact;
    let artifact = service.add_node(artifact_req).await.unwrap();

    // Create DependsOn edge
    let edge_req = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(edge_req).await.unwrap();

    // Classify with propagation_config = None (should use default)
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // Should find the artifact as Direct
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(
        result.classified_nodes[0].impact,
        ClassificationImpact::Direct
    );
}

#[tokio::test]
async fn test_classify_propagation_config_custom_max_depth() {
    // Custom propagation config with max_depth=1 should not find transitive nodes
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create chain: IV1 -> A1 -> A2
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut a2_req = create_test_node_request();
    a2_req.tenant_id = tenant_id;
    a2_req.workflow_id = workflow_id;
    a2_req.node_type = NodeType::Artifact;
    let a2 = service.add_node(a2_req).await.unwrap();

    // A1 -> IV1, A2 -> A1
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a2.id,
        to_node_id: a1.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    // Custom config with max_depth=1
    let custom_config = PropagationConfig {
        max_depth: Some(1),
        traversable_edge_types: vec![
            EdgeType::DependsOn,
            EdgeType::Triggers,
            EdgeType::GeneratedFrom,
        ],
        traversable_directions: vec![EdgeDirection::Both],
        target_node_types: vec![NodeType::Artifact],
    };

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: None,         // Should be overridden by config
            target_node_types: None, // Should be overridden by config
            propagation_config: Some(custom_config),
        })
        .await
        .unwrap();

    // With max_depth=1, only A1 (Direct) should be found, not A2 (Transitive)
    assert_eq!(result.max_depth, 1);
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(result.classified_nodes[0].node.id, a1.id);
    assert_eq!(
        result.classified_nodes[0].impact,
        ClassificationImpact::Direct
    );
}

#[tokio::test]
async fn test_classify_propagation_config_custom_target_types() {
    // Custom config targeting only SideEffect should not classify Artifacts
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Create Artifact
    let mut artifact_req = create_test_node_request();
    artifact_req.tenant_id = tenant_id;
    artifact_req.workflow_id = workflow_id;
    artifact_req.node_type = NodeType::Artifact;
    let artifact = service.add_node(artifact_req).await.unwrap();

    // Create SideEffect
    let mut se_req = create_test_node_request();
    se_req.tenant_id = tenant_id;
    se_req.workflow_id = workflow_id;
    se_req.node_type = NodeType::SideEffect;
    let side_effect = service.add_node(se_req).await.unwrap();

    // Create edges: Artifact -> IV1 (DependsOn), Artifact -> SideEffect (Triggers)
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: side_effect.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    // Custom config targeting only SideEffect
    let custom_config = PropagationConfig {
        max_depth: Some(3),
        traversable_edge_types: vec![
            EdgeType::DependsOn,
            EdgeType::Triggers,
            EdgeType::GeneratedFrom,
        ],
        traversable_directions: vec![EdgeDirection::Both],
        target_node_types: vec![NodeType::SideEffect], // Only SideEffect!
    };

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: None,
            propagation_config: Some(custom_config),
        })
        .await
        .unwrap();

    // Only SideEffect should be classified, not Artifact
    // Note: The propagation still goes through Artifact to reach SideEffect,
    // but Artifact itself is not classified.
    // SideEffect is at depth 2 (transitive via Artifact -> SideEffect),
    // so it should be classified as Transitive, not Direct.
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(result.classified_nodes[0].node.id, side_effect.id);
    assert_eq!(
        result.classified_nodes[0].impact,
        ClassificationImpact::Transitive
    );
}

#[tokio::test]
async fn test_classify_propagation_config_empty_edge_types() {
    // With empty traversable_edge_types, no nodes should be reached
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion and Artifact
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut artifact_req = create_test_node_request();
    artifact_req.tenant_id = tenant_id;
    artifact_req.workflow_id = workflow_id;
    artifact_req.node_type = NodeType::Artifact;
    let artifact = service.add_node(artifact_req).await.unwrap();

    // Create DependsOn edge
    let edge_req = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: artifact.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(edge_req).await.unwrap();

    // Custom config with EMPTY traversable_edge_types
    let custom_config = PropagationConfig {
        max_depth: Some(3),
        traversable_edge_types: vec![], // Nothing traversable!
        traversable_directions: vec![EdgeDirection::Both],
        target_node_types: vec![NodeType::Artifact],
    };

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: None,
            propagation_config: Some(custom_config),
        })
        .await
        .unwrap();

    // No edges should be traversed, so no nodes classified
    assert!(result.classified_nodes.is_empty());
}

#[tokio::test]
async fn test_classify_propagation_config_max_depth_from_request() {
    // When config.max_depth is None, should fall back to request.max_depth
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create chain: IV1 -> A1 -> A2
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut a2_req = create_test_node_request();
    a2_req.tenant_id = tenant_id;
    a2_req.workflow_id = workflow_id;
    a2_req.node_type = NodeType::Artifact;
    let a2 = service.add_node(a2_req).await.unwrap();

    // A1 -> IV1, A2 -> A1
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a2.id,
        to_node_id: a1.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    // Config with max_depth=None, but request with max_depth=2
    let custom_config = PropagationConfig {
        max_depth: None, // Fall back to request
        traversable_edge_types: vec![
            EdgeType::DependsOn,
            EdgeType::Triggers,
            EdgeType::GeneratedFrom,
        ],
        traversable_directions: vec![EdgeDirection::Both],
        target_node_types: vec![NodeType::Artifact],
    };

    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(2), // Should be used since config.max_depth is None
            target_node_types: None,
            propagation_config: Some(custom_config),
        })
        .await
        .unwrap();

    // max_depth should be 2 from request
    assert_eq!(result.max_depth, 2);
    // A1 (Direct) and A2 (Transitive) should be found
    assert_eq!(result.classified_nodes.len(), 2);
}

#[tokio::test]
async fn test_classify_propagation_config_reaches_approval_via_generated_from() {
    // Graph: IV1 -> A1 (DependsOn), A1 -> SE1 (Triggers), SE1 -> AP1 (GeneratedFrom)
    // Starting from IV1, we should reach AP1 via the chain
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create nodes
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut se_req = create_test_node_request();
    se_req.tenant_id = tenant_id;
    se_req.workflow_id = workflow_id;
    se_req.node_type = NodeType::SideEffect;
    let se1 = service.add_node(se_req).await.unwrap();

    let mut ap_req = create_test_node_request();
    ap_req.tenant_id = tenant_id;
    ap_req.workflow_id = workflow_id;
    ap_req.node_type = NodeType::Approval;
    let ap1 = service.add_node(ap_req).await.unwrap();

    // Create edges: A1->IV1, A1->SE1, SE1->AP1
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: se1.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    let e3 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: se1.id,
        to_node_id: ap1.id,
        edge_type: EdgeType::GeneratedFrom,
        properties: None,
    };
    service.add_edge(e3).await.unwrap();

    // Default config should reach all node types
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![
                NodeType::Artifact,
                NodeType::SideEffect,
                NodeType::Approval,
            ]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // Should find all three: A1 (Direct), SE1 (Transitive), AP1 (Transitive)
    assert_eq!(result.classified_nodes.len(), 3);
    let ids: Vec<_> = result.classified_nodes.iter().map(|c| c.node.id).collect();
    assert!(ids.contains(&a1.id));
    assert!(ids.contains(&se1.id));
    assert!(ids.contains(&ap1.id));
}

#[tokio::test]
async fn test_classify_approval_via_validated_by() {
    // Graph: AP1 -> (ValidatedBy) -> IV1
    // Starting from IV1, we should find AP1 via incoming ValidatedBy edge
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create IntentVersion
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    // Create Approval
    let mut ap_req = create_test_node_request();
    ap_req.tenant_id = tenant_id;
    ap_req.workflow_id = workflow_id;
    ap_req.node_type = NodeType::Approval;
    let ap = service.add_node(ap_req).await.unwrap();

    // Create ValidatedBy edge: Approval -> IntentVersion
    // ValidatedBy goes FROM the node doing the validating TO the node being validated
    let e = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: ap.id,
        to_node_id: iv.id,
        edge_type: EdgeType::ValidatedBy,
        properties: None,
    };
    service.add_edge(e).await.unwrap();

    // Classify from IntentVersion
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Approval]),
            propagation_config: None,
        })
        .await
        .unwrap();

    // AP should be classified as Direct (depth 1)
    assert_eq!(result.classified_nodes.len(), 1);
    let classified = &result.classified_nodes[0];
    assert_eq!(classified.node.id, ap.id);
    assert_eq!(classified.impact, ClassificationImpact::Direct);
    assert!(classified.reason.contains("validates this version"));
}

// ===== PR #13 Backward Compat: target_node_types fallback Tests =====

#[tokio::test]
async fn test_classify_backward_compat_request_target_types_when_config_none() {
    // PR #13 fix: When propagation_config is None AND request.target_node_types is Some,
    // the request's target_node_types should be used (backward compat for existing callers).
    //
    // Graph: IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
    // With request.target_node_types = Some([SideEffect]) and propagation_config = None,
    // only SE1 should be classified, NOT A1.
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create nodes
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut se_req = create_test_node_request();
    se_req.tenant_id = tenant_id;
    se_req.workflow_id = workflow_id;
    se_req.node_type = NodeType::SideEffect;
    let se1 = service.add_node(se_req).await.unwrap();

    // Create edges: Artifact -> IV1, Artifact -> SE1
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: se1.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    // Classify with propagation_config=None but request.target_node_types=Some([SideEffect])
    // This should classify ONLY SideEffect, not Artifact
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::SideEffect]), // Only SideEffect!
            propagation_config: None, // Uses default config, but should fall back to request types
        })
        .await
        .unwrap();

    // Should classify only SideEffect (at depth 2, transitive)
    // Artifact should NOT be classified because it's not in request.target_node_types
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(result.classified_nodes[0].node.id, se1.id);
    assert_eq!(
        result.classified_nodes[0].impact,
        ClassificationImpact::Transitive
    );
    // Verify Artifact A1 is NOT in the results
    let artifact_in_results = result.classified_nodes.iter().any(|c| c.node.id == a1.id);
    assert!(
        !artifact_in_results,
        "Artifact should NOT be classified when only SideEffect is in target_node_types"
    );
}

#[tokio::test]
async fn test_classify_request_target_types_ignored_when_config_provided() {
    // When propagation_config is Some, config.target_node_types takes precedence
    // over request.target_node_types (new behavior with explicit config).
    //
    // Graph: IV1 -> (DependsOn) -> Artifact A1 -> (Triggers) -> SideEffect SE1
    // With config.target_node_types = [SideEffect], only SE1 should be classified.
    let repo = Arc::new(InMemoryGraphRepository::new());
    let service = GraphService::new(repo.clone());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Create nodes
    let mut iv_req = create_test_node_request();
    iv_req.tenant_id = tenant_id;
    iv_req.workflow_id = workflow_id;
    iv_req.node_type = NodeType::IntentVersion;
    let iv = service.add_node(iv_req).await.unwrap();

    let mut a1_req = create_test_node_request();
    a1_req.tenant_id = tenant_id;
    a1_req.workflow_id = workflow_id;
    a1_req.node_type = NodeType::Artifact;
    let a1 = service.add_node(a1_req).await.unwrap();

    let mut se_req = create_test_node_request();
    se_req.tenant_id = tenant_id;
    se_req.workflow_id = workflow_id;
    se_req.node_type = NodeType::SideEffect;
    let se1 = service.add_node(se_req).await.unwrap();

    // Create edges: Artifact -> IV1, Artifact -> SE1
    let e1 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: iv.id,
        edge_type: EdgeType::DependsOn,
        properties: None,
    };
    service.add_edge(e1).await.unwrap();

    let e2 = CreateGraphEdgeRequest {
        tenant_id,
        workflow_id,
        from_node_id: a1.id,
        to_node_id: se1.id,
        edge_type: EdgeType::Triggers,
        properties: None,
    };
    service.add_edge(e2).await.unwrap();

    // Config that only targets SideEffect
    let config = PropagationConfig {
        max_depth: Some(3),
        traversable_edge_types: vec![
            EdgeType::DependsOn,
            EdgeType::Triggers,
            EdgeType::GeneratedFrom,
        ],
        traversable_directions: vec![intent_rebase_types::EdgeDirection::Both],
        target_node_types: vec![NodeType::SideEffect], // Only SideEffect in config
    };

    // Classify with explicit config (request.target_node_types is ignored)
    let result = service
        .classify_impact(ClassifyRequest {
            start_node_id: iv.id,
            max_depth: Some(3),
            target_node_types: Some(vec![NodeType::Artifact, NodeType::SideEffect]), // Ignored
            propagation_config: Some(config),
        })
        .await
        .unwrap();

    // Should classify only SideEffect (config takes precedence)
    assert_eq!(result.classified_nodes.len(), 1);
    assert_eq!(result.classified_nodes[0].node.id, se1.id);
}
