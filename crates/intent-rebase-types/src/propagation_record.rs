//! Propagation record domain types for propagation-status Slice 1
//!
//! Provides point-in-time records of downstream system propagation status
//! for intent changes. This is bounded Slice 1 — webhook delivery and event
//! streaming acknowledgment are deferred to later slices.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Propagation status for a downstream system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropagationStatus {
    /// Change signaled but not yet acknowledged
    #[default]
    Pending,
    /// Downstream system confirmed receipt
    Acknowledged,
    /// Downstream system rejected or delivery failed
    Failed,
}

/// A propagation record — tracks the propagation status of an intent change
/// to a single downstream system.
///
/// Bounded Slice 1: This is the database record. Webhook delivery,
/// event streaming, and cross-workflow lineage remain deferred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationRecord {
    /// Unique identifier for this record
    pub id: Uuid,
    /// Tenant this record belongs to
    pub tenant_id: Uuid,
    /// Intent this record is for
    pub intent_id: Uuid,
    /// Downstream system identifier
    pub downstream_system_id: String,
    /// Current propagation status
    pub status: PropagationStatus,
    /// Last intent version the downstream system has processed
    pub last_seen_version: i32,
    /// When the change was signaled
    pub signaled_at: DateTime<Utc>,
    /// When the downstream system acknowledged (None if pending or failed)
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// When delivery failed (None if pending or acknowledged)
    pub failed_at: Option<DateTime<Utc>>,
    /// Reason for failure
    pub failure_reason: Option<String>,
    /// Number of delivery attempts
    pub delivery_attempt_count: i32,
    /// Timestamp of last delivery attempt
    pub last_delivery_attempt_at: Option<DateTime<Utc>>,
    /// Optimistic locking version
    pub lock_version: i32,
    /// When this record was created
    pub created_at: DateTime<Utc>,
    /// When this record was last updated
    pub updated_at: DateTime<Utc>,
}

impl PropagationRecord {
    /// Create a new PropagationRecord with default values.
    pub fn new(tenant_id: Uuid, intent_id: Uuid, downstream_system_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            downstream_system_id,
            status: PropagationStatus::Pending,
            last_seen_version: 0,
            signaled_at: now,
            acknowledged_at: None,
            failed_at: None,
            failure_reason: None,
            delivery_attempt_count: 0,
            last_delivery_attempt_at: None,
            lock_version: 1,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propagation_record_new() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let record = PropagationRecord::new(tenant_id, intent_id, "workflow-runner-a".to_string());

        assert_eq!(record.tenant_id, tenant_id);
        assert_eq!(record.intent_id, intent_id);
        assert_eq!(record.downstream_system_id, "workflow-runner-a");
        assert_eq!(record.status, PropagationStatus::Pending);
        assert_eq!(record.last_seen_version, 0);
        assert_eq!(record.delivery_attempt_count, 0);
        assert_eq!(record.lock_version, 1);
    }

    #[test]
    fn test_propagation_status_default() {
        assert_eq!(PropagationStatus::default(), PropagationStatus::Pending);
    }
}
