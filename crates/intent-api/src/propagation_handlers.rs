//! Propagation handlers for propagation-status and propagation-signal-ingestion endpoints.
//!
//! S6 decomposition slice: Extracted from `query_handlers.rs` as a bounded
//! mechanical decomposition with no behavior changes.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        DownstreamSystemStatus, IngestPropagationSignalRequest, IngestPropagationSignalResponse,
        PropagationStatusQuery, PropagationStatusResponse, PropagationSummary,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Propagation Status Handler (Phase 4+ design-only; bounded stub endpoint)
// ============================================================================

/// GET /intents/{intent_id}/propagation-status — Bounded stub endpoint.
///
/// Returns a contract-shaped response with empty downstream_systems and zeroed
/// summary. Full implementation (webhook delivery, event streaming, cross-workflow
/// lineage) is Phase 4+ deferred scope.
#[cfg(feature = "jwt-auth")]
pub async fn get_propagation_status(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<PropagationStatusQuery>,
) -> Result<Json<PropagationStatusResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("get_propagation_status: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_propagation_status: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Slice 1: If propagation record repo is available, query real records;
    // otherwise fall back to empty stub (preserves backward compatibility)
    let response = if let Some(ref repo) = state.propagation_record_repo {
        let records = repo
            .list_by_intent(intent_id, query.tenant_id)
            .await
            .map_err(ApiErrorResponse)?;

        let downstream_systems: Vec<DownstreamSystemStatus> = records
            .iter()
            .map(|r| DownstreamSystemStatus {
                system_id: r.downstream_system_id.clone(),
                acknowledged_at: r.acknowledged_at,
                status: format!("{:?}", r.status).to_lowercase(),
                last_seen_version: r.last_seen_version,
            })
            .collect();

        let total = downstream_systems.len();
        let acknowledged = downstream_systems
            .iter()
            .filter(|s| s.status == "acknowledged")
            .count();
        let pending = downstream_systems
            .iter()
            .filter(|s| s.status == "pending")
            .count();
        let failed = downstream_systems
            .iter()
            .filter(|s| s.status == "failed")
            .count();

        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems,
            propagation_summary: PropagationSummary {
                total,
                acknowledged,
                pending,
                failed,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    } else {
        // Bounded stub fallback when repository is not configured
        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems: vec![],
            propagation_summary: PropagationSummary {
                total: 0,
                acknowledged: 0,
                pending: 0,
                failed: 0,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    };

    Ok(Json(response))
}

/// GET /intents/{intent_id}/propagation-status — Bounded stub endpoint (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn get_propagation_status(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<PropagationStatusQuery>,
) -> Result<Json<PropagationStatusResponse>, ApiErrorResponse> {
    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != query.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match query tenant_id ({})",
            intent_head.intent.tenant_id, query.tenant_id
        );
        tracing::warn!("get_propagation_status: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Slice 1: If propagation record repo is available, query real records;
    // otherwise fall back to empty stub (preserves backward compatibility)
    let response = if let Some(ref repo) = state.propagation_record_repo {
        let records = repo
            .list_by_intent(intent_id, query.tenant_id)
            .await
            .map_err(ApiErrorResponse)?;

        let downstream_systems: Vec<DownstreamSystemStatus> = records
            .iter()
            .map(|r| DownstreamSystemStatus {
                system_id: r.downstream_system_id.clone(),
                acknowledged_at: r.acknowledged_at,
                status: format!("{:?}", r.status).to_lowercase(),
                last_seen_version: r.last_seen_version,
            })
            .collect();

        let total = downstream_systems.len();
        let acknowledged = downstream_systems
            .iter()
            .filter(|s| s.status == "acknowledged")
            .count();
        let pending = downstream_systems
            .iter()
            .filter(|s| s.status == "pending")
            .count();
        let failed = downstream_systems
            .iter()
            .filter(|s| s.status == "failed")
            .count();

        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems,
            propagation_summary: PropagationSummary {
                total,
                acknowledged,
                pending,
                failed,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    } else {
        // Bounded stub fallback when repository is not configured
        PropagationStatusResponse {
            intent_id,
            tenant_id: query.tenant_id,
            downstream_systems: vec![],
            propagation_summary: PropagationSummary {
                total: 0,
                acknowledged: 0,
                pending: 0,
                failed: 0,
            },
            unsupported_items: vec![
                "webhook subscription management".to_string(),
                "event streaming acknowledgment".to_string(),
                "cross-workflow lineage propagation".to_string(),
                "real-time propagation monitoring".to_string(),
            ],
        }
    };

    Ok(Json(response))
}

// ============================================================================
// Propagation Signal Ingestion Handler (Slice 2 bounded)
// ============================================================================

/// POST /intents/{intent_id}/propagation-signals — Bounded signal ingestion.
///
/// Records that a downstream system has been signaled for an intent change.
/// This is a bounded internal API — no actual webhook delivery or event
/// streaming occurs. The record is created with status `pending`.
///
/// When `state.rls_pool` is Some AND valid JWT claims are present, this handler
/// uses RLS-aware transaction wrapping for tenant isolation. Falls back to
/// non-RLS path when no JWT claims are present (backward compatible).
#[cfg(feature = "jwt-auth")]
pub async fn ingest_propagation_signal(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(body): Json<IngestPropagationSignalRequest>,
) -> Result<Json<IngestPropagationSignalResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(ref rls_claims) = optional_rls_claims {
        if body.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match body tenant_id ({})",
                rls_claims.tenant_id, body.tenant_id
            );
            tracing::warn!("ingest_propagation_signal: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != body.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match body tenant_id ({})",
            intent_head.intent.tenant_id, body.tenant_id
        );
        tracing::warn!("ingest_propagation_signal: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Require propagation_record_repo to be configured
    let repo = state.propagation_record_repo.ok_or_else(|| {
        ApiErrorResponse(IntentRebaseError::Internal(
            "Propagation record repository not configured".to_string(),
        ))
    })?;

    let record = intent_rebase_types::PropagationRecord::new(
        body.tenant_id,
        intent_id,
        body.downstream_system_id.clone(),
    );

    // RLS path: if pool exists AND JWT claims present
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        let record = repo
            .create_record_with_tx(&mut tx, record)
            .await
            .map_err(ApiErrorResponse)?;

        let commit_result = tx.commit().await;
        if let Err(e) = commit_result {
            return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                "failed to commit RLS transaction: {}",
                e
            ))));
        }

        tracing::debug!(
            "ingest_propagation_signal: RLS path success for tenant_id={}",
            rls_claims.tenant_id
        );

        return Ok(Json(IngestPropagationSignalResponse {
            record_id: record.id,
            intent_id,
            tenant_id: body.tenant_id,
            downstream_system_id: body.downstream_system_id,
            status: format!("{:?}", record.status).to_lowercase(),
        }));
    }

    // Fallback non-RLS path (backward compatible)
    let record = repo.create_record(record).await.map_err(ApiErrorResponse)?;

    Ok(Json(IngestPropagationSignalResponse {
        record_id: record.id,
        intent_id,
        tenant_id: body.tenant_id,
        downstream_system_id: body.downstream_system_id,
        status: format!("{:?}", record.status).to_lowercase(),
    }))
}

/// POST /intents/{intent_id}/propagation-signals — Bounded signal ingestion (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn ingest_propagation_signal(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(body): Json<IngestPropagationSignalRequest>,
) -> Result<Json<IngestPropagationSignalResponse>, ApiErrorResponse> {
    // Verify intent exists for tenant validation
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    if intent_head.intent.tenant_id != body.tenant_id {
        let msg = format!(
            "Tenant mismatch: intent tenant_id ({}) does not match body tenant_id ({})",
            intent_head.intent.tenant_id, body.tenant_id
        );
        tracing::warn!("ingest_propagation_signal: intent tenant mismatch rejection");
        return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
    }

    // Require propagation_record_repo to be configured
    let repo = state.propagation_record_repo.ok_or_else(|| {
        ApiErrorResponse(IntentRebaseError::Internal(
            "Propagation record repository not configured".to_string(),
        ))
    })?;

    let record = intent_rebase_types::PropagationRecord::new(
        body.tenant_id,
        intent_id,
        body.downstream_system_id.clone(),
    );
    let record = repo.create_record(record).await.map_err(ApiErrorResponse)?;

    Ok(Json(IngestPropagationSignalResponse {
        record_id: record.id,
        intent_id,
        tenant_id: body.tenant_id,
        downstream_system_id: body.downstream_system_id,
        status: format!("{:?}", record.status).to_lowercase(),
    }))
}
