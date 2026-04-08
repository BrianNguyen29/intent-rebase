//! Checkpoint aligner — bridges planner checkpoint selection to real checkpoint records
//!
//! The rebase planner (`RebasePlan.deferred.checkpoint_selection`) generates
//! checkpoint candidates based on decision class and affected items heuristics.
//! This module aligns those candidates to actual checkpoint records in storage.
//!
//! ## Alignment Outcomes
//!
//! - `Aligned`: Exact checkpoint found and aligned
//! - `ClosestMatch`: No exact match, closest checkpoint selected
//! - `NoCheckpointRequired`: Class A, no checkpoint needed
//! - `NoCheckpointFound`: Checkpoint requested but none available
//! - `MultipleCandidates`: Multiple matching checkpoints, selected best

#[allow(unused_imports)]
use intent_rebase_types::{Checkpoint, CheckpointStatus, CheckpointType, IntentRebaseError};
use rebase_engine::{CheckpointCandidate, CheckpointSelection, DecisionClass, RebasePlan};
use std::sync::Arc;
use uuid::Uuid;

/// Outcome of checkpoint alignment
#[derive(Debug, Clone, PartialEq)]
pub enum CheckpointAlignmentOutcome {
    /// Checkpoint was successfully aligned
    Aligned,
    /// No exact match found, used closest available
    ClosestMatch,
    /// No checkpoint needed for this decision class
    NoCheckpointRequired,
    /// Checkpoint requested but none available in storage
    NoCheckpointFound,
    /// Multiple matching checkpoints, best selected
    MultipleCandidates,
}

/// Result of checkpoint alignment
#[derive(Debug, Clone)]
pub struct CheckpointAlignmentResult {
    /// The aligned checkpoint
    pub checkpoint: Option<Checkpoint>,
    /// Checkpoint ID if found
    pub checkpoint_id: Option<Uuid>,
    /// Alignment outcome
    pub outcome: CheckpointAlignmentOutcome,
    /// Rationale for the alignment decision
    pub rationale: String,
}

/// Aligned checkpoint with full context
#[derive(Debug, Clone)]
pub struct AlignedCheckpoint {
    /// The checkpoint ID to use for replay/resume
    pub checkpoint_id: Option<Uuid>,
    /// The checkpoint record (if found)
    pub checkpoint: Option<Checkpoint>,
    /// Alignment outcome
    pub outcome: CheckpointAlignmentOutcome,
    /// Human-readable explanation
    pub rationale: String,
}

/// Report for debugging/auditing checkpoint alignment
#[derive(Debug, Clone)]
pub struct AlignmentReport {
    pub decision_class: DecisionClass,
    pub selection_ready: bool,
    pub candidates: Vec<CheckpointCandidate>,
    pub selected: Option<CheckpointCandidate>,
    pub available_workflow_checkpoints: usize,
    pub available_intent_checkpoints: usize,
    pub latest_checkpoint: Option<Uuid>,
}

/// Checkpoint aligner service
///
/// Aligns planner checkpoint candidates to real checkpoint records.
/// This is an internal-only service with no external dependencies.
pub struct CheckpointAligner {
    checkpoint_service: Arc<dyn intent_service::CheckpointRepository>,
}

impl CheckpointAligner {
    /// Create a new CheckpointAligner
    pub fn new(checkpoint_service: Arc<dyn intent_service::CheckpointRepository>) -> Self {
        Self { checkpoint_service }
    }

    /// Align a rebase plan's checkpoint selection to real checkpoint records.
    ///
    /// Takes the planner's `CheckpointSelection` and resolves it to actual
    /// checkpoint records based on:
    /// - Decision class (A needs no checkpoint)
    /// - Available checkpoints in storage
    /// - Checkpoint selection rationale
    pub async fn align(
        &self,
        plan: &RebasePlan,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        let selection = &plan.deferred.checkpoint_selection;

        // Class A: No checkpoint needed
        if plan.decision_class == DecisionClass::A {
            return Ok(AlignedCheckpoint {
                checkpoint_id: None,
                checkpoint: None,
                outcome: CheckpointAlignmentOutcome::NoCheckpointRequired,
                rationale: "Class A: No semantic changes, no checkpoint needed".to_string(),
            });
        }

        // If selection is not ready, fall back to best-effort alignment
        if !selection.ready {
            tracing::debug!(
                "Checkpoint selection not ready, performing best-effort alignment for intent {}",
                intent_id
            );
            return self
                .align_best_effort(selection, intent_id, tenant_id, workflow_id)
                .await;
        }

        // If no candidates, try best-effort alignment
        if selection.candidates.is_empty() {
            return self
                .align_best_effort(selection, intent_id, tenant_id, workflow_id)
                .await;
        }

        // Use the selected candidate if available
        if let Some(selected) = &selection.selected {
            return self
                .align_candidate(selected, intent_id, tenant_id, workflow_id)
                .await;
        }

        // Fall back to best-effort
        self.align_best_effort(selection, intent_id, tenant_id, workflow_id)
            .await
    }

    /// Align a specific checkpoint candidate to real records
    async fn align_candidate(
        &self,
        candidate: &CheckpointCandidate,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        // Try to find a checkpoint matching the candidate ID
        let checkpoints = self
            .checkpoint_service
            .list_by_workflow(workflow_id, tenant_id)
            .await?;

        // Find checkpoint by candidate ID pattern
        let matching: Vec<&Checkpoint> = checkpoints
            .iter()
            .filter(|c| {
                c.checkpoint_id.to_string().contains(&candidate.id)
                    || candidate.id.contains("most-recent")
            })
            .filter(|c| {
                c.status == CheckpointStatus::Active || c.status == CheckpointStatus::Created
            })
            .collect();

        let matching_count = matching.len();

        if matching.is_empty() {
            // No exact match, try to get most recent
            return self
                .align_most_recent(intent_id, tenant_id, workflow_id)
                .await;
        }

        if matching_count == 1 {
            let checkpoint = matching[0].clone();
            let checkpoint_id = checkpoint.checkpoint_id;
            return Ok(AlignedCheckpoint {
                checkpoint_id: Some(checkpoint_id),
                checkpoint: Some(checkpoint),
                outcome: CheckpointAlignmentOutcome::Aligned,
                rationale: format!(
                    "Aligned to checkpoint {} via candidate {}",
                    checkpoint_id, candidate.id
                ),
            });
        }

        // Multiple matches, select best (most recent active)
        let best = matching.into_iter().max_by_key(|c| c.created_at);

        if let Some(checkpoint) = best {
            return Ok(AlignedCheckpoint {
                checkpoint_id: Some(checkpoint.checkpoint_id),
                checkpoint: Some(checkpoint.clone()),
                outcome: CheckpointAlignmentOutcome::MultipleCandidates,
                rationale: format!(
                    "Selected most recent from {} candidates for {}",
                    matching_count, candidate.id
                ),
            });
        }

        self.align_most_recent(intent_id, tenant_id, workflow_id)
            .await
    }

    /// Best-effort alignment when planner selection is not ready
    async fn align_best_effort(
        &self,
        _selection: &CheckpointSelection,
        _intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        // For best-effort, try to find a suitable checkpoint
        let checkpoints = self
            .checkpoint_service
            .list_by_workflow(workflow_id, tenant_id)
            .await?;

        if checkpoints.is_empty() {
            return Ok(AlignedCheckpoint {
                checkpoint_id: None,
                checkpoint: None,
                outcome: CheckpointAlignmentOutcome::NoCheckpointFound,
                rationale: "No checkpoints available in storage for this workflow".to_string(),
            });
        }

        // Find most recent active checkpoint
        let most_recent = checkpoints
            .iter()
            .filter(|c| c.status == CheckpointStatus::Active)
            .max_by_key(|c| c.created_at);

        match most_recent {
            Some(checkpoint) => Ok(AlignedCheckpoint {
                checkpoint_id: Some(checkpoint.checkpoint_id),
                checkpoint: Some(checkpoint.clone()),
                outcome: CheckpointAlignmentOutcome::ClosestMatch,
                rationale: "Best-effort alignment: selected most recent active checkpoint"
                    .to_string(),
            }),
            None => {
                // Try any checkpoint regardless of status
                let any_checkpoint = checkpoints.first();
                match any_checkpoint {
                    Some(checkpoint) => Ok(AlignedCheckpoint {
                        checkpoint_id: Some(checkpoint.checkpoint_id),
                        checkpoint: Some(checkpoint.clone()),
                        outcome: CheckpointAlignmentOutcome::ClosestMatch,
                        rationale: "Best-effort alignment: selected most recent checkpoint regardless of status"
                            .to_string(),
                    }),
                    None => Ok(AlignedCheckpoint {
                        checkpoint_id: None,
                        checkpoint: None,
                        outcome: CheckpointAlignmentOutcome::NoCheckpointFound,
                        rationale: "No checkpoints available in storage".to_string(),
                    }),
                }
            }
        }
    }

    /// Align to the most recent checkpoint for an intent
    async fn align_most_recent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<AlignedCheckpoint, IntentRebaseError> {
        let checkpoints = self
            .checkpoint_service
            .list_by_intent(intent_id, tenant_id)
            .await?;

        if checkpoints.is_empty() {
            // Try workflow-level checkpoints
            return self
                .align_best_effort(
                    &CheckpointSelection::deferred(),
                    intent_id,
                    tenant_id,
                    workflow_id,
                )
                .await;
        }

        let most_recent = checkpoints.iter().max_by_key(|c| c.created_at);

        match most_recent {
            Some(checkpoint) => Ok(AlignedCheckpoint {
                checkpoint_id: Some(checkpoint.checkpoint_id),
                checkpoint: Some(checkpoint.clone()),
                outcome: CheckpointAlignmentOutcome::ClosestMatch,
                rationale: format!(
                    "Selected most recent checkpoint for intent {} v{}",
                    intent_id, checkpoint.intent_version
                ),
            }),
            None => Ok(AlignedCheckpoint {
                checkpoint_id: None,
                checkpoint: None,
                outcome: CheckpointAlignmentOutcome::NoCheckpointFound,
                rationale: "No checkpoints found for this intent".to_string(),
            }),
        }
    }

    /// Get alignment report for debugging/auditing
    #[allow(clippy::too_many_arguments)]
    pub async fn get_alignment_report(
        &self,
        plan: &RebasePlan,
        intent_id: Uuid,
        tenant_id: Uuid,
        workflow_id: Uuid,
    ) -> Result<AlignmentReport, IntentRebaseError> {
        let checkpoints = self
            .checkpoint_service
            .list_by_workflow(workflow_id, tenant_id)
            .await?;

        let intent_checkpoints = self
            .checkpoint_service
            .list_by_intent(intent_id, tenant_id)
            .await?;

        Ok(AlignmentReport {
            decision_class: plan.decision_class,
            selection_ready: plan.deferred.checkpoint_selection.ready,
            candidates: plan.deferred.checkpoint_selection.candidates.clone(),
            selected: plan.deferred.checkpoint_selection.selected.clone(),
            available_workflow_checkpoints: checkpoints.len(),
            available_intent_checkpoints: intent_checkpoints.len(),
            latest_checkpoint: intent_checkpoints.first().map(|c| c.checkpoint_id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rebase_engine::RiskTier;

    // Mock implementation for testing
    struct MockCheckpointRepo {
        checkpoints: std::collections::HashMap<Uuid, Checkpoint>,
        by_workflow: std::collections::HashMap<Uuid, Vec<Checkpoint>>,
        by_intent: std::collections::HashMap<Uuid, Vec<Checkpoint>>,
    }

    impl MockCheckpointRepo {
        fn new() -> Self {
            Self {
                checkpoints: std::collections::HashMap::new(),
                by_workflow: std::collections::HashMap::new(),
                by_intent: std::collections::HashMap::new(),
            }
        }

        fn add_checkpoint(&mut self, checkpoint: Checkpoint) {
            self.checkpoints
                .insert(checkpoint.checkpoint_id, checkpoint.clone());
            self.by_workflow
                .entry(checkpoint.workflow_id)
                .or_default()
                .push(checkpoint.clone());
            self.by_intent
                .entry(checkpoint.intent_id)
                .or_default()
                .push(checkpoint);
        }
    }

    #[async_trait::async_trait]
    impl intent_service::CheckpointRepository for MockCheckpointRepo {
        async fn create_checkpoint(
            &self,
            checkpoint: Checkpoint,
        ) -> Result<Checkpoint, IntentRebaseError> {
            Ok(checkpoint)
        }

        async fn get_checkpoint(
            &self,
            checkpoint_id: Uuid,
        ) -> Result<Checkpoint, IntentRebaseError> {
            self.checkpoints
                .get(&checkpoint_id)
                .cloned()
                .ok_or_else(|| IntentRebaseError::StorageError("not found".to_string()))
        }

        async fn list_by_workflow(
            &self,
            workflow_id: Uuid,
            tenant_id: Uuid,
        ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
            Ok(self
                .by_workflow
                .get(&workflow_id)
                .map(|cps| {
                    cps.iter()
                        .filter(|c| c.tenant_id == tenant_id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default())
        }

        async fn list_by_intent(
            &self,
            intent_id: Uuid,
            tenant_id: Uuid,
        ) -> Result<Vec<Checkpoint>, IntentRebaseError> {
            Ok(self
                .by_intent
                .get(&intent_id)
                .map(|cps| {
                    cps.iter()
                        .filter(|c| c.tenant_id == tenant_id)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default())
        }

        async fn update_status(
            &self,
            checkpoint_id: Uuid,
            _status: CheckpointStatus,
        ) -> Result<Checkpoint, IntentRebaseError> {
            let cp = self
                .checkpoints
                .get(&checkpoint_id)
                .cloned()
                .ok_or_else(|| IntentRebaseError::StorageError("not found".to_string()))?;
            Ok(cp)
        }

        async fn expire_checkpoints(&self) -> Result<usize, IntentRebaseError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_align_class_a_no_checkpoint_needed() {
        let repo = MockCheckpointRepo::new();
        let checkpoint_service = Arc::new(repo);

        let aligner = CheckpointAligner::new(checkpoint_service);

        let plan = RebasePlan {
            decision_class: DecisionClass::A,
            rationale: "No changes".to_string(),
            section_decisions: vec![],
            affected_items: intent_rebase_types::AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::A,
                &intent_rebase_types::AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: false,
            risk_tier: RiskTier::Low,
            risk_level: 1,
        };

        let result = aligner
            .align(&plan, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .await
            .unwrap();

        assert_eq!(
            result.outcome,
            CheckpointAlignmentOutcome::NoCheckpointRequired
        );
        assert!(result.checkpoint_id.is_none());
    }

    #[tokio::test]
    async fn test_align_no_checkpoint_found() {
        let repo = MockCheckpointRepo::new();
        let checkpoint_service = Arc::new(repo);

        let aligner = CheckpointAligner::new(checkpoint_service);

        let plan = RebasePlan {
            decision_class: DecisionClass::B,
            rationale: "Low severity".to_string(),
            section_decisions: vec![],
            affected_items: intent_rebase_types::AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::B,
                &intent_rebase_types::AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: false,
            risk_tier: RiskTier::Low,
            risk_level: 2,
        };

        let result = aligner
            .align(&plan, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
            .await
            .unwrap();

        assert_eq!(
            result.outcome,
            CheckpointAlignmentOutcome::NoCheckpointFound
        );
    }

    #[tokio::test]
    async fn test_align_with_checkpoints() {
        let mut repo = MockCheckpointRepo::new();

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Add checkpoints
        for i in 1..=3 {
            let mut checkpoint = Checkpoint::with_required(
                intent_id,
                i,
                workflow_id,
                tenant_id,
                CheckpointType::PreFlight,
            );
            checkpoint.status = CheckpointStatus::Active;
            repo.add_checkpoint(checkpoint);
        }

        let checkpoint_service = Arc::new(repo);
        let aligner = CheckpointAligner::new(checkpoint_service);

        let plan = RebasePlan {
            decision_class: DecisionClass::B,
            rationale: "Low severity".to_string(),
            section_decisions: vec![],
            affected_items: intent_rebase_types::AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::B,
                &intent_rebase_types::AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: false,
            risk_tier: RiskTier::Low,
            risk_level: 2,
        };

        let result = aligner
            .align(&plan, intent_id, tenant_id, workflow_id)
            .await
            .unwrap();

        // Should find closest match (most recent active)
        assert!(result.checkpoint_id.is_some());
        assert!(matches!(
            result.outcome,
            CheckpointAlignmentOutcome::ClosestMatch | CheckpointAlignmentOutcome::Aligned
        ));
    }

    #[tokio::test]
    async fn test_alignment_report() {
        let mut repo = MockCheckpointRepo::new();

        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let checkpoint = Checkpoint::with_required(
            intent_id,
            1,
            workflow_id,
            tenant_id,
            CheckpointType::PreFlight,
        );
        repo.add_checkpoint(checkpoint);

        let checkpoint_service = Arc::new(repo);
        let aligner = CheckpointAligner::new(checkpoint_service);

        let plan = RebasePlan {
            decision_class: DecisionClass::B,
            rationale: "Test".to_string(),
            section_decisions: vec![],
            affected_items: intent_rebase_types::AffectedItemsPreview::unavailable(),
            deferred: rebase_engine::DeferredFields::phase1_baseline(
                DecisionClass::B,
                &intent_rebase_types::AffectedItemsPreview::unavailable(),
            ),
            manual_review_recommended: false,
            risk_tier: RiskTier::Low,
            risk_level: 2,
        };

        let report = aligner
            .get_alignment_report(&plan, intent_id, tenant_id, workflow_id)
            .await
            .unwrap();

        assert_eq!(report.decision_class, DecisionClass::B);
        assert!(report.available_workflow_checkpoints >= 1);
        assert!(report.available_intent_checkpoints >= 1);
    }
}
