//! Side effect rollback record model for compensation execution audit.
//!
//! See [../../../../docs/10-delivery/checklists/checklist-phase-3.md] item 7 in side effect ledger.
//!
//! **Phase 3 Batch 1 scope:** Bounded rollback-record persistence for execute and waive paths.
//! Records are created when compensation is executed (success/failure) or waived.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result of a compensation action execution or waiver.
///
/// Used to classify the outcome when recording a rollback record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackRecordResult {
    /// Compensation executed successfully
    Success,
    /// Compensation execution failed
    Failure,
    /// Compensation action was waived
    Waived,
}

impl RollbackRecordResult {
    /// Convert result to database string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            RollbackRecordResult::Success => "success",
            RollbackRecordResult::Failure => "failure",
            RollbackRecordResult::Waived => "waived",
        }
    }

    /// Parse result from database string representation.
    #[allow(clippy::should_implement_trait)]
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "success" => Some(RollbackRecordResult::Success),
            "failure" => Some(RollbackRecordResult::Failure),
            "waived" => Some(RollbackRecordResult::Waived),
            _ => None,
        }
    }
}

/// A rollback record created when compensation is executed or waived.
///
/// **Phase 3 Batch 1 scope:** Records are created on:
/// - `execute_action` success: result = Success
/// - `execute_action` failure: result = Failure
/// - `waive_action`: result = Waived
///
/// This provides an audit trail of compensation outcomes that can be queried
/// for replay, forensic analysis, or reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffectRollbackRecord {
    /// Unique identifier for this rollback record
    pub id: Uuid,
    /// Tenant this rollback record belongs to
    pub tenant_id: Uuid,
    /// Reference to the compensation action that generated this record
    pub compensation_action_id: Uuid,
    /// Reference to the side effect this record is for
    pub side_effect_id: Uuid,
    /// Reference to the intent this record is scoped to
    pub intent_id: Uuid,
    /// Result of the compensation execution or waiver
    pub result: RollbackRecordResult,
    /// Human-readable summary of what happened
    pub summary: String,
    /// Error code if compensation execution failed
    pub error_code: Option<String>,
    /// Detailed error message if compensation execution failed
    pub error_detail: Option<String>,
    /// Who executed or waived this compensation action
    pub recorded_by: Option<String>,
    /// Timestamp when this record was created
    pub recorded_at: DateTime<Utc>,
    /// Lock version for optimistic concurrency
    pub lock_version: i32,
}

impl SideEffectRollbackRecord {
    /// Create a new rollback record for a successful execution.
    pub fn success(
        tenant_id: Uuid,
        compensation_action_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        summary: &str,
        recorded_by: Option<&str>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            result: RollbackRecordResult::Success,
            summary: summary.to_string(),
            error_code: None,
            error_detail: None,
            recorded_by: recorded_by.map(String::from),
            recorded_at: Utc::now(),
            lock_version: 0,
        }
    }

    /// Create a new rollback record for a failed execution.
    pub fn failure(
        tenant_id: Uuid,
        compensation_action_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        summary: &str,
        error_code: &str,
        error_detail: Option<String>,
    ) -> Self {
        Self::failure_with_actor(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            summary,
            error_code,
            error_detail,
            None,
        )
    }

    /// Create a new rollback record for a failed execution with actor info.
    #[allow(clippy::too_many_arguments)]
    pub fn failure_with_actor(
        tenant_id: Uuid,
        compensation_action_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        summary: &str,
        error_code: &str,
        error_detail: Option<String>,
        recorded_by: Option<&str>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            result: RollbackRecordResult::Failure,
            summary: summary.to_string(),
            error_code: Some(error_code.to_string()),
            error_detail,
            recorded_by: recorded_by.map(String::from),
            recorded_at: Utc::now(),
            lock_version: 0,
        }
    }

    /// Create a new rollback record for a waived action.
    pub fn waived(
        tenant_id: Uuid,
        compensation_action_id: Uuid,
        side_effect_id: Uuid,
        intent_id: Uuid,
        summary: &str,
        recorded_by: Option<&str>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            result: RollbackRecordResult::Waived,
            summary: summary.to_string(),
            error_code: None,
            error_detail: None,
            recorded_by: recorded_by.map(String::from),
            recorded_at: Utc::now(),
            lock_version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rollback_record_result_as_str() {
        assert_eq!(RollbackRecordResult::Success.as_str(), "success");
        assert_eq!(RollbackRecordResult::Failure.as_str(), "failure");
        assert_eq!(RollbackRecordResult::Waived.as_str(), "waived");
    }

    #[test]
    fn test_rollback_record_result_from_db_str() {
        assert_eq!(
            RollbackRecordResult::from_db_str("success"),
            Some(RollbackRecordResult::Success)
        );
        assert_eq!(
            RollbackRecordResult::from_db_str("failure"),
            Some(RollbackRecordResult::Failure)
        );
        assert_eq!(
            RollbackRecordResult::from_db_str("waived"),
            Some(RollbackRecordResult::Waived)
        );
        assert_eq!(RollbackRecordResult::from_db_str("unknown"), None);
    }

    #[test]
    fn test_rollback_record_success() {
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = SideEffectRollbackRecord::success(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            "Rollback completed successfully",
            Some("test-executor"),
        );

        assert_eq!(record.tenant_id, tenant_id);
        assert_eq!(record.compensation_action_id, compensation_action_id);
        assert_eq!(record.side_effect_id, side_effect_id);
        assert_eq!(record.intent_id, intent_id);
        assert_eq!(record.result, RollbackRecordResult::Success);
        assert_eq!(record.summary, "Rollback completed successfully");
        assert!(record.error_code.is_none());
        assert!(record.error_detail.is_none());
        assert_eq!(record.recorded_by, Some("test-executor".to_string()));
        assert_eq!(record.lock_version, 0);
    }

    #[test]
    fn test_rollback_record_failure() {
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = SideEffectRollbackRecord::failure(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            "Rollback failed",
            "ROLLBACK_ERR_001",
            Some("Database connection timeout".to_string()),
        );

        assert_eq!(record.result, RollbackRecordResult::Failure);
        assert_eq!(record.summary, "Rollback failed");
        assert_eq!(record.error_code, Some("ROLLBACK_ERR_001".to_string()));
        assert_eq!(
            record.error_detail,
            Some("Database connection timeout".to_string())
        );
    }

    #[test]
    fn test_rollback_record_waived() {
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = SideEffectRollbackRecord::waived(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            "Compensation waived by operator",
            Some("test-waiver"),
        );

        assert_eq!(record.result, RollbackRecordResult::Waived);
        assert_eq!(record.summary, "Compensation waived by operator");
        assert!(record.error_code.is_none());
        assert!(record.error_detail.is_none());
        assert_eq!(record.recorded_by, Some("test-waiver".to_string()));
    }

    #[test]
    fn test_rollback_record_serialization_round_trip() {
        let tenant_id = Uuid::new_v4();
        let compensation_action_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let record = SideEffectRollbackRecord::failure(
            tenant_id,
            compensation_action_id,
            side_effect_id,
            intent_id,
            "Rollback failed",
            "ERR_001",
            None,
        );

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SideEffectRollbackRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, record.id);
        assert_eq!(deserialized.tenant_id, tenant_id);
        assert_eq!(deserialized.compensation_action_id, compensation_action_id);
        assert_eq!(deserialized.side_effect_id, side_effect_id);
        assert_eq!(deserialized.intent_id, intent_id);
        assert_eq!(deserialized.result, RollbackRecordResult::Failure);
        assert_eq!(deserialized.summary, "Rollback failed");
        assert_eq!(deserialized.error_code, Some("ERR_001".to_string()));
    }
}
