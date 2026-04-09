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
use graph_service::GraphService;
use intent_rebase_types::{
    AffectedItemsPreview, CreateGraphEdgeRequest, CreateGraphNodeRequest, CreateIntentRequest,
    CreateIntentResponse, CreateVersionRequest, CreateVersionResponse, DiffRequest, EdgeType,
    GraphEdge, GraphNode, IntentHeadResponse, IntentRebaseError, IntentVersion,
    ListVersionsResponse, NodeType, PolicySnapshot, ValidateIntentResponse,
};
use intent_service::{ApprovalRequest, ApprovalRequestStatus, IntentService};
use metrics_exporter_prometheus::PrometheusBuilder;
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
}

/// Response for rebase apply.
///
/// Phase 2b: `risk_tier` is the canonical public risk enum field (Low/Medium/High/Critical).
/// `risk_level` (u8 1-5) and `decision_class` remain as supporting fields.
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
            audit_payload,
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
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
                blocked_payload,
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
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
            audit_payload,
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalGranted audit event: {:?}", e);
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
            audit_payload,
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRevoked audit event: {:?}", e);
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
            audit_payload,
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
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

/// Build the Phase 1 router with CORS enabled
pub fn build_router(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
) -> Router {
    let state = AppState {
        service,
        graph_service,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
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
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route("/v1/graph/nodes", post(create_graph_node))
        .route("/v1/graph/nodes", get(list_graph_nodes))
        .route("/v1/graph/nodes/{node_id}", get(get_graph_node))
        .route("/v1/graph/edges", post(create_graph_edge))
        .route("/v1/graph/edges", get(list_graph_edges))
        .route("/v1/graph/nodes/{node_id}/edges", get(list_edges_from_node))
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
/// );
/// ```
pub fn build_router_with_sql_audit_and_approval(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    orchestrator: Arc<RebaseOrchestrator>,
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
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
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
        AppState {
            service,
            graph_service: graph_svc,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
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
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            start_time: Instant::now(),
        };

        // Create an intent
        let create_request = CreateIntentRequest {
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
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            start_time: Instant::now(),
        };

        // Create an intent
        let create_request = CreateIntentRequest {
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
}
