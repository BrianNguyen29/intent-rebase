use super::*;
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
