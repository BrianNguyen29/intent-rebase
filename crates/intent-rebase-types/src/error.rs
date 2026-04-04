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
}
