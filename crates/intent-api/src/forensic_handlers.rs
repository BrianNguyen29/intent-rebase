//! Forensic handlers module
//!
//! Phase 3 Batch 3b + P4 bounded slices: Contains HTTP handlers for forensic bundle
//! operations including creation, listing, download, verification, and archive export.
//!
//! This module was extracted from lib.rs as a bounded handler decomposition slice.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        ForensicBundleContentsSummary, ForensicBundleIntegrityInfo, ForensicBundleReplayRequest,
        ForensicBundleReplayResponse, ForensicBundleRequest, ForensicBundleResponse,
        ForensicBundleSummary, ForensicBundleTimeRange, ForensicExportContentsSummary,
        ForensicExportRequest, ForensicExportResponse, ForensicExportTimeRange,
        ForensicVerificationRequest, ForensicVerificationResponse, ListForensicBundlesQuery,
        ListForensicBundlesResponse,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth::OptionalRlsTenantClaims;

// ============================================================================
// Forensic Bundle Handler (P4 bounded slice)
// ============================================================================

/// POST /forensic/bundle - Generate and store a forensic bundle
///
/// P4 (bounded slice): Collects real data from intent/graph/audit repositories,
/// generates a forensic bundle manifest with integrity hashes, persists the
/// bundle bytes to S3/MinIO, and records the bundle in the repository.
///
/// **Bounded synchronous path:**
/// 1. Collects intent versions, audit events, and policy snapshots via ForensicDataCollector
/// 2. Generates bundle manifest with integrity hashes via BundleGeneratorService
/// 3. Persists bundle JSON to S3/MinIO via BundleStorage
/// 4. Records bundle status=Ready in repository
///
/// **Truthful semantics:**
/// - `bundle_id` is a unique identifier for the persisted bundle
/// - `storage_location` shows where bundle bytes are stored (S3/MinIO path)
/// - `bundle_size_bytes` reflects the JSON-serialized bundle size
/// - `contents.*_count` reflects actual collected data counts
/// - `integrity.manifest_hash` is the SHA-256 of the serialized bundle manifest
///
/// **Tenant scoping:** All data collection is scoped to the provided `tenant_id`.
/// Cross-tenant access is denied by the collector.
///
/// **NOT claimed in this slice:**
/// - Async job orchestration for large bundle generation
/// - Bundle retrieval/download API (GET /forensic/bundle/{id}/download)
/// - Bundle replay (state reproduction from stored bundle)
/// - Hash chain integrity verification
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before creating the forensic bundle.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
#[cfg(feature = "jwt-auth")]
pub async fn create_forensic_bundle(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    Json(request): Json<ForensicBundleRequest>,
) -> Result<(axum::http::StatusCode, Json<ForensicBundleResponse>), ApiErrorResponse> {
    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(ref rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("create_forensic_bundle: tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    let service_request = forensic_service::CreateForensicBundleRequest {
        tenant_id: request.tenant_id,
        intent_ids: request.intent_ids.clone(),
        time_range: forensic_service::BundleTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        created_by: request.created_by.clone(),
    };

    // P4 bounded RLS slice: Try RLS path if pool + SQL service available and JWT present
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, optional_rls_claims) {
        if let Some(sql_service) = state.forensic_bundle_service.as_bundle_service_sqlx() {
            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(crate::IntentRebaseError::Internal(
                        format!("failed to begin RLS transaction: {}", e),
                    )));
                }
            };

            let response = match sql_service
                .create_bundle_with_tx(&mut tx, service_request)
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    return Err(match e {
                        forensic_service::ForensicBundleServiceError::NotFound(_) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(e.to_string()))
                        }
                        forensic_service::ForensicBundleServiceError::Collection(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "collection failed: {}",
                                e
                            )))
                        }
                        forensic_service::ForensicBundleServiceError::Generation(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "generation failed: {}",
                                e
                            )))
                        }
                        forensic_service::ForensicBundleServiceError::Storage(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "storage failed: {}",
                                e
                            )))
                        }
                        forensic_service::ForensicBundleServiceError::Repository(e) => {
                            ApiErrorResponse(e)
                        }
                        forensic_service::ForensicBundleServiceError::Serialization(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "serialization failed: {}",
                                e
                            )))
                        }
                        forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "invalid time range: {}",
                                e
                            )))
                        }
                        forensic_service::ForensicBundleServiceError::Replay(e) => {
                            ApiErrorResponse(crate::IntentRebaseError::Internal(format!(
                                "replay verification failed: {}",
                                e
                            )))
                        }
                    })
                }
            };

            if let Err(e) = tx.commit().await {
                return Err(ApiErrorResponse(crate::IntentRebaseError::StorageError(
                    format!("failed to commit RLS transaction: {}", e),
                )));
            }

            return Ok((
                axum::http::StatusCode::CREATED,
                Json(ForensicBundleResponse {
                    bundle_id: response.bundle.bundle_id,
                    created_at: response.bundle.created_at,
                    created_by: response.bundle.created_by,
                    tenant_id: response.bundle.tenant_id,
                    time_range: ForensicBundleTimeRange {
                        start: response.bundle.time_range.start,
                        end: response.bundle.time_range.end,
                    },
                    status: response.bundle.status,
                    purpose: response.bundle.purpose,
                    contents: ForensicBundleContentsSummary {
                        intent_versions: response.bundle.contents.intent_versions,
                        artifacts: response.bundle.contents.artifacts,
                        approvals: response.bundle.contents.approvals,
                        audit_events: response.bundle.contents.audit_events,
                        policy_snapshots: response.bundle.contents.policy_snapshots,
                    },
                    integrity: ForensicBundleIntegrityInfo {
                        manifest_hash: response.bundle.integrity.manifest_hash,
                        chain_verified: response.bundle.integrity.chain_verified,
                        verification_timestamp: response.bundle.integrity.verification_timestamp,
                    },
                    storage_location: response.storage_location,
                    bundle_size_bytes: response.bundle_size_bytes,
                    message: response.message,
                }),
            ));
        }
    }

    let response = state
        .forensic_bundle_service
        .create_bundle(service_request)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(_) => {
                ApiErrorResponse(crate::IntentRebaseError::Internal(e.to_string()))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ForensicBundleResponse {
            bundle_id: response.bundle.bundle_id,
            created_at: response.bundle.created_at,
            created_by: response.bundle.created_by,
            tenant_id: response.bundle.tenant_id,
            time_range: ForensicBundleTimeRange {
                start: response.bundle.time_range.start,
                end: response.bundle.time_range.end,
            },
            status: response.bundle.status,
            purpose: response.bundle.purpose,
            contents: ForensicBundleContentsSummary {
                intent_versions: response.bundle.contents.intent_versions,
                artifacts: response.bundle.contents.artifacts,
                approvals: response.bundle.contents.approvals,
                audit_events: response.bundle.contents.audit_events,
                policy_snapshots: response.bundle.contents.policy_snapshots,
            },
            integrity: ForensicBundleIntegrityInfo {
                manifest_hash: response.bundle.integrity.manifest_hash,
                chain_verified: response.bundle.integrity.chain_verified,
                verification_timestamp: response.bundle.integrity.verification_timestamp,
            },
            storage_location: response.storage_location,
            bundle_size_bytes: response.bundle_size_bytes,
            message: response.message,
        }),
    ))
}

/// POST /forensic/bundle - Generate and store a forensic bundle (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_forensic_bundle(
    State(state): State<AppState>,
    Json(request): Json<ForensicBundleRequest>,
) -> Result<(axum::http::StatusCode, Json<ForensicBundleResponse>), ApiErrorResponse> {
    let service_request = forensic_service::CreateForensicBundleRequest {
        tenant_id: request.tenant_id,
        intent_ids: request.intent_ids.clone(),
        time_range: forensic_service::BundleTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        created_by: request.created_by.clone(),
    };

    let response = state
        .forensic_bundle_service
        .create_bundle(service_request)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(_) => {
                ApiErrorResponse(crate::IntentRebaseError::Internal(e.to_string()))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ForensicBundleResponse {
            bundle_id: response.bundle.bundle_id,
            created_at: response.bundle.created_at,
            created_by: response.bundle.created_by,
            tenant_id: response.bundle.tenant_id,
            time_range: ForensicBundleTimeRange {
                start: response.bundle.time_range.start,
                end: response.bundle.time_range.end,
            },
            status: response.bundle.status,
            purpose: response.bundle.purpose,
            contents: ForensicBundleContentsSummary {
                intent_versions: response.bundle.contents.intent_versions,
                artifacts: response.bundle.contents.artifacts,
                approvals: response.bundle.contents.approvals,
                audit_events: response.bundle.contents.audit_events,
                policy_snapshots: response.bundle.contents.policy_snapshots,
            },
            integrity: ForensicBundleIntegrityInfo {
                manifest_hash: response.bundle.integrity.manifest_hash,
                chain_verified: response.bundle.integrity.chain_verified,
                verification_timestamp: response.bundle.integrity.verification_timestamp,
            },
            storage_location: response.storage_location,
            bundle_size_bytes: response.bundle_size_bytes,
            message: response.message,
        }),
    ))
}

/// GET /forensic/bundles - List forensic bundles for a tenant
///
/// P4 (bounded slice): Lists all forensic bundles for the specified tenant.
///
/// **Bounded synchronous path:**
/// - Queries bundle repository for bundles matching the tenant
/// - Returns bundle summaries with metadata
///
/// **Tenant scoping:** Results are filtered by the provided `tenant_id`.
#[cfg(feature = "jwt-auth")]
pub async fn list_forensic_bundles(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    axum::extract::Query(query): axum::extract::Query<ListForensicBundlesQuery>,
) -> Result<Json<ListForensicBundlesResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(ref rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("list_forensic_bundles: tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    // P4 bounded RLS slice: Try RLS path if pool + SQL repo available and JWT present
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, optional_rls_claims) {
        if let Some(sql_repo) = state
            .forensic_bundle_service
            .repo()
            .and_then(|r| r.as_sqlx_repo())
        {
            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(crate::IntentRebaseError::Internal(
                        format!("failed to begin RLS transaction: {}", e),
                    )));
                }
            };

            let bundles = match sql_repo
                .list_by_tenant_with_tx(&mut tx, query.tenant_id, query.limit)
                .await
            {
                Ok(bundles) => bundles,
                Err(e) => {
                    return Err(ApiErrorResponse(e));
                }
            };

            if let Err(e) = tx.commit().await {
                return Err(ApiErrorResponse(crate::IntentRebaseError::StorageError(
                    format!("failed to commit RLS transaction: {}", e),
                )));
            }

            let total = bundles.len();
            let summaries: Vec<ForensicBundleSummary> = bundles
                .into_iter()
                .map(ForensicBundleSummary::from)
                .collect();

            return Ok(Json(ListForensicBundlesResponse {
                bundles: summaries,
                total,
            }));
        }
    }

    // Non-RLS fallback path
    let bundles = state
        .forensic_bundle_service
        .list_bundles(query.tenant_id, query.limit)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(_) => {
                ApiErrorResponse(crate::IntentRebaseError::Internal(e.to_string()))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    let total = bundles.len();
    let summaries: Vec<ForensicBundleSummary> = bundles
        .into_iter()
        .map(ForensicBundleSummary::from)
        .collect();

    Ok(Json(ListForensicBundlesResponse {
        bundles: summaries,
        total,
    }))
}

/// GET /forensic/bundles - List forensic bundles for a tenant (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn list_forensic_bundles(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListForensicBundlesQuery>,
) -> Result<Json<ListForensicBundlesResponse>, ApiErrorResponse> {
    let bundles = state
        .forensic_bundle_service
        .list_bundles(query.tenant_id, query.limit)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(_) => {
                ApiErrorResponse(crate::IntentRebaseError::Internal(e.to_string()))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    let total = bundles.len();
    let summaries: Vec<ForensicBundleSummary> = bundles
        .into_iter()
        .map(ForensicBundleSummary::from)
        .collect();

    Ok(Json(ListForensicBundlesResponse {
        bundles: summaries,
        total,
    }))
}

/// GET /forensic/bundles/{bundle_id}/download - Download a forensic bundle
///
/// P4 (bounded slice): Downloads the serialized bytes of a forensic bundle from storage.
///
/// **Bounded synchronous path:**
/// - Verifies bundle exists in repository and belongs to the requesting tenant
/// - Retrieves bundle bytes from S3/MinIO storage
/// - Returns bundle JSON as binary download
///
/// **Tenant scoping:** When JWT is present, verifies bundle tenant matches JWT tenant.
/// When RLS pool + SQL repo are available, uses RLS transaction for tenant verification.
///
/// **Response:** Raw JSON bytes with Content-Type: application/json
#[cfg(feature = "jwt-auth")]
pub async fn download_forensic_bundle(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    Path(bundle_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiErrorResponse> {
    // P4 bounded RLS slice: If JWT present, enforce tenant access via RLS tx when available
    if let Some(ref rls_claims) = optional_rls_claims {
        if let (Some(rls_pool), Some(sql_repo)) = (
            &state.rls_pool,
            state
                .forensic_bundle_service
                .repo()
                .and_then(|r| r.as_sqlx_repo()),
        ) {
            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(crate::IntentRebaseError::Internal(
                        format!("failed to begin RLS transaction: {}", e),
                    )));
                }
            };

            // RLS-enforced get: will fail with NotFound if bundle does not belong to tenant
            match sql_repo.get_with_tx(&mut tx, bundle_id).await {
                Ok(bundle) => {
                    if bundle.tenant_id != rls_claims.tenant_id {
                        let msg = format!(
                            "Tenant mismatch: bundle tenant_id ({}) does not match JWT tenant_id ({})",
                            bundle.tenant_id, rls_claims.tenant_id
                        );
                        tracing::warn!("download_forensic_bundle: tenant mismatch rejection");
                        return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                            msg,
                        )));
                    }
                    if let Err(e) = tx.commit().await {
                        return Err(ApiErrorResponse(crate::IntentRebaseError::StorageError(
                            format!("failed to commit RLS transaction: {}", e),
                        )));
                    }
                }
                Err(IntentRebaseError::ForensicBundleNotFound(_)) => {
                    return Err(ApiErrorResponse(
                        crate::IntentRebaseError::ForensicBundleNotFound(bundle_id),
                    ));
                }
                Err(e) => {
                    return Err(ApiErrorResponse(e));
                }
            }
        } else {
            // Non-RLS JWT path: verify bundle tenant via service get_bundle
            let bundle = state
                .forensic_bundle_service
                .get_bundle(bundle_id)
                .await
                .map_err(|e| match e {
                    forensic_service::ForensicBundleServiceError::NotFound(id) => {
                        ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
                    }
                    other => {
                        ApiErrorResponse(crate::IntentRebaseError::Internal(other.to_string()))
                    }
                })?;
            if bundle.tenant_id != rls_claims.tenant_id {
                let msg = format!(
                    "Tenant mismatch: bundle tenant_id ({}) does not match JWT tenant_id ({})",
                    bundle.tenant_id, rls_claims.tenant_id
                );
                tracing::warn!("download_forensic_bundle: tenant mismatch rejection");
                return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                    msg,
                )));
            }
        }
    }

    let bytes = state
        .forensic_bundle_service
        .download_bundle_bytes(bundle_id)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(id) => {
                ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    Ok(axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.json\"", bundle_id),
        )
        .body(axum::body::Body::from(bytes))
        .expect("Failed to build download response"))
}

/// GET /forensic/bundles/{bundle_id}/download - Download a forensic bundle (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn download_forensic_bundle(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiErrorResponse> {
    let bytes = state
        .forensic_bundle_service
        .download_bundle_bytes(bundle_id)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(id) => {
                ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::ForensicBundleServiceError::Collection(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("collection failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Generation(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("generation failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Storage(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("storage failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Repository(e) => ApiErrorResponse(e),
            forensic_service::ForensicBundleServiceError::Serialization(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("serialization failed: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::InvalidTimeRange(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("invalid time range: {}", e)),
            ),
            forensic_service::ForensicBundleServiceError::Replay(e) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", e)),
            ),
        })?;

    Ok(axum::response::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.json\"", bundle_id),
        )
        .body(axum::body::Body::from(bytes))
        .expect("Failed to build download response"))
}

// ============================================================================
// Forensic Bundle Replay Handler (Bounded replay evidence slice)
// ============================================================================

/// POST /forensic/bundles/{bundle_id}/replay-verify - Verify bundle integrity via replay
///
/// Bounded replay evidence slice: Verifies provided content sections against the
/// per-section integrity hashes stored in the bundle manifest.
///
/// **Bounded read-only path:**
/// 1. Loads the bundle manifest from the repository
/// 2. Recomputes hashes from the provided content sections
/// 3. Compares computed hashes against the stored integrity hashes
/// 4. Returns a per-section verification report
///
/// **What this IS:** read-only integrity verification using stored evidence.
/// **What this IS NOT:** full runtime replay, state reconstruction, or mutation.
///
/// **Tenant scoping:** When JWT is present, verifies the bundle belongs to the
/// requesting tenant before performing verification.
#[cfg(feature = "jwt-auth")]
pub async fn replay_verify_forensic_bundle(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    Path(bundle_id): Path<Uuid>,
    Json(request): Json<ForensicBundleReplayRequest>,
) -> Result<Json<ForensicBundleReplayResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(ref rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("replay_verify_forensic_bundle: tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    // Load bundle to validate tenant access
    let bundle = state
        .forensic_bundle_service
        .get_bundle(bundle_id)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(id) => {
                ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
            }
            other => ApiErrorResponse(crate::IntentRebaseError::Internal(other.to_string())),
        })?;

    // JWT tenant guard on loaded bundle
    if let Some(ref rls_claims) = optional_rls_claims {
        if bundle.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: bundle tenant_id ({}) does not match JWT tenant_id ({})",
                bundle.tenant_id, rls_claims.tenant_id
            );
            tracing::warn!("replay_verify_forensic_bundle: bundle tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    // Build content sections for verification
    let content_sections = forensic_service::ContentSectionsForVerification {
        intent_versions: forensic_service::IntentVersionsForHash {
            versions: request.intent_versions,
        },
        artifacts: forensic_service::ArtifactsForHash {
            artifacts: request.artifacts,
        },
        approvals: forensic_service::ApprovalsForHash {
            approvals: request.approvals,
        },
        audit_events: forensic_service::AuditEventsForHash {
            events: request.audit_events,
        },
        policy_snapshots: forensic_service::PolicySnapshotsForHash {
            snapshots: request.policy_snapshots,
        },
    };

    let result = state
        .forensic_bundle_service
        .verify_bundle_replay(bundle_id, content_sections)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(id) => {
                ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::ForensicBundleServiceError::Replay(msg) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", msg)),
            ),
            other => ApiErrorResponse(crate::IntentRebaseError::Internal(other.to_string())),
        })?;

    Ok(Json(ForensicBundleReplayResponse {
        bundle_id: result.bundle.bundle_id,
        overall_verified: result.report.overall_verified,
        sections_passed: result.report.sections_passed,
        sections_failed: result.report.sections_failed,
        summary: result.report.summary,
        sections: result.report.sections,
    }))
}

/// POST /forensic/bundles/{bundle_id}/replay-verify - Non-JWT fallback
#[cfg(not(feature = "jwt-auth"))]
pub async fn replay_verify_forensic_bundle(
    State(state): State<AppState>,
    Path(bundle_id): Path<Uuid>,
    Json(request): Json<ForensicBundleReplayRequest>,
) -> Result<Json<ForensicBundleReplayResponse>, ApiErrorResponse> {
    let content_sections = forensic_service::ContentSectionsForVerification {
        intent_versions: forensic_service::IntentVersionsForHash {
            versions: request.intent_versions,
        },
        artifacts: forensic_service::ArtifactsForHash {
            artifacts: request.artifacts,
        },
        approvals: forensic_service::ApprovalsForHash {
            approvals: request.approvals,
        },
        audit_events: forensic_service::AuditEventsForHash {
            events: request.audit_events,
        },
        policy_snapshots: forensic_service::PolicySnapshotsForHash {
            snapshots: request.policy_snapshots,
        },
    };

    let result = state
        .forensic_bundle_service
        .verify_bundle_replay(bundle_id, content_sections)
        .await
        .map_err(|e| match e {
            forensic_service::ForensicBundleServiceError::NotFound(id) => {
                ApiErrorResponse(crate::IntentRebaseError::ForensicBundleNotFound(id))
            }
            forensic_service::ForensicBundleServiceError::Replay(msg) => ApiErrorResponse(
                crate::IntentRebaseError::Internal(format!("replay verification failed: {}", msg)),
            ),
            other => ApiErrorResponse(crate::IntentRebaseError::Internal(other.to_string())),
        })?;

    Ok(Json(ForensicBundleReplayResponse {
        bundle_id: result.bundle.bundle_id,
        overall_verified: result.report.overall_verified,
        sections_passed: result.report.sections_passed,
        sections_failed: result.report.sections_failed,
        summary: result.report.summary,
        sections: result.report.sections,
    }))
}

// ============================================================================
// Forensic Verification Handler (Phase 3 Batch 3b bounded slice)
// ============================================================================

/// POST /forensic/verify - Verify forensic bundle feasibility
///
/// Phase 3 Batch 3b (bounded slice): Verifies whether a forensic bundle can be
/// generated for the given parameters WITHOUT generating actual bundles.
///
/// **Bounded request-driven verification:**
/// - Accepts verification parameters (intent_id, time_range, purpose)
/// - Validates entity existence and coverage
/// - Reports what a bundle WOULD contain (counts, not actual data)
/// - Does NOT generate bundles, store data, or perform replay
///
/// **Truthful status semantics:**
/// - `ready`: All referenced entities exist and are within time range
/// - `incomplete`: Some entities are missing or time range has gaps
/// - `not_supported`: Verification mode not implemented
///
/// **NOT claimed:**
/// - Bundle generation (actual data collection)
/// - Bundle storage (S3 or any persistence)
/// - Bundle retrieval (downloading stored bundles)
/// - Bundle replay (reproducing state from a bundle)
/// - Hash chain integrity verification (requires generated bundle)
#[cfg(feature = "jwt-auth")]
pub async fn verify_forensic_bundle(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    Json(request): Json<ForensicVerificationRequest>,
) -> Result<Json<ForensicVerificationResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("verify_forensic_bundle: tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    let service_request = forensic_service::ForensicVerificationRequest {
        tenant_id: request.tenant_id,
        intent_id: request.intent_id,
        time_range: forensic_service::VerificationTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        include_artifacts: request.include_artifacts,
        include_audit_events: request.include_audit_events,
        include_policy_snapshots: request.include_policy_snapshots,
    };

    let response = state.forensic_service.verify(service_request).await;

    Ok(Json(ForensicVerificationResponse::from(response)))
}

/// POST /forensic/verify - Verify forensic bundle feasibility (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn verify_forensic_bundle(
    State(state): State<AppState>,
    Json(request): Json<ForensicVerificationRequest>,
) -> Result<Json<ForensicVerificationResponse>, ApiErrorResponse> {
    let service_request = forensic_service::ForensicVerificationRequest {
        tenant_id: request.tenant_id,
        intent_id: request.intent_id,
        time_range: forensic_service::VerificationTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        include_artifacts: request.include_artifacts,
        include_audit_events: request.include_audit_events,
        include_policy_snapshots: request.include_policy_snapshots,
    };

    let response = state.forensic_service.verify(service_request).await;

    Ok(Json(ForensicVerificationResponse::from(response)))
}

/// POST /forensic/export - Generate forensic archive metadata
///
/// Phase 3 Batch 3b (bounded slice): Generates an in-memory forensic archive
/// from the given parameters. The archive contains scaffolded/fictional data
/// representing what a real bundle would contain.
///
/// **Bounded in-memory archive generation:**
/// - Accepts export parameters (intent_id, time_range, purpose)
/// - Generates scaffolded entries entirely in-memory (no real service queries)
/// - Returns archive metadata including size, content type, and item count
/// - Does NOT stream archive bytes in this bounded slice; response is metadata only
///
/// **Truthful semantics:**
/// - `archive_id` is a unique identifier for the generated archive
/// - `generated_at` timestamps when generation was triggered
/// - `item_count` reflects the count of scaffolded entries generated
/// - `archive_size_bytes` reflects the JSON-serialized size
///
/// **NOT claimed:**
/// - Actual bundle generation from real services (intent service, graph service, etc.)
/// - Bundle storage (S3 or any persistence layer)
/// - Async job orchestration for bundle generation
/// - Real replay engine (state reproduction from bundle)
/// - Hash chain integrity verification
#[cfg(feature = "jwt-auth")]
pub async fn export_forensic_archive(
    State(state): State<AppState>,
    OptionalRlsTenantClaims(optional_rls_claims): OptionalRlsTenantClaims,
    Json(request): Json<ForensicExportRequest>,
) -> Result<Json<ForensicExportResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("export_forensic_archive: tenant mismatch rejection");
            return Err(ApiErrorResponse(crate::IntentRebaseError::Unauthorized(
                msg,
            )));
        }
    }

    let service_request = forensic_service::ForensicExportRequest {
        tenant_id: request.tenant_id,
        intent_id: request.intent_id,
        time_range: forensic_service::ExportTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        include_artifacts: request.include_artifacts,
        include_audit_events: request.include_audit_events,
        include_policy_snapshots: request.include_policy_snapshots,
    };

    let response = state
        .forensic_archive_generator
        .generate(service_request)
        .await;

    Ok(Json(ForensicExportResponse {
        archive_id: response.archive_id,
        generated_at: response.generated_at,
        status: response.status,
        status_reason: response.status_reason,
        tenant_id: response.tenant_id,
        intent_id: response.intent_id,
        time_range: ForensicExportTimeRange {
            start: response.time_range.start,
            end: response.time_range.end,
        },
        purpose: response.purpose,
        contents: ForensicExportContentsSummary {
            intent_versions: response.contents.intent_versions,
            artifacts: response.contents.artifacts,
            audit_events: response.contents.audit_events,
            policy_snapshots: response.contents.policy_snapshots,
        },
        item_count: response.item_count,
        content_type: response.content_type,
        archive_size_bytes: response.archive_size_bytes,
    }))
}

/// POST /forensic/export - Generate forensic archive metadata (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn export_forensic_archive(
    State(state): State<AppState>,
    Json(request): Json<ForensicExportRequest>,
) -> Result<Json<ForensicExportResponse>, ApiErrorResponse> {
    let service_request = forensic_service::ForensicExportRequest {
        tenant_id: request.tenant_id,
        intent_id: request.intent_id,
        time_range: forensic_service::ExportTimeRange {
            start: request.time_range.start,
            end: request.time_range.end,
        },
        purpose: request.purpose,
        include_artifacts: request.include_artifacts,
        include_audit_events: request.include_audit_events,
        include_policy_snapshots: request.include_policy_snapshots,
    };

    let response = state
        .forensic_archive_generator
        .generate(service_request)
        .await;

    Ok(Json(ForensicExportResponse {
        archive_id: response.archive_id,
        generated_at: response.generated_at,
        status: response.status,
        status_reason: response.status_reason,
        tenant_id: response.tenant_id,
        intent_id: response.intent_id,
        time_range: ForensicExportTimeRange {
            start: response.time_range.start,
            end: response.time_range.end,
        },
        purpose: response.purpose,
        contents: ForensicExportContentsSummary {
            intent_versions: response.contents.intent_versions,
            artifacts: response.contents.artifacts,
            audit_events: response.contents.audit_events,
            policy_snapshots: response.contents.policy_snapshots,
        },
        item_count: response.item_count,
        content_type: response.content_type,
        archive_size_bytes: response.archive_size_bytes,
    }))
}
