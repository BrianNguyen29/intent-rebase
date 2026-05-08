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
use uuid::Uuid;

use crate::{
    types::{
        ForensicBundleContentsSummary, ForensicBundleIntegrityInfo, ForensicBundleRequest,
        ForensicBundleResponse, ForensicBundleSummary, ForensicBundleTimeRange,
        ForensicExportContentsSummary, ForensicExportRequest, ForensicExportResponse,
        ForensicExportTimeRange, ForensicVerificationRequest, ForensicVerificationResponse,
        ListForensicBundlesQuery, ListForensicBundlesResponse,
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
    if let Some(rls_claims) = optional_rls_claims {
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
    if let Some(rls_claims) = optional_rls_claims {
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
/// - Verifies bundle exists in repository
/// - Retrieves bundle bytes from S3/MinIO storage
/// - Returns bundle JSON as binary download
///
/// **Response:** Raw JSON bytes with Content-Type: application/json
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

// ============================================================================
// Tests for Forensic Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ForensicIntentVersionCoverage, ForensicVerificationTimeRange};
    use crate::{auth, RebaseOrchestrator};
    use chrono::Utc;
    use compensation_service::{
        CompensationActionService, InMemoryCompensationActionRepository,
        InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
    };
    use forensic_service::{
        ForensicBundleService, InMemoryBundleRepository, InMemoryBundleStorage,
        InMemoryForensicArchiveGenerator, InMemoryForensicDataCollector,
        InMemoryForensicVerificationService,
    };
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_rebase_types::InMemoryAuditRepository;
    use intent_service::{
        InMemoryApprovalRequestRepository, InMemoryCheckpointRepository, InMemoryIntentRepository,
        InMemoryPolicySnapshotRepository, IntentService,
    };
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;
    use std::time::Instant;
    use uuid::Uuid;

    /// Create minimal AppState for forensic handler tests
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
        let audit_repo = Arc::new(InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let side_effect_repo = Arc::new(InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        let compensation_action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let compensation_action_svc =
            Arc::new(CompensationActionService::new(compensation_action_repo));
        let orchestration_run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        let forensic_svc = Arc::new(InMemoryForensicVerificationService::new())
            as Arc<dyn forensic_service::ForensicVerificationService>;
        let forensic_archive_gen = Arc::new(InMemoryForensicArchiveGenerator::new());
        let forensic_bundle_svc = Arc::new(ForensicBundleService::new(
            Arc::new(InMemoryBundleRepository::new()),
            Arc::new(InMemoryBundleStorage::new("test-bucket")),
            Arc::new(InMemoryForensicDataCollector::new()),
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
            forensic_service: forensic_svc,
            forensic_archive_generator: forensic_archive_gen,
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
            rls_pool: None,
        }
    }

    // === Forensic Verification Tests ===

    #[tokio::test]
    async fn test_verify_forensic_bundle_returns_ready_status() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicVerificationRequest {
            tenant_id,
            intent_id,
            time_range: ForensicVerificationTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let result = super::verify_forensic_bundle(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return verification result");

        assert_eq!(result.status, forensic_service::VerificationStatus::Ready);
        assert_eq!(result.tenant_id, tenant_id);
        assert_eq!(result.intent_id, intent_id);
    }

    #[tokio::test]
    async fn test_verify_forensic_bundle_request_deserialization() {
        let json = r#"{
            "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
            "intent_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "time_range": {
                "start": "2025-01-01T00:00:00Z",
                "end": "2025-01-31T23:59:59Z"
            },
            "purpose": "compliance_audit",
            "include_artifacts": true,
            "include_audit_events": false,
            "include_policy_snapshots": true
        }"#;

        let request: ForensicVerificationRequest =
            serde_json::from_str(json).expect("Should deserialize");

        assert_eq!(
            request.purpose,
            forensic_service::VerificationPurpose::ComplianceAudit
        );
        assert!(request.include_artifacts);
        assert!(!request.include_audit_events);
        assert!(request.include_policy_snapshots);
    }

    #[tokio::test]
    async fn test_verify_forensic_bundle_response_serialization() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicVerificationRequest {
            tenant_id,
            intent_id,
            time_range: ForensicVerificationTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::Legal,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: false,
        };

        let result = super::verify_forensic_bundle(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return verification result");

        // Verify serialization works
        let json = serde_json::to_string(&result.0).expect("Should serialize");
        assert!(json.contains("\"status\":\"ready\""));
        assert!(json.contains("\"tenant_id\""));
        assert!(json.contains("\"intent_id\""));
        // artifact_coverage should be present since include_artifacts=true
        assert!(json.contains("\"artifact_coverage\""));
        // policy_snapshot_coverage should be None since include_policy_snapshots=false
        assert!(!json.contains("\"policy_snapshot_coverage\""));
    }

    #[tokio::test]
    async fn test_verify_forensic_bundle_with_incomplete_status() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicVerificationRequest {
            tenant_id,
            intent_id,
            time_range: ForensicVerificationTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::IncidentInvestigation,
            include_artifacts: false,
            include_audit_events: false,
            include_policy_snapshots: false,
        };

        let result = super::verify_forensic_bundle(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return verification result");

        // In-memory service returns ready by default
        assert_eq!(result.status, forensic_service::VerificationStatus::Ready);
        // But with no coverage data since all includes are false
        assert_eq!(result.estimated_bundle_item_count, 0);
    }

    #[tokio::test]
    async fn test_forensic_verification_purpose_serialization() {
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationPurpose::IncidentInvestigation)
                .unwrap(),
            "\"incident_investigation\""
        );
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationPurpose::ComplianceAudit).unwrap(),
            "\"compliance_audit\""
        );
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationPurpose::Legal).unwrap(),
            "\"legal\""
        );
    }

    #[tokio::test]
    async fn test_forensic_verification_status_serialization() {
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationStatus::Ready).unwrap(),
            "\"ready\""
        );
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationStatus::Incomplete).unwrap(),
            "\"incomplete\""
        );
        assert_eq!(
            serde_json::to_string(&forensic_service::VerificationStatus::NotSupported).unwrap(),
            "\"not_supported\""
        );
    }

    #[tokio::test]
    async fn test_forensic_intent_version_coverage_serialization() {
        let coverage = ForensicIntentVersionCoverage {
            intent_exists: true,
            intent_id: Uuid::new_v4(),
            version_count: 5,
            earliest_version: Some(Utc::now()),
            latest_version: Some(Utc::now()),
            has_artifact_traceability: true,
        };

        let json = serde_json::to_string(&coverage).expect("Should serialize");
        assert!(json.contains("\"intent_exists\":true"));
        assert!(json.contains("\"version_count\":5"));
        assert!(json.contains("\"has_artifact_traceability\":true"));
    }
}
