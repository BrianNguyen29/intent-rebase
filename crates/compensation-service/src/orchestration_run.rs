//! Orchestration run model for single-shot execution
//!
//! Phase 3 Batch 1 (bounded single-shot orchestration slice):
//! A persisted run handle represents a single-shot orchestration execution
//! over an explicit list of compensation action IDs.
//!
//! **Bounded scope:**
//! - Single-shot: one run = one explicit action list, one auto-decide pass
//! - No queue polling, no distributed claiming/locking, no scheduler
//! - Runtime auto-decides approve | reapprove | execute | skip using existing planner/write paths

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Run status over its lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run has been created and persisted, awaiting execution
    Pending,
    /// Run is currently executing (actions being processed)
    Running,
    /// Run completed successfully (all actions processed)
    Completed,
    /// Run completed with partial failures
    CompletedWithErrors,
    /// Run failed completely (e.g., all actions failed or system error)
    Failed,
}

/// Phase 3 Batch 1: An orchestration run is a single-shot execution over
/// an explicit list of compensation action IDs.
///
/// The runtime auto-decides for each action whether to:
/// - `approve`: Pending action → Approved
/// - `reapprove`: Failed action with retry budget → Pending
/// - `execute`: Approved + Automatic action → Executed/Failed
/// - `skip`: Terminal state or policy-blocked action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRun {
    /// Unique identifier for this run
    pub id: Uuid,
    /// Tenant this run belongs to
    pub tenant_id: Uuid,
    /// Intent scope for this run (if applicable)
    pub intent_id: Option<Uuid>,
    /// List of compensation action IDs to process in this run
    pub action_ids: Vec<Uuid>,
    /// Current status of the run
    pub status: RunStatus,
    /// Who initiated this run
    pub initiated_by: Option<String>,
    /// When the run was created
    pub created_at: DateTime<Utc>,
    /// When the run started execution
    pub started_at: Option<DateTime<Utc>>,
    /// When the run completed (success or failure)
    pub completed_at: Option<DateTime<Utc>>,
    /// Number of actions processed successfully
    pub succeeded_count: usize,
    /// Number of actions that failed
    pub failed_count: usize,
    /// Number of actions that were skipped (no action possible or policy blocked)
    pub skipped_count: usize,
    /// Number of actions that were not found
    pub not_found_count: usize,
    /// Total number of actions in the run
    pub total_count: usize,
    /// Summary of each item's outcome
    pub item_results: Vec<RunItemResult>,
}

/// Per-item result within a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunItemResult {
    /// The compensation action ID
    pub action_id: Uuid,
    /// What the runtime decided to do
    pub action_taken: OrchestrationActionDecision,
    /// Whether the action succeeded
    pub success: bool,
    /// Human-readable reason or error message
    pub reason: String,
    /// The action's status after processing
    pub resulting_status: String,
}

/// What the runtime decided to do for an action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationActionDecision {
    /// Action was approved (Pending → Approved)
    Approve,
    /// Action was reapproved (Failed → Pending)
    Reapprove,
    /// Action was executed (Approved → Executed)
    Execute,
    /// Action was skipped (terminal state or policy blocked)
    Skip,
    /// Action was not found
    NotFound,
}

impl OrchestrationRun {
    /// Create a new orchestration run (pending execution).
    pub fn new(
        tenant_id: Uuid,
        action_ids: Vec<Uuid>,
        initiated_by: Option<String>,
        intent_id: Option<Uuid>,
    ) -> Self {
        let total_count = action_ids.len();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            action_ids,
            status: RunStatus::Pending,
            initiated_by,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            succeeded_count: 0,
            failed_count: 0,
            skipped_count: 0,
            not_found_count: 0,
            total_count,
            item_results: Vec::new(),
        }
    }

    /// Mark the run as started.
    pub fn mark_started(&mut self) {
        self.status = RunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark the run as completed and compute final status.
    pub fn mark_completed(&mut self) {
        self.completed_at = Some(Utc::now());
        if self.failed_count == 0 && self.not_found_count == 0 {
            self.status = RunStatus::Completed;
        } else if self.succeeded_count == 0 {
            self.status = RunStatus::Failed;
        } else {
            self.status = RunStatus::CompletedWithErrors;
        }
    }

    /// Record a successful action result.
    pub fn record_success(
        &mut self,
        action_id: Uuid,
        decision: OrchestrationActionDecision,
        reason: String,
        resulting_status: String,
    ) {
        self.succeeded_count += 1;
        self.item_results.push(RunItemResult {
            action_id,
            action_taken: decision,
            success: true,
            reason,
            resulting_status,
        });
    }

    /// Record a failed action result.
    pub fn record_failure(
        &mut self,
        action_id: Uuid,
        decision: OrchestrationActionDecision,
        reason: String,
        resulting_status: String,
    ) {
        self.failed_count += 1;
        self.item_results.push(RunItemResult {
            action_id,
            action_taken: decision,
            success: false,
            reason,
            resulting_status,
        });
    }

    /// Record a skipped action result.
    pub fn record_skipped(
        &mut self,
        action_id: Uuid,
        decision: OrchestrationActionDecision,
        reason: String,
    ) {
        self.skipped_count += 1;
        self.item_results.push(RunItemResult {
            action_id,
            action_taken: decision,
            success: true, // Skipped is not a failure
            reason,
            resulting_status: "skipped".to_string(),
        });
    }

    /// Record a not-found action result.
    pub fn record_not_found(&mut self, action_id: Uuid) {
        self.not_found_count += 1;
        self.item_results.push(RunItemResult {
            action_id,
            action_taken: OrchestrationActionDecision::NotFound,
            success: false,
            reason: "Action not found or access denied".to_string(),
            resulting_status: "not_found".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_run_new() {
        let tenant_id = Uuid::new_v4();
        let action_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let run = OrchestrationRun::new(
            tenant_id,
            action_ids.clone(),
            Some("test-user".to_string()),
            None,
        );

        assert_eq!(run.tenant_id, tenant_id);
        assert_eq!(run.action_ids, action_ids);
        assert_eq!(run.status, RunStatus::Pending);
        assert_eq!(run.total_count, 2);
        assert_eq!(run.succeeded_count, 0);
        assert_eq!(run.failed_count, 0);
        assert!(run.started_at.is_none());
        assert!(run.completed_at.is_none());
    }

    #[test]
    fn test_orchestration_run_mark_started() {
        let run = OrchestrationRun::new(Uuid::new_v4(), vec![], None, None);
        let mut run = run;
        run.mark_started();
        assert_eq!(run.status, RunStatus::Running);
        assert!(run.started_at.is_some());
    }

    #[test]
    fn test_orchestration_run_mark_completed() {
        let run = OrchestrationRun::new(Uuid::new_v4(), vec![], None, None);
        let mut run = run;
        run.mark_completed();
        assert!(run.completed_at.is_some());
    }

    #[test]
    fn test_orchestration_run_record_success() {
        let run = OrchestrationRun::new(Uuid::new_v4(), vec![Uuid::new_v4()], None, None);
        let mut run = run;
        run.record_success(
            run.action_ids[0],
            OrchestrationActionDecision::Approve,
            "Approved successfully".to_string(),
            "approved".to_string(),
        );
        assert_eq!(run.succeeded_count, 1);
        assert_eq!(run.item_results.len(), 1);
        assert!(run.item_results[0].success);
    }

    #[test]
    fn test_orchestration_run_completed_status_all_success() {
        let run = OrchestrationRun::new(Uuid::new_v4(), vec![Uuid::new_v4()], None, None);
        let mut run = run;
        run.record_success(
            Uuid::new_v4(),
            OrchestrationActionDecision::Execute,
            "Executed".to_string(),
            "executed".to_string(),
        );
        run.mark_completed();
        assert_eq!(run.status, RunStatus::Completed);
    }

    #[test]
    fn test_orchestration_run_completed_status_partial_failure() {
        let run = OrchestrationRun::new(Uuid::new_v4(), vec![Uuid::new_v4()], None, None);
        let mut run = run;
        run.record_success(
            Uuid::new_v4(),
            OrchestrationActionDecision::Execute,
            "Success".to_string(),
            "executed".to_string(),
        );
        run.record_failure(
            Uuid::new_v4(),
            OrchestrationActionDecision::Execute,
            "Failed".to_string(),
            "failed".to_string(),
        );
        run.mark_completed();
        assert_eq!(run.status, RunStatus::CompletedWithErrors);
    }
}
