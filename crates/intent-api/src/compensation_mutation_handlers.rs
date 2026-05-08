//! Compensation action mutation handlers.
//!
//! Phase 3: Contains POST handlers for approve, waive, execute, and reapprove
//! compensation actions.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{
    types::{
        ApproveCompensationActionBody, CompensationActionResponse, ExecuteCompensationActionBody,
        ReapproveCompensationActionBody, WaiveCompensationActionBody,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Compensation Action Mutation Handlers (Phase 3 bounded mutation slice)
// ============================================================================

/// POST /compensation-actions/{action_id}/approve - Approve a pending compensation action
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before approving the action.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Transition rules:**
/// - Only Pending actions can be approved
/// - Uses optimistic locking via lock_version to prevent concurrent updates
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Pending
/// - Returns 409 Conflict if lock_version doesn't match
///
/// **Executor gate:** Approved actions can be executed via POST /compensation-actions/{action_id}/execute
#[cfg(feature = "jwt-auth")]
pub async fn approve_compensation_action(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ApproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5: Fetch action to get its tenant_id for validation
    let action = state
        .compensation_action_service
        .get_action(action_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if action.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match action tenant_id ({})",
                rls_claims.tenant_id, action.tenant_id
            );
            tracing::warn!("approve_compensation_action: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Phase 3.1: Try RLS path if pool + SQL repo available
        if let (Some(rls_pool), Some(sql_repo)) = (
            &state.rls_pool,
            state.compensation_action_service.repo().as_sqlx_repo(),
        ) {
            // Validate transition: must be Pending to approve
            let validation = action
                .status
                .can_transition_to(compensation_service::CompensationStatus::Approved);
            if !validation.allowed {
                return Err(ApiErrorResponse(
                    IntentRebaseError::InvalidCompensationActionTransition {
                        from_status: format!("{:?}", action.status),
                        to_status: "Approved".into(),
                        reason: validation.reason.unwrap_or_default(),
                    },
                ));
            }

            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                        "failed to begin RLS transaction: {}",
                        e
                    ))));
                }
            };

            let result = sql_repo
                .update_status_with_tx(
                    &mut tx,
                    action_id,
                    compensation_service::CompensationStatus::Approved,
                    body.lock_version,
                    body.approved_by.as_deref(),
                    None,
                )
                .await;

            match result {
                Ok(updated) => {
                    if let Err(e) = tx.commit().await {
                        return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                            "failed to commit RLS transaction: {}",
                            e
                        ))));
                    }
                    tracing::debug!(
                        "approve_compensation_action: RLS path success for tenant_id={}",
                        rls_claims.tenant_id
                    );
                    return Ok(Json(CompensationActionResponse::from(updated)));
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "approve_compensation_action: RLS update failed, rolling back"
                    );
                    return Err(ApiErrorResponse(e));
                }
            }
        }
    }

    // Non-RLS path (fallback)
    let updated = state
        .compensation_action_service
        .approve_action(action_id, body.lock_version, body.approved_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/approve - Approve a pending compensation action (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn approve_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ApproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .approve_action(action_id, body.lock_version, body.approved_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/waive - Waive a pending compensation action
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before waiving the action.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Transition rules:**
/// - Only Pending actions can be waived
/// - Uses optimistic locking via lock_version to prevent concurrent updates
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Pending
/// - Returns 409 Conflict if lock_version doesn't match
///
/// **This slice:** Waived actions are terminal. No reactivation path exists.
#[cfg(feature = "jwt-auth")]
pub async fn waive_compensation_action(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(action_id): Path<Uuid>,
    Json(body): Json<WaiveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5: Fetch action to get its tenant_id for validation
    let action = state
        .compensation_action_service
        .get_action(action_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if action.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match action tenant_id ({})",
                rls_claims.tenant_id, action.tenant_id
            );
            tracing::warn!("waive_compensation_action: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Phase 3.1: Try RLS path if pool + SQL repo available
        if let (Some(rls_pool), Some(sql_repo)) = (
            &state.rls_pool,
            state.compensation_action_service.repo().as_sqlx_repo(),
        ) {
            // Validate transition: must be Pending to waive
            let validation = action
                .status
                .can_transition_to(compensation_service::CompensationStatus::Waived);
            if !validation.allowed {
                return Err(ApiErrorResponse(
                    IntentRebaseError::InvalidCompensationActionTransition {
                        from_status: format!("{:?}", action.status),
                        to_status: "Waived".into(),
                        reason: validation.reason.unwrap_or_default(),
                    },
                ));
            }

            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                        "failed to begin RLS transaction: {}",
                        e
                    ))));
                }
            };

            let result = sql_repo
                .update_status_with_tx(
                    &mut tx,
                    action_id,
                    compensation_service::CompensationStatus::Waived,
                    body.lock_version,
                    None,
                    body.waived_by.as_deref(),
                )
                .await;

            match result {
                Ok(updated) => {
                    // Phase 3.2: Create rollback record in same transaction if SQL rollback repo available
                    // Best-effort (fail-open) - rollback record creation failure does not fail the waive
                    if let Some(rollback_record_repo) =
                        state.compensation_action_service.rollback_record_repo()
                    {
                        if let Some(sql_rollback_repo) = rollback_record_repo.as_sqlx_repo() {
                            let rollback_record =
                                compensation_service::SideEffectRollbackRecord::waived(
                                    action.tenant_id,
                                    action.id,
                                    action.side_effect_id,
                                    action.intent_id,
                                    "Compensation action waived",
                                    body.waived_by.as_deref(),
                                );
                            if let Err(e) = sql_rollback_repo
                                .create_with_tx(&mut tx, rollback_record)
                                .await
                            {
                                tracing::warn!(
                                    "Failed to create rollback record for waived action {}: {:?}",
                                    action_id,
                                    e
                                );
                                // Best-effort: continue even if rollback record creation fails
                            }
                        }
                    }

                    if let Err(e) = tx.commit().await {
                        return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                            "failed to commit RLS transaction: {}",
                            e
                        ))));
                    }

                    tracing::debug!(
                        "waive_compensation_action: RLS path success for tenant_id={}",
                        rls_claims.tenant_id
                    );
                    return Ok(Json(CompensationActionResponse::from(updated)));
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "waive_compensation_action: RLS update failed, rolling back"
                    );
                    return Err(ApiErrorResponse(e));
                }
            }
        }
    }

    // Non-RLS path (fallback)
    let updated = state
        .compensation_action_service
        .waive_action(action_id, body.lock_version, body.waived_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/waive - Waive a pending compensation action (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn waive_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<WaiveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .waive_action(action_id, body.lock_version, body.waived_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/execute - Execute an approved compensation action
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before executing the action.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Executor gate:** Only Approved actions can execute. This prevents accidental
/// execution of pending or already-processed actions.
///
/// **Execution policy gate:** Only service-executable combos can execute:
/// - Rollback + Automatic feasibility (S1InternalReversible)
/// - CounterAction + SemiAutomatic feasibility (S2ExternalReversible)
///
/// **Fails closed on illegal transitions:**
/// - Returns 409 Conflict if action is not Approved
///
/// **This slice:** No retry logic; Failed actions remain Failed.
///
/// **Phase 3.1 note:** The execute handler uses the service method for execution
/// because the executor requires access to `side_effect_repo` which is not exposed
/// from the service. The RLS transaction path is used for approve/waive/reapprove.
#[cfg(feature = "jwt-auth")]
pub async fn execute_compensation_action(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ExecuteCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    // Phase 1 P1-S5h: Fetch action to get its tenant_id for validation
    let action = state
        .compensation_action_service
        .get_action(action_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 1 P1-S5h: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    // Check tenant mismatch FIRST before any status/feasibility gate validation
    if let Some(ref rls_claims) = optional_rls_claims {
        if action.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match action tenant_id ({})",
                rls_claims.tenant_id, action.tenant_id
            );
            tracing::warn!("execute_compensation_action: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Phase 1 P1-S5h: RLS path if pool + SQL repos available
    // Guard condition: rls_pool present AND JWT claims present AND SQL repos available
    if let (Some(rls_pool), Some(rls_claims)) =
        (state.rls_pool.as_ref(), optional_rls_claims.as_ref())
    {
        let sql_action_repo = match state.compensation_action_service.repo().as_sqlx_repo() {
            Some(repo) => repo,
            None => {
                // Fall back to non-RLS path
                let updated = state
                    .compensation_action_service
                    .execute_action(action_id, body.executed_by.as_deref())
                    .await
                    .map_err(ApiErrorResponse)?;
                return Ok(Json(CompensationActionResponse::from(updated)));
            }
        };

        // Executor gate: only Approved actions can execute
        if action.status != compensation_service::CompensationStatus::Approved {
            return Err(ApiErrorResponse(
                IntentRebaseError::CompensationActionNotExecutable(action_id),
            ));
        }

        // Execution policy gate: validate strategy/feasibility combo
        let is_allowed_combo = matches!(
            (action.strategy_type, action.feasibility),
            (
                compensation_service::StrategyType::Rollback,
                compensation_service::CompensationFeasibility::Automatic
            ) | (
                compensation_service::StrategyType::CounterAction,
                compensation_service::CompensationFeasibility::SemiAutomatic
            ) | (
                compensation_service::StrategyType::FollowupNotice,
                compensation_service::CompensationFeasibility::ManualOnly
            ) | (
                compensation_service::StrategyType::Escalation,
                compensation_service::CompensationFeasibility::NotPossible
            )
        );
        if !is_allowed_combo {
            return Err(ApiErrorResponse(
                IntentRebaseError::CompensationActionNotExecutable(action_id),
            ));
        }

        // Capture fields needed for RLS tx
        let lock_version = action.lock_version;
        let tenant_id = action.tenant_id;
        let intent_id = action.intent_id;
        let compensation_plan_id = action.id;
        let actor_id = body
            .executed_by
            .as_deref()
            .unwrap_or("compensation-service/system");

        // Phase 1 P1-S5h: Run the appropriate bounded executor (read-only - returns ExecutionResult)
        use compensation_service::CompensationExecutor;
        let executor_result = if let Some(side_effect_repo) =
            state.compensation_action_service.side_effect_repo()
        {
            match (action.strategy_type, action.feasibility) {
                (
                    compensation_service::StrategyType::Rollback,
                    compensation_service::CompensationFeasibility::Automatic,
                ) => {
                    let executor =
                        compensation_service::RollbackExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await.map_err(ApiErrorResponse)?
                }
                (
                    compensation_service::StrategyType::CounterAction,
                    compensation_service::CompensationFeasibility::SemiAutomatic,
                ) => {
                    let executor =
                        compensation_service::CounterActionExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await.map_err(ApiErrorResponse)?
                }
                (
                    compensation_service::StrategyType::FollowupNotice,
                    compensation_service::CompensationFeasibility::ManualOnly,
                ) => {
                    let executor =
                        compensation_service::FollowupNoticeExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await.map_err(ApiErrorResponse)?
                }
                (
                    compensation_service::StrategyType::Escalation,
                    compensation_service::CompensationFeasibility::NotPossible,
                ) => {
                    let executor =
                        compensation_service::EscalationExecutor::new(side_effect_repo.clone());
                    executor.execute(&action).await.map_err(ApiErrorResponse)?
                }
                _ => {
                    return Err(ApiErrorResponse(
                        IntentRebaseError::CompensationActionNotExecutable(action_id),
                    ));
                }
            }
        } else {
            return Err(ApiErrorResponse(
                IntentRebaseError::CompensationActionNotExecutable(action_id),
            ));
        };

        // Phase 1 P1-S5h: RLS tx wrapping for record_result + rollback_record create
        let mut tx = rls_pool
            .begin_with_tenant(rls_claims.tenant_id)
            .await
            .map_err(|e| {
                tracing::error!("execute_compensation_action: failed to begin RLS tx: {}", e);
                ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "Failed to begin RLS transaction: {}",
                    e
                )))
            })?;

        // Record execution result within RLS tx
        // Signature: record_result_with_tx(tx, action_id, result, lock_version, executed_by)
        let record_result = sql_action_repo
            .record_result_with_tx(
                &mut tx,
                action_id,
                &executor_result,
                lock_version,
                Some(actor_id),
            )
            .await;

        let updated = match record_result {
            Ok(u) => u,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "execute_compensation_action: record_result_with_tx failed, rolling back"
                );
                tx.rollback().await.map_err(|e| {
                    ApiErrorResponse(IntentRebaseError::Internal(format!(
                        "Failed to rollback transaction: {}",
                        e
                    )))
                })?;
                return Err(ApiErrorResponse(e));
            }
        };

        // Create rollback record within RLS tx (best-effort, fail-open)
        if let Some(sql_rollback_repo) = state
            .compensation_action_service
            .rollback_record_repo()
            .and_then(|r| r.as_sqlx_repo())
        {
            use compensation_service::SideEffectRollbackRecord;
            let rollback_record = if executor_result.success {
                SideEffectRollbackRecord::success(
                    tenant_id,
                    compensation_plan_id,
                    action.side_effect_id,
                    intent_id,
                    &executor_result.summary,
                    Some(actor_id),
                )
            } else {
                SideEffectRollbackRecord::failure_with_actor(
                    tenant_id,
                    compensation_plan_id,
                    action.side_effect_id,
                    intent_id,
                    &executor_result.summary,
                    executor_result
                        .error_code
                        .as_deref()
                        .unwrap_or("UNKNOWN_ERROR"),
                    executor_result.error_detail.clone(),
                    Some(actor_id),
                )
            };

            if let Err(e) = sql_rollback_repo
                .create_with_tx(&mut tx, rollback_record)
                .await
            {
                tracing::warn!(
                    "Failed to create rollback record for executed action {}: {:?}",
                    action_id,
                    e
                );
                // Best-effort: continue even if rollback record creation fails
            }
        }

        // Commit RLS tx
        if let Err(e) = tx.commit().await {
            tracing::error!("execute_compensation_action: commit failed: {}", e);
            return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                "failed to commit RLS transaction: {}",
                e
            ))));
        }

        tracing::info!(
            "execute_compensation_action: RLS path success for tenant_id={}",
            tenant_id
        );

        return Ok(Json(CompensationActionResponse::from(updated)));
    }

    // Non-RLS fallback path: use service method for full execution with executor
    let updated = state
        .compensation_action_service
        .execute_action(action_id, body.executed_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/execute - Execute an approved compensation action (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn execute_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ExecuteCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .execute_action(action_id, body.executed_by.as_deref())
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/reapprove - Manually reapprove a failed action
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before reapproving the action.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Policy gates (fail closed):**
/// - Action must be in Failed status
/// - Action must have remaining retry budget (attempt_count < max_retries)
/// - Error code must be retryable (not a permanent failure)
///
/// **Fails closed when:**
/// - Action is not in Failed status → 409 Conflict
/// - Retry budget exhausted → 409 Conflict
/// - Error is non-retryable → 409 Conflict
/// - Optimistic lock conflict → 409 Conflict
///
/// **Note:** This does NOT reset the attempt_count. The action retains its
/// failure history. Reapproval just allows another execution attempt within
/// the retry budget.
#[cfg(feature = "jwt-auth")]
pub async fn reapprove_compensation_action(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ReapproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5: Fetch action to get its tenant_id for validation
    let action = state
        .compensation_action_service
        .get_action(action_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if action.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match action tenant_id ({})",
                rls_claims.tenant_id, action.tenant_id
            );
            tracing::warn!("reapprove_compensation_action: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Phase 3.1: Try RLS path if pool + SQL repo available
        if let (Some(rls_pool), Some(sql_repo)) = (
            &state.rls_pool,
            state.compensation_action_service.repo().as_sqlx_repo(),
        ) {
            // Policy gate 1: Must be in Failed status
            if action.status != compensation_service::CompensationStatus::Failed {
                return Err(ApiErrorResponse(
                    IntentRebaseError::InvalidCompensationActionTransition {
                        from_status: format!("{:?}", action.status),
                        to_status: "Pending".into(),
                        reason: "Only Failed actions can be reapproved".to_string(),
                    },
                ));
            }

            // Policy gate 2: Check retry budget
            if action.attempt_count >= action.max_retries {
                return Err(ApiErrorResponse(
                    IntentRebaseError::CompensationActionNotReapprovable(
                        action_id,
                        format!(
                            "Retry budget exhausted: {} attempts made (max={})",
                            action.attempt_count, action.max_retries
                        ),
                    ),
                ));
            }

            // Policy gate 3: Error must be retryable
            if let Some(denial_reason) = action.reapproval_denial_reason() {
                return Err(ApiErrorResponse(
                    IntentRebaseError::CompensationActionNotReapprovable(action_id, denial_reason),
                ));
            }

            let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                        "failed to begin RLS transaction: {}",
                        e
                    ))));
                }
            };

            let result = sql_repo
                .reapprove_with_tx(&mut tx, action_id, body.lock_version)
                .await;

            match result {
                Ok(updated) => {
                    if let Err(e) = tx.commit().await {
                        return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                            "failed to commit RLS transaction: {}",
                            e
                        ))));
                    }
                    tracing::debug!(
                        "reapprove_compensation_action: RLS path success for tenant_id={}",
                        rls_claims.tenant_id
                    );
                    return Ok(Json(CompensationActionResponse::from(updated)));
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "reapprove_compensation_action: RLS update failed, rolling back"
                    );
                    return Err(ApiErrorResponse(e));
                }
            }
        }
    }

    // Non-RLS path (fallback)
    let updated = state
        .compensation_action_service
        .reapprove_action(action_id, body.lock_version)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

/// POST /compensation-actions/{action_id}/reapprove - Manually reapprove a failed action (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
pub async fn reapprove_compensation_action(
    State(state): State<AppState>,
    Path(action_id): Path<Uuid>,
    Json(body): Json<ReapproveCompensationActionBody>,
) -> Result<Json<CompensationActionResponse>, ApiErrorResponse> {
    let updated = state
        .compensation_action_service
        .reapprove_action(action_id, body.lock_version)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(CompensationActionResponse::from(updated)))
}

// ============================================================================
// Tests for Compensation Mutation Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ApproveCompensationActionBody, CompensationActionResponse, ExecuteCompensationActionBody,
        WaiveCompensationActionBody,
    };
    use crate::RebaseOrchestrator;
    use axum::http::StatusCode;
    use chrono::Utc;
    use compensation_service::{
        CompensationActionService, CompensationFeasibility, InMemoryCompensationActionRepository,
        InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
        RebaseContext, SideEffectService, StrategyType,
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

    /// Create minimal AppState for compensation action handler tests
    #[cfg(not(feature = "jwt-auth"))]
    fn create_test_service_with_executor() -> AppState {
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
        let side_effect_svc = Arc::new(SideEffectService::new(side_effect_repo));
        // Use in-memory compensation action repo with stub executor
        let compensation_action_repo = Arc::new(InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(CompensationActionService::new(
            compensation_action_repo.clone(),
        ));
        let orchestration_run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
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
            forensic_service: Arc::new(InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            forensic_archive_generator: Arc::new(InMemoryForensicArchiveGenerator::new()),
            forensic_bundle_service: Arc::new(ForensicBundleService::new(
                Arc::new(InMemoryBundleRepository::new()),
                Arc::new(InMemoryBundleStorage::new("test-bucket")),
                Arc::new(InMemoryForensicDataCollector::new()),
            )),
            start_time: Instant::now(),
            rls_pool: None,
        }
    }

    // === Compensation Action API Tests ===

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_approve_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action directly via the service
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly,
            StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Approve the action via the API
        let request = ApproveCompensationActionBody {
            lock_version: created.lock_version,
            approved_by: Some("test-approver".to_string()),
        };
        let result =
            super::approve_compensation_action(State(state), Path(created.id), Json(request))
                .await
                .unwrap();

        assert_eq!(result.status, "approved");
        assert_eq!(result.approved_by, Some("test-approver".to_string()));
    }

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_approve_compensation_action_not_found() {
        let state = create_test_service_with_executor();

        let request = ApproveCompensationActionBody {
            lock_version: 0,
            approved_by: None,
        };
        let result =
            super::approve_compensation_action(State(state), Path(Uuid::new_v4()), Json(request))
                .await;
        assert!(result.is_err());
    }

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_waive_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly,
            StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Waive the action via the API
        let request = WaiveCompensationActionBody {
            lock_version: created.lock_version,
            waived_by: Some("test-waiver".to_string()),
        };
        let result =
            super::waive_compensation_action(State(state), Path(created.id), Json(request))
                .await
                .unwrap();

        assert_eq!(result.status, "waived");
    }

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_execute_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::Automatic,
            StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // First approve it
        let approved = state
            .compensation_action_service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute the action via the API
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };
        let result =
            super::execute_compensation_action(State(state), Path(approved.id), Json(request))
                .await
                .unwrap();

        assert_eq!(result.status, "executed");
        assert_eq!(result.executed_by, Some("test-executor".to_string()));
    }

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_execute_compensation_action_fails_on_pending() {
        let state = create_test_service_with_executor();

        // Create a compensation action (starts in Pending status)
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly,
            StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Try to execute without approval - should fail
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };
        let result =
            super::execute_compensation_action(State(state), Path(created.id), Json(request)).await;

        assert!(result.is_err());
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_compensation_action_response_serialization() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::ManualOnly,
            StrategyType::Rollback,
            "Test rollback",
        );

        let response = CompensationActionResponse::from(action);

        assert_eq!(response.status, "pending");
        assert_eq!(response.strategy_type, "rollback");
        assert_eq!(response.feasibility, "manual_only");
        assert_eq!(response.tenant_id, tenant_id);
        assert_eq!(response.intent_id, intent_id);
    }
}
