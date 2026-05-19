//! Rebase Orchestrator — internal orchestration layer for Phase 2
//!
//! This crate provides internal-only orchestration for:
//! - Checkpoint-to-intent-version alignment logic
//! - Graph update orchestration for state-only mutations
//! - Internal low/medium apply pipeline (Low/Medium auto-apply, High/Critical blocked)
//!
//! ## Design Principles
//!
//! - **No public HTTP endpoints** — this is pure internal compute
//! - **No Temporal/S3/frontend/auth integration** — deferred to Phase 3
//! - **MockAdapter/trait seams only** — no real runtime integration
//! - **High/Critical blocked** — manual review required, no auto-apply
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
//!   │     ├── Low/Medium risk_tier: auto-proceed with notification
//!   │     └── High/Critical risk_tier: blocked, requires manual review
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

    /// Create a replay-only success result: replay completed but no signal was sent.
    ///
    /// This is used by the standalone replay() path which calls replay_from_checkpoint()
    /// directly without sending a signal first. The replay operation succeeded,
    /// but no rebase signal was transmitted.
    pub fn replay_succeeded() -> Self {
        Self {
            status: RuntimeExecutionStatus::Succeeded,
            signal_sent: false,
            replay_completed: true,
            replay_attempted: true,
            status_message: "Replay completed".to_string(),
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
    HighCriticalGuard, RiskTierGuard,
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
use rebase_engine::{AffectedItemsPreview, DecisionClass, RebasePlan, RiskTier};
use std::sync::Arc;
use uuid::Uuid;

/// Parameters for a bounded replay operation.
///
/// Groups the common replay inputs so that `replay` and `replay_with_tx`
/// stay below the clippy `too_many_arguments` threshold.
#[derive(Debug, Clone)]
pub struct ReplayParams {
    /// Intent ID being replayed
    pub intent_id: Uuid,
    /// Tenant ID for tenant isolation
    pub tenant_id: Uuid,
    /// Workflow ID the intent belongs to
    pub workflow_id: Uuid,
    /// Source version for replay
    pub from_version: i32,
    /// Target version for replay
    pub to_version: i32,
    /// Specific checkpoint ID to use (optional)
    pub checkpoint_id: Option<Uuid>,
}

/// Result of a bounded replay operation.
///
/// Phase 2b bounded replay slice: Returns cooperative signal-based replay outcome
/// using existing runtime/checkpoint seams. This is NOT native Temporal reset.
#[derive(Debug, Clone)]
pub struct ReplayResult {
    /// Intent ID being replayed
    pub intent_id: Uuid,
    /// Source version for replay
    pub from_version: i32,
    /// Target version for replay
    pub to_version: i32,
    /// Checkpoint ID used for replay (if any)
    pub aligned_checkpoint_id: Option<Uuid>,
    /// Checkpoint selection outcome label
    pub checkpoint_selection_outcome: String,
    /// Runtime execution result for the replay attempt
    pub runtime_execution_result: RuntimeExecutionResult,
}

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

    /// Align a rebase plan's checkpoint selection within an existing transaction.
    ///
    /// Phase 4 D4: Transaction-aware checkpoint alignment for RLS-wrapped reads.
    pub async fn align_checkpoint_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
        plan: &RebasePlan,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        self.checkpoint_aligner
            .align_with_tx(tx, plan, intent_id, tenant_id, workflow_id)
            .await
    }

    /// Send a rebase signal to the runtime adapter for internal execution.
    ///
    /// Gates execution on adapter readiness. If the adapter is not ready,
    /// returns `RuntimeExecutionResult::skipped_not_ready()` without attempting
    /// signal or replay.
    ///
    /// Returns `RuntimeExecutionResult` indicating signal and replay status.
    pub async fn send_runtime_rebase_signal(
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

    /// Evaluate the apply decision for a rebase plan without executing side effects.
    ///
    /// Phase 4 D3: Exposes the apply decision so the caller can sequence
    /// transaction boundaries around the proceed / blocked / no-op paths.
    pub fn evaluate_apply(
        &self,
        risk_tier: &rebase_engine::RiskTier,
        decision_class: rebase_engine::DecisionClass,
    ) -> ApplyDecision {
        self.apply_pipeline.evaluate(risk_tier, decision_class)
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

    /// Apply bounded graph state updates within an existing transaction.
    ///
    /// Phase 4 D3: Transaction-aware graph state update for RLS-wrapped mutations.
    /// Mirrors `update_graph_state` but uses `GraphUpdater::update_node_state_if_affected_with_tx`
    /// so all mutations happen inside the caller's transaction boundary.
    pub async fn update_graph_state_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        affected_items: &AffectedItemsPreview,
        intent_id: Uuid,
        _tenant_id: Uuid,
        intent_version: i32,
    ) -> Result<Vec<GraphUpdateResult>, IntentRebaseError> {
        if affected_items.status != AffectedItemsStatus::Available {
            tracing::debug!(
                "Skipping graph state update: affected items status is {:?}",
                affected_items.status
            );
            return Ok(vec![]);
        }

        let mut results = Vec::new();

        for artifact in &affected_items.affected_artifacts {
            let result = self
                .graph_updater
                .update_node_state_if_affected_with_tx(
                    tx,
                    artifact.node_id,
                    intent_rebase_types::NodeState::Stale,
                    format!("Affected by intent {} v{}", intent_id, intent_version),
                )
                .await?;
            results.push(result);
        }

        for approval in &affected_items.affected_approvals {
            let result = self
                .graph_updater
                .update_node_state_if_affected_with_tx(
                    tx,
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

        for side_effect in &affected_items.side_effects {
            if matches!(
                side_effect.impact,
                intent_rebase_types::ClassificationImpact::Direct
            ) {
                let result = self
                    .graph_updater
                    .update_node_state_if_affected_with_tx(
                        tx,
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
    /// - NoOp: No apply needed (no semantic changes or Class A)
    /// - Low/Medium risk_tier: Auto-proceed with notification, align checkpoint, update graph
    /// - High/Critical risk_tier: Blocked, return with manual review required
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
        // Phase 2b: risk_tier is the controlling policy contract
        let apply_decision = self
            .apply_pipeline
            .evaluate(&plan.risk_tier, plan.decision_class);

        match apply_decision {
            ApplyDecision::NoOp => {
                tracing::info!(
                    "Rebase for intent {} v{} -> v{} is a no-op (risk_tier {:?})",
                    intent_id,
                    from_version.version_number,
                    to_version.version_number,
                    plan.risk_tier
                );
                Ok(RebaseApplyResult {
                    outcome: ApplyOutcome::NoOp,
                    aligned_checkpoint: None,
                    graph_updates: vec![],
                    notification_required: false,
                    rationale: format!(
                        "{:?} risk_tier: no semantic changes detected (decision_class {:?})",
                        plan.risk_tier, plan.decision_class
                    )
                    .to_string(),
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
                    "Rebase for intent {} v{} -> v{} auto-proceeding (risk_tier {:?})",
                    intent_id,
                    from_version.version_number,
                    to_version.version_number,
                    plan.risk_tier
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
                        "{:?} risk_tier auto-proceeded. Checkpoint aligned: {:?}, {} graph updates applied",
                        plan.risk_tier,
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

    /// Execute a bounded replay operation for an intent.
    ///
    /// Phase 2b bounded replay slice: This uses the existing cooperative signal-based
    /// replay seam (`replay_from_checkpoint`) with bounded checkpoint selection strategy.
    /// This is NOT native Temporal reset — it is cooperative signal-based replay.
    ///
    /// Bounded checkpoint selection strategy:
    /// - If `checkpoint_id` is provided, use that specific checkpoint
    /// - Otherwise, use the most recent active checkpoint for the workflow
    ///
    /// Returns a result indicating replay outcome without implying full Phase 2 replay compatibility.
    pub async fn replay(&self, params: ReplayParams) -> Result<ReplayResult, IntentRebaseError> {
        // Phase 2b: Bounded replay uses existing checkpoint alignment seam
        let checkpoint_repo = self.checkpoint_aligner.checkpoint_service();
        let aligned = if let Some(cp_id) = params.checkpoint_id {
            // Use specific checkpoint if provided
            let checkpoint = checkpoint_repo.get_checkpoint(cp_id).await;

            match checkpoint {
                Ok(cp) => AlignedCheckpoint {
                    checkpoint_id: Some(cp.checkpoint_id),
                    checkpoint: Some(cp),
                    outcome: CheckpointAlignmentOutcome::Aligned,
                    rationale: format!("Replay using specified checkpoint {}", cp_id),
                },
                Err(_) => {
                    // Map not-found storage error to proper CheckpointNotFound error for 400 response
                    return Err(IntentRebaseError::CheckpointNotFound(cp_id));
                }
            }
        } else {
            // Use most recent active checkpoint (best-effort alignment)
            let checkpoints = checkpoint_repo
                .list_by_workflow(params.workflow_id, params.tenant_id)
                .await?;

            let most_recent = checkpoints
                .iter()
                .filter(|c| c.status == intent_rebase_types::CheckpointStatus::Active)
                .max_by_key(|c| c.created_at);

            match most_recent {
                Some(cp) => AlignedCheckpoint {
                    checkpoint_id: Some(cp.checkpoint_id),
                    checkpoint: Some(cp.clone()),
                    outcome: CheckpointAlignmentOutcome::ClosestMatch,
                    rationale: "Replay using most recent active checkpoint".to_string(),
                },
                None => {
                    // No active checkpoints, try any checkpoint
                    let any_cp = checkpoints.first();
                    match any_cp {
                        Some(cp) => AlignedCheckpoint {
                            checkpoint_id: Some(cp.checkpoint_id),
                            checkpoint: Some(cp.clone()),
                            outcome: CheckpointAlignmentOutcome::ClosestMatch,
                            rationale: "Replay using most recent checkpoint (no active)"
                                .to_string(),
                        },
                        None => AlignedCheckpoint {
                            checkpoint_id: None,
                            checkpoint: None,
                            outcome: CheckpointAlignmentOutcome::NoCheckpointFound,
                            rationale: "Replay skipped: no checkpoints available".to_string(),
                        },
                    }
                }
            }
        };

        // Build replay result
        let checkpoint_selection_outcome = format!("{:?}", aligned.outcome);
        let aligned_checkpoint_id = aligned.checkpoint_id;

        // Attempt replay if checkpoint available and adapter ready
        let runtime_result = if aligned.checkpoint_id.is_some() {
            match self.runtime_adapter.is_adapter_ready().await {
                Ok(runtime_adapter::AdapterStatus::Ready) => {
                    let cp = aligned.checkpoint.as_ref().unwrap();
                    let runtime_cp = runtime_adapter::Checkpoint {
                        id: cp.checkpoint_id.to_string(),
                        label: format!("Replay checkpoint for intent {}", params.intent_id),
                        description: format!(
                            "Replay from intent {} v{} to v{}",
                            params.intent_id, params.from_version, params.to_version
                        ),
                        timestamp: cp.created_at,
                        validated: true,
                    };

                    let intent_ref = runtime_adapter::IntentRef::new(
                        params.intent_id.to_string(),
                        params.tenant_id.to_string(),
                        params.workflow_id.to_string(),
                        "active".to_string(),
                    );

                    match self
                        .runtime_adapter
                        .replay_from_checkpoint(runtime_cp, intent_ref)
                        .await
                    {
                        Ok(()) => RuntimeExecutionResult::replay_succeeded(),
                        Err(e) => RuntimeExecutionResult::degraded(
                            false, // signal_sent: false - replay_from_checkpoint doesn't send signal
                            true,  // replay_attempted: true - we attempted replay
                            &format!("Replay failed: {}", e),
                        ),
                    }
                }
                Ok(_) => RuntimeExecutionResult::skipped_not_ready(),
                Err(_e) => RuntimeExecutionResult::skipped_not_ready(),
            }
        } else {
            RuntimeExecutionResult::skipped_not_ready()
        };

        Ok(ReplayResult {
            intent_id: params.intent_id,
            from_version: params.from_version,
            to_version: params.to_version,
            aligned_checkpoint_id,
            checkpoint_selection_outcome,
            runtime_execution_result: runtime_result,
        })
    }

    /// Execute a bounded replay operation for an intent within an existing RLS transaction.
    ///
    /// Phase 4 D1: Transaction-aware replay for RLS-wrapped checkpoint reads.
    /// Mirrors the non-transactional `replay` behavior but reads through the
    /// provided transaction for defense-in-depth tenant isolation.
    pub async fn replay_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        params: ReplayParams,
    ) -> Result<ReplayResult, IntentRebaseError> {
        let checkpoint_service = self.checkpoint_aligner.checkpoint_service();
        let sql_repo = checkpoint_service.as_sqlx_repo().ok_or_else(|| {
            IntentRebaseError::Internal(
                "replay_with_tx requires SQL checkpoint repository".to_string(),
            )
        })?;

        let aligned = if let Some(cp_id) = params.checkpoint_id {
            let checkpoint = sql_repo.get_checkpoint_with_tx(tx, cp_id).await;

            match checkpoint {
                Ok(cp) => AlignedCheckpoint {
                    checkpoint_id: Some(cp.checkpoint_id),
                    checkpoint: Some(cp),
                    outcome: CheckpointAlignmentOutcome::Aligned,
                    rationale: format!("Replay using specified checkpoint {}", cp_id),
                },
                Err(_) => {
                    return Err(IntentRebaseError::CheckpointNotFound(cp_id));
                }
            }
        } else {
            let checkpoints = sql_repo
                .list_by_workflow_with_tx(tx, params.workflow_id, params.tenant_id)
                .await?;

            let most_recent = checkpoints
                .iter()
                .filter(|c| c.status == intent_rebase_types::CheckpointStatus::Active)
                .max_by_key(|c| c.created_at);

            match most_recent {
                Some(cp) => AlignedCheckpoint {
                    checkpoint_id: Some(cp.checkpoint_id),
                    checkpoint: Some(cp.clone()),
                    outcome: CheckpointAlignmentOutcome::ClosestMatch,
                    rationale: "Replay using most recent active checkpoint".to_string(),
                },
                None => {
                    let any_cp = checkpoints.first();
                    match any_cp {
                        Some(cp) => AlignedCheckpoint {
                            checkpoint_id: Some(cp.checkpoint_id),
                            checkpoint: Some(cp.clone()),
                            outcome: CheckpointAlignmentOutcome::ClosestMatch,
                            rationale: "Replay using most recent checkpoint (no active)"
                                .to_string(),
                        },
                        None => AlignedCheckpoint {
                            checkpoint_id: None,
                            checkpoint: None,
                            outcome: CheckpointAlignmentOutcome::NoCheckpointFound,
                            rationale: "Replay skipped: no checkpoints available".to_string(),
                        },
                    }
                }
            }
        };

        let checkpoint_selection_outcome = format!("{:?}", aligned.outcome);
        let aligned_checkpoint_id = aligned.checkpoint_id;

        let runtime_result = if aligned.checkpoint_id.is_some() {
            match self.runtime_adapter.is_adapter_ready().await {
                Ok(runtime_adapter::AdapterStatus::Ready) => {
                    let cp = aligned.checkpoint.as_ref().unwrap();
                    let runtime_cp = runtime_adapter::Checkpoint {
                        id: cp.checkpoint_id.to_string(),
                        label: format!("Replay checkpoint for intent {}", params.intent_id),
                        description: format!(
                            "Replay from intent {} v{} to v{}",
                            params.intent_id, params.from_version, params.to_version
                        ),
                        timestamp: cp.created_at,
                        validated: true,
                    };

                    let intent_ref = runtime_adapter::IntentRef::new(
                        params.intent_id.to_string(),
                        params.tenant_id.to_string(),
                        params.workflow_id.to_string(),
                        "active".to_string(),
                    );

                    match self
                        .runtime_adapter
                        .replay_from_checkpoint(runtime_cp, intent_ref)
                        .await
                    {
                        Ok(()) => RuntimeExecutionResult::replay_succeeded(),
                        Err(e) => RuntimeExecutionResult::degraded(
                            false,
                            true,
                            &format!("Replay failed: {}", e),
                        ),
                    }
                }
                Ok(_) => RuntimeExecutionResult::skipped_not_ready(),
                Err(_e) => RuntimeExecutionResult::skipped_not_ready(),
            }
        } else {
            RuntimeExecutionResult::skipped_not_ready()
        };

        Ok(ReplayResult {
            intent_id: params.intent_id,
            from_version: params.from_version,
            to_version: params.to_version,
            aligned_checkpoint_id,
            checkpoint_selection_outcome,
            runtime_execution_result: runtime_result,
        })
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
    /// graph update applied/failed counts, and notification requirement for internal
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
            graph_updates_applied: self.graph_updates.iter().filter(|u| u.success).count(),
            graph_updates_failed: self.graph_updates.iter().filter(|u| !u.success).count(),
            notification_required: self.notification_required,
            rationale: self.rationale.clone(),
        }
    }
}

/// Internal audit summary for rebase apply operations.
///
/// Aggregates runtime outcome, checkpoint alignment, graph update applied/failed counts,
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
    /// Number of successful graph updates applied
    pub graph_updates_applied: usize,
    /// Number of failed graph updates
    pub graph_updates_failed: usize,
    /// Whether notification is required
    pub notification_required: bool,
    /// Detailed rationale for the decision
    pub rationale: String,
}

#[cfg(test)]
mod orchestrator_tests;
