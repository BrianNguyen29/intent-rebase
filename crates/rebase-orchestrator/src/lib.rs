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
//! - **Runtime adapter injection** — RuntimeAdapter for internal execution loop
//!
//! ## Architecture
//!
//! ```text
//! RebaseOrchestrator
//!   ├── checkpoint_aligner: CheckpointAligner
//!   │     └── Aligns planner checkpoint candidates to real checkpoint records
//!   ├── graph_updater: GraphUpdater
//!   │     └── Applies bounded state mutations from classification results
//!   ├── apply_pipeline: ApplyPipeline
//!   │     ├── Class A/B/C: auto-proceed with notification
//!   │     └── Class D/E: blocked, requires manual review
//!   └── runtime_adapter: Arc<dyn RuntimeAdapter>
//!         └── send_rebase_signal, replay_from_checkpoint (internal execution)
//! ```

/// Status of runtime execution for internal rebase operations.
///
/// Distinguishes between different execution outcomes to support
/// explicit not-ready / execution skipped semantics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RuntimeExecutionStatus {
    /// Not applicable — no execution attempted (no-op or blocked path)
    #[default]
    NotApplicable,
    /// Skipped — adapter not ready, execution skipped
    SkippedNotReady,
    /// Degraded — signal sent but replay failed
    Degraded,
    /// Succeeded — signal sent and replay completed successfully
    Succeeded,
    /// SucceededNoReplay — signal sent but no checkpoint available for replay
    SucceededNoReplay,
}

impl std::fmt::Display for RuntimeExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeExecutionStatus::NotApplicable => write!(f, "NotApplicable"),
            RuntimeExecutionStatus::SkippedNotReady => write!(f, "SkippedNotReady"),
            RuntimeExecutionStatus::Degraded => write!(f, "Degraded"),
            RuntimeExecutionStatus::Succeeded => write!(f, "Succeeded"),
            RuntimeExecutionStatus::SucceededNoReplay => write!(f, "SucceededNoReplay"),
        }
    }
}

/// Runtime execution outcome for internal rebase operations.
///
/// Reports the result of runtime adapter operations during the proceed path.
/// The status field distinguishes not-applicable, skipped-not-ready, degraded,
/// succeeded, and succeeded-no-replay outcomes.
#[derive(Debug, Clone)]
pub struct RuntimeExecutionResult {
    /// Execution status enum
    pub status: RuntimeExecutionStatus,
    /// Signal sent successfully
    pub signal_sent: bool,
    /// Replay completed successfully
    pub replay_completed: bool,
    /// Replay was attempted (even if it failed)
    pub replay_attempted: bool,
    /// Human-readable status message (detail lives here, not in rationale)
    pub status_message: String,
}

impl Default for RuntimeExecutionResult {
    fn default() -> Self {
        Self {
            status: RuntimeExecutionStatus::NotApplicable,
            signal_sent: false,
            replay_completed: false,
            replay_attempted: false,
            status_message: "Not executed".to_string(),
        }
    }
}

impl RuntimeExecutionResult {
    /// Create a result indicating execution was skipped due to adapter not ready
    pub fn skipped_not_ready() -> Self {
        Self {
            status: RuntimeExecutionStatus::SkippedNotReady,
            signal_sent: false,
            replay_completed: false,
            replay_attempted: false,
            status_message: "Skipped: adapter not ready".to_string(),
        }
    }

    /// Create a degraded result: signal sent but replay failed
    pub fn degraded(signal_sent: bool, replay_attempted: bool, reason: &str) -> Self {
        Self {
            status: RuntimeExecutionStatus::Degraded,
            signal_sent,
            replay_completed: false,
            replay_attempted,
            status_message: format!("Degraded: {}", reason),
        }
    }

    /// Create a success result: signal sent and replay completed
    pub fn succeeded() -> Self {
        Self {
            status: RuntimeExecutionStatus::Succeeded,
            signal_sent: true,
            replay_completed: true,
            replay_attempted: true,
            status_message: "Signal sent and replay completed".to_string(),
        }
    }

    /// Create a no-checkpoint result: signal sent but no checkpoint available for replay
    pub fn no_checkpoint() -> Self {
        Self {
            status: RuntimeExecutionStatus::SucceededNoReplay,
            signal_sent: true,
            replay_completed: false,
            replay_attempted: false,
            status_message: "Signal sent, no checkpoint for replay".to_string(),
        }
    }
}

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

use runtime_adapter::{AdapterError, RebaseSignal, RuntimeAdapter};

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
    runtime_adapter: Arc<dyn RuntimeAdapter>,
}

impl RebaseOrchestrator {
    /// Create a new RebaseOrchestrator with the given dependencies.
    ///
    /// # Arguments
    ///
    /// * `checkpoint_service` - Checkpoint repository for alignment
    /// * `graph_service` - Graph service for state mutations
    /// * `runtime_adapter` - Runtime adapter for internal execution (send_rebase_signal, replay)
    pub fn new(
        checkpoint_service: Arc<dyn intent_service::CheckpointRepository>,
        graph_service: Arc<graph_service::GraphService>,
        runtime_adapter: Arc<dyn RuntimeAdapter>,
    ) -> Self {
        Self {
            checkpoint_aligner: CheckpointAligner::new(checkpoint_service),
            graph_updater: GraphUpdater::new(graph_service),
            apply_pipeline: ApplyPipeline::new(),
            runtime_adapter,
        }
    }

    /// Create a new RebaseOrchestrator with default MockAdapter for testing.
    ///
    /// This is a convenience constructor that creates a MockAdapter internally,
    /// useful for tests that don't need to verify runtime adapter behavior.
    #[cfg(test)]
    pub fn with_mock_adapter(
        checkpoint_service: Arc<dyn intent_service::CheckpointRepository>,
        graph_service: Arc<graph_service::GraphService>,
    ) -> Self {
        use runtime_adapter::MockAdapter;
        Self::new(
            checkpoint_service,
            graph_service,
            Arc::new(MockAdapter::ready()),
        )
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

    /// Send a rebase signal to the runtime adapter for internal execution.
    ///
    /// Gates execution on adapter readiness. If the adapter is not ready,
    /// returns `RuntimeExecutionResult::skipped_not_ready()` without attempting
    /// signal or replay.
    ///
    /// Returns `RuntimeExecutionResult` indicating signal and replay status.
    async fn send_runtime_rebase_signal(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
        aligned: &AlignedCheckpoint,
    ) -> Result<RuntimeExecutionResult, AdapterError> {
        // Gate on adapter readiness - skip execution if not ready
        match self.runtime_adapter.is_adapter_ready().await {
            Ok(runtime_adapter::AdapterStatus::Ready) => {
                // Ready, proceed
            }
            Ok(_) => {
                // Not ready or initializing
                tracing::info!(
                    "Runtime adapter not ready, skipping signal for intent {}",
                    intent_id
                );
                return Ok(RuntimeExecutionResult::skipped_not_ready());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to check adapter readiness for intent {}: {:?}",
                    intent_id,
                    e
                );
                return Ok(RuntimeExecutionResult::skipped_not_ready());
            }
        }

        // Build the rebase signal for the runtime
        let signal = RebaseSignal {
            intent_id: intent_id.to_string(),
            signal_type: "proceed".to_string(),
            metadata: serde_json::json!({
                "tenant_id": tenant_id.to_string(),
                "workflow_id": workflow_id.to_string(),
                "checkpoint_id": aligned.checkpoint_id.map(|id| id.to_string()),
                "checkpoint_outcome": format!("{:?}", aligned.outcome),
            }),
        };

        // Send the signal
        if let Err(e) = self.runtime_adapter.send_rebase_signal(signal).await {
            // Signal failed - return degraded result with signal_sent: false
            tracing::warn!("Runtime signal failed for intent {}: {:?}", intent_id, e);
            return Ok(RuntimeExecutionResult::degraded(
                false,
                false, // replay_attempted: false - never reached replay
                &format!("Signal failed: {}", e),
            ));
        }

        // Signal sent successfully - now attempt replay if checkpoint available
        if let Some(checkpoint_id) = aligned.checkpoint_id {
            // Convert to runtime checkpoint format for replay
            let runtime_checkpoint = runtime_adapter::Checkpoint {
                id: checkpoint_id.to_string(),
                label: format!("Aligned checkpoint for intent {}", intent_id),
                description: aligned.rationale.clone(),
                timestamp: chrono::Utc::now(),
                validated: true,
            };

            let intent_ref = runtime_adapter::IntentRef::new(
                intent_id.to_string(),
                tenant_id.to_string(),
                workflow_id.to_string(),
                "active".to_string(),
            );

            // Attempt replay from checkpoint
            match self
                .runtime_adapter
                .replay_from_checkpoint(runtime_checkpoint, intent_ref)
                .await
            {
                Ok(()) => Ok(RuntimeExecutionResult::succeeded()),
                Err(e) => {
                    // Signal sent but replay failed - degraded
                    tracing::warn!("Replay failed for intent {}: {:?}", intent_id, e);
                    Ok(RuntimeExecutionResult::degraded(
                        true, // signal_sent: true
                        true, // replay_attempted: true - we attempted replay
                        &format!("Replay failed: {}", e),
                    ))
                }
            }
        } else {
            // Signal sent, no checkpoint for replay - still success (no replay needed)
            Ok(RuntimeExecutionResult::no_checkpoint())
        }
    }

    /// Check if the runtime adapter is ready.
    ///
    /// Returns `true` if the runtime is ready to accept signals and replay operations.
    pub async fn is_runtime_ready(&self) -> bool {
        match self.runtime_adapter.is_adapter_ready().await {
            Ok(status) => status == runtime_adapter::AdapterStatus::Ready,
            Err(_) => false,
        }
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
                    runtime_execution_result: RuntimeExecutionResult::default(),
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
                    runtime_execution_result: RuntimeExecutionResult::default(),
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

                // Step 3: Send rebase signal to runtime (internal execution loop)
                // Rationale is kept separate from runtime execution detail
                let runtime_result = self
                    .send_runtime_rebase_signal(intent_id, tenant_id, workflow_id, &aligned)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!("Runtime signal failed, continuing: {:?}", e);
                        RuntimeExecutionResult::degraded(
                            false, // signal_sent: false
                            false, // replay_attempted: false
                            &format!("Signal failed: {}", e),
                        )
                    });

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
                    // Rationale focuses on apply decision; runtime detail lives in structured outcome
                    rationale: format!(
                        "Class {:?} auto-proceeded. Checkpoint aligned: {:?}, {} graph updates applied",
                        plan.decision_class,
                        aligned_outcome,
                        graph_updates_count,
                    ),
                    runtime_execution_result: runtime_result,
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
    /// Runtime execution result (for proceed path) or default (no-op/blocked)
    pub runtime_execution_result: RuntimeExecutionResult,
}

impl RebaseApplyResult {
    /// Generate an internal audit summary for this apply result.
    ///
    /// The summary aggregates runtime outcome, checkpoint alignment/id,
    /// graph update counts, and notification requirement for internal
    /// audit/reporting purposes.
    ///
    /// This is a derived summary that does not add new persistent fields
    /// to `RebaseApplyResult`, preserving the existing shape.
    pub fn audit_summary(&self) -> RebaseApplySummary {
        RebaseApplySummary {
            outcome: self.outcome.clone(),
            runtime_status: self.runtime_execution_result.status.clone(),
            checkpoint_outcome: self.aligned_checkpoint.as_ref().map(|a| a.outcome.clone()),
            checkpoint_id: self
                .aligned_checkpoint
                .as_ref()
                .and_then(|a| a.checkpoint_id),
            graph_updates_count: self.graph_updates.len(),
            notification_required: self.notification_required,
            rationale: self.rationale.clone(),
        }
    }
}

/// Internal audit summary for rebase apply operations.
///
/// Aggregates runtime outcome, checkpoint alignment, graph update counts,
/// and notification requirement for audit/reporting purposes.
#[derive(Debug, Clone, PartialEq)]
pub struct RebaseApplySummary {
    /// Apply outcome (NoOp, AutoProceeded, BlockedManualReview)
    pub outcome: ApplyOutcome,
    /// Runtime execution status
    pub runtime_status: RuntimeExecutionStatus,
    /// Checkpoint alignment outcome (if checkpoint was aligned)
    pub checkpoint_outcome: Option<CheckpointAlignmentOutcome>,
    /// Checkpoint ID if alignment succeeded
    pub checkpoint_id: Option<Uuid>,
    /// Number of graph updates applied
    pub graph_updates_count: usize,
    /// Whether notification is required
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
        assert_eq!(result.runtime_execution_result.signal_sent, false);
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
        assert_eq!(
            result.runtime_execution_result.status_message,
            "Not executed"
        );
    }

    #[tokio::test]
    async fn test_orchestrator_class_d_blocked() {
        // Test Class D blocked path (medium severity + manual review required)
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

        // Class D is blocked due to: medium severity + manual_review required
        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
        assert!(result.notification_required);
        // Blocked path should have NotApplicable runtime execution status
        assert_eq!(
            result.runtime_execution_result.status,
            RuntimeExecutionStatus::NotApplicable
        );
        assert_eq!(result.runtime_execution_result.signal_sent, false);
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
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
        assert_eq!(result.runtime_execution_result.signal_sent, false);
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
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
        assert_eq!(result.runtime_execution_result.signal_sent, true);
        // Replay should be skipped because no checkpoint exists
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        // Replay was NOT attempted because no checkpoint was available
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
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
        assert_eq!(result.runtime_execution_result.signal_sent, true);
        assert_eq!(result.runtime_execution_result.replay_completed, true);
        assert_eq!(result.runtime_execution_result.replay_attempted, true);
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

        // Class B should auto-proceed with runtime execution
        assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
        // Rationale should focus on apply decision (runtime detail lives in structured outcome)
        assert!(
            result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded")
        );
        // Status should be Succeeded (replay completed successfully)
        assert_eq!(
            result.runtime_execution_result.status,
            RuntimeExecutionStatus::Succeeded
        );
        assert_eq!(result.runtime_execution_result.replay_attempted, true);
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
        assert!(
            result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded")
        );
        // Verify runtime execution result reflects the failure (degraded)
        assert_eq!(
            result.runtime_execution_result.status,
            RuntimeExecutionStatus::Degraded
        );
        assert_eq!(result.runtime_execution_result.signal_sent, false);
        // Replay was not attempted because signal failed first
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
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
            rationale: "Test runtime replay failure".to_string(),
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

        // Class B should still auto-proceed even if replay fails
        assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
        // Rationale should focus on apply decision (runtime detail lives in structured outcome)
        assert!(
            result.rationale.contains("Class B") || result.rationale.contains("auto-proceeded")
        );
        // Verify runtime execution result reflects partial success (signal sent but replay failed)
        assert_eq!(
            result.runtime_execution_result.status,
            RuntimeExecutionStatus::Degraded
        );
        assert_eq!(result.runtime_execution_result.signal_sent, true);
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        // Replay was attempted but failed
        assert_eq!(result.runtime_execution_result.replay_attempted, true);
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

        // Class B should still auto-proceed (apply pipeline proceeds, runtime skipped)
        assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
        // Runtime execution should be SkippedNotReady
        assert_eq!(
            result.runtime_execution_result.status,
            RuntimeExecutionStatus::SkippedNotReady
        );
        assert_eq!(result.runtime_execution_result.signal_sent, false);
        assert_eq!(result.runtime_execution_result.replay_completed, false);
        assert_eq!(result.runtime_execution_result.replay_attempted, false);
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
        assert_eq!(summary.graph_updates_count, 0);
        assert!(!summary.notification_required);
        assert!(!summary.rationale.is_empty());
    }

    #[tokio::test]
    async fn test_audit_summary_class_d_blocked() {
        // Test audit_summary for Class D blocked path
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
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope.push("item2".to_string());

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

        let summary = result.audit_summary();

        assert_eq!(summary.outcome, ApplyOutcome::BlockedManualReview);
        assert_eq!(
            summary.runtime_status,
            RuntimeExecutionStatus::NotApplicable
        );
        assert!(summary.checkpoint_outcome.is_none());
        assert!(summary.checkpoint_id.is_none());
        assert_eq!(summary.graph_updates_count, 0);
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
            rationale: "Test: audit summary proceed success".to_string(),
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

        let summary = result.audit_summary();

        assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
        assert_eq!(summary.runtime_status, RuntimeExecutionStatus::Succeeded);
        assert!(summary.checkpoint_outcome.is_some());
        assert!(summary.checkpoint_id.is_some());
        assert_eq!(summary.graph_updates_count, 0);
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
        assert_eq!(summary.graph_updates_count, 0);
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
            rationale: "Test: audit summary degraded".to_string(),
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

        let summary = result.audit_summary();

        assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
        assert_eq!(summary.runtime_status, RuntimeExecutionStatus::Degraded);
        assert!(summary.checkpoint_outcome.is_some());
        assert!(summary.checkpoint_id.is_some());
        assert_eq!(summary.graph_updates_count, 0);
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
            rationale: "Test: audit summary skipped not ready".to_string(),
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

        let summary = result.audit_summary();

        assert_eq!(summary.outcome, ApplyOutcome::AutoProceeded);
        assert_eq!(
            summary.runtime_status,
            RuntimeExecutionStatus::SkippedNotReady
        );
        assert!(summary.checkpoint_outcome.is_some());
        assert!(summary.checkpoint_id.is_some());
        assert_eq!(summary.graph_updates_count, 0);
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
            rationale: "Test: audit summary with graph updates".to_string(),
            section_decisions: vec![],
            affected_items: AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::B,
                &AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: false,
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
        assert_eq!(summary.graph_updates_count, 1);
        assert!(!summary.notification_required);
    }
}
