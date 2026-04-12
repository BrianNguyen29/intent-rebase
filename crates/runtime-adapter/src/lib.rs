//! Runtime adapter crate for Intent Rebase Engine.
//!
//! This crate defines the `RuntimeAdapter` trait that serves as the bridge
//! between the rebase engine and various runtime implementations (Temporal, etc.).
//! It also provides a `MockAdapter` for testing purposes.
//!
//! ## Capability Contract
//!
//! The trait defines capabilities without specific Temporal/NATS/Kafka dependencies,
//! allowing the rebase engine to remain runtime-agnostic until Phase 2 integration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[cfg(feature = "temporal")]
pub mod temporal_adapter;

#[cfg(feature = "temporal")]
pub use temporal_adapter::{TemporalAdapter, TemporalAdapterConfig};

/// Errors that can occur during runtime adapter operations.
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum AdapterError {
    #[error("Adapter not ready: {0}")]
    NotReady(String),

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),

    #[error("Intent mapping failed: {0}")]
    IntentMappingFailed(String),

    #[error("Rebase signal failed: {0}")]
    RebaseSignalFailed(String),

    #[error("Replay failed: {0}")]
    ReplayFailed(String),

    #[error("Internal adapter error: {0}")]
    Internal(String),
}

/// Result type for adapter operations.
pub type AdapterResult<T> = Result<T, AdapterError>;

/// Readiness status for the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterStatus {
    /// Adapter is ready to accept requests.
    Ready,
    /// Adapter is initializing.
    Initializing,
    /// Adapter is not ready.
    NotReady,
}

/// A checkpoint candidate for rebase resume.
///
/// This represents a potential point in the execution history
/// where replay could resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointCandidate {
    /// Checkpoint identifier
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Description of what state this checkpoint captures
    pub description: String,
    /// Whether this checkpoint has been validated
    pub validated: bool,
}

/// A concrete checkpoint for replay.
///
/// This represents a specific point in execution history with
/// all the metadata needed to resume from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint identifier
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Description of what state this checkpoint captures
    pub description: String,
    /// Timestamp when the checkpoint was created
    pub timestamp: DateTime<Utc>,
    /// Whether this checkpoint has been validated
    pub validated: bool,
}

/// Rebase signal to notify the runtime of rebase operations.
///
/// This is sent to the runtime to signal that a rebase should begin,
/// continue, or complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseSignal {
    /// Intent ID this signal is associated with
    pub intent_id: String,
    /// Type of signal (e.g., "start", "continue", "complete")
    pub signal_type: String,
    /// Additional metadata for the signal
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Intent reference for adapter operations.
///
/// A minimal intent reference used by the runtime adapter
/// to track which intent is being processed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRef {
    /// Intent identifier
    pub id: String,
    /// Tenant identifier
    pub tenant_id: String,
    /// Workflow identifier
    pub workflow_id: String,
    /// Intent status
    pub status: String,
}

impl IntentRef {
    /// Create a new IntentRef from components.
    pub fn new(id: String, tenant_id: String, workflow_id: String, status: String) -> Self {
        Self {
            id,
            tenant_id,
            workflow_id,
            status,
        }
    }
}

/// Runtime adapter trait that defines the capability contract for interacting
/// with various runtime implementations (Temporal, mock, etc.).
///
/// This trait is designed to be implemented by different backends without
/// coupling the rebase engine to specific technologies.
#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    /// Get available checkpoints for replay.
    ///
    /// Returns a list of checkpoint candidates that can be used for replay.
    async fn get_checkpoints(&self) -> AdapterResult<Vec<CheckpointCandidate>>;

    /// Send a rebase signal to the runtime.
    ///
    /// This notifies the runtime that a rebase operation should begin or continue.
    async fn send_rebase_signal(&self, signal: RebaseSignal) -> AdapterResult<()>;

    /// Map an intent to an appropriate checkpoint.
    ///
    /// Analyzes the intent and selects the best checkpoint for resuming execution.
    async fn map_intent_to_checkpoint(
        &self,
        intent: IntentRef,
    ) -> AdapterResult<CheckpointCandidate>;

    /// Replay execution from a specific checkpoint.
    ///
    /// Resumes execution starting from the given checkpoint with the provided intent.
    async fn replay_from_checkpoint(
        &self,
        checkpoint: Checkpoint,
        intent: IntentRef,
    ) -> AdapterResult<()>;

    /// Check if the adapter is ready to accept requests.
    async fn is_adapter_ready(&self) -> AdapterResult<AdapterStatus>;
}

/// Mock adapter implementation for testing.
///
/// Returns canned responses without requiring actual runtime infrastructure.
#[derive(Debug, Clone)]
pub struct MockAdapter {
    /// Whether the mock adapter reports itself as ready.
    is_ready: bool,
    /// Canned checkpoint candidates to return.
    checkpoints: Vec<CheckpointCandidate>,
    /// Whether send_rebase_signal should succeed.
    signal_success: bool,
    /// Whether map_intent_to_checkpoint should succeed.
    mapping_success: bool,
    /// Whether replay_from_checkpoint should succeed.
    replay_success: bool,
}

impl MockAdapter {
    /// Create a new MockAdapter with default canned responses.
    pub fn new() -> Self {
        Self {
            is_ready: true,
            checkpoints: vec![
                CheckpointCandidate {
                    id: "checkpoint-001".to_string(),
                    label: "Initial State".to_string(),
                    description: "Initial checkpoint after system startup".to_string(),
                    validated: true,
                },
                CheckpointCandidate {
                    id: "checkpoint-002".to_string(),
                    label: "Pre-Flight Check".to_string(),
                    description: "State after pre-flight validation".to_string(),
                    validated: true,
                },
                CheckpointCandidate {
                    id: "checkpoint-003".to_string(),
                    label: "Intent Received".to_string(),
                    description: "State after intent was received and parsed".to_string(),
                    validated: false,
                },
            ],
            signal_success: true,
            mapping_success: true,
            replay_success: true,
        }
    }

    /// Create a MockAdapter that is configured to be ready.
    pub fn ready() -> Self {
        Self::new()
    }

    /// Create a MockAdapter that is configured to NOT be ready.
    pub fn not_ready() -> Self {
        Self {
            is_ready: false,
            checkpoints: vec![],
            signal_success: false,
            mapping_success: false,
            replay_success: false,
        }
    }

    /// Create a MockAdapter with custom checkpoint data.
    pub fn with_checkpoints(mut self, checkpoints: Vec<CheckpointCandidate>) -> Self {
        self.checkpoints = checkpoints;
        self
    }

    /// Configure whether send_rebase_signal succeeds.
    pub fn with_signal_success(mut self, success: bool) -> Self {
        self.signal_success = success;
        self
    }

    /// Configure whether map_intent_to_checkpoint succeeds.
    pub fn with_mapping_success(mut self, success: bool) -> Self {
        self.mapping_success = success;
        self
    }

    /// Configure whether replay_from_checkpoint succeeds.
    pub fn with_replay_success(mut self, success: bool) -> Self {
        self.replay_success = success;
        self
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeAdapter for MockAdapter {
    async fn get_checkpoints(&self) -> AdapterResult<Vec<CheckpointCandidate>> {
        Ok(self.checkpoints.clone())
    }

    async fn send_rebase_signal(&self, _signal: RebaseSignal) -> AdapterResult<()> {
        if self.signal_success {
            Ok(())
        } else {
            Err(AdapterError::RebaseSignalFailed(
                "Mock adapter configured to fail".to_string(),
            ))
        }
    }

    async fn map_intent_to_checkpoint(
        &self,
        intent: IntentRef,
    ) -> AdapterResult<CheckpointCandidate> {
        if self.mapping_success {
            if let Some(checkpoint) = self.checkpoints.first() {
                Ok(checkpoint.clone())
            } else {
                Ok(CheckpointCandidate {
                    id: format!("checkpoint-{}", Uuid::new_v4()),
                    label: "Default Checkpoint".to_string(),
                    description: format!("Checkpoint for intent: {}", intent.id),
                    validated: false,
                })
            }
        } else {
            Err(AdapterError::IntentMappingFailed(
                "Mock adapter configured to fail".to_string(),
            ))
        }
    }

    async fn replay_from_checkpoint(
        &self,
        checkpoint: Checkpoint,
        _intent: IntentRef,
    ) -> AdapterResult<()> {
        if self.replay_success {
            tracing::debug!("Mock replay from checkpoint: {}", checkpoint.id);
            Ok(())
        } else {
            Err(AdapterError::ReplayFailed(
                "Mock adapter configured to fail".to_string(),
            ))
        }
    }

    async fn is_adapter_ready(&self) -> AdapterResult<AdapterStatus> {
        Ok(if self.is_ready {
            AdapterStatus::Ready
        } else {
            AdapterStatus::NotReady
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_intent() -> IntentRef {
        IntentRef::new(
            "test-intent-123".to_string(),
            "tenant-1".to_string(),
            "workflow-1".to_string(),
            "active".to_string(),
        )
    }

    fn create_test_checkpoint() -> Checkpoint {
        Checkpoint {
            id: "checkpoint-001".to_string(),
            label: "Test Checkpoint".to_string(),
            description: "A checkpoint for testing".to_string(),
            timestamp: chrono::Utc::now(),
            validated: true,
        }
    }

    #[tokio::test]
    async fn test_mock_adapter_ready() {
        let adapter = MockAdapter::ready();
        let status = adapter.is_adapter_ready().await.unwrap();
        assert_eq!(status, AdapterStatus::Ready);
    }

    #[tokio::test]
    async fn test_mock_adapter_not_ready() {
        let adapter = MockAdapter::not_ready();
        let status = adapter.is_adapter_ready().await.unwrap();
        assert_eq!(status, AdapterStatus::NotReady);
    }

    #[tokio::test]
    async fn test_mock_adapter_get_checkpoints() {
        let adapter = MockAdapter::ready();
        let checkpoints = adapter.get_checkpoints().await.unwrap();
        assert!(!checkpoints.is_empty());
        assert_eq!(checkpoints.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_adapter_send_rebase_signal_success() {
        let adapter = MockAdapter::ready();
        let signal = RebaseSignal {
            intent_id: "test-intent-123".to_string(),
            signal_type: "start".to_string(),
            metadata: serde_json::json!({}),
        };
        let result = adapter.send_rebase_signal(signal).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_adapter_send_rebase_signal_failure() {
        let adapter = MockAdapter::ready().with_signal_success(false);
        let signal = RebaseSignal {
            intent_id: "test-intent-123".to_string(),
            signal_type: "start".to_string(),
            metadata: serde_json::json!({}),
        };
        let result = adapter.send_rebase_signal(signal).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_adapter_map_intent_to_checkpoint() {
        let adapter = MockAdapter::ready();
        let intent = create_test_intent();
        let checkpoint = adapter.map_intent_to_checkpoint(intent).await.unwrap();
        assert_eq!(checkpoint.id, "checkpoint-001");
    }

    #[tokio::test]
    async fn test_mock_adapter_map_intent_to_checkpoint_failure() {
        let adapter = MockAdapter::ready().with_mapping_success(false);
        let intent = create_test_intent();
        let result = adapter.map_intent_to_checkpoint(intent).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_adapter_replay_from_checkpoint() {
        let adapter = MockAdapter::ready();
        let checkpoint = create_test_checkpoint();
        let intent = create_test_intent();
        let result = adapter.replay_from_checkpoint(checkpoint, intent).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_adapter_replay_failure() {
        let adapter = MockAdapter::ready().with_replay_success(false);
        let checkpoint = create_test_checkpoint();
        let intent = create_test_intent();
        let result = adapter.replay_from_checkpoint(checkpoint, intent).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_adapter_with_custom_checkpoints() {
        let custom_checkpoints = vec![
            CheckpointCandidate {
                id: "custom-1".to_string(),
                label: "Custom 1".to_string(),
                description: "First custom checkpoint".to_string(),
                validated: true,
            },
            CheckpointCandidate {
                id: "custom-2".to_string(),
                label: "Custom 2".to_string(),
                description: "Second custom checkpoint".to_string(),
                validated: false,
            },
        ];
        let adapter = MockAdapter::ready().with_checkpoints(custom_checkpoints);
        let checkpoints = adapter.get_checkpoints().await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert_eq!(checkpoints[0].id, "custom-1");
        assert_eq!(checkpoints[1].id, "custom-2");
    }

    #[tokio::test]
    async fn test_mock_adapter_clone_works() {
        let adapter = MockAdapter::ready();
        let cloned = adapter.clone();
        let status1 = adapter.is_adapter_ready().await.unwrap();
        let status2 = cloned.is_adapter_ready().await.unwrap();
        assert_eq!(status1, status2);
    }

    #[tokio::test]
    async fn test_mock_adapter_default() {
        let adapter = MockAdapter::default();
        let status = adapter.is_adapter_ready().await.unwrap();
        assert_eq!(status, AdapterStatus::Ready);
    }

    #[tokio::test]
    async fn test_adapter_error_display() {
        let err = AdapterError::NotReady("test".to_string());
        assert_eq!(format!("{}", err), "Adapter not ready: test");

        let err = AdapterError::CheckpointNotFound("cp-1".to_string());
        assert_eq!(format!("{}", err), "Checkpoint not found: cp-1");
    }
}
