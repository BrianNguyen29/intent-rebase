use super::*;
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
