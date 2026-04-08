//! Common error types for IRE

use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum IntentRebaseError {
    #[error("intent not found: {0}")]
    IntentNotFound(Uuid),

    #[error("intent version not found: {0}")]
    IntentVersionNotFound(Uuid),

    #[error("artifact not found: {0}")]
    ArtifactNotFound(Uuid),

    #[error("graph node not found: {0}")]
    GraphNodeNotFound(Uuid),

    #[error("graph edge not found: {0}")]
    GraphEdgeNotFound(Uuid),

    #[error("graph integrity error: {0}")]
    GraphIntegrityError(String),

    #[error("invalid intent version: {0}")]
    InvalidIntentVersion(String),

    #[error("rebase conflict detected: {0}")]
    RebaseConflict(String),

    #[error("storage error: {0}")]
    StorageError(String),

    #[error("broker error: {0}")]
    BrokerError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("invalid request header: {0}")]
    InvalidHeader(String),

    #[error("optimistic concurrency conflict: intent {0} has been modified")]
    ConcurrencyConflict(Uuid),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("invalid ingest request: {0}")]
    InvalidIngestRequest(String),

    #[error("artifact must have at least one IntentVersion dependency")]
    ArtifactTraceabilityEmpty,

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("invalid API key: {0}")]
    InvalidApiKey(String),

    #[error("approval request not found: {0}")]
    ApprovalRequestNotFound(Uuid),

    #[error("approval request {0} is not pending (current status: {1})")]
    ApprovalRequestNotPending(Uuid, String),

    /// Phase 2b: Checkpoint not found during replay
    #[error("checkpoint not found: {0}")]
    CheckpointNotFound(Uuid),

    /// Phase 2 governance: Policy snapshot not found
    #[error("policy snapshot not found: {0}")]
    PolicySnapshotNotFound(Uuid),
}
