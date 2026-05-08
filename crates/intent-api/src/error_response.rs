//! Error response types for the Intent API
//!
//! This module contains the `ApiErrorResponse` wrapper type that implements
//! IntoResponse to map `IntentRebaseError` to appropriate HTTP responses.

use axum::{http::StatusCode, response::IntoResponse, Json};

use crate::types::{ApiError, ErrorDetails};
use intent_rebase_types::IntentRebaseError;

/// Newtype wrapper for IntentRebaseError that implements IntoResponse
#[derive(Debug)]
pub struct ApiErrorResponse(pub IntentRebaseError);

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let err = &self.0;
        let (status, code, retryable) = match err {
            IntentRebaseError::IntentNotFound(_) => {
                (StatusCode::NOT_FOUND, "INTENT_NOT_FOUND", false)
            }
            IntentRebaseError::IntentVersionNotFound(_) => {
                (StatusCode::NOT_FOUND, "VERSION_NOT_FOUND", false)
            }
            IntentRebaseError::ConcurrencyConflict(_) => {
                (StatusCode::CONFLICT, "CONCURRENCY_CONFLICT", true)
            }
            IntentRebaseError::InvalidIntentVersion(msg) => {
                // Distinguish between "not found" (404) vs "bad request" (400)
                // Version not found messages contain "not found" or "version {} not found"
                // Ordering error messages contain "must be less than" or "must be greater than"
                if msg.contains("must be ") || msg.contains("Cannot diff") {
                    (StatusCode::BAD_REQUEST, "INVALID_VERSION_ORDER", false)
                } else {
                    (StatusCode::NOT_FOUND, "VERSION_NOT_FOUND", false)
                }
            }
            IntentRebaseError::StorageError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR", true)
            }
            IntentRebaseError::SerializationError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "SERIALIZATION_ERROR",
                true,
            ),
            IntentRebaseError::InvalidHeader(_) => {
                (StatusCode::BAD_REQUEST, "INVALID_HEADER", false)
            }
            IntentRebaseError::RebaseConflict(_) => {
                (StatusCode::CONFLICT, "REBASE_CONFLICT", false)
            }
            IntentRebaseError::ArtifactNotFound(_) => {
                (StatusCode::NOT_FOUND, "ARTIFACT_NOT_FOUND", false)
            }
            IntentRebaseError::GraphNodeNotFound(_) => {
                (StatusCode::NOT_FOUND, "GRAPH_NODE_NOT_FOUND", false)
            }
            IntentRebaseError::GraphEdgeNotFound(_) => {
                (StatusCode::NOT_FOUND, "GRAPH_EDGE_NOT_FOUND", false)
            }
            IntentRebaseError::GraphIntegrityError(_) => {
                (StatusCode::BAD_REQUEST, "GRAPH_INTEGRITY_ERROR", false)
            }
            IntentRebaseError::BrokerError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "BROKER_ERROR", true)
            }
            IntentRebaseError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", false)
            }
            IntentRebaseError::InvalidIngestRequest(_) => {
                (StatusCode::BAD_REQUEST, "INVALID_INGEST_REQUEST", false)
            }
            IntentRebaseError::ArtifactTraceabilityEmpty => (
                StatusCode::BAD_REQUEST,
                "ARTIFACT_TRACEABILITY_EMPTY",
                false,
            ),
            IntentRebaseError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", false),
            IntentRebaseError::InvalidApiKey(_) => {
                (StatusCode::UNAUTHORIZED, "INVALID_API_KEY", false)
            }
            IntentRebaseError::ApprovalRequestNotFound(_) => {
                (StatusCode::NOT_FOUND, "APPROVAL_REQUEST_NOT_FOUND", false)
            }
            IntentRebaseError::ApprovalRequestNotPending(_, _) => {
                (StatusCode::CONFLICT, "APPROVAL_REQUEST_NOT_PENDING", false)
            }
            IntentRebaseError::CheckpointNotFound(_) => {
                (StatusCode::BAD_REQUEST, "CHECKPOINT_NOT_FOUND", false)
            }
            IntentRebaseError::PolicySnapshotNotFound(_) => {
                (StatusCode::NOT_FOUND, "POLICY_SNAPSHOT_NOT_FOUND", false)
            }
            IntentRebaseError::CompensationActionNotFound(_) => (
                StatusCode::NOT_FOUND,
                "COMPENSATION_ACTION_NOT_FOUND",
                false,
            ),
            IntentRebaseError::SideEffectNotFound(_) => {
                (StatusCode::NOT_FOUND, "SIDE_EFFECT_NOT_FOUND", false)
            }
            IntentRebaseError::UnknownEffectClass(_) => {
                (StatusCode::BAD_REQUEST, "UNKNOWN_EFFECT_CLASS", false)
            }
            IntentRebaseError::InvalidCompensationActionTransition { .. } => (
                StatusCode::CONFLICT,
                "INVALID_COMPENSATION_ACTION_TRANSITION",
                false,
            ),
            IntentRebaseError::CompensationActionNotExecutable(_) => (
                StatusCode::CONFLICT,
                "COMPENSATION_ACTION_NOT_EXECUTABLE",
                false,
            ),
            IntentRebaseError::CompensationActionConcurrencyConflict(_) => (
                StatusCode::CONFLICT,
                "COMPENSATION_ACTION_CONCURRENCY_CONFLICT",
                true,
            ),
            IntentRebaseError::CompensationActionNotReapprovable(_, _) => (
                StatusCode::CONFLICT,
                "COMPENSATION_ACTION_NOT_REAPPROVABLE",
                false,
            ),
            IntentRebaseError::CompensationActionRetryExhausted(_, _) => (
                StatusCode::CONFLICT,
                "COMPENSATION_ACTION_RETRY_EXHAUSTED",
                false,
            ),
            IntentRebaseError::CompensationActionNonRetryableError(_, _) => (
                StatusCode::CONFLICT,
                "COMPENSATION_ACTION_NON_RETRYABLE_ERROR",
                false,
            ),
            IntentRebaseError::OrchestrationRunNotFound(_) => {
                (StatusCode::NOT_FOUND, "ORCHESTRATION_RUN_NOT_FOUND", false)
            }
            IntentRebaseError::RollbackRecordNotFound(_) => {
                (StatusCode::NOT_FOUND, "ROLLBACK_RECORD_NOT_FOUND", false)
            }
            IntentRebaseError::QuotaExceeded { .. } => {
                (StatusCode::FORBIDDEN, "QUOTA_EXCEEDED", false)
            }
            IntentRebaseError::TenantNotFound(_) => {
                (StatusCode::NOT_FOUND, "TENANT_NOT_FOUND", false)
            }
            IntentRebaseError::TenantNotFoundBySlug(_) => {
                (StatusCode::NOT_FOUND, "TENANT_NOT_FOUND", false)
            }
            IntentRebaseError::ForensicBundleNotFound(_) => {
                (StatusCode::NOT_FOUND, "FORENSIC_BUNDLE_NOT_FOUND", false)
            }
            IntentRebaseError::InvalidForensicBundleStatusTransition { .. } => (
                StatusCode::CONFLICT,
                "INVALID_BUNDLE_STATUS_TRANSITION",
                false,
            ),
        };

        let body = ApiError {
            error: ErrorDetails {
                code: code.to_string(),
                message: err.to_string(),
                retryable,
                details: None,
            },
        };

        (status, Json(body)).into_response()
    }
}
