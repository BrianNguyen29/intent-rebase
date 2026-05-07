//! Orchestration run handlers for compensation action orchestration runs.
//!
//! Phase 3 Batch 1: Contains POST /compensation-actions/runs and
//! GET /compensation-actions/runs/{run_id} handlers for bounded single-shot
//! HTTP orchestration.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use intent_rebase_types::IntentRebaseError;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    ApiErrorResponse, AppState, CreateOrchestrationRunRequest, OrchestrationRunQuery,
    OrchestrationRunResponse,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Orchestration Run Handlers (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
// ============================================================================

/// POST /compensation-actions/runs - Create and execute a single-shot orchestration run
///
/// Phase 3 P3-S5 bounded slice: Creates an orchestration run for the provided
/// compensation action IDs and returns 202 Accepted immediately while execution
/// proceeds in the background.
///
/// **Bounded slice semantics:**
/// - Compensation action IDs are validated to exist and be owned by the tenant
/// - Run is created in Pending state and returned immediately
/// - Background execution proceeds asynchronously via `execute_existing_run`
/// - Errors during background execution are logged but cannot be reported to HTTP client
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present AND rls_pool is available,
/// this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when rls_pool is None or JWT is absent (backward compatible).
///
/// **RLS note:** P1-S5i adds migration 015 for orchestration_runs RLS policy and wires
/// the RLS-aware create path. Handler-level tenant guard remains as defense-in-depth.
#[cfg(feature = "jwt-auth")]
pub async fn create_orchestration_run(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Query(query): Query<OrchestrationRunQuery>,
    Json(request): Json<CreateOrchestrationRunRequest>,
) -> Result<(StatusCode, Json<OrchestrationRunResponse>), ApiErrorResponse> {
    // Phase 3 P1-S5i: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = &optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("create_orchestration_run: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // P1-S5i: Check if RLS path is available (pool exists AND JWT claims present)
    let run = if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Use RLS-aware transaction path
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

        // Create run object for RLS transaction
        let run = compensation_service::OrchestrationRun::new(
            query.tenant_id,
            request.action_ids.clone(),
            request.initiated_by.clone(),
            request.intent_id,
        );

        // Get the SQL repo and create run within the transaction
        if let Some(sql_repo) = state.orchestration_runtime.run_repo().as_sqlx_repo() {
            let run_result = sql_repo.create_run_with_tx(&mut tx, run).await;
            let created_run = match run_result {
                Ok(run) => run,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS run creation failed: {}",
                        e
                    ))));
                }
            };

            let commit_result = tx.commit().await;
            if let Err(e) = commit_result {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "create_orchestration_run: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );
            created_run
        } else {
            // Fallback to non-RLS if repo doesn't support SQL
            tracing::warn!(
                "create_orchestration_run: rls_pool set but repo doesn't support SQL, falling back"
            );
            // Drop the transaction since we can't use it
            drop(tx);
            state
                .orchestration_runtime
                .create_run(
                    query.tenant_id,
                    request.action_ids,
                    request.initiated_by,
                    request.intent_id,
                )
                .await
                .map_err(ApiErrorResponse)?
        }
    } else {
        // Non-RLS path (no JWT claims or rls_pool is None)
        state
            .orchestration_runtime
            .create_run(
                query.tenant_id,
                request.action_ids,
                request.initiated_by,
                request.intent_id,
            )
            .await
            .map_err(ApiErrorResponse)?
    };

    let run_id = run.id;

    // Step 2: Spawn background execution
    // The run handle is already returned to the client; execution proceeds in the background.
    // Propagate current span context into the spawned task for distributed tracing.
    let runtime = state.orchestration_runtime.clone();
    let span = tracing::info_span!(
        "background_orchestration_run",
        run_id = %run_id,
        otel.kind = "internal"
    );
    tokio::spawn(
        async move {
            // Background execution; errors are logged but cannot be reported to the HTTP client
            match runtime.execute_existing_run(run_id).await {
                Ok(_) => {
                    tracing::debug!("Background orchestration run {} completed", run_id);
                }
                Err(e) => {
                    tracing::error!("Background orchestration run {} failed: {}", run_id, e);
                }
            }
        }
        .instrument(span),
    );

    // Return 202 Accepted with the persisted (pending) run handle immediately
    Ok((
        StatusCode::ACCEPTED,
        Json(OrchestrationRunResponse::from(run)),
    ))
}

/// POST /compensation-actions/runs - Create and execute a single-shot orchestration run (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_orchestration_run(
    State(state): State<AppState>,
    Query(query): Query<OrchestrationRunQuery>,
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
    // Propagate current span context into the spawned task for distributed tracing.
    let runtime = state.orchestration_runtime.clone();
    let span = tracing::info_span!(
        "background_orchestration_run",
        run_id = %run_id,
        otel.kind = "internal"
    );
    tokio::spawn(
        async move {
            // Background execution; errors are logged but cannot be reported to the HTTP client
            match runtime.execute_existing_run(run_id).await {
                Ok(_) => {
                    tracing::debug!("Background orchestration run {} completed", run_id);
                }
                Err(e) => {
                    tracing::error!("Background orchestration run {} failed: {}", run_id, e);
                }
            }
        }
        .instrument(span),
    );

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
#[cfg(feature = "jwt-auth")]
pub async fn get_orchestration_run(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(run_id): Path<Uuid>,
    Query(query): Query<OrchestrationRunQuery>,
) -> Result<Json<OrchestrationRunResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("get_orchestration_run: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let run = state
        .orchestration_runtime
        .get_run(run_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Verify tenant ownership (pre-existing check, kept for non-JWT path)
    if run.tenant_id != query.tenant_id {
        return Err(ApiErrorResponse(
            IntentRebaseError::OrchestrationRunNotFound(run_id),
        ));
    }

    Ok(Json(OrchestrationRunResponse::from(run)))
}

/// GET /compensation-actions/runs/{run_id} - Get an orchestration run by ID
#[cfg(not(feature = "jwt-auth"))]
pub async fn get_orchestration_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Query(query): Query<OrchestrationRunQuery>,
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
