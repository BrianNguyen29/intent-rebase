use super::*;
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
