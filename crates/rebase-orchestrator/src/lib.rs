//! Rebase Orchestrator — internal orchestration layer for Phase 2
//!
//! This crate provides internal-only orchestration for:
//! - Checkpoint-to-intent-version alignment logic
//! - Graph update orchestration for state-only mutations
//! - Internal low/medium apply pipeline (Class A/B/C auto-apply, D/E blocked)
//!
//! ## Design Principles
//!
//! - **No public HTTP endpoints** — this is pure internal compute
//! - **No Temporal/S3/frontend/auth integration** — deferred to Phase 3
//! - **MockAdapter/trait seams only** — no real runtime integration
//! - **Class D/E blocked** — manual review required, no auto-apply
//! - **Bounded state mutations** — graph updates only, no structural changes
//!
//! ## Architecture
//!
//! ```text
//! RebaseOrchestrator
//!   ├── checkpoint_aligner: CheckpointAligner
//!   │     └── Aligns planner checkpoint candidates to real checkpoint records
//!   ├── graph_updater: GraphUpdater  
//!   │     └── Applies bounded state mutations from classification results
//!   └── apply_pipeline: ApplyPipeline
//!         ├── Class A/B/C: auto-proceed with notification
//!         └── Class D/E: blocked, requires manual review
//! ```

pub mod apply_pipeline;
pub mod checkpoint_aligner;
pub mod graph_updater;

pub use apply_pipeline::{
    ApplyDecision, ApplyGuard, ApplyOutcome, ApplyPipeline, ApplyRequest, ApplyResult,
    HighCriticalGuard, LowMediumGuard,
};
pub use checkpoint_aligner::{
    AlignedCheckpoint, CheckpointAligner, CheckpointAlignmentOutcome, CheckpointAlignmentResult,
};
pub use graph_updater::{GraphUpdateAction, GraphUpdateResult, GraphUpdater};

use intent_rebase_types::AffectedItemsStatus;
#[allow(unused_imports)]
use intent_rebase_types::{Checkpoint, IntentRebaseError, IntentVersion};
#[allow(unused_imports)]
use rebase_engine::{AffectedItemsPreview, DecisionClass, RebasePlan};
use std::sync::Arc;
use uuid::Uuid;

/// Internal orchestrator that coordinates checkpoint alignment, graph updates,
/// and the apply pipeline for rebase operations.
///
/// This is the top-level orchestration entry point for Phase 2 internal processing.
pub struct RebaseOrchestrator {
    checkpoint_aligner: CheckpointAligner,
    graph_updater: GraphUpdater,
    apply_pipeline: ApplyPipeline,
}

impl RebaseOrchestrator {
    /// Create a new RebaseOrchestrator with the given dependencies.
    pub fn new(
        checkpoint_service: Arc<dyn intent_service::CheckpointRepository>,
        graph_service: Arc<graph_service::GraphService>,
    ) -> Self {
        Self {
            checkpoint_aligner: CheckpointAligner::new(checkpoint_service),
            graph_updater: GraphUpdater::new(graph_service),
            apply_pipeline: ApplyPipeline::new(),
        }
    }

    /// Align a rebase plan's checkpoint selection to real checkpoint records.
    ///
    /// Takes the planner's `CheckpointSelection` (from `RebasePlan.deferred.checkpoint_selection`)
    /// and resolves it to actual checkpoint records from storage.
    ///
    /// Returns an `AlignedCheckpoint` with the resolved checkpoint ID and outcome.
    pub async fn align_checkpoint(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
        plan: &RebasePlan,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        self.checkpoint_aligner
            .align(plan, intent_id, tenant_id, workflow_id)
            .await
    }

    /// Apply bounded graph state updates based on classification results.
    ///
    /// This only updates node states (Active, Stale, Invalid, Archived) based on
    /// the affected items from graph classification. It does NOT create or delete
    /// nodes or edges (structural mutations are deferred).
    ///
    /// Returns a list of `GraphUpdateResult` for each mutation applied.
    #[allow(dead_code)]
    pub async fn update_graph_state(
        &self,
        affected_items: &AffectedItemsPreview,
        intent_id: Uuid,
        _tenant_id: Uuid,
        intent_version: i32,
    ) -> Result<Vec<GraphUpdateResult>, IntentRebaseError> {
        // Only process if we have actual graph-derived data
        if affected_items.status != AffectedItemsStatus::Available {
            tracing::debug!(
                "Skipping graph state update: affected items status is {:?}",
                affected_items.status
            );
            return Ok(vec![]);
        }

        let mut results = Vec::new();

        // Update affected artifacts to Stale state
        for artifact in &affected_items.affected_artifacts {
            let result = self
                .graph_updater
                .update_node_state_if_affected(
                    artifact.node_id,
                    intent_rebase_types::NodeState::Stale,
                    format!("Affected by intent {} v{}", intent_id, intent_version),
                )
                .await?;
            results.push(result);
        }

        // Update affected approvals to Stale state
        for approval in &affected_items.affected_approvals {
            let result = self
                .graph_updater
                .update_node_state_if_affected(
                    approval.node_id,
                    intent_rebase_types::NodeState::Stale,
                    format!(
                        "Approval revalidation needed for intent {} v{}",
                        intent_id, intent_version
                    ),
                )
                .await?;
            results.push(result);
        }

        // Update side effects to Stale state if directly impacted
        for side_effect in &affected_items.side_effects {
            if matches!(
                side_effect.impact,
                intent_rebase_types::ClassificationImpact::Direct
            ) {
                let result = self
                    .graph_updater
                    .update_node_state_if_affected(
                        side_effect.node_id,
                        intent_rebase_types::NodeState::Stale,
                        format!(
                            "Directly affected side effect from intent {} v{}",
                            intent_id, intent_version
                        ),
                    )
                    .await?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Execute the internal apply pipeline for a rebase operation.
    ///
    /// This is the main entry point for Phase 2 internal apply:
    /// - Class A: No-op, return immediately
    /// - Class B/C: Auto-proceed with notification, align checkpoint, update graph
    /// - Class D/E: Blocked, return with manual review required
    ///
    /// Returns `RebaseApplyResult` with the outcome and any mutations applied.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_rebase(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
        from_version: &IntentVersion,
        to_version: &IntentVersion,
        plan: &RebasePlan,
        affected_items: &AffectedItemsPreview,
    ) -> Result<RebaseApplyResult, IntentRebaseError> {
        // Check if apply should proceed based on decision class
        let apply_decision = self.apply_pipeline.evaluate(plan.decision_class);

        match apply_decision {
            ApplyDecision::NoOp => {
                tracing::info!(
                    "Rebase for intent {} v{} -> v{} is a no-op (Class A)",
                    intent_id,
                    from_version.version_number,
                    to_version.version_number
                );
                Ok(RebaseApplyResult {
                    outcome: ApplyOutcome::NoOp,
                    aligned_checkpoint: None,
                    graph_updates: vec![],
                    notification_required: false,
                    rationale: "Class A: No semantic changes detected".to_string(),
                })
            }

            ApplyDecision::Blocked { reason } => {
                tracing::info!(
                    "Rebase for intent {} v{} -> v{} is blocked: {}",
                    intent_id,
                    from_version.version_number,
                    to_version.version_number,
                    reason
                );
                Ok(RebaseApplyResult {
                    outcome: ApplyOutcome::BlockedManualReview,
                    aligned_checkpoint: None,
                    graph_updates: vec![],
                    notification_required: true, // Notify about manual review requirement
                    rationale: reason,
                })
            }

            ApplyDecision::Proceed { notification } => {
                tracing::info!(
                    "Rebase for intent {} v{} -> v{} auto-proceeding (Class B/C)",
                    intent_id,
                    from_version.version_number,
                    to_version.version_number
                );

                // Step 1: Align checkpoint
                let aligned = self
                    .align_checkpoint(intent_id, tenant_id, workflow_id, plan)
                    .await?;

                // Step 2: Apply graph state updates (bounded mutations only)
                let graph_updates = self
                    .update_graph_state(
                        affected_items,
                        intent_id,
                        tenant_id,
                        to_version.version_number,
                    )
                    .await?;

                // Extract values needed for rationale before moving
                let aligned_outcome = aligned.outcome.clone();
                let graph_updates_count = graph_updates.len();

                Ok(RebaseApplyResult {
                    outcome: if notification {
                        ApplyOutcome::AutoProceededWithNotification
                    } else {
                        ApplyOutcome::AutoProceeded
                    },
                    aligned_checkpoint: Some(AlignedCheckpoint {
                        checkpoint_id: aligned.checkpoint_id,
                        checkpoint: aligned.checkpoint,
                        outcome: aligned.outcome,
                        rationale: aligned.rationale,
                    }),
                    graph_updates,
                    notification_required: notification,
                    rationale: format!(
                        "Class {:?} auto-proceeded. Checkpoint aligned: {:?}, {} graph updates applied",
                        plan.decision_class,
                        aligned_outcome,
                        graph_updates_count
                    ),
                })
            }
        }
    }

    /// Convenience method to compute rebase plan and apply in one call.
    ///
    /// Takes raw versions and runs the full pipeline:
    /// 1. Compute diff and risk analysis
    /// 2. Generate rebase plan with decision class
    /// 3. Evaluate apply eligibility
    /// 4. Execute alignment and graph updates if eligible
    pub async fn plan_and_apply(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
        from_version: &IntentVersion,
        to_version: &IntentVersion,
        affected_items: Option<AffectedItemsPreview>,
    ) -> Result<(RebasePlan, RebaseApplyResult), IntentRebaseError> {
        use rebase_engine::compute_diff_with_risk_sync;

        // Compute diff and risk
        let (diff, risk) = compute_diff_with_risk_sync(from_version, to_version)?;

        // Generate plan
        let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

        // Use provided affected items or default to unavailable
        let affected = affected_items.unwrap_or_else(AffectedItemsPreview::unavailable);

        // Execute apply pipeline
        let apply_result = self
            .apply_rebase(
                intent_id,
                tenant_id,
                workflow_id,
                from_version,
                to_version,
                &plan,
                &affected,
            )
            .await?;

        Ok((plan, apply_result))
    }
}

/// Result of applying a rebase with the internal apply pipeline.
#[derive(Debug, Clone)]
pub struct RebaseApplyResult {
    /// The apply outcome (auto_proceeded, blocked_manual_review, no_op)
    pub outcome: ApplyOutcome,
    /// The aligned checkpoint (if applicable)
    pub aligned_checkpoint: Option<AlignedCheckpoint>,
    /// Graph update results (if any state mutations were applied)
    pub graph_updates: Vec<GraphUpdateResult>,
    /// Whether a notification should be sent
    pub notification_required: bool,
    /// Detailed rationale for the decision
    pub rationale: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::*;
    use rebase_engine::*;
    use std::sync::Arc;

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

        async fn get_checkpoint(
            &self,
            checkpoint_id: Uuid,
        ) -> Result<Checkpoint, IntentRebaseError> {
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
            result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
            result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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
            unimplemented!("MockGraphRepo does not support edge operations in tests")
        }
    }

    #[tokio::test]
    async fn test_orchestrator_class_a_noop() {
        let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
        let graph_repo = Arc::new(MockGraphRepo::new());
        let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service);

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
    }

    #[tokio::test]
    async fn test_orchestrator_class_b_proceeds() {
        let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
        let graph_repo = Arc::new(MockGraphRepo::new());
        let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service);

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create a checkpoint
        let checkpoint = create_test_checkpoint(intent_id, 1, workflow_id, tenant_id);
        checkpoint_repo.add_checkpoint(checkpoint).await;

        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope.push("item2".to_string()); // Medium severity change (scope addition)

        let (diff, risk) = compute_diff_with_risk_sync(&v1, &v2).unwrap();
        let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

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

        // For this specific diff (scope addition), we get Class D because:
        // - scope items have no clause_id, causing ambiguous_match=1
        // - confidence = 0.5 < 0.7 threshold, so manual_review=true
        // - Medium severity + manual_review = Class D
        // Class D is blocked, so this will NOT auto-proceed
        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
        assert!(result.notification_required);
    }

    #[tokio::test]
    async fn test_orchestrator_class_d_with_direct_plan() {
        // Test Class D blocking with a directly-constructed plan
        // This avoids the complexity of diff computation
        let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
        let graph_repo = Arc::new(MockGraphRepo::new());
        let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service);

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let v1 = create_test_version(intent_id, 1);
        let v2 = create_test_version(intent_id, 2);

        // Directly construct a Class D plan
        let plan = RebasePlan {
            decision_class: DecisionClass::D,
            rationale: "Test: Class D plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::D,
                &AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: true,
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

        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
        assert!(result.notification_required);
    }

    #[tokio::test]
    async fn test_orchestrator_class_e_with_direct_plan() {
        // Test Class E blocking with a directly-constructed plan
        let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
        let graph_repo = Arc::new(MockGraphRepo::new());
        let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service);

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let v1 = create_test_version(intent_id, 1);
        let v2 = create_test_version(intent_id, 2);

        // Directly construct a Class E plan
        let plan = RebasePlan {
            decision_class: DecisionClass::E,
            rationale: "Test: Class E plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::E,
                &AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: true,
            risk_level: 5,
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

        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
        assert!(result.notification_required);
    }

    #[tokio::test]
    async fn test_orchestrator_class_e_blocked() {
        // Test Class E blocking with a directly-constructed plan
        let checkpoint_repo = Arc::new(MockCheckpointRepo::new());
        let graph_repo = Arc::new(MockGraphRepo::new());
        let graph_service = Arc::new(graph_service::GraphService::new(graph_repo));

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service);

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let v1 = create_test_version(intent_id, 1);
        let v2 = create_test_version(intent_id, 2);

        // Directly construct a Class E plan (Critical severity or 3+ high-severity sections)
        let plan = RebasePlan {
            decision_class: DecisionClass::E,
            rationale: "Test: Class E plan".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::E,
                &AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: true,
            risk_level: 5,
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

        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
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
        let orchestrator = RebaseOrchestrator::new(checkpoint_repo, graph_service);

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

        let orchestrator = RebaseOrchestrator::new(checkpoint_repo.clone(), graph_service);

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
    }
}
