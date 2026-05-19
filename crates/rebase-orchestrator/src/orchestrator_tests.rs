use crate::*;
use intent_rebase_types::*;
use rebase_engine::*;
use std::sync::Arc;
use uuid::Uuid;

// Helper to create test versions
fn create_test_version(intent_id: Uuid, version_num: i32) -> IntentVersion {
    IntentVersion {
        id: Uuid::new_v4(),
        intent_id,
        version_number: version_num,
        parent_version_id: None,
        created_at: chrono::Utc::now(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "test".to_string(),
        },
        change_reason: "test".to_string(),
        change_channel: ChangeChannel::UserEdit,
        status: VersionStatus::Active,
        hash: "test_hash".to_string(),
        payload: IntentPayload {
            objective: IntentObjective {
                summary: "Test objective".to_string(),
                success_statement: "Test success".to_string(),
                domain: "test".to_string(),
            },
            scope: IntentScope {
                in_scope: vec!["item1".to_string()],
                out_of_scope: vec![],
            },
            constraints: IntentConstraints {
                functional: vec![],
                non_functional: vec![],
                policy: vec![],
                budget: vec![],
                time: vec![],
            },
            acceptance_criteria: AcceptanceCriteria {
                required: vec![],
                optional: vec![],
            },
            authority: IntentAuthority {
                allowed_actions: vec![],
                forbidden_actions: vec![],
                approval_requirements: vec![],
            },
            preferences: IntentPreferences { tradeoffs: vec![] },
            references: IntentReferences {
                specs: vec![],
                tickets: vec![],
                repos: vec![],
                policies: vec![],
            },
            assumptions: IntentAssumptions { explicit: vec![] },
            metadata: IntentMetadataV1 {
                risk_tier: RiskTier::Low,
                urgency: Urgency::Low,
                confidence: 0.9,
            },
        },
    }
}

// Helper to create a test Checkpoint
fn create_test_checkpoint(
    intent_id: Uuid,
    intent_version: i32,
    workflow_id: Uuid,
    tenant_id: Uuid,
) -> Checkpoint {
    Checkpoint::with_required(
        intent_id,
        intent_version,
        workflow_id,
        tenant_id,
        CheckpointType::PreFlight,
    )
}

// Mock checkpoint repository for testing
struct MockCheckpointRepo {
    checkpoints: tokio::sync::RwLock<std::collections::HashMap<Uuid, Checkpoint>>,
}

impl MockCheckpointRepo {
    fn new() -> Self {
        Self {
            checkpoints: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    async fn add_checkpoint(&self, checkpoint: Checkpoint) {
        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.insert(checkpoint.checkpoint_id, checkpoint);
    }
}

#[async_trait::async_trait]
impl intent_service::CheckpointRepository for MockCheckpointRepo {
    async fn create_checkpoint(
        &self,
        checkpoint: Checkpoint,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let mut checkpoints = self.checkpoints.write().await;
        checkpoints.insert(checkpoint.checkpoint_id, checkpoint.clone());
        Ok(checkpoint)
    }

    async fn get_checkpoint(&self, checkpoint_id: Uuid) -> Result<Checkpoint, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints
            .get(&checkpoint_id)
            .cloned()
            .ok_or_else(|| IntentRebaseError::StorageError("not found".to_string()))
    }

    async fn list_by_workflow(
        &self,
        workflow_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        let mut result: Vec<Checkpoint> = checkpoints
            .values()
            .filter(|c| c.workflow_id == workflow_id && c.tenant_id == tenant_id)
            .cloned()
            .collect();
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(result)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
        let checkpoints = self.checkpoints.read().await;
        let mut result: Vec<Checkpoint> = checkpoints
            .values()
            .filter(|c| c.intent_id == intent_id && c.tenant_id == tenant_id)
            .cloned()
            .collect();
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(result)
    }

    async fn update_status(
        &self,
        checkpoint_id: Uuid,
        status: CheckpointStatus,
    ) -> Result<Checkpoint, IntentRebaseError> {
        let mut checkpoints = self.checkpoints.write().await;
        let checkpoint = checkpoints
            .get_mut(&checkpoint_id)
            .ok_or_else(|| IntentRebaseError::StorageError("not found".to_string()))?;
        checkpoint.status = status;
        Ok(checkpoint.clone())
    }

    async fn expire_checkpoints(&self) -> Result<usize, IntentRebaseError> {
        let now = chrono::Utc::now();
        let mut checkpoints = self.checkpoints.write().await;
        let mut expired = 0;
        for checkpoint in checkpoints.values_mut() {
            if let Some(expires_at) = checkpoint.expires_at {
                if expires_at < now
                    && checkpoint.status != CheckpointStatus::Expired
                    && checkpoint.status != CheckpointStatus::Superseded
                {
                    checkpoint.status = CheckpointStatus::Expired;
                    expired += 1;
                }
            }
        }
        Ok(expired)
    }
}

// Mock graph repository for testing
struct MockGraphRepo {
    nodes: tokio::sync::RwLock<std::collections::HashMap<Uuid, GraphNode>>,
}

impl MockGraphRepo {
    fn new() -> Self {
        Self {
            nodes: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    async fn add_node(&self, node: GraphNode) {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.id, node);
    }
}

#[async_trait::async_trait]
impl graph_service::GraphRepository for MockGraphRepo {
    async fn create_node(
        &self,
        request: CreateGraphNodeRequest,
    ) -> Result<GraphNode, IntentRebaseError> {
        let node = GraphNode {
            id: Uuid::new_v4(),
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: request.node_type,
            external_ref: request.external_ref,
            label: request.label,
            state: NodeState::Active,
            properties: request.properties.unwrap_or(serde_json::json!({})),
            created_at: chrono::Utc::now(),
        };
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.id, node.clone());
        Ok(node)
    }

    async fn get_node(&self, id: Uuid) -> Result<GraphNode, IntentRebaseError> {
        let nodes = self.nodes.read().await;
        nodes
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::GraphNodeNotFound(id))
    }

    async fn list_nodes(
        &self,
        filter: GraphNodeFilter,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        let nodes = self.nodes.read().await;
        let mut result: Vec<GraphNode> = nodes.values().cloned().collect();
        if let Some(tenant_id) = filter.tenant_id {
            result.retain(|n| n.tenant_id == tenant_id);
        }
        if let Some(workflow_id) = filter.workflow_id {
            result.retain(|n| n.workflow_id == workflow_id);
        }
        if let Some(node_type) = filter.node_type {
            result.retain(|n| n.node_type == node_type);
        }
        if let Some(state) = filter.state {
            result.retain(|n| n.state == state);
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
        _request: CreateGraphEdgeRequest,
    ) -> Result<GraphEdge, IntentRebaseError> {
        unimplemented!("MockGraphRepo does not support edge creation in tests")
    }

    async fn get_edge(&self, _id: Uuid) -> Result<GraphEdge, IntentRebaseError> {
        unimplemented!("MockGraphRepo does not support edge operations in tests")
    }

    async fn list_edges(
        &self,
        _filter: GraphEdgeFilter,
    ) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        Ok(vec![])
    }

    async fn list_edges_from(&self, _node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        Ok(vec![])
    }

    async fn list_edges_to(&self, _node_id: Uuid) -> Result<Vec<GraphEdge>, IntentRebaseError> {
        Ok(vec![])
    }

    async fn delete_edge(&self, _id: Uuid) -> Result<(), IntentRebaseError> {
        unimplemented!("MockGraphRepo does not support edge operations in tests")
    }
}

#[tokio::test]
async fn test_orchestrator_class_a_noop() {
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator = RebaseOrchestrator::with_mock_adapter(checkpoint_repo, graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2); // Same content = Class A

    let (diff, _risk) = compute_diff_with_risk_sync(&v1, &v2).unwrap();
    let plan = RebasePlan::from_diff_and_risk(
        &diff,
        &risk::DiffRiskAnalysis {
            severity: risk::Severity::Low,
            confidence: 1.0,
            manual_review: false,
            manual_review_reasons: vec![],
            section_risks: vec![],
            rationale: Some("No changes".to_string()),
        },
    );

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    assert_eq!(result.outcome, ApplyOutcome::NoOp);
    assert!(result.aligned_checkpoint.is_none());
    assert!(result.graph_updates.is_empty());
    assert!(!result.notification_required);
    // No-op path should have NotApplicable runtime execution status
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::NotApplicable
    );
    assert!(!result.runtime_execution_result.signal_sent);
    assert!(!result.runtime_execution_result.replay_completed);
    assert!(!result.runtime_execution_result.replay_attempted);
    assert_eq!(
        result.runtime_execution_result.status_message,
        "Not executed"
    );
}

#[tokio::test]
async fn test_orchestrator_high_risk_tier_blocked() {
    // Phase 2b: Test HIGH risk_tier blocked path (new risk-tier policy)
    // HIGH/CRITICAL risk_tier: blocked, requires manual approval
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator =
        RebaseOrchestrator::with_mock_adapter(checkpoint_repo.clone(), graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    // Directly construct a plan with HIGH risk_tier to test blocked policy
    let plan = RebasePlan {
        decision_class: DecisionClass::D,
        rationale: "Test: HIGH risk_tier blocked".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::D,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: true,
        risk_tier: RiskTier::High, // HIGH risk_tier triggers blocked
        risk_level: 4,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // HIGH risk_tier is blocked due to: risk-tier policy
    assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
    assert!(result.notification_required);
    // Blocked path should have NotApplicable runtime execution status
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::NotApplicable
    );
    assert!(!result.runtime_execution_result.signal_sent);
    assert!(!result.runtime_execution_result.replay_completed);
    assert!(!result.runtime_execution_result.replay_attempted);
    assert_eq!(
        result.runtime_execution_result.status_message,
        "Not executed"
    );
}

#[tokio::test]
async fn test_orchestrator_class_e_blocked() {
    // Test Class E blocked path (high/critical severity)
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator =
        RebaseOrchestrator::with_mock_adapter(checkpoint_repo.clone(), graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    // Directly construct a Class E plan (high/critical severity, manual review required)
    let plan = RebasePlan {
        decision_class: DecisionClass::E,
        rationale: "Test: Class E blocked".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::E,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: true,
        risk_tier: RiskTier::High,
        risk_level: 4, // High risk tier
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class E is blocked due to: high/critical severity
    assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
    assert!(result.notification_required);
    // Blocked path should have NotApplicable runtime execution status
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::NotApplicable
    );
    assert!(!result.runtime_execution_result.signal_sent);
    assert!(!result.runtime_execution_result.replay_completed);
    assert!(!result.runtime_execution_result.replay_attempted);
    assert_eq!(
        result.runtime_execution_result.status_message,
        "Not executed"
    );
}

#[tokio::test]
async fn test_orchestrator_class_b_proceeds_no_checkpoint() {
    // Test Class B proceed path when no checkpoint exists (replay skipped)
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // NO checkpoint created - this is the no-checkpoint scenario

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    // Directly construct a Class B plan (low severity, no manual review)
    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: Class B no checkpoint".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B should auto-proceed
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Status should be SucceededNoReplay (signal sent, no checkpoint for replay)
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::SucceededNoReplay
    );
    // Signal should be sent
    assert!(result.runtime_execution_result.signal_sent);
    // Replay should be skipped because no checkpoint exists
    assert!(!result.runtime_execution_result.replay_completed);
    // Replay was NOT attempted because no checkpoint was available
    assert!(!result.runtime_execution_result.replay_attempted);
    assert!(result
        .runtime_execution_result
        .status_message
        .contains("no checkpoint"));
}

#[tokio::test]
async fn test_graph_state_update() {
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());

    // Add a test node
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
    graph_repo.add_node(node).await;

    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let orchestrator = RebaseOrchestrator::with_mock_adapter(checkpoint_repo, graph_service);

    let affected_item = AffectedItem {
        node_id,
        label: "Test Artifact".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Directly affected".to_string(),
        external_ref: None,
    };

    let affected_items =
        AffectedItemsPreview::from_classification(vec![affected_item], vec![], vec![]);

    let updates = orchestrator
        .update_graph_state(&affected_items, Uuid::new_v4(), Uuid::new_v4(), 2)
        .await
        .unwrap();

    assert_eq!(updates.len(), 1);
    let action = updates[0].action.as_ref().unwrap();
    assert_eq!(action.previous_state, NodeState::Active);
    assert_eq!(action.new_state, NodeState::Stale);
}

#[tokio::test]
async fn test_plan_and_apply() {
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator =
        RebaseOrchestrator::with_mock_adapter(checkpoint_repo.clone(), graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    // Directly construct a Class B plan (low severity, no manual review)
    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: Class B plan".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B should auto-proceed
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Verify runtime_execution_result is Succeeded (signal sent and replay completed)
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::Succeeded
    );
    assert!(result.runtime_execution_result.signal_sent);
    assert!(result.runtime_execution_result.replay_completed);
    assert!(result.runtime_execution_result.replay_attempted);
}

#[tokio::test]
async fn test_runtime_execution_success() {
    // Test that MockAdapter with success config allows runtime execution
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint so replay path is exercised
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test runtime execution".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B is auto-proceeded (low/medium severity, no manual review required)
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Rationale should focus on apply decision (runtime detail lives in structured outcome)
    assert!(result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded"));
    // Status should be Succeeded (replay completed successfully)
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::Succeeded
    );
    assert!(result.runtime_execution_result.replay_attempted);
}

#[tokio::test]
async fn test_runtime_signal_failure_graceful_continuation() {
    // Test that runtime signal failure doesn't block the apply
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    // Configure mock to fail on signal
    let mock_adapter = Arc::new(MockAdapter::ready().with_signal_success(false));

    let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test runtime signal failure".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B should still auto-proceed even if runtime signal fails
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Rationale should focus on apply decision (not runtime detail)
    assert!(result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded"));
    // Verify runtime execution result reflects the failure (degraded)
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::Degraded
    );
    assert!(!result.runtime_execution_result.signal_sent);
    // Replay was not attempted because signal failed first
    assert!(!result.runtime_execution_result.replay_attempted);
}

#[tokio::test]
async fn test_runtime_replay_failure_graceful_continuation() {
    // Test that runtime replay failure doesn't block the apply
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo.clone()));
    // Configure mock to succeed on signal but fail on replay
    let mock_adapter = Arc::new(
        MockAdapter::ready()
            .with_signal_success(true)
            .with_replay_success(false),
    );

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint so replay path is exercised
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test runtime execution".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B should still auto-proceed even if replay fails
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Rationale should focus on apply decision (runtime detail lives in structured outcome)
    assert!(result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded"));
    // Verify runtime execution result reflects partial success (signal sent but replay failed)
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::Degraded
    );
    assert!(result.runtime_execution_result.signal_sent);
    assert!(!result.runtime_execution_result.replay_completed);
    // Replay was attempted but failed
    assert!(result.runtime_execution_result.replay_attempted);
}

#[tokio::test]
async fn test_runtime_ready_check() {
    // Test runtime readiness check
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service, mock_adapter);

    let is_ready = orchestrator.is_runtime_ready().await;
    assert!(is_ready, "MockAdapter should report ready");
}

#[tokio::test]
async fn test_runtime_not_ready_check() {
    // Test runtime not-ready detection
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::not_ready());

    let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service, mock_adapter);

    let is_ready = orchestrator.is_runtime_ready().await;
    assert!(!is_ready, "MockAdapter should report not ready");
}

#[tokio::test]
async fn test_skipped_not_ready_when_adapter_not_ready() {
    // Test that when adapter is not ready, runtime execution is skipped
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    // Use not_ready adapter - signal and replay should be skipped
    let mock_adapter = Arc::new(MockAdapter::not_ready());

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Create a checkpoint (aligns but execution should be skipped)
    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: adapter not ready".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    // Class B is auto-proceeded (low/medium severity, no manual review required)
    assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
    // Runtime execution should be SkippedNotReady
    assert_eq!(
        result.runtime_execution_result.status,
        RuntimeExecutionStatus::SkippedNotReady
    );
    assert!(!result.runtime_execution_result.signal_sent);
    assert!(!result.runtime_execution_result.replay_completed);
    assert!(!result.runtime_execution_result.replay_attempted);
    assert!(result
        .runtime_execution_result
        .status_message
        .contains("not ready"));
}

#[tokio::test]
async fn test_audit_summary_class_a_noop() {
    // Test audit_summary for Class A no-op path
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator = RebaseOrchestrator::with_mock_adapter(checkpoint_repo, graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2); // Same content = Class A

    let (diff, _risk) = compute_diff_with_risk_sync(&v1, &v2).unwrap();
    let plan = RebasePlan::from_diff_and_risk(
        &diff,
        &risk::DiffRiskAnalysis {
            severity: risk::Severity::Low,
            confidence: 1.0,
            manual_review: false,
            manual_review_reasons: vec![],
            section_risks: vec![],
            rationale: Some("No changes".to_string()),
        },
    );

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::NoOp);
    assert_eq!(
        summary.runtime_status,
        RuntimeExecutionStatus::NotApplicable
    );
    assert!(summary.checkpoint_outcome.is_none());
    assert!(summary.checkpoint_id.is_none());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
    assert!(!summary.rationale.is_empty());
}

#[tokio::test]
async fn test_audit_summary_high_risk_tier_blocked() {
    // Phase 2b: Test audit_summary for HIGH risk_tier blocked path (new risk-tier policy)
    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

    let orchestrator =
        RebaseOrchestrator::with_mock_adapter(checkpoint_repo.clone(), graph_service);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    // Directly construct a plan with HIGH risk_tier to test blocked policy
    let plan = RebasePlan {
        decision_class: DecisionClass::D,
        rationale: "Test: HIGH risk_tier blocked".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::D,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: true,
        risk_tier: RiskTier::High, // HIGH risk_tier triggers blocked
        risk_level: 4,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::BlockedManualReview);
    assert_eq!(
        summary.runtime_status,
        RuntimeExecutionStatus::NotApplicable
    );
    assert!(summary.checkpoint_outcome.is_none());
    assert!(summary.checkpoint_id.is_none());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(summary.notification_required);
    assert!(!summary.rationale.is_empty());
}

#[tokio::test]
async fn test_audit_summary_proceed_success() {
    // Test audit_summary for successful proceed path (Class B with checkpoint)
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: audit summary skipped not ready".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
    assert_eq!(summary.runtime_status, RuntimeExecutionStatus::Succeeded);
    assert!(summary.checkpoint_outcome.is_some());
    assert!(summary.checkpoint_id.is_some());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
    assert!(!summary.rationale.is_empty());
}

#[tokio::test]
async fn test_audit_summary_no_checkpoint() {
    // Test audit_summary for no-checkpoint proceed path
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: audit summary no checkpoint".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
    assert_eq!(
        summary.runtime_status,
        RuntimeExecutionStatus::SucceededNoReplay
    );
    // No checkpoint was found, so checkpoint_outcome should reflect that
    assert!(summary.checkpoint_outcome.is_some());
    // checkpoint_id is None because no checkpoint was available
    assert!(summary.checkpoint_id.is_none());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
}

#[tokio::test]
async fn test_audit_summary_degraded() {
    // Test audit_summary for degraded path (signal sent but replay failed)
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(
        MockAdapter::ready()
            .with_signal_success(true)
            .with_replay_success(false),
    );

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test runtime replay failure".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
    assert_eq!(summary.runtime_status, RuntimeExecutionStatus::Degraded);
    assert!(summary.checkpoint_outcome.is_some());
    assert!(summary.checkpoint_id.is_some());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
}

#[tokio::test]
async fn test_audit_summary_skipped_not_ready() {
    // Test audit_summary for skipped-not-ready path
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());
    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::not_ready());

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: adapter not ready".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &AffectedItemsPreview::unavailable(),
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
    assert_eq!(
        summary.runtime_status,
        RuntimeExecutionStatus::SkippedNotReady
    );
    assert!(summary.checkpoint_outcome.is_some());
    assert!(summary.checkpoint_id.is_some());
    assert_eq!(summary.graph_updates_applied, 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
}

#[tokio::test]
async fn test_audit_summary_with_graph_updates() {
    // Test audit_summary with actual graph updates
    use runtime_adapter::MockAdapter;

    let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
    let graph_repo = Arc::new(MockGraphRepo::new());

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
    graph_repo.add_node(node).await;

    let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));
    let mock_adapter = Arc::new(MockAdapter::ready());

    let orchestrator =
        RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service, mock_adapter);

    let intent_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
    checkpoint_repo.add_checkpoint(checkpoint).await;

    let v1 = create_test_version(intent_id, 1);
    let v2 = create_test_version(intent_id, 2);

    let plan = RebasePlan {
        decision_class: DecisionClass::B,
        rationale: "Test: audit summary degraded".to_string(),
        section_decisions: vec![],
        affected_items: AffectedItemsPreview::unavailable(),
        deferred: rebase_engine::DeferredFields::phase1_baseline(
            DecisionClass::B,
            &AffectedItemsPreview::unavailable(),
        ),
        manual_review_recommended: false,
        risk_tier: RiskTier::Low,
        risk_level: 2,
    };

    let affected_item = AffectedItem {
        node_id,
        label: "Test Artifact".to_string(),
        impact: ClassificationImpact::Direct,
        reason: "Directly affected".to_string(),
        external_ref: None,
    };

    let affected_items =
        AffectedItemsPreview::from_classification(vec![affected_item], vec![], vec![]);

    let result = orchestrator
        .apply_rebase(
            intent_id,
            tenant_id,
            workflow_id,
            &v1,
            &v2,
            &plan,
            &affected_items,
        )
        .await
        .unwrap();

    let summary = result.audit_summary();

    assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
    assert_eq!(summary.runtime_status, RuntimeExecutionStatus::Succeeded);
    assert!(summary.graph_updates_applied > 0);
    assert_eq!(summary.graph_updates_failed, 0);
    assert!(!summary.notification_required);
}
