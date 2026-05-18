use super::*;
use intent_rebase_types::{EdgeType, ExternalRef, ExternalRefType};

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

// Integration tests requiring live Postgres - marked #[ignore]
// Run with: cargo test --test integration -- --ignored

#[tokio::test]
#[ignore]
async fn test_sqlx_graph_repository_create_and_get_node() {
    // Skip if no DATABASE_URL
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return,
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let repo = SqlxGraphRepository::new(pool);
    let service = GraphService::new(Arc::new(repo));

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
#[ignore]
async fn test_sqlx_graph_repository_create_and_get_edge() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return,
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let repo = SqlxGraphRepository::new(pool);
    let service = GraphService::new(Arc::new(repo));

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

    // Create edge
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
#[ignore]
async fn test_sqlx_graph_repository_list_edges_from_and_to() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return,
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let repo = SqlxGraphRepository::new(pool);
    let service = GraphService::new(Arc::new(repo));

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
    service.add_edge(edge_ab).await.unwrap();

    // B -> C
    let edge_bc = create_test_edge_request_with_ids(
        node_a.tenant_id,
        node_a.workflow_id,
        node_b.id,
        node_c.id,
    );
    service.add_edge(edge_bc).await.unwrap();

    // Check edges from A
    let edges_from_a = service.list_edges_from(node_a.id).await.unwrap();
    assert_eq!(edges_from_a.len(), 1);
    assert_eq!(edges_from_a[0].to_node_id, node_b.id);

    // Check edges to C
    let edges_to_c = service.list_edges_to(node_c.id).await.unwrap();
    assert_eq!(edges_to_c.len(), 1);
    assert_eq!(edges_to_c[0].from_node_id, node_b.id);
}

#[tokio::test]
#[ignore]
async fn test_sqlx_graph_repository_delete_edge() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return,
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let repo = SqlxGraphRepository::new(pool);
    let service = GraphService::new(Arc::new(repo));

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
#[ignore]
async fn test_sqlx_graph_repository_update_node_state() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => url,
        _ => return,
    };

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let repo = SqlxGraphRepository::new(pool);
    let service = GraphService::new(Arc::new(repo));

    let node = service.add_node(create_test_node_request()).await.unwrap();
    assert_eq!(node.state, NodeState::Active);

    let updated = service
        .update_node_state(node.id, NodeState::Stale)
        .await
        .unwrap();
    assert_eq!(updated.state, NodeState::Stale);
}

// Unit tests for type conversion helpers (no DB required)

#[test]
fn test_node_type_to_from_string() {
    for node_type in [
        NodeType::Intent,
        NodeType::IntentVersion,
        NodeType::Artifact,
        NodeType::Approval,
        NodeType::PolicySnapshot,
        NodeType::SideEffect,
        NodeType::Checkpoint,
        NodeType::Workflow,
        NodeType::Generic,
    ] {
        let s = node_type_to_string(&node_type);
        let round_trip = node_type_from_string(&s).unwrap();
        assert_eq!(node_type, round_trip);
    }
}

#[test]
fn test_edge_type_to_from_string() {
    for edge_type in [
        EdgeType::DependsOn,
        EdgeType::Produces,
        EdgeType::Approves,
        EdgeType::Triggers,
        EdgeType::Defines,
        EdgeType::GeneratedFrom,
        EdgeType::ValidatedBy,
        EdgeType::GovernedBy,
        EdgeType::DerivedFrom,
        EdgeType::StoredIn,
        EdgeType::Supersedes,
        EdgeType::Blocks,
        EdgeType::Compensates,
    ] {
        let s = edge_type_to_string(&edge_type);
        let round_trip = edge_type_from_string(&s).unwrap();
        assert_eq!(edge_type, round_trip);
    }
}

#[test]
fn test_node_state_to_from_string() {
    for state in [
        NodeState::Active,
        NodeState::Stale,
        NodeState::Invalid,
        NodeState::Archived,
    ] {
        let s = node_state_to_string(&state);
        let round_trip = node_state_from_string(&s).unwrap();
        assert_eq!(state, round_trip);
    }
}

#[test]
fn test_external_ref_type_to_from_string() {
    for ref_type in [
        ExternalRefType::Intent,
        ExternalRefType::IntentVersion,
        ExternalRefType::Artifact,
        ExternalRefType::Approval,
        ExternalRefType::PolicySnapshot,
        ExternalRefType::SideEffect,
        ExternalRefType::Checkpoint,
    ] {
        let s = external_ref_type_to_string(&ref_type);
        let round_trip = external_ref_type_from_string(&s).unwrap();
        assert_eq!(ref_type, round_trip);
    }
}
