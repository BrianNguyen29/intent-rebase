//! Common error types for IRE

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IntentRebaseError {
    #[error("intent not found: {0}")]
    IntentNotFound(u64),

    #[error("artifact not found: {0}")]
    ArtifactNotFound(u64),

    #[error("graph node not found: {0}")]
    GraphNodeNotFound(u64),

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

    #[error("internal error: {0}")]
    Internal(String),
}
