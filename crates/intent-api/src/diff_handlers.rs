//! Diff computation handlers.
//!
//! Bounded handler decomposition slice: Contains the compute_diff handler
//! for computing diffs between intent versions.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::DiffRequest;
use uuid::Uuid;

use crate::{ApiErrorResponse, AppState, DiffResponse};

// ============================================================================
// Diff Computation Handler
// ============================================================================

/// Record diff compute duration
pub(crate) fn record_diff_compute_duration(duration_secs: f64) {
    metrics::histogram!("intent_api_diff_compute_duration_seconds").record(duration_secs);
}

/// POST /intents/{intent_id}/diff - Compute diff between two versions
///
/// Request body: { from_version, to_version }
/// Response: version context plus diff and risk analysis
pub async fn compute_diff(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<DiffResponse>, ApiErrorResponse> {
    let start = std::time::Instant::now();
    let result = state
        .service
        .compute_diff(intent_id, request.from_version, request.to_version)
        .await;

    let duration = start.elapsed().as_secs_f64();
    record_diff_compute_duration(duration);

    match result {
        Ok((from_version, to_version, diff, risk)) => Ok(Json(DiffResponse {
            intent_id,
            from_version,
            to_version,
            diff,
            risk,
        })),
        Err(e) => Err(ApiErrorResponse(e)),
    }
}
