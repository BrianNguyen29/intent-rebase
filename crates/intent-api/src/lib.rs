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
use intent_rebase_types::{
    CreateIntentRequest, CreateIntentResponse, CreateVersionRequest, CreateVersionResponse,
    DiffRequest, IntentHeadResponse, IntentRebaseError, IntentVersion, ListVersionsResponse,
};
use intent_service::IntentService;
use rebase_engine::{DecisionClass, DiffRiskAnalysis, IntentVersionDiff, SectionDecision};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

/// Response for diff computation including version context, diff, and risk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub diff: IntentVersionDiff,
    pub risk: DiffRiskAnalysis,
}

/// Response for rebase preview (Phase 1 - preview-only baseline)
///
/// Exposes semantically reliable planner summary fields only.
/// Does NOT include placeholder fields like `affected_items` or `deferred`
/// which require graph integration (Phase 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebasePreviewResponse {
    pub intent_id: Uuid,
    pub from_version: IntentVersion,
    pub to_version: IntentVersion,
    pub decision_class: DecisionClass,
    pub rationale: String,
    pub section_decisions: Vec<SectionDecision>,
    pub manual_review_recommended: bool,
    pub risk_level: u8,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub service: Arc<intent_service::IntentService>,
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

/// POST /intents - Create a new intent
async fn create_intent(
    State(state): State<AppState>,
    Json(request): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), ApiErrorResponse> {
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
/// If provided, enables optimistic concurrency control. Returns 409 on conflict.
/// If headers are malformed (non-integer), returns 400 Bad Request.
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

/// POST /intents/{intent_id}/rebase-preview - Generate rebase preview plan
///
/// Request body: { from_version, to_version }
/// Response: rebase preview with decision class, rationale, and section decisions
///
/// Phase 1 preview-only endpoint. Does NOT expose:
/// - affected_items (requires graph integration - Phase 2)
/// - deferred fields (Phase 2)
async fn rebase_preview(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
    let plan = state
        .service
        .compute_rebase_preview(intent_id, request.from_version, request.to_version)
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
        manual_review_recommended: plan.manual_review_recommended,
        risk_level: plan.risk_level,
    }))
}

/// Build the Phase 1 router with CORS enabled
pub fn build_router(service: Arc<IntentService>) -> Router {
    let state = AppState { service };

    Router::new()
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
        .with_state(state)
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use intent_service::{InMemoryIntentRepository, IntentService};
    use std::sync::Arc;

    fn create_test_service() -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = Arc::new(IntentService::new(repo));
        AppState { service }
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
            .with_state(state);
        // Router builds successfully - this is a compile-time check essentially
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
}
