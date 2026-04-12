//! Checkpoint domain type for Phase 2 Temporal checkpoint mapping workstream.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Checkpoint type enum representing different kinds of checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckpointType {
    /// Initial checkpoint at workflow start.
    #[default]
    Initial,
    /// Pre-flight checkpoint before intent processing.
    PreFlight,
    /// Intent received checkpoint after parsing.
    IntentReceived,
    /// Intent validated checkpoint after validation.
    IntentValidated,
    /// Rebase started checkpoint when rebase begins.
    RebaseStarted,
    /// Rebase completed checkpoint when rebase finishes.
    RebaseCompleted,
    /// Final checkpoint at workflow completion.
    Final,
    /// Custom checkpoint for domain-specific use.
    Custom,
}

/// Checkpoint status enum representing the lifecycle state of a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CheckpointStatus {
    /// Checkpoint is pending creation.
    #[default]
    Pending,
    /// Checkpoint is created but not yet validated.
    Created,
    /// Checkpoint has been validated and is active.
    Active,
    /// Checkpoint has been superseded by a newer checkpoint.
    Superseded,
    /// Checkpoint has expired and is no longer valid.
    Expired,
    /// Checkpoint has been invalidated due to an error.
    Invalidated,
}

/// Core checkpoint domain type for Temporal checkpoint mapping.
///
/// Represents a specific point in workflow execution history with
/// all metadata needed to resume execution from that point.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Checkpoint {
    /// Unique checkpoint identifier (UUID primary key).
    pub checkpoint_id: Uuid,
    /// Intent this checkpoint is associated with.
    pub intent_id: Uuid,
    /// Version of the intent at checkpoint time.
    pub intent_version: i32,
    /// Workflow this checkpoint belongs to.
    pub workflow_id: Uuid,
    /// Tenant this checkpoint belongs to.
    pub tenant_id: Uuid,
    /// Serialized workflow state at checkpoint time.
    pub workflow_state: serde_json::Value,
    /// Type of checkpoint indicating its position in the workflow.
    pub checkpoint_type: CheckpointType,
    /// Timestamp when checkpoint was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when checkpoint expires (nullable).
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Current status of the checkpoint.
    pub status: CheckpointStatus,
    /// Additional metadata as JSONB.
    pub metadata: serde_json::Value,
}

impl Checkpoint {
    /// Create a new Checkpoint with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a Checkpoint with required fields.
    pub fn with_required(
        intent_id: Uuid,
        intent_version: i32,
        workflow_id: Uuid,
        tenant_id: Uuid,
        checkpoint_type: CheckpointType,
    ) -> Self {
        Self {
            checkpoint_id: Uuid::new_v4(),
            intent_id,
            intent_version,
            workflow_id,
            tenant_id,
            workflow_state: serde_json::Value::Object(serde_json::Map::new()),
            checkpoint_type,
            created_at: chrono::Utc::now(),
            expires_at: None,
            status: CheckpointStatus::Pending,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Check if this checkpoint has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < chrono::Utc::now()
        } else {
            false
        }
    }

    /// Check if this checkpoint is in an active state.
    pub fn is_active(&self) -> bool {
        matches!(self.status, CheckpointStatus::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_new() {
        let checkpoint = Checkpoint::new();
        assert_eq!(checkpoint.checkpoint_id, Uuid::nil());
        assert_eq!(checkpoint.status, CheckpointStatus::Pending);
        assert_eq!(checkpoint.checkpoint_type, CheckpointType::Initial);
    }

    #[test]
    fn test_checkpoint_with_required() {
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

        assert_ne!(checkpoint.checkpoint_id, Uuid::nil());
        assert_eq!(checkpoint.intent_id, intent_id);
        assert_eq!(checkpoint.intent_version, 1);
        assert_eq!(checkpoint.workflow_id, workflow_id);
        assert_eq!(checkpoint.tenant_id, tenant_id);
        assert_eq!(checkpoint.checkpoint_type, CheckpointType::PreFlight);
        assert_eq!(checkpoint.status, CheckpointStatus::Pending);
    }

    #[test]
    fn test_checkpoint_is_expired_no_expiry() {
        let checkpoint = Checkpoint::new();
        assert!(!checkpoint.is_expired());
    }

    #[test]
    fn test_checkpoint_is_expired_with_future_expiry() {
        let mut checkpoint = Checkpoint::new();
        checkpoint.expires_at = Some(chrono::Utc::now() + chrono::Duration::hours(1));
        assert!(!checkpoint.is_expired());
    }

    #[test]
    fn test_checkpoint_is_expired_with_past_expiry() {
        let mut checkpoint = Checkpoint::new();
        checkpoint.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        assert!(checkpoint.is_expired());
    }

    #[test]
    fn test_checkpoint_is_active() {
        let mut checkpoint = Checkpoint::new();
        assert!(!checkpoint.is_active());

        checkpoint.status = CheckpointStatus::Active;
        assert!(checkpoint.is_active());

        checkpoint.status = CheckpointStatus::Superseded;
        assert!(!checkpoint.is_active());
    }

    #[test]
    fn test_checkpoint_serialization() {
        let checkpoint = Checkpoint::with_required(
            Uuid::new_v4(),
            1,
            Uuid::new_v4(),
            Uuid::new_v4(),
            CheckpointType::RebaseCompleted,
        );

        let json = serde_json::to_string(&checkpoint).unwrap();
        let deserialized: Checkpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.checkpoint_id, checkpoint.checkpoint_id);
        assert_eq!(deserialized.intent_id, checkpoint.intent_id);
        assert_eq!(deserialized.checkpoint_type, checkpoint.checkpoint_type);
    }
}
