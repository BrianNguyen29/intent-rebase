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
    IntentHeadResponse, IntentRebaseError, IntentVersion, ListVersionsResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

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
            IntentRebaseError::InvalidIntentVersion(_) => {
                (StatusCode::NOT_FOUND, "VERSION_NOT_FOUND", false)
            }
            IntentRebaseError::StorageError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR", true)
            }
            IntentRebaseError::SerializationError(_) => {
                (StatusCode::BAD_REQUEST, "SERIALIZATION_ERROR", false)
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
            IntentRebaseError::BrokerError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "BROKER_ERROR", true)
            }
            IntentRebaseError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", false)
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
async fn create_version(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<CreateVersionResponse>), ApiErrorResponse> {
    let expected_version = parse_optional_header(&headers, "x-expected-version");
    let expected_row_version = parse_optional_header(&headers, "x-expected-row-version");

    state
        .service
        .create_version(intent_id, request, expected_version, expected_row_version)
        .await
        .map(|r| (StatusCode::CREATED, Json(r)))
        .map_err(ApiErrorResponse)
}

/// Parse an optional i32 header value
fn parse_optional_header(headers: &HeaderMap, name: &str) -> Option<i32> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
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

/// Build the Phase 1 router with CORS enabled
pub fn build_router(service: Arc<intent_service::IntentService>) -> Router {
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
        .with_state(state)
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
