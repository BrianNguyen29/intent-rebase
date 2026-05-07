//! Intent read-only query handlers.
//!
//! Phase 2 bounded slice: Contains GET handlers for reading intent data
//! including intent head, version list, and specific version retrieval.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::{IntentHeadResponse, IntentVersion, ListVersionsResponse};
use uuid::Uuid;

use crate::{ApiErrorResponse, AppState};

// ============================================================================
// Intent Read-Only Query Handlers (Phase 2 bounded slice)
// ============================================================================

/// GET /intents/{intent_id} - Get intent head (current version)
///
/// Returns the current (head) version of an intent by its ID.
/// This is a read-only query that does not modify state.
pub async fn get_intent_head(
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

/// GET /intents/{intent_id}/versions - List all versions (descending order)
///
/// Returns all versions of an intent in descending order (newest first).
/// This is a read-only query that does not modify state.
pub async fn list_versions(
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
///
/// Returns a specific version of an intent by its intent_id and version_number.
/// This is a read-only query that does not modify state.
pub async fn get_version(
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
