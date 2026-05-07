//! Policy snapshot handlers (Phase 2 bounded read-only slice)
//!
//! Extracted from lib.rs as a bounded handler decomposition slice.

use crate::PolicySnapshotResponse;
use crate::{ApiErrorResponse, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

/// GET /policy-snapshots/{id} - Get a policy snapshot by ID
pub async fn get_policy_snapshot(
    State(state): State<AppState>,
    Path(snapshot_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<super::GetPolicySnapshotQuery>,
) -> Result<Json<PolicySnapshotResponse>, ApiErrorResponse> {
    let snapshot = state
        .policy_snapshot_repo
        .get_snapshot(snapshot_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(PolicySnapshotResponse::from(snapshot)))
}

/// GET /policy-snapshots/intent/{intent_id}/latest - Get latest policy snapshot for an intent
pub async fn get_latest_policy_snapshot(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<super::GetLatestPolicySnapshotQuery>,
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
pub async fn get_policy_snapshot_by_version(
    State(state): State<AppState>,
    Path((intent_id, version)): Path<(Uuid, i32)>,
    axum::extract::Query(query): axum::extract::Query<super::GetPolicySnapshotByVersionQuery>,
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
pub async fn list_policy_snapshots(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<super::ListPolicySnapshotsQuery>,
) -> Result<Json<super::ListPolicySnapshotsResponse>, ApiErrorResponse> {
    let snapshots = state
        .policy_snapshot_repo
        .list_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    let responses: Vec<PolicySnapshotResponse> = snapshots
        .into_iter()
        .map(PolicySnapshotResponse::from)
        .collect();

    Ok(Json(super::ListPolicySnapshotsResponse {
        total: responses.len(),
        policy_snapshots: responses,
    }))
}
