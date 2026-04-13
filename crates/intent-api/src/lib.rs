//! Intent API HTTP transport layer
//!
//! Phase 1: Exposes intent/version endpoints via axum.
//! Routes are manually wired to match the OpenAPI spec in docs/04-api/openapi.yaml.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use forensic_service::{
    bundle::{BundlePurpose, BundleTimeRange, ForensicBundle},
    bundle_gen::{BundleGenerationService, CreateBundleRequest as GenCreateBundleRequest},
    bundle_repo::InMemoryBundleRepository,
};
use graph_service::GraphService;
use intent_rebase_types::{
    AffectedItemsPreview, CreateGraphEdgeRequest, CreateGraphNodeRequest, CreateIntentRequest,
    CreateIntentResponse, CreateVersionRequest, CreateVersionResponse, DiffRequest, EdgeType,
    GraphEdge, GraphNode, IntentHeadResponse, IntentRebaseError, IntentVersion,
    ListVersionsResponse, NodeType, PolicySnapshot, ValidateIntentResponse,
};
use intent_service::{ApprovalRequest, ApprovalRequestStatus, IntentService};
use metrics_exporter_prometheus::PrometheusBuilder;
use rebase_engine::planner::CompensationPlanningSummary;
use rebase_engine::{
    DecisionClass, DiffRiskAnalysis, IntentVersionDiff, RiskTier, SectionDecision,
};
use rebase_orchestrator::{
    apply_pipeline::ApplyOutcome, checkpoint_aligner::CheckpointAlignmentOutcome,
    RebaseOrchestrator, RuntimeExecutionStatus,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use uuid::Uuid;
use validator::Validate;

/// Response for diff computation including version context, diff, and risk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub diff: IntentVersionDiff,
    pub risk: DiffRiskAnalysis,
}

/// Response for rebase preview (Phase 1 PR #16 - graph-integrated affected items)
///
/// Exposes semantically reliable planner summary fields plus graph-integrated
/// affected items when graph data is available. The `affected_items.status` field
/// indicates whether graph classification succeeded.
///
/// When `status` is `Unavailable`, the graph service was not available or the
/// IntentVersion node was not found in the graph. The endpoint remains functional
/// even without graph coverage - this is NOT an error condition.
///
/// Phase 2b: `risk_tier` is the canonical public risk enum field (Low/Medium/High/Critical).
/// `risk_level` (u8 1-5) and `decision_class` remain as supporting fields.
///
/// Phase 3 Batch 1 (bounded slice): `compensation_planning` exposes read-only
/// compensation planning summary from the rebase planner. This is a skeleton/preview
/// only — does not indicate execution capability or actual compensation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebasePreviewResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub decision_class: DecisionClass,
    pub rationale: String,
    pub section_decisions: Vec<SectionDecision>,
    pub affected_items: AffectedItemsPreview,
    pub manual_review_recommended: bool,
    /// Phase 2b: Canonical public risk tier (primary public risk field)
    pub risk_tier: RiskTier,
    /// Supporting risk level (1=lowest, 5=highest)
    pub risk_level: u8,
    /// Phase 3 Batch 1: Read-only compensation planning summary.
    /// This is planner-generated preview data, NOT executed compensation actions.
    /// The `ready` field indicates whether full compensation planning is available;
    /// when `false`, the action list is empty and execution is not supported.
    pub compensation_planning: CompensationPlanningSummary,
}

/// Response for rebase apply.
///
/// Phase 2b: `risk_tier` is the canonical public risk enum field (Low/Medium/High/Critical).
/// `risk_level` (u8 1-5) and `decision_class` remain as supporting fields.
///
/// Phase 3 Batch 1 (bounded slice): `compensation_planning` exposes read-only
/// compensation planning summary from the rebase planner. This is a skeleton/preview
/// only — does not indicate execution capability or actual compensation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseApplyResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub decision_class: DecisionClass,
    /// Phase 2b: Canonical public risk tier (primary public risk field)
    pub risk_tier: RiskTier,
    /// Supporting risk level (1=lowest, 5=highest)
    pub risk_level: u8,
    pub outcome: String,
    pub manual_review_required: bool,
    pub notification_required: bool,
    pub rationale: String,
    pub aligned_checkpoint_id: Option<Uuid>,
    pub checkpoint_alignment_outcome: Option<String>,
    pub runtime_execution_status: String,
    pub signal_sent: bool,
    pub replay_attempted: bool,
    pub replay_completed: bool,
    pub graph_updates_applied: usize,
    pub graph_updates_failed: usize,
    /// Phase 3 Batch 1: Read-only compensation planning summary.
    /// This is planner-generated preview data, NOT executed compensation actions.
    /// The `ready` field indicates whether full compensation planning is available;
    /// when `false`, the action list is empty and execution is not supported.
    pub compensation_planning: CompensationPlanningSummary,
}

/// Request body for replay endpoint (Phase 2b bounded replay slice).
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Note: This is cooperative signal-based replay, NOT native Temporal reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRequest {
    /// Source version for replay (optional, uses current head if not specified)
    #[serde(default)]
    pub from_version: Option<i32>,
    /// Target version for replay (required)
    pub to_version: i32,
    /// Optional specific checkpoint ID to use for replay
    #[serde(default)]
    pub checkpoint_id: Option<Uuid>,
}

/// Response for replay endpoint (Phase 2b bounded replay slice).
///
/// Reflects cooperative signal-based replay semantics using existing
/// runtime/checkpoint seams. This is NOT native Temporal reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResponse {
    pub intent_id: Uuid,
    pub from_version: i32,
    pub to_version: i32,
    pub aligned_checkpoint_id: Option<Uuid>,
    pub checkpoint_selection_outcome: String,
    pub runtime_execution_status: String,
    pub signal_sent: bool,
    pub replay_attempted: bool,
    pub replay_completed: bool,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub service: Arc<IntentService>,
    pub graph_service: Arc<GraphService>,
    pub orchestrator: Arc<RebaseOrchestrator>,
    pub audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    pub approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    pub policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    /// Phase 2b: Optional event publisher for audit event streaming.
    /// When None, events are persisted to audit storage but NOT streamed.
    /// When Some, events are also published to the event stream (best-effort, fail-open).
    /// Consumers, DLQ, and real NATS integration are Phase 3 items.
    pub event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    /// Phase 3 Batch 1 (groundwork): Side effect service for recording and querying
    /// side effects from artifact-producing operations.
    pub side_effect_service: Arc<compensation_service::SideEffectService>,
    /// Phase 3 Batch 1: Compensation action service for querying compensation actions.
    /// This is a read-only query facade; mutation/execution is Batch 1+ scope.
    pub compensation_action_service: Arc<compensation_service::CompensationActionService>,
    /// Phase 3 Batch 1 (bounded single-shot): Orchestration runtime for executing
    /// compensation actions via HTTP accepted flow.
    pub orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    /// Phase 3 Batch 3b (P4 bounded slice): Forensic bundle generation service.
    /// Manages bundle lifecycle (Pending -> Generating -> Ready/Failed).
    /// S3 storage, actual content collection, integrity verification are Phase 4 scope.
    pub forensic_bundle_service: Arc<BundleGenerationService<InMemoryBundleRepository>>,
    pub start_time: Instant,
}

/// API error response matching OpenAPI Error schema
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

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
            IntentRebaseError::InvalidForensicBundleStatusTransition { .. } => {
                (StatusCode::BAD_REQUEST, "INVALID_BUNDLE_STATUS_TRANSITION", false)
            }
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

// ============================================================================
// API Key Authentication Scaffold (Phase 1)
// ============================================================================

/// API key extracted from X-API-Key header.
/// Phase 1: This is stored in request extensions but NOT validated.
/// Phase 2: Will integrate with actual API key validation and tenant resolution.
#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

/// Extension key for storing API key in request extensions.
#[derive(Clone, Copy)]
pub struct ApiKeyExtensionKey;

impl std::fmt::Display for ApiKeyExtensionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKeyExtensionKey")
    }
}

/// Rejection type for API key extraction — implements IntoResponse for axum compatibility.
#[derive(Debug)]
pub struct ApiKeyRejection(pub String);

impl IntoResponse for ApiKeyRejection {
    fn into_response(self) -> axum::response::Response {
        let body = ApiError {
            error: ErrorDetails {
                code: "INVALID_API_KEY".to_string(),
                message: self.0,
                retryable: false,
                details: None,
            },
        };
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    }
}

#[async_trait::async_trait]
impl<S> axum::extract::FromRequestParts<S> for ApiKey
where
    S: Clone + Send + Sync,
{
    type Rejection = ApiKeyRejection;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Phase 1: Look for X-API-Key header, return empty if not present
        // Phase 2: This will become mandatory and validated against a key store
        match parts.headers.get("x-api-key") {
            Some(value) => {
                let key = value
                    .to_str()
                    .map_err(|_| ApiKeyRejection("X-API-Key header is not valid UTF-8".into()))?;
                Ok(ApiKey(key.to_string()))
            }
            None => {
                // Phase 1: Return empty API key (no blocking)
                // The middleware logs this for observability
                Ok(ApiKey(String::new()))
            }
        }
    }
}

/// Middleware that extracts X-API-Key header and stores it in request extensions.
/// Phase 1: This middleware logs the presence/absence of API keys but does NOT block requests.
/// Phase 2: Will validate API keys and enforce authentication.
pub async fn api_key_extractor_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let api_key = request
        .headers()
        .get("x-api-key")
        .map(|v| v.to_str().unwrap_or("<invalid utf8>"));

    match api_key {
        Some(key) if !key.is_empty() => {
            tracing::debug!("API key present: {}...", &key[..key.len().min(8)]);
        }
        _ => {
            tracing::debug!("No API key present in request (Phase 1 - allowed)");
        }
    }

    // Phase 1: Pass through without blocking
    // Phase 2: Add actual validation here
    next.run(request).await
}

// ============================================================================
// Input Validation
// ============================================================================

/// Validates required fields in CreateIntentRequest.
/// Returns Err with specific validation error if any field is invalid.
pub fn validate_create_intent_request(
    request: &CreateIntentRequest,
) -> Result<(), IntentRebaseError> {
    // Validate workflow_id is not nil/zero UUID
    if request.workflow_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "workflow_id cannot be nil".into(),
        ));
    }

    // Validate payload.objective.summary is not empty
    if request.payload.objective.summary.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.summary cannot be empty".into(),
        ));
    }

    // Validate payload.objective.success_statement is not empty
    if request
        .payload
        .objective
        .success_statement
        .trim()
        .is_empty()
    {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.success_statement cannot be empty".into(),
        ));
    }

    // Validate payload.objective.domain is not empty
    if request.payload.objective.domain.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.domain cannot be empty".into(),
        ));
    }

    Ok(())
}

/// Validates required fields in CreateVersionRequest.
/// Returns Err with specific validation error if any field is invalid.
pub fn validate_create_version_request(
    request: &CreateVersionRequest,
) -> Result<(), IntentRebaseError> {
    // Validate payload.objective.summary is not empty
    if request.payload.objective.summary.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.summary cannot be empty".into(),
        ));
    }

    // Validate payload.objective.success_statement is not empty
    if request
        .payload
        .objective
        .success_statement
        .trim()
        .is_empty()
    {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.success_statement cannot be empty".into(),
        ));
    }

    // Validate payload.objective.domain is not empty
    if request.payload.objective.domain.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.domain cannot be empty".into(),
        ));
    }

    Ok(())
}

/// Validates required fields in ArtifactIngestRequest.
/// Returns Err with specific validation error if any field is invalid.
///
/// Phase 3 Batch 1 (groundwork): When side_effect_context is provided, validates:
/// - source_intent_id cannot be nil
/// - source_intent_version must be > 0
/// - effect_type cannot be empty or whitespace-only
/// - target cannot be empty or whitespace-only
pub fn validate_artifact_ingest_request(
    request: &ArtifactIngestRequest,
) -> Result<(), IntentRebaseError> {
    // Validate tenant_id is not nil/zero UUID
    if request.tenant_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "tenant_id cannot be nil".into(),
        ));
    }

    // Validate workflow_id is not nil/zero UUID
    if request.workflow_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "workflow_id cannot be nil".into(),
        ));
    }

    // Validate label is not empty
    if request.label.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "label cannot be empty".into(),
        ));
    }

    // Validate external_ref.ref_id is not nil UUID
    if request.external_ref.ref_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "external_ref.ref_id cannot be nil".into(),
        ));
    }

    // Phase 3 Batch 1: Validate side_effect_context if provided
    if let Some(ref context) = request.side_effect_context {
        // source_intent_id cannot be nil
        if context.source_intent_id == Uuid::nil() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.source_intent_id cannot be nil".into(),
            ));
        }

        // source_intent_version must be > 0
        if context.source_intent_version <= 0 {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "side_effect_context.source_intent_version must be > 0, got {}",
                context.source_intent_version
            )));
        }

        // effect_type cannot be empty or whitespace-only
        if context.effect_type.trim().is_empty() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.effect_type cannot be empty".into(),
            ));
        }

        // target cannot be empty or whitespace-only
        if context.target.trim().is_empty() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.target cannot be empty".into(),
            ));
        }

        // idempotency_key, if provided, cannot be empty or whitespace-only
        if let Some(ref key) = context.idempotency_key {
            if key.trim().is_empty() {
                return Err(IntentRebaseError::InvalidIngestRequest(
                    "side_effect_context.idempotency_key cannot be empty".into(),
                ));
            }
        }
    }

    Ok(())
}

/// POST /intents - Create a new intent
async fn create_intent(
    State(state): State<AppState>,
    Json(request): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), ApiErrorResponse> {
    // Phase 1: Input validation
    validate_create_intent_request(&request).map_err(ApiErrorResponse)?;

    state
        .service
        .create_intent(request)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
        .map_err(ApiErrorResponse)
}

/// GET /intents/{intent_id} - Get intent head (current version)
async fn get_intent_head(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
) -> Result<Json<IntentHeadResponse>, ApiErrorResponse> {
    state
        .service
        .get_intent_head(intent_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// POST /intents/{intent_id}/versions - Create a new version
///
/// Optional OCC headers:
/// - `X-Expected-Version`: the version number the client expects to be current
/// - `X-Expected-Row-Version`: the row_version the client last observed
///   If provided, enables optimistic concurrency control. Returns 409 on conflict.
///   If headers are malformed (non-integer), returns 400 Bad Request.
async fn create_version(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<CreateVersionResponse>), ApiErrorResponse> {
    let expected_version =
        parse_optional_header(&headers, "x-expected-version").map_err(ApiErrorResponse)?;
    let expected_row_version =
        parse_optional_header(&headers, "x-expected-row-version").map_err(ApiErrorResponse)?;

    state
        .service
        .create_version(intent_id, request, expected_version, expected_row_version)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
        .map_err(ApiErrorResponse)
}

/// Parse an optional i32 header value.
/// Returns Ok(None) if header is absent, Ok(Some(value)) if present and valid.
/// Returns Err(InvalidHeader) if header is present but malformed.
fn parse_optional_header(
    headers: &HeaderMap,
    name: &str,
) -> Result<Option<i32>, IntentRebaseError> {
    match headers.get(name) {
        None => Ok(None),
        Some(v) => {
            let s = v.to_str().map_err(|_| {
                IntentRebaseError::InvalidHeader(format!("{} header is not valid UTF-8", name))
            })?;
            s.parse::<i32>().map(Some).map_err(|_| {
                IntentRebaseError::InvalidHeader(format!(
                    "{} header must be an integer, got: {}",
                    name, s
                ))
            })
        }
    }
}

/// Recursively collect nested validation errors from ValidationErrors
fn collect_nested_errors(
    errors: &validator::ValidationErrors,
    prefix: &str,
    out: &mut Vec<(String, validator::ValidationError)>,
) {
    for (field, kind) in errors.0.iter() {
        match kind {
            validator::ValidationErrorsKind::Field(field_errors) => {
                for e in field_errors {
                    let full_field = if prefix.is_empty() {
                        field.to_string()
                    } else {
                        format!("{prefix}.{field}")
                    };
                    out.push((
                        full_field,
                        validator::ValidationError {
                            code: e.code.clone(),
                            message: e.message.clone(),
                            params: e.params.clone(),
                        },
                    ));
                }
            }
            validator::ValidationErrorsKind::Struct(nested) => {
                let new_prefix = if prefix.is_empty() {
                    field.to_string()
                } else {
                    format!("{prefix}.{field}")
                };
                collect_nested_errors(nested, &new_prefix, out);
            }
            validator::ValidationErrorsKind::List(_) => {
                // Skip list errors for now (collections not used in Phase 1)
            }
        }
    }
}

/// POST /v1/intents/validate - Validate an intent request without persisting
async fn validate_intent(Json(request): Json<CreateIntentRequest>) -> Json<ValidateIntentResponse> {
    use intent_rebase_types::ValidationError;

    match request.validate() {
        Ok(()) => Json(ValidateIntentResponse {
            valid: true,
            errors: vec![],
        }),
        Err(errs) => {
            let mut raw_errors: Vec<(String, validator::ValidationError)> = Vec::new();
            collect_nested_errors(&errs, "", &mut raw_errors);
            let validation_errors: Vec<ValidationError> = raw_errors
                .into_iter()
                .map(|(field, e)| ValidationError {
                    field,
                    message: e.message.as_ref().unwrap_or(&e.code).to_string(),
                })
                .collect();

            Json(ValidateIntentResponse {
                valid: validation_errors.is_empty(),
                errors: validation_errors,
            })
        }
    }
}

/// GET /intents/{intent_id}/versions - List all versions (descending order)
async fn list_versions(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
) -> Result<Json<ListVersionsResponse>, ApiErrorResponse> {
    state
        .service
        .list_versions(intent_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /intents/{intent_id}/versions/{version_number} - Get specific version
async fn get_version(
    State(state): State<AppState>,
    Path((intent_id, version_number)): Path<(Uuid, i32)>,
) -> Result<Json<IntentVersion>, ApiErrorResponse> {
    state
        .service
        .get_version(intent_id, version_number)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// POST /intents/{intent_id}/diff - Compute diff between two versions
///
/// Request body: { from_version, to_version }
/// Response: version context plus diff and risk analysis
async fn compute_diff(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<DiffResponse>, ApiErrorResponse> {
    let (from_version, to_version, diff, risk) = state
        .service
        .compute_diff(intent_id, request.from_version, request.to_version)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(DiffResponse {
        intent_id,
        from_version,
        to_version,
        diff,
        risk,
    }))
}

// /// POST /intents/{intent_id}/rebase-preview - Generate rebase preview plan
// ///
// /// Request body: { from_version, to_version }
// /// Response: rebase preview with decision class, rationale, section decisions,
// /// and graph-integrated affected items when available.
// ///
// /// Phase 1 PR #16: Includes graph-integrated affected items when graph service
// /// is available. The `affected_items.status` field indicates whether classification
// /// succeeded. When `status` is `Unavailable`, the endpoint remains functional but
// /// the affected items arrays may be incomplete.

// ============================================================================
// Graph Handlers (Phase 1 - Internal CRUD only)
// ============================================================================

/// Query parameters for listing graph nodes
#[derive(Debug, Deserialize)]
pub struct ListGraphNodesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub node_type: Option<NodeType>,
}

/// Query parameters for listing graph edges
#[derive(Debug, Deserialize)]
pub struct ListGraphEdgesQuery {
    pub tenant_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub from_node_id: Option<Uuid>,
    pub edge_type: Option<EdgeType>,
}

/// POST /v1/graph/nodes - Create a new graph node
async fn create_graph_node(
    State(state): State<AppState>,
    Json(request): Json<CreateGraphNodeRequest>,
) -> Result<(StatusCode, Json<GraphNode>), ApiErrorResponse> {
    state
        .graph_service
        .add_node(request)
        .await
        .map(|node| (StatusCode::CREATED, Json(node)))
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes - List graph nodes with optional filters
async fn list_graph_nodes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListGraphNodesQuery>,
) -> Result<Json<Vec<GraphNode>>, ApiErrorResponse> {
    use intent_rebase_types::GraphNodeFilter;

    let filter = GraphNodeFilter {
        tenant_id: query.tenant_id,
        workflow_id: query.workflow_id,
        node_type: query.node_type,
        state: None,
    };

    state
        .graph_service
        .list_nodes(filter)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes/{node_id} - Get a single graph node by ID
async fn get_graph_node(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<GraphNode>, ApiErrorResponse> {
    state
        .graph_service
        .get_node(node_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// POST /v1/graph/edges - Create a new graph edge
async fn create_graph_edge(
    State(state): State<AppState>,
    Json(request): Json<CreateGraphEdgeRequest>,
) -> Result<(StatusCode, Json<GraphEdge>), ApiErrorResponse> {
    state
        .graph_service
        .add_edge(request)
        .await
        .map(|edge| (StatusCode::CREATED, Json(edge)))
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/edges - List graph edges with optional filters
async fn list_graph_edges(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListGraphEdgesQuery>,
) -> Result<Json<Vec<GraphEdge>>, ApiErrorResponse> {
    use intent_rebase_types::GraphEdgeFilter;

    let filter = GraphEdgeFilter {
        tenant_id: query.tenant_id,
        workflow_id: query.workflow_id,
        from_node_id: query.from_node_id,
        to_node_id: None,
        edge_type: query.edge_type,
    };

    state
        .graph_service
        .list_edges(filter)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /v1/graph/nodes/{node_id}/edges - List edges outgoing from a node
async fn list_edges_from_node(
    State(state): State<AppState>,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Vec<GraphEdge>>, ApiErrorResponse> {
    state
        .graph_service
        .list_edges_from(node_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

async fn rebase_preview(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    // Always use graph-integrated preview - the service handles unavailability gracefully
    let plan = state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Get version info for response context
    let from_version = state
        .service
        .get_version(intent_id, request.from_version)
        .await
        .map_err(ApiErrorResponse)?;
    let to_version = state
        .service
        .get_version(intent_id, request.to_version)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(RebasePreviewResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        rationale: plan.rationale,
        section_decisions: plan.section_decisions,
        affected_items: plan.affected_items,
        manual_review_recommended: plan.manual_review_recommended,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    }))
}

async fn rebase_apply(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<(StatusCode, Json<RebaseApplyResponse>), ApiErrorResponse> {
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;
    let from_version = state
        .service
        .get_version(intent_id, request.from_version)
        .await
        .map_err(ApiErrorResponse)?;
    let to_version = state
        .service
        .get_version(intent_id, request.to_version)
        .await
        .map_err(ApiErrorResponse)?;
    let plan = state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
        .map_err(ApiErrorResponse)?;
    let apply_result = state
        .orchestrator
        .apply_rebase(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            &from_version,
            &to_version,
            &plan,
            &plan.affected_items,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b bounded slice: Record audit event for all external apply outcomes
    // Best-effort actor attribution: fallback external-api/unknown
    let actor_id = "external-api/unknown";
    let audit_payload = intent_rebase_types::RebaseApplyAuditPayload {
        from_version: request.from_version,
        to_version: request.to_version,
        decision_class: format!("{:?}", plan.decision_class),
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
    };

    // Record audit event (best-effort, don't fail the response)
    if let Err(e) = state
        .audit_service
        .record_rebase_applied(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "RebaseApplied",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Phase 2b bounded slice: Create pending approval_request when blocked D/E
    if matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview) {
        let blocked_payload = intent_rebase_types::RebaseApplyBlockedAuditPayload {
            from_version: request.from_version,
            to_version: request.to_version,
            decision_class: format!("{:?}", plan.decision_class),
            risk_level: plan.risk_level,
            rationale: apply_result.rationale.clone(),
            requestor_id: actor_id.to_string(),
            requestor_type: "external-api".to_string(),
        };

        // Record blocked audit event (best-effort)
        if let Err(e) = state
            .audit_service
            .record_rebase_apply_blocked(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                blocked_payload.clone(),
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "RebaseApplyBlocked",
                &serde_json::to_value(blocked_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        // Create pending approval_request record
        let approval_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            request.from_version,
            request.to_version,
            intent_head.intent.workflow_id,
            intent_head.intent.tenant_id,
            actor_id,
            "external-api",
            &format!("{:?}", plan.decision_class),
            &apply_result.rationale,
        );

        if let Err(e) = state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
        {
            tracing::warn!("Failed to create approval_request record: {:?}", e);
        }
    }

    let response = RebaseApplyResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        notification_required: apply_result.notification_required,
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    };

    Ok((apply_status_code(&apply_result.outcome), Json(response)))
}

fn apply_status_code(outcome: &ApplyOutcome) -> StatusCode {
    match outcome {
        ApplyOutcome::BlockedManualReview => StatusCode::ACCEPTED,
        ApplyOutcome::NoOp
        | ApplyOutcome::AutoProceeded
        | ApplyOutcome::AutoProceededWithNotification => StatusCode::OK,
    }
}

fn apply_outcome_label(outcome: &ApplyOutcome) -> &'static str {
    match outcome {
        ApplyOutcome::NoOp => "no_op",
        ApplyOutcome::AutoProceeded => "auto_proceeded",
        ApplyOutcome::AutoProceededWithNotification => "auto_proceeded_with_notification",
        ApplyOutcome::BlockedManualReview => "blocked_manual_review",
    }
}

fn checkpoint_alignment_label(outcome: &CheckpointAlignmentOutcome) -> &'static str {
    match outcome {
        CheckpointAlignmentOutcome::Aligned => "aligned",
        CheckpointAlignmentOutcome::ClosestMatch => "closest_match",
        CheckpointAlignmentOutcome::NoCheckpointRequired => "no_checkpoint_required",
        CheckpointAlignmentOutcome::NoCheckpointFound => "no_checkpoint_found",
        CheckpointAlignmentOutcome::MultipleCandidates => "multiple_candidates",
    }
}

fn runtime_execution_status_label(status: &RuntimeExecutionStatus) -> &'static str {
    match status {
        RuntimeExecutionStatus::NotApplicable => "not_applicable",
        RuntimeExecutionStatus::SkippedNotReady => "skipped_not_ready",
        RuntimeExecutionStatus::Degraded => "degraded",
        RuntimeExecutionStatus::Succeeded => "succeeded",
        RuntimeExecutionStatus::SucceededNoReplay => "succeeded_no_replay",
    }
}

// ============================================================================
// Phase 2b: Event Publishing Helpers (bounded event-streaming slice)
// ============================================================================

/// Phase 2b: Publish an audit event to the event stream (best-effort, fail-open).
///
/// This function is used after successful audit persistence to also publish
/// the event to the configured event stream.
///
/// **Bounded slice behavior**:
/// - Audit persistence is the source of truth (already completed successfully)
/// - Event publishing is best-effort: failures are logged but don't fail the operation
/// - When `event_publisher` is None, this is a no-op
/// - When event_publisher fails, the overall operation continues
///
/// **Phase 3 items** (not implemented in Phase 2b):
/// - Consumers (checkpoint-creator, snapshot-creator, notifier)
/// - Dead-letter queue (DLQ) for failed event processing
/// - Real NATS JetStream integration (only InMemoryEventPublisher is available)
async fn publish_audit_event(
    event_publisher: &Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    tenant_id: uuid::Uuid,
    event_type: &str,
    payload: &serde_json::Value,
) {
    let publisher = match event_publisher {
        Some(p) => p.as_ref(),
        None => return, // No publisher configured - silently skip
    };

    let subject = intent_rebase_types::EventSubject::from_audit_event(tenant_id, event_type);
    match publisher.publish(&subject, payload).await {
        intent_rebase_types::PublishResult::Published {
            subject: s,
            sequence,
        } => {
            tracing::debug!("Published audit event to '{}' (seq={})", s, sequence);
        }
        intent_rebase_types::PublishResult::Skipped { reason } => {
            tracing::warn!(
                "Skipped publishing audit event to '{}': {}",
                subject.subject,
                reason
            );
        }
    }
}

// ============================================================================
// Approval Request Handlers (Phase 2b bounded slice)
// ============================================================================

/// Response for listing pending approval requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPendingApprovalRequestsResponse {
    pub approval_requests: Vec<ApprovalRequestSummary>,
    pub total: usize,
}

/// Summary of an approval request for list responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestSummary {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub decision_class: String,
    pub reason: String,
    pub requestor_id: String,
    pub requestor_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<ApprovalRequest> for ApprovalRequestSummary {
    fn from(req: ApprovalRequest) -> Self {
        Self {
            id: req.id,
            intent_id: req.intent_id,
            intent_version_from: req.intent_version_from,
            intent_version_to: req.intent_version_to,
            decision_class: req.decision_class,
            reason: req.reason,
            requestor_id: req.requestor_id,
            requestor_type: req.requestor_type,
            status: format!("{:?}", req.status),
            created_at: req.created_at,
            expires_at: req.expires_at,
        }
    }
}

/// Request body for approving an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveApprovalRequestBody {
    #[serde(default)]
    pub resolution_notes: Option<String>,
}

/// Request body for rejecting an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectApprovalRequestBody {
    #[serde(default)]
    pub resolution_notes: Option<String>,
}

/// Request body for expiring an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpireApprovalRequestBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response for approve/reject approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequestResponse {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub status: String,
    pub resolved_by: String,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_notes: Option<String>,
}

/// Query parameters for listing pending approval requests
#[derive(Debug, Deserialize)]
pub struct ListPendingApprovalRequestsQuery {
    pub tenant_id: Uuid,
}

/// GET /approval-requests/pending - List pending approval requests for a tenant
async fn list_pending_approval_requests(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListPendingApprovalRequestsQuery>,
) -> Result<Json<ListPendingApprovalRequestsResponse>, ApiErrorResponse> {
    let pending = state
        .approval_request_repo
        .list_pending_by_tenant(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summaries: Vec<ApprovalRequestSummary> = pending
        .into_iter()
        .map(ApprovalRequestSummary::from)
        .collect();

    let total = summaries.len();

    Ok(Json(ListPendingApprovalRequestsResponse {
        approval_requests: summaries,
        total,
    }))
}

/// POST /approval-requests/{id}/approve - Approve a pending approval request
///
/// Phase 2b bounded slice: Only updates status to approved and emits audit event.
/// Does NOT resume or re-trigger apply.
async fn approve_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ApproveApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/approver)
    let actor_id = "external-api/approver";

    // Update status to Approved
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            ApprovalRequestStatus::Approved,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalGranted audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalGrantedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_granted(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalGranted audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalGranted",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/reject - Reject a pending approval request
///
/// Phase 2b bounded slice: Only updates status to rejected and emits audit event.
/// Does NOT resume or re-trigger apply.
async fn reject_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<RejectApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to external-api/rejector)
    let actor_id = "external-api/rejector";

    // Update status to Rejected
    let updated = state
        .approval_request_repo
        .update_approval_request_status(
            approval_request_id,
            ApprovalRequestStatus::Rejected,
            actor_id,
            body.resolution_notes.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalRevoked audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalRevokedAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        resolved_by: actor_id.to_string(),
        resolution_notes: body.resolution_notes.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_revoked(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRevoked audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalRevoked",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// POST /approval-requests/{id}/expire - Mark a pending approval request as expired
///
/// Phase 2b bounded slice: Manual expiry transition for pending approval requests.
/// Only updates status to expired and emits audit event.
///
/// **No automatic expiry in Phase 2b** - this is a manual transition only.
/// No background worker or automatic expiry machinery exists.
///
/// Does NOT trigger re-approval workflow or resume/re-trigger apply.
async fn expire_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
    Json(body): Json<ExpireApprovalRequestBody>,
) -> Result<Json<ApprovalRequestResponse>, ApiErrorResponse> {
    // Get the approval request first to access its metadata
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Best-effort actor attribution (fallback to system/expire)
    let actor_id = "system/expire";

    // Use provided reason or default
    let reason = body
        .reason
        .unwrap_or_else(|| "Approval time limit exceeded".to_string());

    // Use the mark_expired repository method for atomic transition
    let updated = state
        .approval_request_repo
        .mark_expired(approval_request_id, actor_id, &reason)
        .await
        .map_err(ApiErrorResponse)?;

    // Emit ApprovalExpired audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalExpiredAuditPayload {
        approval_request_id,
        intent_id: approval_request.intent_id,
        intent_version_from: approval_request.intent_version_from,
        intent_version_to: approval_request.intent_version_to,
        decision_class: approval_request.decision_class.clone(),
        expired_by: actor_id.to_string(),
        expiry_reason: reason.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_expired(
            approval_request.tenant_id,
            actor_id,
            approval_request.intent_id,
            audit_payload.clone(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalExpired audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            approval_request.tenant_id,
            "ApprovalExpired",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ApprovalRequestResponse {
        id: updated.id,
        intent_id: updated.intent_id,
        status: format!("{:?}", updated.status),
        resolved_by: updated.resolved_by.unwrap_or_default(),
        resolved_at: updated.resolved_at,
        resolution_notes: updated.resolution_notes,
    }))
}

/// GET /approval-requests/{approval_request_id}/revalidate - Check if approval remains valid
///
/// Phase 2b bounded read-only slice: Compares approval-basis snapshot scope_hash
/// with latest snapshot scope_hash for the same intent.
///
/// Comparison strategy:
/// - Get the policy snapshot for the approval's `intent_version_from` (the approval basis)
/// - Get the latest policy snapshot for the same intent
/// - Compare scope_hash values: if different, approval is no longer valid
///
/// Returns 404 if:
/// - Approval request not found
/// - Approval basis snapshot not found (should exist if approval exists)
///
/// Returns 200 with valid=false if latest snapshot is missing (policy not yet computed
/// for current intent version) - this is NOT a 404, as the approval still exists
/// but we cannot determine current validity without a latest snapshot.
async fn revalidate_approval_request(
    State(state): State<AppState>,
    Path(approval_request_id): Path<Uuid>,
) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
    // Step 1: Fetch the approval request
    let approval_request = state
        .approval_request_repo
        .get_approval_request(approval_request_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2: Fetch the approval-basis policy snapshot (snapshot for intent_version_from)
    let approval_basis_snapshot = state
        .policy_snapshot_repo
        .get_by_intent_version(
            approval_request.intent_id,
            approval_request.intent_version_from,
            approval_request.tenant_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    let approval_basis_scope_hash = match approval_basis_snapshot {
        Some(snapshot) => snapshot.scope_hash,
        None => {
            // Approval basis snapshot missing - this is unexpected but return 404
            return Err(ApiErrorResponse(IntentRebaseError::PolicySnapshotNotFound(
                approval_request.intent_id,
            )));
        }
    };

    // Step 3: Fetch the latest policy snapshot for this intent
    let latest_snapshot = state
        .policy_snapshot_repo
        .get_latest_by_intent(approval_request.intent_id, approval_request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 4: Compare scope_hash values
    let (valid, reason) = match &latest_snapshot {
        Some(latest) if latest.scope_hash == approval_basis_scope_hash => {
            // Scope unchanged - approval remains valid
            (
                true,
                "Scope unchanged since approval was granted".to_string(),
            )
        }
        Some(latest) if latest.scope_hash != approval_basis_scope_hash => {
            // Scope changed - approval no longer valid
            (
                false,
                "Scope has changed since approval was granted".to_string(),
            )
        }
        None => {
            // No latest snapshot available - cannot determine validity
            // Return valid=false but with a clear reason (not a 404)
            (
                false,
                "No latest policy snapshot available for comparison".to_string(),
            )
        }
        // Should not reach here, but handle defensively
        _ => (false, "Unable to determine approval validity".to_string()),
    };

    let current_scope_hash = latest_snapshot.map(|s| s.scope_hash);

    Ok(Json(ApprovalRevalidationResponse {
        approval_id: approval_request_id,
        valid,
        reason,
        approval_basis_scope_hash,
        current_scope_hash,
        revalidation_required: !valid,
        intent_id: approval_request.intent_id,
        approval_basis_version: approval_request.intent_version_from,
    }))
}

// ============================================================================
// Policy Snapshot Handlers (Phase 2 bounded read-only slice)
// ============================================================================

/// Query parameters for getting policy snapshot by ID
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting latest policy snapshot by intent
#[derive(Debug, Deserialize)]
pub struct GetLatestPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting policy snapshot by intent version
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotByVersionQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for listing policy snapshots by intent
#[derive(Debug, Deserialize)]
pub struct ListPolicySnapshotsQuery {
    pub tenant_id: Uuid,
}

/// Response type for a single policy snapshot
#[derive(Debug, Serialize)]
pub struct PolicySnapshotResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version: i32,
    pub rule_pack_version: String,
    pub scope_definition: intent_rebase_types::ScopeDefinition,
    pub scope_hash: String,
    pub snapshot_uri: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub canonicalized_at: chrono::DateTime<chrono::Utc>,
}

impl From<PolicySnapshot> for PolicySnapshotResponse {
    fn from(s: PolicySnapshot) -> Self {
        Self {
            id: s.id,
            tenant_id: s.tenant_id,
            intent_id: s.intent_id,
            intent_version: s.intent_version,
            rule_pack_version: s.rule_pack_version,
            scope_definition: s.scope_definition,
            scope_hash: s.scope_hash,
            snapshot_uri: s.snapshot_uri,
            created_at: s.created_at,
            canonicalized_at: s.canonicalized_at,
        }
    }
}

/// Response for listing policy snapshots
#[derive(Debug, Serialize)]
pub struct ListPolicySnapshotsResponse {
    pub policy_snapshots: Vec<PolicySnapshotResponse>,
    pub total: usize,
}

/// Response for approval revalidation (GET /approval-requests/{id}/revalidate)
///
/// Bounded read-only scope comparison: compares approval-basis snapshot scope_hash
/// with latest snapshot scope_hash for the same intent to determine if approval
/// remains valid. This is a Point-in-Time snapshot comparison, not a live policy evaluation.
///
/// Phase 2b bounded slice: Does NOT trigger re-approval workflow, queue notifications,
/// or modify approval status. Returns 404 if approval or snapshots are not found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRevalidationResponse {
    /// ID of the approval request being revalidated
    pub approval_id: Uuid,
    /// Whether the approval scope remains valid (scope_hash unchanged)
    pub valid: bool,
    /// Human-readable reason for invalidation status
    pub reason: String,
    /// The scope_hash at the time of original approval
    pub approval_basis_scope_hash: String,
    /// The current latest scope_hash for this intent (None if no latest snapshot exists)
    pub current_scope_hash: Option<String>,
    /// Whether re-approval would be required (always true when valid=false)
    pub revalidation_required: bool,
    /// Intent ID this approval is for
    pub intent_id: Uuid,
    /// Intent version when approval was originally granted
    pub approval_basis_version: i32,
}

/// GET /policy-snapshots/{id} - Get a policy snapshot by ID
async fn get_policy_snapshot(
    State(state): State<AppState>,
    Path(snapshot_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<GetPolicySnapshotQuery>,
) -> Result<Json<PolicySnapshotResponse>, ApiErrorResponse> {
    let snapshot = state
        .policy_snapshot_repo
        .get_snapshot(snapshot_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(PolicySnapshotResponse::from(snapshot)))
}

/// GET /policy-snapshots/intent/{intent_id}/latest - Get latest policy snapshot for an intent
async fn get_latest_policy_snapshot(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<GetLatestPolicySnapshotQuery>,
) -> Result<Json<PolicySnapshotResponse>, ApiErrorResponse> {
    let snapshot = state
        .policy_snapshot_repo
        .get_latest_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    match snapshot {
        Some(s) => Ok(Json(PolicySnapshotResponse::from(s))),
        None => Err(ApiErrorResponse(IntentRebaseError::PolicySnapshotNotFound(
            intent_id,
        ))),
    }
}

/// GET /policy-snapshots/intent/{intent_id}/versions/{version} - Get policy snapshot by intent version
async fn get_policy_snapshot_by_version(
    State(state): State<AppState>,
    Path((intent_id, version)): Path<(Uuid, i32)>,
    axum::extract::Query(query): axum::extract::Query<GetPolicySnapshotByVersionQuery>,
) -> Result<Json<PolicySnapshotResponse>, ApiErrorResponse> {
    let snapshot = state
        .policy_snapshot_repo
        .get_by_intent_version(intent_id, version, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    match snapshot {
        Some(s) => Ok(Json(PolicySnapshotResponse::from(s))),
        None => Err(ApiErrorResponse(IntentRebaseError::PolicySnapshotNotFound(
            intent_id,
        ))),
    }
}

/// GET /policy-snapshots/intent/{intent_id} - List all policy snapshots for an intent
async fn list_policy_snapshots(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListPolicySnapshotsQuery>,
) -> Result<Json<ListPolicySnapshotsResponse>, ApiErrorResponse> {
    let snapshots = state
        .policy_snapshot_repo
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let responses: Vec<PolicySnapshotResponse> = snapshots
        .into_iter()
        .map(PolicySnapshotResponse::from)
        .collect();

    Ok(Json(ListPolicySnapshotsResponse {
        total: responses.len(),
        policy_snapshots: responses,
    }))
}

// ============================================================================
// Replay Handler (Phase 2b bounded replay slice)
// ============================================================================

/// POST /intents/{intent_id}/replay - Initiate a bounded replay operation
///
/// Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
/// seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided in request, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Returns bounded replay outcome with checkpoint alignment details.
async fn replay_intent(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    // Get intent head to find workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    let from_version = request
        .from_version
        .unwrap_or(intent_head.version.version_number);
    let to_version = request.to_version;

    // Phase 2b: Validate target version exists before attempting replay
    state
        .service
        .get_version(intent_id, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b: Validate source version exists if explicitly specified
    if request.from_version.is_some() {
        state
            .service
            .get_version(intent_id, from_version)
            .await
            .map_err(ApiErrorResponse)?;
    }

    // Execute bounded replay via orchestrator
    let replay_result = state
        .orchestrator
        .replay(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            from_version,
            to_version,
            request.checkpoint_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Record ReplayInitiated audit event (best-effort)
    let actor_id = "external-api/replay";
    let audit_payload = intent_rebase_types::ReplayAuditPayload {
        from_version: Some(from_version),
        to_version: Some(to_version),
        checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
        replay_initiated_via: "post-intents-intent-id-replay".to_string(),
        rationale: format!(
            "Bounded replay initiated from v{} to v{} via public replay endpoint",
            from_version, to_version
        ),
    };

    if let Err(e) = state
        .audit_service
        .record_replay_initiated(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ReplayInitiated",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ReplayResponse {
        intent_id,
        from_version,
        to_version,
        aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
        runtime_execution_status: runtime_execution_status_label(
            &replay_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: replay_result.runtime_execution_result.signal_sent,
        replay_attempted: replay_result.runtime_execution_result.replay_attempted,
        replay_completed: replay_result.runtime_execution_result.replay_completed,
    }))
}

/// Initialize tracing with JSON formatting using RUST_LOG env var
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json())
        .init();
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

/// GET /health - Returns health status with uptime
async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_seconds: uptime,
    })
}

/// GET /ready - Returns readiness status
async fn ready_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready".to_string(),
        uptime_seconds: 0,
    })
}

/// GET /metrics - Returns Prometheus-formatted metrics
async fn metrics_handler() -> impl IntoResponse {
    use metrics_exporter_prometheus::PrometheusHandle;
    // Use a static handle initialized once — install_recorder() starts a background server
    static HANDLE: std::sync::OnceLock<PrometheusHandle> = std::sync::OnceLock::new();
    let handle = HANDLE.get_or_init(|| {
        PrometheusBuilder::new()
            .install_recorder()
            .expect("Failed to install Prometheus recorder")
    });
    let metrics = handle.render();
    axum::response::Response::builder()
        .header(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )
        .body(axum::body::Body::from(metrics))
        .expect("Failed to build metrics response")
}

// ============================================================================
// Side Effect Handlers (Phase 3 Batch 1 groundwork)
// ============================================================================

/// Query parameters for listing side effects
#[derive(Debug, Deserialize)]
pub struct ListSideEffectsQuery {
    pub tenant_id: Uuid,
}

/// Response for listing side effects
#[derive(Debug, Serialize)]
pub struct ListSideEffectsResponse {
    pub side_effects: Vec<compensation_service::SideEffect>,
    pub total: usize,
}

/// GET /intents/{intent_id}/side-effects - List side effects for an intent
///
/// Phase 3 Batch 1 (groundwork): Returns all side effects recorded for the given
/// intent, scoped to the specified tenant. Side effects are ordered by
/// occurred_at descending (newest first).
///
/// This endpoint provides the query API for compensation planning input.
/// The actual compensation planning/execution is not included in this slice.
async fn list_side_effects(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListSideEffectsQuery>,
) -> Result<Json<ListSideEffectsResponse>, ApiErrorResponse> {
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = side_effects.len();

    Ok(Json(ListSideEffectsResponse {
        side_effects,
        total,
    }))
}

// ============================================================================
// Intent Orchestration Dashboard (Phase 3 Batch 1 bounded read-only slice)
// ============================================================================

/// Query parameters for orchestration dashboard
#[derive(Debug, Deserialize)]
pub struct OrchestrationDashboardQuery {
    pub tenant_id: Uuid,
}

/// Summary counts for compensation actions by status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompensationActionStatusCounts {
    pub pending: usize,
    pub approved: usize,
    pub executed: usize,
    pub failed: usize,
    pub waived: usize,
}

/// Summary of side effects for an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffectSummary {
    pub total: usize,
    pub irreversible_count: usize,
    pub auto_compensatable_count: usize,
}

/// Summary of compensation actions for an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationActionSummary {
    pub total: usize,
    pub status_counts: CompensationActionStatusCounts,
    pub retryable_failed_count: usize,
    pub dlq_candidate_count: usize,
    pub reapprovable_count: usize,
    pub auto_executable_count: usize,
}

/// Response for the intent orchestration dashboard endpoint
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns a consolidated view
/// of side effects and compensation actions for a single intent within a tenant.
///
/// **This endpoint is READ-ONLY** - it does not trigger any mutations.
/// It only queries existing data and computes summary statistics.
///
/// **Summary fields are truthful:**
/// - `side_effect_summary` counts are derived from persisted side effects
/// - `compensation_action_summary` counts are derived from persisted compensation actions
/// - No batch execution, orchestration engine, or background processing is claimed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationDashboardResponse {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub side_effects: Vec<compensation_service::SideEffect>,
    pub side_effect_summary: SideEffectSummary,
    pub compensation_actions: Vec<compensation_service::CompensationAction>,
    pub compensation_action_summary: CompensationActionSummary,
}

/// GET /intents/{intent_id}/orchestration-dashboard - Get orchestration dashboard for an intent
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns a consolidated view
/// of side effects and compensation actions for a single intent within a tenant.
///
/// **This endpoint is READ-ONLY** - it does not trigger compensation execution,
/// approval workflows, or any mutation. It only queries existing compensation
/// action records and side effects, then computes summary statistics.
///
/// **Truthful summary fields:**
/// - `side_effect_summary.total`: count of all side effects for this intent
/// - `side_effect_summary.irreversible_count`: count of S4Irreversible side effects
/// - `side_effect_summary.auto_compensatable_count`: count of S0/S1 side effects
/// - `compensation_action_summary.status_counts.*`: count by CompensationStatus
/// - `compensation_action_summary.retryable_failed_count`: Failed actions with retryable errors
/// - `compensation_action_summary.dlq_candidate_count`: Failed + exhausted budget OR non-retryable error
/// - `compensation_action_summary.reapprovable_count`: Failed + retryable error + remaining budget
/// - `compensation_action_summary.auto_executable_count`: Approved + Automatic feasibility
///
/// **No batch execution or orchestration engine claims:**
/// This endpoint only aggregates existing persisted data. It does not execute
/// any compensation actions, trigger workflows, or involve background processing.
async fn get_orchestration_dashboard(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationDashboardQuery>,
) -> Result<Json<OrchestrationDashboardResponse>, ApiErrorResponse> {
    // Fetch side effects for this intent
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Fetch compensation actions for this intent
    let compensation_actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Compute side effect summary
    let side_effect_summary = {
        let total = side_effects.len();
        let irreversible_count = side_effects
            .iter()
            .filter(|se| se.effect_class == compensation_service::SideEffectClass::S4Irreversible)
            .count();
        let auto_compensatable_count = side_effects
            .iter()
            .filter(|se| se.is_auto_compensatable())
            .count();
        SideEffectSummary {
            total,
            irreversible_count,
            auto_compensatable_count,
        }
    };

    // Compute compensation action summary
    let compensation_action_summary = {
        let total = compensation_actions.len();

        // Count by status
        let mut status_counts = CompensationActionStatusCounts::default();
        for action in &compensation_actions {
            match action.status {
                compensation_service::CompensationStatus::Pending => status_counts.pending += 1,
                compensation_service::CompensationStatus::Approved => status_counts.approved += 1,
                compensation_service::CompensationStatus::Executed => status_counts.executed += 1,
                compensation_service::CompensationStatus::Failed => status_counts.failed += 1,
                compensation_service::CompensationStatus::Waived => status_counts.waived += 1,
            }
        }

        // Count retryable failed (Failed + retryable error code)
        let retryable_failed_count = compensation_actions
            .iter()
            .filter(|action| {
                if action.status != compensation_service::CompensationStatus::Failed {
                    return false;
                }
                // Check if error is retryable
                if let Some(ref result) = action.execution_result_payload {
                    if let Some(ref error_code) = result.error_code {
                        let classification =
                            compensation_service::CompensationAction::classify_error_code(
                                error_code,
                            );
                        return classification.retryable
                            == compensation_service::RetryableErrorClass::Retryable;
                    }
                }
                false
            })
            .count();

        // Count DLQ candidates (Failed + exhausted OR non-retryable)
        let dlq_candidate_count = compensation_actions
            .iter()
            .filter(|action| action.is_dlq_candidate())
            .count();

        // Count reapprovable (Failed + retryable error + remaining budget)
        let reapprovable_count = compensation_actions
            .iter()
            .filter(|action| action.can_be_reapproved())
            .count();

        // Count service-executable (Approved + service-executable: Rollback+Automatic or CounterAction+SemiAutomatic)
        let auto_executable_count = compensation_actions
            .iter()
            .filter(|action| {
                action.status == compensation_service::CompensationStatus::Approved
                    && action.is_service_executable()
            })
            .count();

        CompensationActionSummary {
            total,
            status_counts,
            retryable_failed_count,
            dlq_candidate_count,
            reapprovable_count,
            auto_executable_count,
        }
    };

    Ok(Json(OrchestrationDashboardResponse {
        intent_id,
        tenant_id: query.tenant_id,
        side_effects,
        side_effect_summary,
        compensation_actions,
        compensation_action_summary,
    }))
}

// ============================================================================
// Compensation Action Handlers (Phase 3 Batch 1 bounded execution slice)
// ============================================================================

/// Query parameters for listing compensation actions
#[derive(Debug, Deserialize)]
pub struct ListCompensationActionsQuery {
    pub tenant_id: Uuid,
}

/// Response for listing compensation actions
#[derive(Debug, Serialize)]
pub struct ListCompensationActionsResponse {
    pub compensation_actions: Vec<compensation_service::CompensationAction>,
    pub total: usize,
}

/// GET /intents/{intent_id}/compensation-actions - List compensation actions for an intent
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns all compensation actions
/// recorded for the given intent, scoped to the specified tenant. Actions are
/// ordered by generated_at descending (newest first).
///
/// **This endpoint is READ-ONLY** - it does not trigger compensation execution,
/// approval workflows, or any mutation. It only queries existing compensation
/// action records.
///
/// **Planner vs Executor distinction:**
/// - This endpoint returns actual compensation action records stored via the
///   compensation action service/repository
/// - The `compensation_planning` field in rebase-preview/apply responses shows
///   planner-generated skeleton/preview data (not stored records)
/// - Full compensation execution (executor trigger) is Batch 1+ scope
async fn list_compensation_actions(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListCompensationActionsQuery>,
) -> Result<Json<ListCompensationActionsResponse>, ApiErrorResponse> {
    let actions = state
        .compensation_action_service
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = actions.len();

    Ok(Json(ListCompensationActionsResponse {
        compensation_actions: actions,
        total,
    }))
}

/// Request body for approve compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct ApproveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
    /// Optional actor who approved (for audit purposes)
    #[serde(default)]
    pub approved_by: Option<String>,
}

/// Request body for waive compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct WaiveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
    /// Optional actor who waived (for audit purposes)
    #[serde(default)]
    pub waived_by: Option<String>,
}

/// Request body for execute compensation action
#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteCompensationActionBody {
    /// Optional actor who executed (for audit purposes)
    #[serde(default)]
    pub executed_by: Option<String>,
}

/// Response for compensation action mutation (approve/waive/execute)
#[derive(Debug, Clone, Serialize)]
pub struct CompensationActionResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub status: String,
    pub strategy_type: String,
    pub feasibility: String,
    pub rationale: String,
    pub attempt_count: i32,
    pub lock_version: i32,
    pub approved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub approved_by: Option<String>,
    pub waived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub waived_by: Option<String>,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub executed_by: Option<String>,
    pub failed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub execution_result_payload: Option<serde_json::Value>,
}

impl From<compensation_service::CompensationAction> for CompensationActionResponse {
    fn from(action: compensation_service::CompensationAction) -> Self {
        // Use serde_json to serialize enum fields to snake_case strings
        // instead of Debug formatting (which produces PascalCase).
        // serde_json::to_string returns the JSON representation including quotes,
        // so we trim the surrounding quotes.
        fn to_snake_case_string<T: serde::Serialize>(val: &T) -> String {
            serde_json::to_string(val)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default()
        }

        Self {
            id: action.id,
            tenant_id: action.tenant_id,
            intent_id: action.intent_id,
            status: to_snake_case_string(&action.status),
            strategy_type: to_snake_case_string(&action.strategy_type),
            feasibility: to_snake_case_string(&action.feasibility),
            rationale: action.rationale,
            attempt_count: action.attempt_count,
            lock_version: action.lock_version,
            approved_at: action.approved_at,
            approved_by: action.approved_by,
            waived_at: action.waived_at,
            waived_by: action.waived_by,
            executed_at: action.executed_at,
            executed_by: action.executed_by,
            failed_at: action.failed_at,
            execution_result_payload: action
                .execution_result_payload
                .map(|r| serde_json::to_value(&r).unwrap_or_else(|_| serde_json::json!({}))),
        }
    }
}

/// POST /compensation-actions/{action_id}/approve - Approve a pending compensation action
///
/// Phase 3 Batch 1 (bounded execution slice): Transitions a Pending compensation action
/// to Approved status, enabling it to be executed.
///
/// **Transition rules:**
/// - Only Pending actions can be approved
/// - Uses optimistic locking via lock_version to prevent concurrent updates
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Pending
/// - Returns 409 Conflict if lock_version doesn't match
///
/// **Executor gate:** Approved actions can be executed via POST /compensation-actions/{action_id}/execute
async fn approve_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ApproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .approve_action(action_id, body.lock_version, body.approved_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/waive - Waive a pending compensation action
///
/// Phase 3 Batch 1 (bounded execution slice): Transitions a Pending compensation action
/// to Waived status, marking it as intentionally skipped.
///
/// **Transition rules:**
/// - Only Pending actions can be waived
/// - Uses optimistic locking via lock_version to prevent concurrent updates
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Pending
/// - Returns 409 Conflict if lock_version doesn't match
///
/// **This slice:** Waived actions are terminal. No reactivation path exists.
async fn waive_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<WaiveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .waive_action(action_id, body.lock_version, body.waived_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/execute - Execute an approved compensation action
///
/// Phase 3 Batch 1 (bounded execution slice): Executes an Approved compensation action
/// using the compensation executor.
///
/// **Executor gate:** Only Approved actions can execute. This prevents accidental
/// execution of pending or already-processed actions.
///
/// **Execution policy gate:** Only service-executable combos can execute:
/// - Rollback + Automatic feasibility (S1InternalReversible)
/// - CounterAction + SemiAutomatic feasibility (S2ExternalReversible)
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Approved
///
/// **This slice:** No retry logic; Failed actions remain Failed.
async fn execute_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ExecuteCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .execute_action(action_id, body.executed_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// Request body for reapprove compensation action (manual retry)
#[derive(Debug, Clone, Deserialize)]
pub struct ReapproveCompensationActionBody {
    /// Lock version for optimistic concurrency control
    pub lock_version: i32,
}

/// Response for listing DLQ candidates
#[derive(Debug, Clone, Serialize)]
pub struct ListDlqCandidatesResponse {
    pub dlq_candidates: Vec<compensation_service::CompensationAction>,
    pub total: usize,
}

/// Response for listing batch candidates across all categories
#[derive(Debug, Clone, Serialize)]
pub struct ListBatchCandidatesResponse {
    /// Actions in Pending status awaiting approval
    pub pending_approval_candidates: Vec<compensation_service::CompensationAction>,
    /// Approved actions with Service-executable feasibility that can be service-executed
    pub approved_service_executable_candidates: Vec<compensation_service::CompensationAction>,
    /// Failed actions that can be reapproved (retryable error + budget remains)
    pub retryable_failed_candidates: Vec<compensation_service::CompensationAction>,
    /// Failed actions that exhausted retry budget or have non-retryable errors
    pub dlq_candidates: Vec<compensation_service::CompensationAction>,
    /// Summary counts for each category
    pub summary: BatchCandidatesSummary,
}

/// Summary counts for batch candidate categories
#[derive(Debug, Clone, Serialize)]
pub struct BatchCandidatesSummary {
    pub pending_approval_count: usize,
    pub approved_service_executable_count: usize,
    pub retryable_failed_count: usize,
    pub dlq_count: usize,
}

/// Query parameters for listing DLQ candidates
#[derive(Debug, Deserialize)]
pub struct ListDlqCandidatesQuery {
    pub tenant_id: Uuid,
}

/// GET /compensation-actions/dlq - List DLQ (Dead Letter Queue) candidates
///
/// Phase 3 Batch 1 (bounded manual retry slice): Returns all compensation
/// actions that are DLQ candidates.
///
/// **Derived DLQ condition:** An action is a DLQ candidate when:
/// 1. Status is Failed AND
/// 2. Either:
///    a. attempt_count >= max_retries (exhausted retry budget), OR
///    b. The error code is non-retryable (permanent failure)
///
/// **No DLQ table:** This is a read-only derived query from existing data.
/// DLQ candidates cannot be reapproved - they represent failures that have
/// exhausted automated retry possibilities.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
/// **Manual intervention is the only path forward for DLQ candidates.**
async fn list_dlq_candidates(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListDlqCandidatesQuery>,
) -> Result<Json<ListDlqCandidatesResponse>, ApiErrorResponse> {
    let dlq_candidates = state
        .compensation_action_service
        .list_dlq_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let total = dlq_candidates.len();

    Ok(Json(ListDlqCandidatesResponse {
        dlq_candidates,
        total,
    }))
}

/// Query parameters for listing batch candidates
#[derive(Debug, Deserialize)]
pub struct ListBatchCandidatesQuery {
    pub tenant_id: Uuid,
}

/// GET /compensation-actions/batch-candidates - List batch candidates across all categories
///
/// Phase 3 Batch 1 (bounded read-only batch candidate queue slice): Returns a
/// consolidated view of all actionable compensation categories for batch processing.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Four candidate categories:**
/// 1. `pending_approval_candidates` - Actions in Pending status awaiting approval
    /// 2. `approved_service_executable_candidates` - Approved actions executable by the service
    ///    Phase 3 Batch 1 P7: Includes both Rollback+Automatic and CounterAction+SemiAutomatic
/// 3. `retryable_failed_candidates` - Failed actions that can be reapproved (retryable error + budget remains)
/// 4. `dlq_candidates` - Failed actions that exhausted retry budget or have non-retryable errors
///
/// **No execution, orchestration, or policy gate:**
/// This is a read-only query endpoint. It does not trigger any mutations,
/// execute any actions, or involve background workers.
///
/// **Tenant-scoped:** Results are filtered by the provided tenant_id.
async fn list_batch_candidates(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListBatchCandidatesQuery>,
) -> Result<Json<ListBatchCandidatesResponse>, ApiErrorResponse> {
    let batch = state
        .compensation_action_service
        .list_batch_candidates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let summary = BatchCandidatesSummary {
        pending_approval_count: batch.pending_approval_candidates.len(),
        approved_service_executable_count: batch.approved_service_executable_candidates.len(),
        retryable_failed_count: batch.retryable_failed_candidates.len(),
        dlq_count: batch.dlq_candidates.len(),
    };

    Ok(Json(ListBatchCandidatesResponse {
        pending_approval_candidates: batch.pending_approval_candidates,
        approved_service_executable_candidates: batch.approved_service_executable_candidates,
        retryable_failed_candidates: batch.retryable_failed_candidates,
        dlq_candidates: batch.dlq_candidates,
        summary,
    }))
}

/// POST /compensation-actions/{action_id}/reapprove - Manually reapprove a failed action
///
/// Phase 3 Batch 1 (bounded manual retry slice): Transitions a Failed compensation
/// action back to Pending status, enabling it to be approved and executed again.
///
/// **Policy gates (fail closed):**
/// - Action must be in Failed status
/// - Action must have remaining retry budget (attempt_count < max_retries)
/// - Error code must be retryable (not a permanent failure)
///
/// **Fails closed when:**
/// - Action is not in Failed status → 409 Conflict
/// - Retry budget exhausted → 409 Conflict
/// - Error is non-retryable → 409 Conflict
/// - Optimistic lock conflict → 409 Conflict
///
/// **Note:** This does NOT reset the attempt_count. The action retains its
/// failure history. Reapproval just allows another execution attempt within
/// the retry budget.
async fn reapprove_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ReapproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .reapprove_action(action_id, body.lock_version)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

// ============================================================================
// Bounded Compensation Planner (Phase 3 bounded planner slice)
// ============================================================================

/// Request body for planning compensation actions from side effects.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanCompensationActionsRequest {
    /// Intent ID to plan compensation for
    pub intent_id: Uuid,
    /// Tenant ID for scoping
    pub tenant_id: Uuid,
    /// Source version before rebase
    pub from_version: i32,
    /// Target version after rebase
    pub to_version: i32,
    /// Workflow ID that initiated the rebase
    pub workflow_id: Uuid,
}

/// Response for compensation action planning.
#[derive(Debug, Clone, Serialize)]
pub struct PlanCompensationActionsResponse {
    /// Generated compensation actions
    pub actions: Vec<CompensationActionResponse>,
    /// Total count of generated actions
    pub total: usize,
    /// Count by feasibility level
    pub feasibility_counts: FeasibilityCounts,
}

/// Counts of actions by feasibility level.
#[derive(Debug, Clone, Serialize)]
pub struct FeasibilityCounts {
    pub automatic: usize,
    pub semi_automatic: usize,
    pub manual_only: usize,
    pub not_possible: usize,
}

/// POST /compensation-actions/plan - Plan compensation actions from side effects
///
/// Phase 3 (bounded planner slice): Fetches side effects for the given intent,
/// classifies them using S0-S4 classification, and generates appropriate
/// compensation actions.
///
/// **S0-S4 classification:**
/// | Class | Strategy | Feasibility | Action |
/// |-------|----------|-------------|--------|
/// | S0PureRead | (none) | NotPossible | Skip - no action needed |
/// | S1InternalReversible | Rollback | Automatic | Auto rollback |
/// | S2ExternalReversible | CounterAction | SemiAutomatic | Counter-action with manual trigger |
/// | S3ExternalPartiallyReversible | FollowupNotice | ManualOnly | Manual followup required |
/// | S4Irreversible | Escalation | NotPossible | Escalation required |
///
/// **Returns:** All generated compensation actions (S0 produces no action).
async fn plan_compensation_actions(
    State(state): State<AppState>,
    Json(request): Json<PlanCompensationActionsRequest>,
) -> Result<Json<PlanCompensationActionsResponse>, ApiErrorResponse> {
    let rebase_context = compensation_service::RebaseContext::new(
        request.intent_id,
        request.from_version,
        request.to_version,
        request.workflow_id,
    );

    let actions = state
        .compensation_action_service
        .plan_compensation_actions(request.intent_id, request.tenant_id, rebase_context)
        .await
        .map_err(ApiErrorResponse)?;

    // Count by feasibility
    let mut feasibility_counts = FeasibilityCounts {
        automatic: 0,
        semi_automatic: 0,
        manual_only: 0,
        not_possible: 0,
    };

    for action in &actions {
        match action.feasibility {
            compensation_service::CompensationFeasibility::Automatic => {
                feasibility_counts.automatic += 1
            }
            compensation_service::CompensationFeasibility::SemiAutomatic => {
                feasibility_counts.semi_automatic += 1
            }
            compensation_service::CompensationFeasibility::ManualOnly => {
                feasibility_counts.manual_only += 1
            }
            compensation_service::CompensationFeasibility::NotPossible => {
                feasibility_counts.not_possible += 1
            }
        }
    }

    let total = actions.len();
    let action_responses: Vec<CompensationActionResponse> = actions
        .into_iter()
        .map(CompensationActionResponse::from)
        .collect();

    Ok(Json(PlanCompensationActionsResponse {
        actions: action_responses,
        total,
        feasibility_counts,
    }))
}

// ============================================================================
// Orchestration Run Handlers (Phase 3 Batch 1 bounded single-shot HTTP slice)
// ============================================================================

/// Request body for creating an orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrchestrationRunRequest {
    /// List of compensation action IDs to process in this run.
    pub action_ids: Vec<Uuid>,
    /// Optional intent scope for this run.
    #[serde(default)]
    pub intent_id: Option<Uuid>,
    /// Optional actor who initiated this run (for audit purposes).
    #[serde(default)]
    pub initiated_by: Option<String>,
}

/// Query parameters for getting/listing orchestration runs.
#[derive(Debug, Deserialize)]
pub struct OrchestrationRunQuery {
    pub tenant_id: Uuid,
}

/// Response for an orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationRunResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub action_ids: Vec<Uuid>,
    pub status: String,
    pub initiated_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub not_found_count: usize,
    pub total_count: usize,
    pub item_results: Vec<RunItemResultResponse>,
}

/// Per-item result within a run (API version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunItemResultResponse {
    pub action_id: Uuid,
    pub action_taken: String,
    pub success: bool,
    pub reason: String,
    pub resulting_status: String,
}

impl From<compensation_service::OrchestrationRun> for OrchestrationRunResponse {
    fn from(run: compensation_service::OrchestrationRun) -> Self {
        Self {
            id: run.id,
            tenant_id: run.tenant_id,
            intent_id: run.intent_id,
            action_ids: run.action_ids,
            status: format_run_status(&run.status),
            initiated_by: run.initiated_by,
            created_at: run.created_at,
            started_at: run.started_at,
            completed_at: run.completed_at,
            succeeded_count: run.succeeded_count,
            failed_count: run.failed_count,
            skipped_count: run.skipped_count,
            not_found_count: run.not_found_count,
            total_count: run.total_count,
            item_results: run
                .item_results
                .into_iter()
                .map(|r| RunItemResultResponse {
                    action_id: r.action_id,
                    action_taken: format_action_decision(&r.action_taken),
                    success: r.success,
                    reason: r.reason,
                    resulting_status: r.resulting_status,
                })
                .collect(),
        }
    }
}

fn format_run_status(s: &compensation_service::RunStatus) -> String {
    match s {
        compensation_service::RunStatus::Pending => "pending".to_string(),
        compensation_service::RunStatus::Running => "running".to_string(),
        compensation_service::RunStatus::Completed => "completed".to_string(),
        compensation_service::RunStatus::CompletedWithErrors => "completed_with_errors".to_string(),
        compensation_service::RunStatus::Failed => "failed".to_string(),
    }
}

fn format_action_decision(d: &compensation_service::OrchestrationActionDecision) -> String {
    match d {
        compensation_service::OrchestrationActionDecision::Approve => "approve".to_string(),
        compensation_service::OrchestrationActionDecision::Reapprove => "reapprove".to_string(),
        compensation_service::OrchestrationActionDecision::Execute => "execute".to_string(),
        compensation_service::OrchestrationActionDecision::Skip => "skip".to_string(),
        compensation_service::OrchestrationActionDecision::NotFound => "not_found".to_string(),
    }
}

/// POST /compensation-actions/runs - Create and execute a single-shot orchestration run
///
/// Phase 3 Batch 1 (bounded single-shot HTTP orchestration slice):
/// Creates a new orchestration run for the given compensation action IDs,
/// persists it in Pending state, and returns HTTP 202 immediately with the
/// run handle. Execution proceeds in the background via execute_existing_run.
///
/// **Bounded single-shot semantics:**
/// - Single-shot: one run = one explicit action list, one auto-decide pass
/// - HTTP accepted: returns immediately with a persisted run handle (HTTP 202)
/// - Background execution: run executes asynchronously after response is sent
/// - Runtime auto-decides per action: approve | reapprove | execute | skip
/// - Uses existing CompensationActionService methods (does not replace enforcement)
/// - Partial-success: continues on per-item failures, records all outcomes
///
/// **Auto-decide logic:**
/// - `Pending` → approve via approve_action
/// - `Failed` (can_be_reapproved) → reapprove via reapprove_action
    /// - `Approved` (is_service_executable: Rollback+Automatic or CounterAction+SemiAutomatic) → execute via execute_action
/// - Terminal / policy-blocked → skip
/// - Not found → record not_found
///
/// **Run status (HTTP 202 Accepted):**
/// - `pending` → run created, awaiting execution
/// - `running` → run is executing
/// - `completed` → all actions succeeded
/// - `completed_with_errors` → some actions failed
/// - `failed` → all actions failed or system error
async fn create_orchestration_run(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationRunQuery>,
    Json(request): Json<CreateOrchestrationRunRequest>,
) -> Result<(StatusCode, Json<OrchestrationRunResponse>), ApiErrorResponse> {
    // Step 1: Create run in Pending state and return 202 immediately
    let run = state
        .orchestration_runtime
        .create_run(
            query.tenant_id,
            request.action_ids,
            request.initiated_by,
            request.intent_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    let run_id = run.id;

    // Step 2: Spawn background execution
    // The run handle is already returned to the client; execution proceeds in the background.
    let runtime = state.orchestration_runtime.clone();
    tokio::spawn(async move {
        // Background execution; errors are logged but cannot be reported to the HTTP client
        match runtime.execute_existing_run(run_id).await {
            Ok(_) => {
                tracing::debug!("Background orchestration run {} completed", run_id);
            }
            Err(e) => {
                tracing::error!("Background orchestration run {} failed: {}", run_id, e);
            }
        }
    });

    // Return 202 Accepted with the persisted (pending) run handle immediately
    Ok((
        StatusCode::ACCEPTED,
        Json(OrchestrationRunResponse::from(run)),
    ))
}

/// GET /compensation-actions/runs/{run_id} - Get an orchestration run by ID
///
/// Phase 3 Batch 1 (bounded single-shot HTTP orchestration slice):
/// Returns the run including its current status, counts, and per-item results.
async fn get_orchestration_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationRunQuery>,
) -> Result<Json<OrchestrationRunResponse>, ApiErrorResponse> {
    let run = state
        .orchestration_runtime
        .get_run(run_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Verify tenant ownership
    if run.tenant_id != query.tenant_id {
        return Err(ApiErrorResponse(
            IntentRebaseError::OrchestrationRunNotFound(run_id),
        ));
    }

    Ok(Json(OrchestrationRunResponse::from(run)))
}

// ============================================================================
// Policy Gate Evaluation (Phase 3 Batch 1 bounded read-only slice)
// ============================================================================

/// Query parameters for tenant-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct CompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct IntentCompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// Response DTOs for policy gate evaluation
use compensation_service::{
    PolicyGateEvaluation as ServicePolicyGateEvaluation,
    PolicyGateEvaluationResult as ServicePolicyGateEvaluationResult,
    PolicyGateMetadata as ServicePolicyGateMetadata, PolicyGateStatus as ServicePolicyGateStatus,
    PolicyGateSummary as ServicePolicyGateSummary, RiskMetadata as ServiceRiskMetadata,
};

/// API response for policy gate evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationPolicyGateResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub evaluations: Vec<PolicyGateEvaluationResponse>,
    pub summary: PolicyGateSummaryResponse,
}

/// Policy gate evaluation for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateEvaluationResponse {
    pub action: compensation_service::CompensationAction,
    pub gate_status: String,
    pub gate_reason: String,
    pub policy_metadata: PolicyGateMetadataResponse,
    pub risk_metadata: RiskMetadataResponse,
}

/// Policy gate metadata for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateMetadataResponse {
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub has_non_retryable_error: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Risk metadata for a single action (API version)
///
/// Phase 3 Batch 1 (bounded read-only slice): Derived from existing action state fields.
/// Provides risk-relevant signals: strategy severity, retry exhaustion risk, feasibility risk,
/// error severity, remaining retry budget, error classification, terminal state flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetadataResponse {
    pub strategy_severity: String,
    pub retry_exhaustion_risk: String,
    pub feasibility_risk: String,
    pub error_severity: String,
    pub retry_budget_remaining: i32,
    pub error_classification: Option<ErrorClassificationResponse>,
    pub is_terminal: bool,
    pub requires_manual_intervention: bool,
}

/// Error classification response (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorClassificationResponse {
    pub error_code: String,
    pub retryable: bool,
    pub reason: String,
}

/// Summary of policy gate evaluations (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateSummaryResponse {
    pub total_actions: usize,
    pub eligible_count: usize,
    pub blocked_count: usize,
    pub manual_review_required_count: usize,
    pub dlq_candidate_count: usize,
    pub pending_approval_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServicePolicyGateEvaluationResult> for CompensationPolicyGateResponse {
    fn from(result: ServicePolicyGateEvaluationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            evaluations: result
                .evaluations
                .into_iter()
                .map(PolicyGateEvaluationResponse::from)
                .collect(),
            summary: PolicyGateSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServicePolicyGateEvaluation> for PolicyGateEvaluationResponse {
    fn from(eval: ServicePolicyGateEvaluation) -> Self {
        Self {
            action: eval.action,
            gate_status: format_gate_status(&eval.gate_status),
            gate_reason: eval.gate_reason,
            policy_metadata: PolicyGateMetadataResponse::from(eval.policy_metadata),
            risk_metadata: RiskMetadataResponse::from(eval.risk_metadata),
        }
    }
}

impl From<ServicePolicyGateMetadata> for PolicyGateMetadataResponse {
    fn from(meta: ServicePolicyGateMetadata) -> Self {
        Self {
            auto_executable: meta.auto_executable,
            is_dlq_candidate: meta.is_dlq_candidate,
            can_reapprove: meta.can_reapprove,
            retry_budget_exhausted: meta.retry_budget_exhausted,
            has_non_retryable_error: meta.has_non_retryable_error,
            feasibility: format_feasibility(&meta.feasibility),
            strategy_type: format_strategy_type(&meta.strategy_type),
            status: format_compensation_status(&meta.status),
            attempt_count: meta.attempt_count,
            max_retries: meta.max_retries,
        }
    }
}

impl From<ServicePolicyGateSummary> for PolicyGateSummaryResponse {
    fn from(summary: ServicePolicyGateSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            eligible_count: summary.eligible_count,
            blocked_count: summary.blocked_count,
            manual_review_required_count: summary.manual_review_required_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            pending_approval_count: summary.pending_approval_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}

impl From<ServiceRiskMetadata> for RiskMetadataResponse {
    fn from(risk: ServiceRiskMetadata) -> Self {
        Self {
            strategy_severity: format_strategy_severity(&risk.strategy_severity),
            retry_exhaustion_risk: format_retry_exhaustion_risk(&risk.retry_exhaustion_risk),
            feasibility_risk: format_feasibility_risk(&risk.feasibility_risk),
            error_severity: format_error_severity(&risk.error_severity),
            retry_budget_remaining: risk.retry_budget_remaining,
            error_classification: risk
                .error_classification
                .map(ErrorClassificationResponse::from),
            is_terminal: risk.is_terminal,
            requires_manual_intervention: risk.requires_manual_intervention,
        }
    }
}

impl From<compensation_service::ErrorClassification> for ErrorClassificationResponse {
    fn from(ec: compensation_service::ErrorClassification) -> Self {
        Self {
            error_code: ec.error_code,
            retryable: ec.retryable,
            reason: ec.reason,
        }
    }
}

fn format_gate_status(status: &ServicePolicyGateStatus) -> String {
    match status {
        ServicePolicyGateStatus::Eligible => "eligible".to_string(),
        ServicePolicyGateStatus::Blocked => "blocked".to_string(),
        ServicePolicyGateStatus::ManualReviewRequired => "manual_review_required".to_string(),
    }
}

fn format_feasibility(f: &compensation_service::CompensationFeasibility) -> String {
    match f {
        compensation_service::CompensationFeasibility::Automatic => "automatic".to_string(),
        compensation_service::CompensationFeasibility::SemiAutomatic => {
            "semi_automatic".to_string()
        }
        compensation_service::CompensationFeasibility::ManualOnly => "manual_only".to_string(),
        compensation_service::CompensationFeasibility::NotPossible => "not_possible".to_string(),
    }
}

fn format_strategy_type(s: &compensation_service::StrategyType) -> String {
    match s {
        compensation_service::StrategyType::Rollback => "rollback".to_string(),
        compensation_service::StrategyType::CounterAction => "counter_action".to_string(),
        compensation_service::StrategyType::FollowupNotice => "followup_notice".to_string(),
        compensation_service::StrategyType::Quarantine => "quarantine".to_string(),
        compensation_service::StrategyType::Escalation => "escalation".to_string(),
    }
}

fn format_compensation_status(s: &compensation_service::CompensationStatus) -> String {
    match s {
        compensation_service::CompensationStatus::Pending => "pending".to_string(),
        compensation_service::CompensationStatus::Approved => "approved".to_string(),
        compensation_service::CompensationStatus::Executed => "executed".to_string(),
        compensation_service::CompensationStatus::Failed => "failed".to_string(),
        compensation_service::CompensationStatus::Waived => "waived".to_string(),
    }
}

fn format_strategy_severity(s: &compensation_service::StrategySeverity) -> String {
    match s {
        compensation_service::StrategySeverity::Low => "low".to_string(),
        compensation_service::StrategySeverity::Medium => "medium".to_string(),
        compensation_service::StrategySeverity::High => "high".to_string(),
        compensation_service::StrategySeverity::Critical => "critical".to_string(),
    }
}

fn format_retry_exhaustion_risk(r: &compensation_service::RetryExhaustionRisk) -> String {
    match r {
        compensation_service::RetryExhaustionRisk::Low => "low".to_string(),
        compensation_service::RetryExhaustionRisk::Medium => "medium".to_string(),
        compensation_service::RetryExhaustionRisk::High => "high".to_string(),
        compensation_service::RetryExhaustionRisk::Critical => "critical".to_string(),
    }
}

fn format_feasibility_risk(f: &compensation_service::FeasibilityRisk) -> String {
    match f {
        compensation_service::FeasibilityRisk::Low => "low".to_string(),
        compensation_service::FeasibilityRisk::Medium => "medium".to_string(),
        compensation_service::FeasibilityRisk::High => "high".to_string(),
        compensation_service::FeasibilityRisk::Critical => "critical".to_string(),
    }
}

fn format_error_severity(e: &compensation_service::ErrorSeverity) -> String {
    match e {
        compensation_service::ErrorSeverity::None => "none".to_string(),
        compensation_service::ErrorSeverity::Low => "low".to_string(),
        compensation_service::ErrorSeverity::Medium => "medium".to_string(),
        compensation_service::ErrorSeverity::High => "high".to_string(),
    }
}

/// GET /compensation-actions/policy-gate - Tenant-scoped policy gate evaluation
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
/// for all compensation actions belonging to the specified tenant.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Canonical gate statuses:**
/// - `eligible`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
/// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget, terminal status)
/// - `manual_review_required`: Needs human intervention (Pending, SemiAutomatic/ManualOnly feasibility)
///
/// **Gate evaluation is derived from existing surfaces:**
/// - Status, feasibility, attempt_count, max_retries, error_code fields
/// - No new policy engine or external risk surface is queried
///
/// **Response includes:**
/// - Individual action evaluations with gate outcome and reason
/// - Summary counts by gate status
/// - Policy/risk metadata useful for UI display
async fn get_compensation_policy_gate(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<CompensationPolicyGateQuery>,
) -> Result<Json<CompensationPolicyGateResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .evaluate_policy_gates(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let mut response = CompensationPolicyGateResponse::from(result);
    response.tenant_id = query.tenant_id;

    Ok(Json(response))
}

/// GET /intents/{intent_id}/compensation-policy-gate - Intent-scoped policy gate evaluation
///
/// Phase 3 Batch 1 (bounded read-only slice): Returns policy gate evaluations
/// for all compensation actions belonging to the specified intent.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Canonical gate statuses:**
/// - `eligible`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
/// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget, terminal status)
/// - `manual_review_required`: Needs human intervention (Pending, SemiAutomatic/ManualOnly feasibility)
///
/// **Gate evaluation is derived from existing surfaces:**
/// - Status, feasibility, attempt_count, max_retries, error_code fields
/// - No new policy engine or external risk surface is queried
///
/// **Response includes:**
/// - Individual action evaluations with gate outcome and reason
/// - Summary counts by gate status
/// - Policy/risk metadata useful for UI display
async fn get_intent_compensation_policy_gate(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<IntentCompensationPolicyGateQuery>,
) -> Result<Json<CompensationPolicyGateResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .evaluate_policy_gates_for_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let mut response = CompensationPolicyGateResponse::from(result);
    response.tenant_id = query.tenant_id;
    response.intent_id = Some(intent_id);

    Ok(Json(response))
}

// ============================================================================
// Orchestration Coordination Status (Phase 3 Batch 1 bounded read-only view)
// ============================================================================

/// Query parameters for tenant-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct OrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct IntentOrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// Response DTOs for orchestration coordination status
use compensation_service::{
    CoordinationRecord as ServiceCoordinationRecord,
    CoordinationResult as ServiceCoordinationResult,
    CoordinationStatus as ServiceCoordinationStatus,
    CoordinationSummary as ServiceCoordinationSummary,
};

/// API response for orchestration coordination status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationCoordinationResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub records: Vec<CoordinationRecordResponse>,
    pub summary: CoordinationSummaryResponse,
}

/// Coordination record for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationRecordResponse {
    pub action: compensation_service::CompensationAction,
    pub coordination_status: String,
    pub coordination_reason: String,
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Summary of coordination records (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationSummaryResponse {
    pub total_actions: usize,
    pub ready_count: usize,
    pub awaiting_policy_count: usize,
    pub awaiting_manual_review_count: usize,
    pub blocked_count: usize,
    pub terminal_count: usize,
    pub dlq_candidate_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServiceCoordinationResult> for OrchestrationCoordinationResponse {
    fn from(result: ServiceCoordinationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            records: result
                .records
                .into_iter()
                .map(CoordinationRecordResponse::from)
                .collect(),
            summary: CoordinationSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServiceCoordinationRecord> for CoordinationRecordResponse {
    fn from(record: ServiceCoordinationRecord) -> Self {
        Self {
            action: record.action,
            coordination_status: format_coordination_status(&record.coordination_status),
            coordination_reason: record.coordination_reason,
            auto_executable: record.auto_executable,
            is_dlq_candidate: record.is_dlq_candidate,
            can_reapprove: record.can_reapprove,
            retry_budget_exhausted: record.retry_budget_exhausted,
            feasibility: format_feasibility(&record.feasibility),
            strategy_type: format_strategy_type(&record.strategy_type),
            status: format_compensation_status(&record.status),
            attempt_count: record.attempt_count,
            max_retries: record.max_retries,
        }
    }
}

impl From<ServiceCoordinationSummary> for CoordinationSummaryResponse {
    fn from(summary: ServiceCoordinationSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            ready_count: summary.ready_count,
            awaiting_policy_count: summary.awaiting_policy_count,
            awaiting_manual_review_count: summary.awaiting_manual_review_count,
            blocked_count: summary.blocked_count,
            terminal_count: summary.terminal_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}

fn format_coordination_status(status: &ServiceCoordinationStatus) -> String {
    match status {
        ServiceCoordinationStatus::Ready => "ready".to_string(),
        ServiceCoordinationStatus::AwaitingPolicy => "awaiting_policy".to_string(),
        ServiceCoordinationStatus::AwaitingManualReview => "awaiting_manual_review".to_string(),
        ServiceCoordinationStatus::Blocked => "blocked".to_string(),
        ServiceCoordinationStatus::Terminal => "terminal".to_string(),
    }
}

/// GET /compensation-actions/orchestration-coordination - Tenant-scoped orchestration coordination status
///
/// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
/// coordination status for all compensation actions belonging to the specified tenant.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Canonical coordination statuses:**
/// - `ready`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
/// - `awaiting_policy`: Action awaits policy approval (Pending status)
/// - `awaiting_manual_review`: Action requires human intervention
/// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget)
/// - `terminal`: Action has reached terminal state (Executed, Waived)
///
/// **Response includes:**
/// - Per-item coordination records with status, reason, and action details
/// - Summary counts by coordination status
///
/// **Derivation:** Coordination status is derived from existing CompensationAction fields
/// at query time. No new orchestration engine is queried.
async fn get_orchestration_coordination(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationCoordinationQuery>,
) -> Result<Json<OrchestrationCoordinationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .evaluate_coordination_status(query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let mut response = OrchestrationCoordinationResponse::from(result);
    response.tenant_id = query.tenant_id;

    Ok(Json(response))
}

/// GET /intents/{intent_id}/orchestration-coordination - Intent-scoped orchestration coordination status
///
/// Phase 3 Batch 1 (bounded read-only orchestration coordination view): Returns
/// coordination status for all compensation actions belonging to the specified intent.
///
/// **This endpoint is READ-ONLY** - it only queries existing data.
///
/// **Canonical coordination statuses:**
/// - `ready`: Action can proceed (Approved + Automatic feasibility + no blocking conditions)
/// - `awaiting_policy`: Action awaits policy approval (Pending status)
/// - `awaiting_manual_review`: Action requires human intervention
/// - `blocked`: Action cannot proceed (DLQ, non-retryable error, exhausted budget)
/// - `terminal`: Action has reached terminal state (Executed, Waived)
///
/// **Response includes:**
/// - Per-item coordination records with status, reason, and action details
/// - Summary counts by coordination status
///
/// **Derivation:** Coordination status is derived from existing CompensationAction fields
/// at query time. No new orchestration engine is queried.
async fn get_intent_orchestration_coordination(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<IntentOrchestrationCoordinationQuery>,
) -> Result<Json<OrchestrationCoordinationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .evaluate_coordination_status_for_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let mut response = OrchestrationCoordinationResponse::from(result);
    response.tenant_id = query.tenant_id;
    response.intent_id = Some(intent_id);

    Ok(Json(response))
}

// ============================================================================
// Manual Orchestration & Dry-Run Planner (Phase 3 Batch 1 bounded slice)
// ============================================================================

/// Request body for dry-run orchestration action planning.
#[derive(Debug, Clone, Deserialize)]
pub struct OrchestrationDryRunRequest {
    /// List of compensation action IDs to plan for
    pub action_ids: Vec<Uuid>,
}

/// Response for dry-run orchestration action planning.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunResponse {
    /// Per-item proposals
    pub proposals: Vec<OrchestrationDryRunProposalResponse>,
    /// Actions that were not found
    pub not_found: Vec<uuid::Uuid>,
    /// Summary counts
    pub summary: OrchestrationDryRunSummaryResponse,
}

/// A single proposal from the dry-run planner.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunProposalResponse {
    /// The compensation action ID
    pub action_id: Uuid,
    /// The proposed action (approve | reapprove | execute | no_action)
    pub proposed_action: String,
    /// Human-readable reason for the proposal
    pub reason: String,
    /// Current status of the action
    pub current_status: String,
}

/// Summary for dry-run results.
#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationDryRunSummaryResponse {
    pub total: usize,
    pub can_approve: usize,
    pub can_reapprove: usize,
    pub can_execute: usize,
    pub no_action: usize,
    pub not_found: usize,
}

/// Request body for batch orchestration commands.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchOrchestrationRequest {
    /// List of compensation action IDs to process
    pub action_ids: Vec<Uuid>,
    /// Optional actor who initiated the batch (for audit purposes)
    #[serde(default)]
    pub initiated_by: Option<String>,
}

/// Response for batch orchestration commands.
#[derive(Debug, Clone, Serialize)]
pub struct BatchOrchestrationResponse {
    /// Per-item outcomes
    pub outcomes: Vec<BatchItemOutcomeResponse>,
    /// Actions that were not found
    pub not_found: Vec<uuid::Uuid>,
    /// Summary counts
    pub summary: BatchOrchestrationSummaryResponse,
}

/// A single item outcome from a batched command.
#[derive(Debug, Clone, Serialize)]
pub struct BatchItemOutcomeResponse {
    /// The compensation action ID
    pub action_id: Uuid,
    /// Whether this item succeeded
    pub success: bool,
    /// The resulting action (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<CompensationActionResponse>,
    /// The error that occurred (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Summary for batched orchestration results.
#[derive(Debug, Clone, Serialize)]
pub struct BatchOrchestrationSummaryResponse {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub not_found: usize,
}

/// Query parameters for manual orchestration endpoints
#[derive(Debug, Deserialize)]
pub struct OrchestrationQuery {
    pub tenant_id: Uuid,
}

/// POST /compensation-actions/orchestration-dry-run - Plan orchestration actions (dry-run)
///
/// Phase 3 Batch 1 (bounded dry-run slice): For each provided compensation_action_id,
/// determines the proposed action (approve | reapprove | execute | no_action) based
/// on the action's current state.
///
/// **This is READ-ONLY** - it does not execute any actions.
///
/// **Action determination logic:**
/// - `approve`: Action is Pending (can transition to Approved)
/// - `reapprove`: Action is Failed AND can_be_reapproved() (retryable error + budget remains)
    /// - `execute`: Action is Approved AND is_service_executable() (Rollback+Automatic or CounterAction+SemiAutomatic)
/// - `no_action`: Action is in a terminal state or cannot perform any valid transition
///
/// **Bounded partial-success semantics:**
/// - If an action_id is not found, it's added to `not_found` and does not cause failure
/// - All found actions are processed, even if some have no_action
///
/// **No background worker or queue claiming:**
/// This is a direct query-based planner that reads current state and proposes actions.
async fn orchestration_dry_run(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<OrchestrationDryRunRequest>,
) -> Result<Json<OrchestrationDryRunResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .plan_orchestration_actions(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    let proposals = result
        .proposals
        .into_iter()
        .map(|p| OrchestrationDryRunProposalResponse {
            action_id: p.action_id,
            proposed_action: p.proposed_action.as_str().to_string(),
            reason: p.reason,
            current_status: format_compensation_status(&p.current_status),
        })
        .collect();

    let response = OrchestrationDryRunResponse {
        proposals,
        not_found: result.not_found,
        summary: OrchestrationDryRunSummaryResponse {
            total: result.summary.total,
            can_approve: result.summary.can_approve,
            can_reapprove: result.summary.can_reapprove,
            can_execute: result.summary.can_execute,
            no_action: result.summary.no_action,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

/// POST /compensation-actions/batch-approve - Batch approve compensation actions
///
/// Phase 3 Batch 1 (bounded manual orchestration slice): Approves multiple Pending
/// compensation actions with partial-success semantics.
///
/// **Bounded partial-success semantics:**
/// - If an action_id is not found, it's recorded as `not_found` and continues
/// - If an action fails validation, it's recorded as `failed` with error reason
/// - Successful approvals are recorded as `succeeded`
/// - Does NOT fail-fast on first error - all items are processed
///
/// **Transition rules:**
/// - Only Pending actions can be approved
/// - Uses optimistic locking via lock_version
///
/// **No background worker or queue claiming:**
/// This is a direct service method that processes actions sequentially.
async fn batch_approve_compensation_actions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_approve(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    let outcomes = result
        .outcomes
        .into_iter()
        .map(|o| {
            let (result, error) = match &o.result {
                Ok(a) => (Some(CompensationActionResponse::from(a.clone())), None),
                Err(e) => (None, Some(e.clone())),
            };
            BatchItemOutcomeResponse {
                action_id: o.action_id,
                success: o.success,
                result,
                error,
            }
        })
        .collect();

    let response = BatchOrchestrationResponse {
        outcomes,
        not_found: result.not_found,
        summary: BatchOrchestrationSummaryResponse {
            total: result.summary.total,
            succeeded: result.summary.succeeded,
            failed: result.summary.failed,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

/// POST /compensation-actions/batch-reapprove - Batch reapprove compensation actions
///
/// Phase 3 Batch 1 (bounded manual orchestration slice): Reapproves multiple Failed
/// compensation actions that are eligible for retry, with partial-success semantics.
///
/// **Bounded partial-success semantics:** Same as batch_approve.
///
/// **Policy gates (fail closed):**
/// - Action must be in Failed status
/// - Action must have remaining retry budget
/// - Error code must be retryable
async fn batch_reapprove_compensation_actions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_reapprove(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    let outcomes = result
        .outcomes
        .into_iter()
        .map(|o| {
            let (result, error) = match &o.result {
                Ok(a) => (Some(CompensationActionResponse::from(a.clone())), None),
                Err(e) => (None, Some(e.clone())),
            };
            BatchItemOutcomeResponse {
                action_id: o.action_id,
                success: o.success,
                result,
                error,
            }
        })
        .collect();

    let response = BatchOrchestrationResponse {
        outcomes,
        not_found: result.not_found,
        summary: BatchOrchestrationSummaryResponse {
            total: result.summary.total,
            succeeded: result.summary.succeeded,
            failed: result.summary.failed,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

/// POST /compensation-actions/batch-execute - Batch execute compensation actions
///
/// Phase 3 Batch 1 (bounded manual orchestration slice): Executes multiple Approved
/// compensation actions that are auto-executable, with partial-success semantics.
///
/// **Bounded partial-success semantics:** Same as batch_approve.
///
/// **Executor gate:** Only Approved + Automatic feasibility actions can execute.
async fn batch_execute_compensation_actions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_execute(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    let outcomes = result
        .outcomes
        .into_iter()
        .map(|o| {
            let (result, error) = match &o.result {
                Ok(a) => (Some(CompensationActionResponse::from(a.clone())), None),
                Err(e) => (None, Some(e.clone())),
            };
            BatchItemOutcomeResponse {
                action_id: o.action_id,
                success: o.success,
                result,
                error,
            }
        })
        .collect();

    let response = BatchOrchestrationResponse {
        outcomes,
        not_found: result.not_found,
        summary: BatchOrchestrationSummaryResponse {
            total: result.summary.total,
            succeeded: result.summary.succeeded,
            failed: result.summary.failed,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

/// Request body for artifact ingest with optional side effect capture
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the artifact (e.g., from artifact service)
    pub external_ref: intent_rebase_types::ExternalRef,
    /// Human-readable label for the artifact
    pub label: String,
    /// IntentVersion node IDs this artifact depends on
    pub depends_on_intent_versions: Vec<Uuid>,
    /// Optional properties to attach to the artifact node
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    /// Phase 3 Batch 1 (groundwork): Optional context for side effect capture.
    /// When provided with sufficient fields, enables capture-on-write to the
    /// compensation ledger after successful artifact ingest.
    #[serde(default)]
    pub side_effect_context: Option<intent_rebase_types::SideEffectCaptureContext>,
}

/// Response for artifact ingest with side effect capture result
#[derive(Debug, Serialize)]
pub struct ArtifactIngestResponse {
    pub node: intent_rebase_types::GraphNode,
    pub edges: Vec<intent_rebase_types::GraphEdge>,
    /// Phase 3 Batch 1 (groundwork): Indicates whether a side effect was recorded
    pub side_effect_recorded: bool,
    pub side_effect_id: Option<Uuid>,
}

/// POST /v1/graph/artifacts - Ingest an artifact with optional side effect capture
///
/// Phase 3 Batch 1 (groundwork): Creates an Artifact node in the graph and wires
/// DependsOn edges to the specified IntentVersion nodes. When `side_effect_context`
/// is provided with sufficient fields, also records a side effect to the compensation
/// ledger (capture-on-write groundwork).
///
/// This is the primary path for artifact-producing operations to record side effects.
async fn ingest_artifact(
    State(state): State<AppState>,
    Json(request): Json<ArtifactIngestRequest>,
) -> Result<(StatusCode, Json<ArtifactIngestResponse>), ApiErrorResponse> {
    // Phase 1: Input validation - validate request before processing
    validate_artifact_ingest_request(&request).map_err(ApiErrorResponse)?;

    // Extract side effect context before consuming request for side effect recording
    // after successful graph ingest. This preserves the context for the compensation
    // ledger write even though graph_service.ingest_artifact consumes the request.
    let side_effect_context = request.side_effect_context.clone();

    // Delegate artifact ingest to graph_service - this handles prevalidation of
    // IntentVersion nodes, artifact node creation, and DependsOn edge wiring.
    // This avoids duplicating the core artifact ingest logic in intent-api.
    let graph_request = intent_rebase_types::ArtifactIngestRequest {
        tenant_id: request.tenant_id,
        workflow_id: request.workflow_id,
        external_ref: request.external_ref,
        label: request.label,
        depends_on_intent_versions: request.depends_on_intent_versions,
        properties: request.properties,
        side_effect_context: None, // Consumed above for post-ingest recording
    };

    let ingest_result = state
        .graph_service
        .ingest_artifact(graph_request)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 3 Batch 1 (groundwork): Optionally record side effect if context provided
    let mut side_effect_recorded = false;
    let mut side_effect_id = None;

    if let Some(ref context) = side_effect_context {
        let effect_class = match context.effect_class {
            Some(intent_rebase_types::SideEffectClass::S0PureRead) => {
                compensation_service::SideEffectClass::S0PureRead
            }
            Some(intent_rebase_types::SideEffectClass::S1InternalReversible) => {
                compensation_service::SideEffectClass::S1InternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S2ExternalReversible) | None => {
                compensation_service::SideEffectClass::S2ExternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S3ExternalPartiallyReversible) => {
                compensation_service::SideEffectClass::S3ExternalPartiallyReversible
            }
            Some(intent_rebase_types::SideEffectClass::S4Irreversible) => {
                compensation_service::SideEffectClass::S4Irreversible
            }
        };

        let recorded = if let Some(ref idempotency_key) = context.idempotency_key {
            state
                .side_effect_service
                .record_side_effect_with_idempotency(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                    idempotency_key,
                )
                .await
        } else {
            state
                .side_effect_service
                .record_side_effect(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                )
                .await
        };

        match recorded {
            Ok(effect) => {
                side_effect_recorded = true;
                side_effect_id = Some(effect.id);
                tracing::debug!(
                    "Recorded side effect {} for artifact {} (intent_id={}, version={})",
                    effect.id,
                    ingest_result.node.id,
                    context.source_intent_id,
                    context.source_intent_version
                );
            }
            Err(e) => {
                // Log but don't fail the artifact ingest if side effect recording fails
                tracing::warn!(
                    "Failed to record side effect for artifact {}: {:?}",
                    ingest_result.node.id,
                    e
                );
            }
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(ArtifactIngestResponse {
            node: ingest_result.node,
            edges: ingest_result.edges,
            side_effect_recorded,
            side_effect_id,
        }),
    ))
}

// ============================================================================
// Audit Event Query Endpoints (Phase 3 P3-S4 bounded slice)
// ============================================================================

/// Query parameters for listing audit events by tenant
#[derive(Debug, Deserialize)]
pub struct ListAuditEventsQuery {
    pub tenant_id: Uuid,
    /// Maximum number of events to return (default 100, max 1000)
    #[serde(default = "default_audit_limit")]
    pub limit: usize,
}

fn default_audit_limit() -> usize {
    100
}

/// Query parameters for getting a single audit event
#[derive(Debug, Deserialize)]
pub struct GetAuditEventQuery {
    pub tenant_id: Uuid,
}

/// Response for listing audit events
#[derive(Debug, Serialize)]
pub struct ListAuditEventsResponse {
    pub events: Vec<intent_rebase_types::AuditEvent>,
    pub total: usize,
}

/// GET /audit/events - List audit events for a tenant
///
/// Phase 3 P3-S4 (bounded tenant-scoped audit query slice):
/// Returns all audit events belonging to the specified tenant, ordered by
/// occurred_at descending (newest first).
///
/// **Tenant-scoped:** Results are filtered strictly to the provided tenant_id.
/// Cross-tenant access is blocked at the repository layer.
///
/// **Pagination:** Uses limit parameter (default 100, max 1000) for pagination.
///
/// **This endpoint is READ-ONLY** - it only queries existing audit data.
async fn list_audit_events(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListAuditEventsQuery>,
) -> Result<Json<ListAuditEventsResponse>, ApiErrorResponse> {
    let limit = query.limit.min(1000); // Cap at 1000 to prevent abuse

    let events = state
        .audit_service
        .list_by_tenant(query.tenant_id, limit)
        .await
        .map_err(ApiErrorResponse)?;

    let total = events.len();

    Ok(Json(ListAuditEventsResponse { events, total }))
}

/// GET /audit/events/{event_id} - Get a single audit event by ID
///
/// Phase 3 P3-S4 (bounded tenant-scoped audit query slice):
/// Returns a single audit event by its ID, scoped to the specified tenant.
///
/// **Tenant-scoped:** Returns 404 if the event doesn't exist OR if it belongs
/// to a different tenant. This enforces tenant isolation at the API layer.
///
/// **This endpoint is READ-ONLY** - it only queries existing audit data.
async fn get_audit_event(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<GetAuditEventQuery>,
) -> Result<Json<intent_rebase_types::AuditEvent>, ApiErrorResponse> {
    state
        .audit_service
        .get_audit_event(event_id, query.tenant_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

// ============================================================================
// Forensic Bundle Handlers (Phase 3 Batch 3b bounded slice)
// ============================================================================

/// Request body for creating a forensic bundle
#[derive(Debug, Clone, Deserialize)]
pub struct CreateForensicBundleRequest {
    pub tenant_id: Uuid,
    pub time_range: BundleTimeRange,
    pub purpose: BundlePurpose,
    pub created_by: String,
}

/// Response for forensic bundle creation
#[derive(Debug, Clone, Serialize)]
pub struct CreateForensicBundleResponse {
    pub bundle: ForensicBundle,
    pub message: String,
}

/// Response for listing forensic bundles
#[derive(Debug, Clone, Serialize)]
pub struct ListForensicBundlesResponse {
    pub bundles: Vec<ForensicBundle>,
    pub total: usize,
}

/// Query parameters for listing forensic bundles
#[derive(Debug, Deserialize)]
pub struct ListForensicBundlesQuery {
    pub tenant_id: Uuid,
    pub limit: Option<usize>,
}

/// Query parameters for getting a forensic bundle
#[derive(Debug, Deserialize)]
pub struct GetForensicBundleQuery {
    pub tenant_id: Uuid,
}

/// POST /forensic-bundles - Create a new forensic bundle
///
/// Phase 3 Batch 3b (P4 bounded slice): Creates a new forensic bundle manifest
/// with Pending status. The bundle generation service manages the lifecycle
/// (Pending -> Generating -> Ready/Failed).
///
/// **Bounded slice scope:**
/// - Creates bundle manifest with Pending status
/// - Status transitions via explicit transition endpoints
/// - S3 storage, actual content collection, integrity verification are Phase 4
async fn create_forensic_bundle(
    State(state): State<AppState>,
    Json(request): Json<CreateForensicBundleRequest>,
) -> Result<(StatusCode, Json<CreateForensicBundleResponse>), ApiErrorResponse> {
    let gen_request = GenCreateBundleRequest {
        tenant_id: request.tenant_id,
        time_range: request.time_range,
        purpose: request.purpose,
        created_by: request.created_by,
    };

    let response = state
        .forensic_bundle_service
        .initiate_bundle(gen_request)
        .await
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(_) => {
                ApiErrorResponse(IntentRebaseError::Internal("unexpected not found".into()))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition { .. } => {
                ApiErrorResponse(IntentRebaseError::Internal("invalid transition".into()))
            }
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateForensicBundleResponse {
            bundle: response.bundle,
            message: response.message,
        }),
    ))
}

/// GET /forensic-bundles/{bundle_id} - Get a forensic bundle by ID
///
/// Phase 3 Batch 3b (P4 bounded slice): Returns a single forensic bundle
/// by its ID, scoped to the specified tenant. Returns 404 if bundle not found
/// or if the bundle belongs to a different tenant (fail-not-found pattern).
async fn get_forensic_bundle(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<GetForensicBundleQuery>,
) -> Result<Json<ForensicBundle>, ApiErrorResponse> {
    let bundle = state
        .forensic_bundle_service
        .get_bundle(bundle_id)
        .await
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition { .. } => {
                ApiErrorResponse(IntentRebaseError::Internal("invalid transition".into()))
            }
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })?;

    // Verify tenant ownership (fail-not-found pattern)
    if bundle.tenant_id != query.tenant_id {
        return Err(ApiErrorResponse(
            IntentRebaseError::ForensicBundleNotFound(bundle_id),
        ));
    }

    Ok(Json(bundle))
}

/// GET /forensic-bundles - List forensic bundles for a tenant
///
/// Phase 3 Batch 3b (P4 bounded slice): Returns all forensic bundles
/// for the specified tenant, ordered by created_at descending.
async fn list_forensic_bundles(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListForensicBundlesQuery>,
) -> Result<Json<ListForensicBundlesResponse>, ApiErrorResponse> {
    let bundles = state
        .forensic_bundle_service
        .list_bundles(query.tenant_id, query.limit)
        .await
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition { .. } => {
                ApiErrorResponse(IntentRebaseError::Internal("invalid transition".into()))
            }
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })?;

    let total = bundles.len();

    Ok(Json(ListForensicBundlesResponse { bundles, total }))
}

/// POST /forensic-bundles/{bundle_id}/transition-to-generating - Transition bundle to Generating
///
/// Phase 3 Batch 3b (P4 bounded slice): Transitions a Pending bundle to Generating.
/// This is a placeholder for actual content collection (Phase 4 scope).
async fn transition_bundle_to_generating(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
) -> Result<Json<ForensicBundle>, ApiErrorResponse> {
    state
        .forensic_bundle_service
        .transition_to_generating(bundle_id)
        .await
        .map(Json)
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition {
                from,
                to,
                reason,
            } => ApiErrorResponse(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", from),
                to_status: format!("{:?}", to),
                reason,
            }),
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })
}

/// POST /forensic-bundles/{bundle_id}/transition-to-ready - Transition bundle to Ready
///
/// Phase 3 Batch 3b (P4 bounded slice): Transitions a Generating bundle to Ready.
/// This is a placeholder for actual bundle assembly (Phase 4 scope).
async fn transition_bundle_to_ready(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
) -> Result<Json<ForensicBundle>, ApiErrorResponse> {
    state
        .forensic_bundle_service
        .transition_to_ready(bundle_id)
        .await
        .map(Json)
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition {
                from,
                to,
                reason,
            } => ApiErrorResponse(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", from),
                to_status: format!("{:?}", to),
                reason,
            }),
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })
}

/// POST /forensic-bundles/{bundle_id}/transition-to-failed - Transition bundle to Failed
///
/// Phase 3 Batch 3b (P4 bounded slice): Transitions a Generating bundle to Failed.
async fn transition_bundle_to_failed(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
) -> Result<Json<ForensicBundle>, ApiErrorResponse> {
    state
        .forensic_bundle_service
        .transition_to_failed(bundle_id)
        .await
        .map(Json)
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition {
                from,
                to,
                reason,
            } => ApiErrorResponse(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", from),
                to_status: format!("{:?}", to),
                reason,
            }),
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })
}

/// POST /forensic-bundles/{bundle_id}/complete - Complete bundle creation
///
/// Phase 3 Batch 3b (P4 bounded slice): Transitions a bundle through the full
/// Pending -> Generating -> Ready lifecycle. This is a convenience method for
/// testing the complete flow.
///
/// **Bounded slice:** This simulates the full generation flow. Actual content
/// collection and S3 storage are Phase 4 scope.
async fn complete_forensic_bundle(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
) -> Result<Json<ForensicBundle>, ApiErrorResponse> {
    state
        .forensic_bundle_service
        .complete_bundle_creation(bundle_id)
        .await
        .map(Json)
        .map_err(|e| match e {
            forensic_service::bundle_gen::BundleGenError::NotFound(id) => {
                ApiErrorResponse(IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::bundle_gen::BundleGenError::InvalidTransition {
                from,
                to,
                reason,
            } => ApiErrorResponse(IntentRebaseError::InvalidForensicBundleStatusTransition {
                from_status: format!("{:?}", from),
                to_status: format!("{:?}", to),
                reason,
            }),
            forensic_service::bundle_gen::BundleGenError::Repository(err) => {
                ApiErrorResponse(err)
            }
        })
}

/// Build the Phase 1 router with CORS enabled
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
/// When `None` (default), audit events are persisted but NOT streamed.
/// When `Some`, events are also published to the event stream (best-effort, fail-open).
#[allow(clippy::too_many_arguments)]
pub fn build_router(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_bundle_service: Arc<BundleGenerationService<InMemoryBundleRepository>>,
) -> Router {
    let state = AppState {
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_bundle_service,
        start_time: Instant::now(),
    };

    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/metrics", get(metrics_handler))
        .route("/v1/intents/validate", post(validate_intent))
        .route("/intents", post(create_intent))
        .route("/intents/{intent_id}", get(get_intent_head))
        .route("/intents/{intent_id}/versions", post(create_version))
        .route("/intents/{intent_id}/versions", get(list_versions))
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(get_version),
        )
        .route("/intents/{intent_id}/diff", post(compute_diff))
        .route("/intents/{intent_id}/rebase-preview", post(rebase_preview))
        .route("/intents/{intent_id}/rebase-apply", post(rebase_apply))
        // Replay endpoint (Phase 2b bounded replay slice)
        .route("/intents/{intent_id}/replay", post(replay_intent))
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route("/intents/{intent_id}/side-effects", get(list_side_effects))
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/orchestration-dashboard",
            get(get_orchestration_dashboard),
        )
        // Compensation actions query endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/compensation-actions",
            get(list_compensation_actions),
        )
        // Compensation action mutation endpoints (Phase 3 Batch 1 bounded execution slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/{action_id}/approve",
            post(approve_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/waive",
            post(waive_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/execute",
            post(execute_compensation_action),
        )
        // Compensation action manual retry and DLQ endpoints (Phase 3 Batch 1 bounded manual retry slice)
        .route(
            "/compensation-actions/{action_id}/reapprove",
            post(reapprove_compensation_action),
        )
        // Bounded compensation planner endpoint (Phase 3 bounded planner slice)
        .route(
            "/compensation-actions/plan",
            post(plan_compensation_actions),
        )
        .route("/compensation-actions/dlq", get(list_dlq_candidates))
        // Batch candidates query endpoint (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
        .route(
            "/compensation-actions/batch-candidates",
            get(list_batch_candidates),
        )
        // Policy gate evaluation endpoints (Phase 3 Batch 1 bounded read-only slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/policy-gate",
            get(get_compensation_policy_gate),
        )
        .route(
            "/intents/{intent_id}/compensation-policy-gate",
            get(get_intent_compensation_policy_gate),
        )
        // Orchestration coordination status endpoints (Phase 3 Batch 1 bounded read-only orchestration view)
        .route(
            "/compensation-actions/orchestration-coordination",
            get(get_orchestration_coordination),
        )
        .route(
            "/intents/{intent_id}/orchestration-coordination",
            get(get_intent_orchestration_coordination),
        )
        // Manual orchestration & dry-run planner endpoints (Phase 3 Batch 1 bounded slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/orchestration-dry-run",
            post(orchestration_dry_run),
        )
        .route(
            "/compensation-actions/batch-approve",
            post(batch_approve_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-reapprove",
            post(batch_reapprove_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-execute",
            post(batch_execute_compensation_actions),
        )
        // Orchestration run endpoints (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
        .route("/compensation-actions/runs", post(create_orchestration_run))
        .route(
            "/compensation-actions/runs/{run_id}",
            get(get_orchestration_run),
        )
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route("/v1/graph/nodes", post(create_graph_node))
        .route("/v1/graph/nodes", get(list_graph_nodes))
        .route("/v1/graph/nodes/{node_id}", get(get_graph_node))
        .route("/v1/graph/edges", post(create_graph_edge))
        .route("/v1/graph/edges", get(list_graph_edges))
        .route("/v1/graph/nodes/{node_id}/edges", get(list_edges_from_node))
        // Artifact ingest with optional side effect capture (Phase 3 Batch 1 groundwork)
        .route("/v1/graph/artifacts", post(ingest_artifact))
        // Approval request endpoints (Phase 2b bounded slice)
        .route(
            "/approval-requests/pending",
            get(list_pending_approval_requests),
        )
        .route(
            "/approval-requests/{approval_request_id}/approve",
            post(approve_approval_request),
        )
        .route(
            "/approval-requests/{approval_request_id}/reject",
            post(reject_approval_request),
        )
        // POST expire - bounded manual expiry transition (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/expire",
            post(expire_approval_request),
        )
        // GET revalidate - bounded read-only scope comparison (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/revalidate",
            get(revalidate_approval_request),
        )
        // Policy snapshot endpoints (Phase 2 bounded read-only slice)
        .route("/policy-snapshots/{snapshot_id}", get(get_policy_snapshot))
        .route(
            "/policy-snapshots/intent/{intent_id}/latest",
            get(get_latest_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/versions/{version}",
            get(get_policy_snapshot_by_version),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}",
            get(list_policy_snapshots),
        )
        // Audit event query endpoints (Phase 3 P3-S4 bounded slice)
        .route("/audit/events", get(list_audit_events))
        .route(
            "/audit/events/{event_id}",
            get(get_audit_event),
        )
        // Forensic bundle endpoints (Phase 3 Batch 3b bounded slice)
        .route("/forensic-bundles", post(create_forensic_bundle))
        .route("/forensic-bundles/{bundle_id}", get(get_forensic_bundle))
        .route("/forensic-bundles", get(list_forensic_bundles))
        .route(
            "/forensic-bundles/{bundle_id}/transition-to-generating",
            post(transition_bundle_to_generating),
        )
        .route(
            "/forensic-bundles/{bundle_id}/transition-to-ready",
            post(transition_bundle_to_ready),
        )
        .route(
            "/forensic-bundles/{bundle_id}/transition-to-failed",
            post(transition_bundle_to_failed),
        )
        .route(
            "/forensic-bundles/{bundle_id}/complete",
            post(complete_forensic_bundle),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

/// Build the router with SQL-backed audit and approval repositories.
///
/// This is the production bootstrap helper that constructs SQL-backed repositories
/// from a `PgPool` and injects them into the router. Use this in production
/// deployments where PostgreSQL-backed persistence is required.
///
/// For testing or in-memory deployments, use `build_router` directly with
/// `InMemoryAuditRepository` and `InMemoryApprovalRequestRepository`.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool used to construct SQL-backed repositories
/// * `service` - Pre-configured intent service (typically with SQL-backed intent repository)
/// * `graph_service` - Graph service instance
/// * `orchestrator` - Pre-configured orchestrator (typically with SQL-backed checkpoint repository)
///
/// # Example
///
/// ```ignore
/// let pool = PgPool::connect(&database_url).await?;
/// let intent_repo = SqlxIntentRepository::new(pool.clone());
/// let intent_service = IntentService::new(Arc::new(intent_repo));
/// let checkpoint_repo = SqlxCheckpointRepository::new(pool.clone());
/// let orchestrator = RebaseOrchestrator::new(
///     Arc::new(checkpoint_repo),
///     graph_service.clone(),
///     runtime_adapter,
/// );
///
/// let router = build_router_with_sql_audit_and_approval(
///     pool,
///     Arc::new(intent_service),
///     Arc::new(graph_service),
///     Arc::new(orchestrator),
///     Some(event_publisher),  // Phase 2b: optional event publisher
/// );
/// ```
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
/// When `None` (default), audit events are persisted but NOT streamed.
/// When `Some`, events are also published to the event stream (best-effort, fail-open).
///
/// Phase 3: Full NATS JetStream integration with consumers and DLQ.
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_sql_audit_and_approval(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_bundle_service: Arc<BundleGenerationService<InMemoryBundleRepository>>,
) -> Router {
    // Construct SQL-backed audit, approval, and policy snapshot repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
    );
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> =
        Arc::new(intent_service::SqlxPolicySnapshotRepository::new(pool));

    build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_bundle_service,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;

    fn create_test_service() -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        // Phase 3 Batch 1: In-memory side effect repository for tests
        let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        // Phase 3 Batch 1: In-memory compensation action repository for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo),
        );
        // Phase 3 Batch 1: In-memory orchestration run repository for tests
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle repository and service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        AppState {
            service,
            graph_service: graph_svc,
            side_effect_service: side_effect_svc,
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_router_builds_successfully() {
        let state = create_test_service();
        let _router: axum::Router = Router::new()
            .route("/intents", post(create_intent))
            .route("/intents/{intent_id}", get(get_intent_head))
            .route("/intents/{intent_id}/versions", post(create_version))
            .route("/intents/{intent_id}/versions", get(list_versions))
            .route(
                "/intents/{intent_id}/versions/{version_number}",
                get(get_version),
            )
            .route("/intents/{intent_id}/diff", post(compute_diff))
            .route("/intents/{intent_id}/rebase-preview", post(rebase_preview))
            .route("/intents/{intent_id}/rebase-apply", post(rebase_apply))
            .with_state(state);
        // Router builds successfully - this is a compile-time check essentially
    }

    #[test]
    fn test_apply_status_code_blocked_returns_accepted() {
        assert_eq!(
            apply_status_code(&ApplyOutcome::BlockedManualReview),
            StatusCode::ACCEPTED
        );
    }

    #[test]
    fn test_apply_outcome_label_serialization_values() {
        assert_eq!(apply_outcome_label(&ApplyOutcome::NoOp), "no_op");
        assert_eq!(
            apply_outcome_label(&ApplyOutcome::AutoProceededWithNotification),
            "auto_proceeded_with_notification"
        );
    }

    #[test]
    fn test_api_error_serialization() {
        let api_error = ApiError {
            error: ErrorDetails {
                code: "TEST_ERROR".to_string(),
                message: "Test message".to_string(),
                retryable: false,
                details: None,
            },
        };
        let json = serde_json::to_string(&api_error).unwrap();
        assert!(json.contains("TEST_ERROR"));
        assert!(json.contains("Test message"));
    }

    #[test]
    fn test_parse_optional_header_absent() {
        let headers = HeaderMap::new();
        let result = parse_optional_header(&headers, "x-expected-version").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_optional_header_valid_integer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-expected-version", HeaderValue::from_static("5"));
        let result = parse_optional_header(&headers, "x-expected-version").unwrap();
        assert_eq!(result, Some(5));
    }

    #[test]
    fn test_parse_optional_header_malformed_non_integer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-expected-version",
            HeaderValue::from_static("not-a-number"),
        );
        let result = parse_optional_header(&headers, "x-expected-version");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidHeader(_)));
        let msg = err.to_string();
        assert!(msg.contains("x-expected-version"));
        assert!(msg.contains("not-a-number"));
    }

    #[test]
    fn test_parse_optional_header_malformed_negative_integer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-expected-row-version", HeaderValue::from_static("-1"));
        let result = parse_optional_header(&headers, "x-expected-row-version");
        // -1 is a valid i32, so it should parse successfully
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(-1));
    }

    #[test]
    fn test_api_error_response_for_invalid_header() {
        let err =
            IntentRebaseError::InvalidHeader("X-Expected-Version must be an integer".to_string());
        let api_err_response = ApiErrorResponse(err).into_response();
        assert_eq!(api_err_response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_api_error_response_for_serialization_error() {
        // SerializationError represents internal data corruption during SQL read/write,
        // not client input errors, so it should return 500 Internal Server Error
        let err =
            IntentRebaseError::SerializationError("payload corrupted in database".to_string());
        let api_err_response = ApiErrorResponse(err).into_response();
        assert_eq!(api_err_response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // === Diff Handler Tests ===

    #[tokio::test]
    async fn test_compute_diff_success() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            }
        }

        let state = create_test_service();

        // Create an intent first
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Test the compute_diff handler directly
        let diff_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = compute_diff(State(state), Path(intent_id), Json(diff_request))
            .await
            .expect("Diff computation should succeed");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version.version_number, 1);
        assert_eq!(result.to_version.version_number, 2);
    }

    #[tokio::test]
    async fn test_compute_diff_invalid_version_ordering() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let state = create_test_service();

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Test with reversed version order (from_version > to_version)
        let diff_request = DiffRequest {
            from_version: 2,
            to_version: 1,
        };
        let result = compute_diff(State(state), Path(intent_id), Json(diff_request)).await;
        // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // === Rebase Preview Handler Tests ===

    #[tokio::test]
    async fn test_rebase_preview_success() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            }
        }

        let state = create_test_service();

        // Create an intent first
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Test the rebase_preview handler directly
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = rebase_preview(State(state), Path(intent_id), Json(preview_request))
            .await
            .expect("Rebase preview should succeed");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version.version_number, 1);
        assert_eq!(result.to_version.version_number, 2);
        // Verify response has semantically reliable fields only
        assert!(result.rationale.len() > 0);
        assert!(result.risk_level >= 1 && result.risk_level <= 5);
    }

    #[tokio::test]
    async fn test_rebase_preview_invalid_version_ordering() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let state = create_test_service();

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Test with reversed version order (from_version > to_version)
        let preview_request = intent_rebase_types::DiffRequest {
            from_version: 2,
            to_version: 1,
        };
        let result = rebase_preview(State(state), Path(intent_id), Json(preview_request)).await;
        // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // === Graph-Available Affected Items Tests ===

    #[tokio::test]
    async fn test_rebase_preview_with_graph_classifies_affected_items() {
        use graph_service::{GraphRepository, GraphService, InMemoryGraphRepository};
        use intent_rebase_types::{
            AffectedItemsStatus, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, ExternalRef, ExternalRefType, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, NodeType, RiskTier, SourceRef, Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent with graph".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            }
        }

        // Create service with graph service available
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));

        // Create service with graph integration
        let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        };

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Get the version to access its ID
        let to_version = state.service.get_version(intent_id, 2).await.unwrap();

        // Create IntentVersion graph node for v2
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node in the graph that maps to our version
        let iv_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: to_version.id,
                }),
                label: "IntentVersion v2".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Create an artifact that depends on this IntentVersion
        let artifact_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::Artifact,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::Artifact,
                    ref_id: Uuid::new_v4(),
                }),
                label: "Test Artifact".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Create DependsOn edge: Artifact -> IntentVersion
        graph_repo
            .create_edge(intent_rebase_types::CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: artifact_node.id,
                to_node_id: iv_node.id,
                edge_type: intent_rebase_types::EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();

        // Call rebase_preview which should use graph classification
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = rebase_preview(State(state), Path(intent_id), Json(preview_request))
            .await
            .expect("Rebase preview should succeed even with graph");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.affected_items.status, AffectedItemsStatus::Available);
        // Verify affected artifacts contains our artifact
        assert!(!result.affected_items.affected_artifacts.is_empty());
        assert_eq!(
            result.affected_items.affected_artifacts[0].node_id,
            artifact_node.id
        );
    }

    #[tokio::test]
    async fn test_rebase_preview_fallback_when_graph_node_not_found() {
        use graph_service::{GraphService, InMemoryGraphRepository};
        use intent_rebase_types::{
            AffectedItemsStatus, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent no graph".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            }
        }

        // Create service with graph service but NO graph data
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));
        let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        };

        // Create a test intent

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Call rebase_preview - graph node won't be found but should NOT fail
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = rebase_preview(State(state), Path(intent_id), Json(preview_request))
            .await
            .expect("Rebase preview should succeed even when graph node not found");

        assert_eq!(result.intent_id, intent_id);
        // Status should be Unavailable since IntentVersion node not in graph
        assert_eq!(
            result.affected_items.status,
            AffectedItemsStatus::Unavailable
        );
        // But endpoint still returns useful data
        assert!(result.rationale.len() > 0);
    }

    // === Input Validation Tests ===

    #[test]
    fn test_validate_create_intent_request_valid() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, SourceRef, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_create_intent_request_nil_workflow_id() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::nil(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("workflow_id"));
    }

    #[test]
    fn test_validate_create_intent_request_empty_summary() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("summary"));
    }

    #[test]
    fn test_validate_create_intent_request_whitespace_summary() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "   ".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("summary"));
    }

    #[test]
    fn test_validate_create_version_request_valid() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "Updating".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        let result = validate_create_version_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_create_version_request_empty_domain() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "Updating".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        let result = validate_create_version_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("domain"));
    }

    #[test]
    fn test_api_error_response_for_unauthorized() {
        let err = IntentRebaseError::Unauthorized("Missing credentials".to_string());
        let api_err_response = ApiErrorResponse(err).into_response();
        assert_eq!(api_err_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_api_error_response_for_invalid_api_key() {
        let err = IntentRebaseError::InvalidApiKey("Invalid key format".to_string());
        let api_err_response = ApiErrorResponse(err).into_response();
        assert_eq!(api_err_response.status(), StatusCode::UNAUTHORIZED);
    }

    // === Validate Intent Handler Tests ===

    #[tokio::test]
    async fn test_validate_intent_valid_request() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, SourceRef, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success statement".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let result = validate_intent(Json(request)).await;
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_validate_intent_empty_summary() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 0.5,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let result = validate_intent(Json(request)).await;
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
        assert!(
            field_names.iter().any(|f| f.contains("summary")),
            "Expected summary validation error"
        );
    }

    #[tokio::test]
    async fn test_validate_intent_confidence_out_of_range() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.5, // Out of range (should be 0.0-1.0)
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let result = validate_intent(Json(request)).await;
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
        assert!(
            field_names.iter().any(|f| f.contains("confidence")),
            "Expected confidence validation error"
        );
    }

    // === Replay Endpoint Tests (Phase 2b bounded replay slice) ===

    #[tokio::test]
    async fn test_replay_intent_no_checkpoint_available() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

        let state = create_test_service();

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent v2".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Test the replay endpoint - no checkpoints available, so should get no_checkpoint_found outcome
        let replay_request = ReplayRequest {
            from_version: Some(1),
            to_version: 2,
            checkpoint_id: None,
        };
        let result = replay_intent(State(state), Path(intent_id), Json(replay_request))
            .await
            .expect("Replay should return even with no checkpoints");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version, 1);
        assert_eq!(result.to_version, 2);
        assert!(result.aligned_checkpoint_id.is_none());
        assert_eq!(result.checkpoint_selection_outcome, "NoCheckpointFound");
        // Skipped because no checkpoint and adapter not used for no-checkpoint path
        assert_eq!(result.runtime_execution_status, "skipped_not_ready");
    }

    // === Approval Revalidation Handler Tests ===

    #[tokio::test]
    async fn test_revalidate_approval_request_valid_when_scope_unchanged() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create a policy snapshot for version 1 (same as approval basis)
        let scope = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot.clone())
            .await
            .unwrap();

        // Create latest snapshot with SAME scope_hash (same scope)
        let latest_snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope);
        state
            .policy_snapshot_repo
            .create_snapshot(latest_snapshot.clone())
            .await
            .unwrap();

        // Test revalidate - should be valid since scope_hash matches
        let result = revalidate_approval_request(State(state), Path(approval_id))
            .await
            .expect("Revalidate should succeed");

        assert_eq!(result.approval_id, approval_id);
        assert!(result.valid);
        assert_eq!(result.approval_basis_scope_hash, snapshot.scope_hash);
        assert_eq!(result.current_scope_hash, Some(latest_snapshot.scope_hash));
        assert!(!result.revalidation_required);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_invalid_when_scope_changed() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create a policy snapshot for version 1 with Partial scope
        let scope_v1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot_v1 =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope_v1);
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot_v1.clone())
            .await
            .unwrap();

        // Create latest snapshot with DIFFERENT scope (Full instead of Partial)
        let scope_v2 = ScopeDefinition {
            scope_type: ScopeType::Full,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 2,
        };
        let snapshot_v2 =
            PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope_v2);
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot_v2.clone())
            .await
            .unwrap();

        // Test revalidate - should be invalid since scope_hash differs
        let result = revalidate_approval_request(State(state), Path(approval_id))
            .await
            .expect("Revalidate should succeed");

        assert_eq!(result.approval_id, approval_id);
        assert!(!result.valid);
        assert_eq!(result.approval_basis_scope_hash, snapshot_v1.scope_hash);
        assert_eq!(result.current_scope_hash, Some(snapshot_v2.scope_hash));
        assert!(result.revalidation_required);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_valid_when_only_basis_snapshot_exists() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create only the approval-basis snapshot (no newer snapshots exist)
        // When no newer policy snapshots exist, the approval basis is the latest,
        // so scope_hash matches and the approval is still valid
        let scope = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot.clone())
            .await
            .unwrap();

        // Test revalidate - should return valid=true because latest (only) snapshot
        // matches approval basis, meaning no newer policy exists to invalidate the approval
        let result = revalidate_approval_request(State(state), Path(approval_id))
            .await
            .expect("Revalidate should succeed when only basis snapshot exists");

        assert_eq!(result.approval_id, approval_id);
        assert!(result.valid);
        assert!(!result.revalidation_required);
        assert_eq!(result.current_scope_hash, Some(snapshot.scope_hash));
        assert!(result.reason.contains("Scope unchanged"));
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_not_found() {
        let state = create_test_service();
        let non_existent_id = Uuid::new_v4();

        // Test revalidate - should return 404
        let result = revalidate_approval_request(State(state), Path(non_existent_id)).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_basis_snapshot_not_found() {
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request but NO policy snapshots at all
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Test revalidate - should return 404 because approval basis snapshot doesn't exist
        let result = revalidate_approval_request(State(state), Path(approval_id)).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // Phase 2b: Event Publishing Tests (bounded event-streaming slice)
    // =========================================================================

    /// Helper: Create AppState with an in-memory event publisher for testing
    fn create_test_service_with_publisher(
        publisher: Arc<dyn intent_rebase_types::EventPublisher>,
    ) -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        AppState {
            service,
            graph_service: graph_svc,
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: Some(publisher),
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_event_publisher_none_skips_publishing() {
        // Test that when event_publisher is None, publish_audit_event is a no-op
        let publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>> = None;
        let tenant_id = Uuid::new_v4();

        // Should not panic or error - just silently skip
        publish_audit_event(
            &publisher,
            tenant_id,
            "RebaseApplied",
            &serde_json::json!({ "test": true }),
        )
        .await;
    }

    #[tokio::test]
    async fn test_event_publisher_inmemory_stores_events() {
        // Test that InMemoryEventPublisher stores events correctly
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let state = create_test_service_with_publisher(publisher.clone());

        // Verify publisher is ready
        assert!(state.event_publisher.as_ref().unwrap().is_ready());
    }

    #[tokio::test]
    async fn test_publish_audit_event_helper_success() {
        // Test publish_audit_event helper with InMemoryEventPublisher
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded"
        });

        let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
            Some(publisher.clone());
        publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;

        // Verify event was published
        let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
        let events = publisher.get_events_for_subject(&subject_str).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].schema_version, "v1");
        assert_eq!(events[0].payload, payload);
    }

    #[tokio::test]
    async fn test_publish_audit_event_helper_multiple_events() {
        // Test that multiple events are published with monotonic sequences
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();

        let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
            Some(publisher.clone());

        // Publish 3 events
        for i in 1..=3 {
            let payload = serde_json::json!({ "index": i });
            publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;
        }

        // Verify sequence is monotonic
        let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
        let events = publisher.get_events_for_subject(&subject_str).await;
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[2].sequence, 3);
    }

    #[tokio::test]
    async fn test_noop_event_publisher_skips() {
        // Test that NoOpEventPublisher skips all events (always returns Skipped)
        use intent_rebase_types::EventPublisher;
        let publisher = Arc::new(intent_rebase_types::NoOpEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let payload = serde_json::json!({ "test": true });
        let subject =
            intent_rebase_types::EventSubject::from_audit_event(tenant_id, "RebaseApplied");

        // NoOpEventPublisher should skip (return Skipped)
        let result = publisher.publish(&subject, &payload).await;
        match result {
            intent_rebase_types::PublishResult::Skipped { reason } => {
                assert!(reason.contains("disabled"));
            }
            _ => panic!("Expected Skipped result from NoOpEventPublisher"),
        }
    }

    #[tokio::test]
    async fn test_build_router_accepts_event_publisher() {
        // Test that build_router accepts event_publisher parameter
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let event_publisher = Some(Arc::new(intent_rebase_types::InMemoryEventPublisher::new())
            as Arc<dyn intent_rebase_types::EventPublisher>);
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(Arc::new(
            compensation_service::InMemorySideEffectRepository::new(),
        )));
        let compensation_action_svc =
            Arc::new(compensation_service::CompensationActionService::new(
                Arc::new(compensation_service::InMemoryCompensationActionRepository::new()),
            ));
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));

        let _router: axum::Router = build_router(
            service,
            graph_svc,
            side_effect_svc,
            compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_repo,
            approval_repo,
            policy_snapshot_repo,
            event_publisher,
            forensic_bundle_svc,
        );
        // Router builds successfully - this verifies the signature change works
    }

    // =========================================================================
    // Artifact Ingest Handler Tests (Phase 3 Batch 1 groundwork)
    // =========================================================================

    #[tokio::test]
    async fn test_ingest_artifact_success() {
        use graph_service::{GraphRepository, GraphService, InMemoryGraphRepository};
        use intent_rebase_types::{ExternalRef, ExternalRefType, NodeType};

        // Create a graph repo with an IntentVersion node that the artifact can depend on
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));

        // Use the same tenant_id and workflow_id for both the IntentVersion and the artifact
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node in the graph first
        let intent_version_ref_id = Uuid::new_v4();
        let _iv_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: intent_version_ref_id,
                }),
                label: "IntentVersion v1".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Build state with the graph service that has the IntentVersion node
        let repo = Arc::new(InMemoryIntentRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: None,
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        };

        // Create artifact request with the IntentVersion dependency and matching tenant/workflow IDs
        let request = ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![_iv_node.id],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request))
            .await
            .expect("Artifact ingest should succeed");

        assert_eq!(result.0, StatusCode::CREATED);
        assert_eq!(result.1.node.label, "Test Artifact");
        assert!(!result.1.side_effect_recorded);
    }

    #[tokio::test]
    async fn test_ingest_artifact_nil_tenant_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::nil(), // Invalid: nil UUID
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_nil_workflow_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::nil(), // Invalid: nil UUID
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_empty_label_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "".to_string(), // Invalid: empty label
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_whitespace_label_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "   ".to_string(), // Invalid: whitespace-only label
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_nil_external_ref_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::nil(), // Invalid: nil UUID
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // Artifact Ingest Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_artifact_ingest_request_valid() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_tenant_id() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::nil(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("tenant_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_workflow_id() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::nil(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("workflow_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_empty_label() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("label"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_whitespace_label() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "   ".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("label"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_external_ref_id() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::nil(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("external_ref.ref_id"));
    }

    // =========================================================================
    // Side Effect Context Validation Tests (Phase 3 Batch 1)
    // =========================================================================

    #[test]
    fn test_validate_artifact_ingest_request_valid_side_effect_context() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_nil_source_intent_id() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::nil(), // Invalid: nil UUID
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.source_intent_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_zero_version() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 0, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("source_intent_version"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_negative_version() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: -1, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("source_intent_version"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_effect_type() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "".to_string(), // Invalid: empty
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.effect_type"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_effect_type() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "   ".to_string(), // Invalid: whitespace-only
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.effect_type"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_target() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "".to_string(), // Invalid: empty
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.target"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_target() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "   ".to_string(), // Invalid: whitespace-only
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.target"));
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_invalid_source_intent_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::nil(), // Invalid: nil UUID
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_invalid_version_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 0, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_effect_type_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "".to_string(), // Invalid: empty
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_target_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "".to_string(), // Invalid: empty
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_idempotency_key() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("".to_string()), // Invalid: empty
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.idempotency_key"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_idempotency_key() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("   ".to_string()), // Invalid: whitespace-only
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.idempotency_key"));
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_idempotency_key_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("".to_string()), // Invalid: empty
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_whitespace_idempotency_key_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("   ".to_string()), // Invalid: whitespace-only
            }),
        };

        let result = ingest_artifact(State(state), Json(request)).await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // === Compensation Action API Tests ===

    fn create_test_service_with_executor() -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        // Use in-memory compensation action repo with stub executor
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic bundle service for tests
        let forensic_bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let forensic_bundle_svc = Arc::new(forensic_service::BundleGenerationService::new(
            forensic_bundle_repo,
        ));
        AppState {
            service,
            graph_service: graph_svc,
            side_effect_service: side_effect_svc,
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: None,
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_approve_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action directly via the service
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Approve the action via the API
        let request = ApproveCompensationActionBody {
            lock_version: created.lock_version,
            approved_by: Some("test-approver".to_string()),
        };
        let result = approve_compensation_action(State(state), Path(created.id), Json(request))
            .await
            .unwrap();

        assert_eq!(result.status, "approved");
        assert_eq!(result.approved_by, Some("test-approver".to_string()));
    }

    #[tokio::test]
    async fn test_approve_compensation_action_not_found() {
        let state = create_test_service_with_executor();

        let request = ApproveCompensationActionBody {
            lock_version: 0,
            approved_by: None,
        };
        let result =
            approve_compensation_action(State(state), Path(Uuid::new_v4()), Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_waive_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Waive the action via the API
        let request = WaiveCompensationActionBody {
            lock_version: created.lock_version,
            waived_by: Some("test-waiver".to_string()),
        };
        let result = waive_compensation_action(State(state), Path(created.id), Json(request))
            .await
            .unwrap();

        assert_eq!(result.status, "waived");
    }

    #[tokio::test]
    async fn test_execute_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // First approve it
        let approved = state
            .compensation_action_service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute the action via the API
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };
        let result = execute_compensation_action(State(state), Path(approved.id), Json(request))
            .await
            .unwrap();

        assert_eq!(result.status, "executed");
        assert_eq!(result.executed_by, Some("test-executor".to_string()));
    }

    #[tokio::test]
    async fn test_execute_compensation_action_fails_on_pending() {
        let state = create_test_service_with_executor();

        // Create a compensation action (starts in Pending status)
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Try to execute without approval - should fail
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };
        let result =
            execute_compensation_action(State(state), Path(created.id), Json(request)).await;

        assert!(result.is_err());
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_compensation_action_response_serialization() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );

        let response = CompensationActionResponse::from(action);

        assert_eq!(response.status, "pending");
        assert_eq!(response.strategy_type, "rollback");
        assert_eq!(response.feasibility, "manual_only");
        assert_eq!(response.tenant_id, tenant_id);
        assert_eq!(response.intent_id, intent_id);
    }

    // =========================================================================
    // Orchestration Dashboard Tests (Phase 3 Batch 1 bounded read-only slice)
    // =========================================================================

    #[tokio::test]
    async fn test_orchestration_dashboard_empty_state() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return even with no data");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.tenant_id, tenant_id);
        assert!(result.side_effects.is_empty());
        assert_eq!(result.side_effect_summary.total, 0);
        assert!(result.compensation_actions.is_empty());
        assert_eq!(result.compensation_action_summary.total, 0);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_with_side_effects() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Record some side effects
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "metadata_write",
                "db-record-123",
            )
            .await
            .unwrap();

        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S4Irreversible,
                "money_transfer",
                "account-xyz",
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return data");

        assert_eq!(result.side_effects.len(), 2);
        assert_eq!(result.side_effect_summary.total, 2);
        assert_eq!(result.side_effect_summary.irreversible_count, 1);
        assert_eq!(result.side_effect_summary.auto_compensatable_count, 1);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_with_compensation_actions() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create actions in different statuses
        // Pending action
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let pending_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        state
            .compensation_action_service
            .create_action(pending_action)
            .await
            .unwrap();

        // Approved + Automatic action (auto-executable)
        let approved_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback 2",
        );
        let approved = state
            .compensation_action_service
            .create_action(approved_action)
            .await
            .unwrap();
        state
            .compensation_action_service
            .approve_action(approved.id, approved.lock_version, Some("test"))
            .await
            .unwrap();

        // Failed + retryable error (reapprovable)
        let failed_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback 3",
        );
        let failed = state
            .compensation_action_service
            .create_action(failed_action)
            .await
            .unwrap();
        // Approve then fail with retryable error
        let failed_approved = state
            .compensation_action_service
            .approve_action(failed.id, failed.lock_version, Some("test"))
            .await
            .unwrap();
        let failed_result = compensation_service::ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                failed_approved.id,
                &failed_result,
                failed_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return data");

        assert_eq!(result.compensation_actions.len(), 3);
        assert_eq!(result.compensation_action_summary.total, 3);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
        assert_eq!(result.compensation_action_summary.status_counts.approved, 1);
        assert_eq!(result.compensation_action_summary.status_counts.failed, 1);
        assert_eq!(result.compensation_action_summary.retryable_failed_count, 1);
        assert_eq!(result.compensation_action_summary.reapprovable_count, 1);
        assert_eq!(result.compensation_action_summary.auto_executable_count, 1);
        // Approved + Automatic
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_dlq_candidates() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create a failed action with non-retryable error (DLQ candidate)
        let dlq_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        let dlq = state
            .compensation_action_service
            .create_action(dlq_action)
            .await
            .unwrap();
        // Approve then fail with non-retryable error
        let dlq_approved = state
            .compensation_action_service
            .approve_action(dlq.id, dlq.lock_version, Some("test"))
            .await
            .unwrap();
        let dlq_result = compensation_service::ExecutionResult::failure(
            "Permanent failure",
            "INVALID_CONFIGURATION",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                dlq_approved.id,
                &dlq_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return data");

        assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
        // Non-retryable error + exhausted budget = DLQ candidate, not reapprovable
        assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_exhausted_budget_dlq() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with max_retries = 1
        let mut dlq_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        dlq_action.max_retries = 1; // Exhaust on first failure

        let dlq = state
            .compensation_action_service
            .create_action(dlq_action)
            .await
            .unwrap();
        // Approve then fail with retryable error (but budget exhausted)
        let dlq_approved = state
            .compensation_action_service
            .approve_action(dlq.id, dlq.lock_version, Some("test"))
            .await
            .unwrap();
        let dlq_result = compensation_service::ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                dlq_approved.id,
                &dlq_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return data");

        // Exhausted budget makes it a DLQ candidate even with retryable error
        assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
        assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_response_shape() {
        use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};

        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create a side effect
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S2ExternalReversible,
                "pr_opened",
                "https://github.com/example/pull/123",
            )
            .await
            .unwrap();

        // Create a compensation action
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::FollowupNotice,
            "Send follow-up",
        );
        state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result =
            get_orchestration_dashboard(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .expect("Dashboard should return data");

        // Verify response structure
        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.tenant_id, tenant_id);
        assert_eq!(result.side_effects.len(), 1);
        assert_eq!(result.compensation_actions.len(), 1);

        // Verify side effect summary
        assert_eq!(result.side_effect_summary.total, 1);
        assert_eq!(result.side_effect_summary.irreversible_count, 0);
        assert_eq!(result.side_effect_summary.auto_compensatable_count, 0); // S2 is not auto

        // Verify compensation action summary
        assert_eq!(result.compensation_action_summary.total, 1);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
        assert_eq!(result.compensation_action_summary.auto_executable_count, 0);
        // SemiAutomatic is not auto
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_tenant_isolation() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Record side effects for tenant 1
        state
            .side_effect_service
            .record_side_effect(
                tenant_id_1,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "effect_1",
                "target_1",
            )
            .await
            .unwrap();

        // Record side effects for tenant 2
        state
            .side_effect_service
            .record_side_effect(
                tenant_id_2,
                intent_id,
                1,
                compensation_service::SideEffectClass::S2ExternalReversible,
                "effect_2",
                "target_2",
            )
            .await
            .unwrap();

        // Query for tenant 1
        let query1 = OrchestrationDashboardQuery {
            tenant_id: tenant_id_1,
        };
        let result1 = get_orchestration_dashboard(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query1),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result1.side_effect_summary.total, 1);
        assert_eq!(result1.side_effects[0].effect_type, "effect_1");

        // Query for tenant 2
        let query2 = OrchestrationDashboardQuery {
            tenant_id: tenant_id_2,
        };
        let result2 = get_orchestration_dashboard(
            State(state),
            Path(intent_id),
            axum::extract::Query(query2),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result2.side_effect_summary.total, 1);
        assert_eq!(result2.side_effects[0].effect_type, "effect_2");
    }

    // === Audit Event Cross-Tenant Isolation Tests ===

    #[tokio::test]
    async fn test_list_audit_events_returns_only_tenant_events() {
        let state = create_test_service();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create audit events for tenant 1
        let event1 = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_1,
            event_type: intent_rebase_types::AuditEventType::RebaseApplied,
            actor_id: "test-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({ "test": "event1" }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(event1.clone())
            .await
            .unwrap();

        let event2 = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_1,
            event_type: intent_rebase_types::AuditEventType::ApprovalGranted,
            actor_id: "test-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({ "test": "event2" }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(event2.clone())
            .await
            .unwrap();

        // Create audit event for tenant 2
        let event3 = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_2,
            event_type: intent_rebase_types::AuditEventType::RebaseApplied,
            actor_id: "other-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({ "test": "event3" }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(event3.clone())
            .await
            .unwrap();

        // List events for tenant 1
        let query1 = ListAuditEventsQuery {
            tenant_id: tenant_1,
            limit: 100,
        };
        let result1 = list_audit_events(State(state.clone()), axum::extract::Query(query1))
            .await
            .expect("Should return events");

        // Should only see tenant 1's events
        assert_eq!(result1.events.len(), 2);
        assert!(result1.events.iter().all(|e| e.tenant_id == tenant_1));
        assert!(result1.events.iter().any(|e| e.id == event1.id));
        assert!(result1.events.iter().any(|e| e.id == event2.id));
        assert!(result1.events.iter().find(|e| e.id == event3.id).is_none());
    }

    #[tokio::test]
    async fn test_list_audit_events_respects_limit() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        // Create 5 audit events
        for i in 0..5 {
            let event = intent_rebase_types::AuditEvent {
                id: Uuid::new_v4(),
                tenant_id,
                event_type: intent_rebase_types::AuditEventType::RebaseApplied,
                actor_id: "test-user".to_string(),
                intent_id: Some(Uuid::new_v4()),
                artifact_id: None,
                payload: serde_json::json!({ "index": i }),
                trace_id: None,
                span_id: None,
                occurred_at: chrono::Utc::now(),
            };
            state
                .audit_service
                .create_audit_event(event)
                .await
                .unwrap();
        }

        // Query with limit=3
        let query = ListAuditEventsQuery {
            tenant_id,
            limit: 3,
        };
        let result = list_audit_events(State(state), axum::extract::Query(query))
            .await
            .expect("Should return events");

        assert_eq!(result.events.len(), 3);
        assert_eq!(result.total, 3);
    }

    #[tokio::test]
    async fn test_get_audit_event_returns_event_for_correct_tenant() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        let event = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: intent_rebase_types::AuditEventType::RebaseApplied,
            actor_id: "test-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({ "test": "data" }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(event.clone())
            .await
            .unwrap();

        // Get event with correct tenant
        let query = GetAuditEventQuery { tenant_id };
        let result = get_audit_event(State(state.clone()), Path(event.id), axum::extract::Query(query))
            .await
            .expect("Should return event");

        assert_eq!(result.id, event.id);
        assert_eq!(result.tenant_id, tenant_id);
    }

    #[tokio::test]
    async fn test_get_audit_event_blocked_for_wrong_tenant() {
        let state = create_test_service();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create event for tenant 1
        let event = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_1,
            event_type: intent_rebase_types::AuditEventType::RebaseApplied,
            actor_id: "test-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({ "test": "data" }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(event.clone())
            .await
            .unwrap();

        // Try to get event with tenant 2's credentials - should get 404
        let query = GetAuditEventQuery { tenant_id: tenant_2 };
        let result = get_audit_event(State(state), Path(event.id), axum::extract::Query(query)).await;

        // Should return error (404 via ApiErrorResponse)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_audit_event_not_found_for_nonexistent_event() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let nonexistent_event_id = Uuid::new_v4();

        let query = GetAuditEventQuery { tenant_id };
        let result = get_audit_event(State(state), Path(nonexistent_event_id), axum::extract::Query(query)).await;

        // Should return error (404)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cross_tenant_audit_events_completely_isolated() {
        // This test verifies that even if we create events for multiple tenants,
        // querying with the wrong tenant_id never leaks any data
        let state = create_test_service();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Create secret event for tenant A
        let secret_event = intent_rebase_types::AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_a,
            event_type: intent_rebase_types::AuditEventType::ArtifactProduced,
            actor_id: "secret-actor".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: Some(Uuid::new_v4()),
            payload: serde_json::json!({
                "secret_data": "this should never be visible to tenant b",
                "sensitive_field": "classified"
            }),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };
        state
            .audit_service
            .create_audit_event(secret_event.clone())
            .await
            .unwrap();

        // Tenant B lists events - should see nothing
        let list_query = ListAuditEventsQuery {
            tenant_id: tenant_b,
            limit: 100,
        };
        let list_result = list_audit_events(State(state.clone()), axum::extract::Query(list_query))
            .await
            .expect("Should return events (empty list is valid)");

        assert!(list_result.events.is_empty());
        assert_eq!(list_result.total, 0);

        // Tenant B tries to get the secret event directly - should get 404
        let get_query = GetAuditEventQuery { tenant_id: tenant_b };
        let get_result = get_audit_event(State(state.clone()), Path(secret_event.id), axum::extract::Query(get_query)).await;

        assert!(get_result.is_err());

        // Verify the secret event still exists and is accessible with correct tenant
        let get_query_a = GetAuditEventQuery { tenant_id: tenant_a };
        let get_result_a = get_audit_event(
            State(state),
            Path(secret_event.id),
            axum::extract::Query(get_query_a),
        )
        .await;

        assert!(get_result_a.is_ok());
        let retrieved = get_result_a.unwrap();
        assert_eq!(retrieved.payload, secret_event.payload);
    }
}
