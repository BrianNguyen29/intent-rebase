//! Batch compensation action handlers.
//!
//! Phase 3 P3-S5: Contains POST handlers for batch approve, reapprove, and execute
//! compensation actions with per-item RLS transaction semantics.

use axum::{
    extract::{Query, State},
    Json,
};

use crate::{
    types::{
        BatchItemOutcomeResponse, BatchOrchestrationRequest, BatchOrchestrationResponse,
        BatchOrchestrationSummaryResponse, CompensationActionResponse, OrchestrationQuery,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Private helper: maps BatchOrchestrationResult to BatchOrchestrationResponse
// ============================================================================

/// Maps a `BatchOrchestrationResult` from the compensation service to a
/// `BatchOrchestrationResponse`, preserving outcome/summary/not_found semantics.
fn map_batch_result_to_response(
    result: compensation_service::BatchOrchestrationResult,
) -> BatchOrchestrationResponse {
    let outcomes = result
        .outcomes
        .into_iter()
        .map(|o| {
            let (result, error) = match &o.result {
                Ok(a) => (Some(CompensationActionResponse::from(a.clone())), None),
                Err(e) => (None, Some(e.clone())),
            };
            BatchItemOutcomeResponse {
                action_id: o.action_id,
                success: o.success,
                result,
                error,
            }
        })
        .collect();

    BatchOrchestrationResponse {
        outcomes,
        not_found: result.not_found,
        summary: BatchOrchestrationSummaryResponse {
            total: result.summary.total,
            succeeded: result.summary.succeeded,
            failed: result.summary.failed,
            not_found: result.summary.not_found,
        },
    }
}

// ============================================================================
// Batch Compensation Action Handlers (Phase 3 P3-S5 bounded slice)
// ============================================================================

/// POST /compensation-actions/batch-approve - Batch approve compensation actions
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates that ALL actions belong to the JWT's tenant before processing.
/// Fails closed if ANY action has a different tenant; fails open when JWT is absent.
///
/// **Bounded partial-success semantics:**
/// - If an action_id is not found, it's recorded as `not_found` and continues
/// - If an action fails validation, it's recorded as `failed` and continues
/// - Successful approvals are recorded as `succeeded`
/// - Does NOT fail-fast on first error - all items are processed
///
/// **Transition rules:**
/// - Only Pending actions can be approved
/// - Uses optimistic locking via lock_version
///
/// **RLS wiring (Phase 4.1):** Per-item RLS transactions when rls_pool is available,
/// preserving per-item partial-success semantics. Each action is processed in its own
/// RLS transaction. If one action fails (concurrency conflict, etc.), other actions
/// still proceed in their own transactions.
///
/// **No background worker or queue claiming:**
/// This is a direct service method that processes actions sequentially.
#[cfg(feature = "jwt-auth")]
pub async fn batch_approve_compensation_actions(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5: When JWT is present, validate ALL actions belong to JWT's tenant
    // Fail-closed if ANY action has a different tenant
    if let Some(rls_claims) = optional_rls_claims {
        // Pre-validate: track not_found items but don't fail on them
        let mut not_found = Vec::new();
        for action_id in &request.action_ids {
            match state
                .compensation_action_service
                .get_action(*action_id)
                .await
            {
                Ok(action) => {
                    if action.tenant_id != rls_claims.tenant_id {
                        let msg = format!(
                            "Tenant mismatch: JWT tenant_id ({}) does not match action {} tenant_id ({})",
                            rls_claims.tenant_id, action_id, action.tenant_id
                        );
                        tracing::warn!(
                            "batch_approve_compensation_actions: tenant mismatch rejection for action {}",
                            action_id
                        );
                        return Err(ApiErrorResponse(
                            intent_rebase_types::IntentRebaseError::Unauthorized(msg),
                        ));
                    }
                }
                Err(intent_rebase_types::IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(*action_id);
                }
                Err(e) => {
                    return Err(ApiErrorResponse(e));
                }
            }
        }

        // RLS path: per-item transactions preserving partial-success semantics
        let mut outcomes = Vec::new();
        let total = request.action_ids.len();
        let mut succeeded = 0;
        let mut failed = 0;

        for action_id in request.action_ids {
            // Fetch action - if not found, add to not_found and continue
            let action = match state
                .compensation_action_service
                .get_action(action_id)
                .await
            {
                Ok(a) => a,
                Err(intent_rebase_types::IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    failed += 1;
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id,
                        success: false,
                        result: None,
                        error: Some("Action not found".to_string()),
                    });
                    continue;
                }
                Err(e) => {
                    failed += 1;
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id,
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };

            // Validate transition: must be Pending to approve
            let validation = action
                .status
                .can_transition_to(compensation_service::CompensationStatus::Approved);
            if !validation.allowed {
                failed += 1;
                outcomes.push(BatchItemOutcomeResponse {
                    action_id,
                    success: false,
                    result: None,
                    error: Some(format!(
                        "Invalid transition: {:?} -> Approved ({})",
                        action.status,
                        validation.reason.unwrap_or_default()
                    )),
                });
                continue;
            }

            // Try RLS path if pool + SQL repo available
            if let (Some(rls_pool), Some(sql_repo)) = (
                &state.rls_pool,
                state.compensation_action_service.repo().as_sqlx_repo(),
            ) {
                let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(format!("failed to begin RLS transaction: {}", e)),
                        });
                        continue;
                    }
                };

                let result = sql_repo
                    .update_status_with_tx(
                        &mut tx,
                        action_id,
                        compensation_service::CompensationStatus::Approved,
                        action.lock_version,
                        request.initiated_by.as_deref(),
                        None,
                    )
                    .await;

                match result {
                    Ok(updated) => {
                        if let Err(e) = tx.commit().await {
                            failed += 1;
                            outcomes.push(BatchItemOutcomeResponse {
                                action_id,
                                success: false,
                                result: None,
                                error: Some(format!("failed to commit: {}", e)),
                            });
                            continue;
                        }
                        tracing::debug!(
                            "batch_approve_compensation_actions: RLS success for action {}",
                            action_id
                        );
                        succeeded += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: true,
                            result: Some(CompensationActionResponse::from(updated)),
                            error: None,
                        });
                    }
                    Err(e) => {
                        // Transaction auto-rollbacks on drop, just record failure
                        tracing::error!(
                            error = %e,
                            "batch_approve_compensation_actions: RLS update failed for action {}",
                            action_id
                        );
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            } else {
                // Fallback to non-RLS service method
                match state
                    .compensation_action_service
                    .approve_action(
                        action_id,
                        action.lock_version,
                        request.initiated_by.as_deref(),
                    )
                    .await
                {
                    Ok(updated) => {
                        succeeded += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: true,
                            result: Some(CompensationActionResponse::from(updated)),
                            error: None,
                        });
                    }
                    Err(e) => {
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }
        }

        return Ok(Json(BatchOrchestrationResponse {
            outcomes,
            not_found: not_found.clone(),
            summary: BatchOrchestrationSummaryResponse {
                total,
                succeeded,
                failed,
                not_found: not_found.len(),
            },
        }));
    }

    // Non-JWT path (backward compatible): use query param tenant_id
    let result = state
        .compensation_action_service
        .batch_approve(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

/// POST /compensation-actions/batch-approve - Batch approve compensation actions (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler uses the query param tenant_id.
#[cfg(not(feature = "jwt-auth"))]
pub async fn batch_approve_compensation_actions(
    State(state): State<AppState>,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_approve(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

/// POST /compensation-actions/batch-reapprove - Batch reapprove compensation actions
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates that ALL actions belong to the JWT's tenant before processing.
/// Fails closed if ANY action has a different tenant; fails open when JWT is absent.
///
/// **Bounded partial-success semantics:** Same as batch_approve.
///
/// **Policy gates (fail closed):**
/// - Action must be in Failed status
/// - Action must have remaining retry budget
/// - Error code must be retryable
///
/// **RLS wiring (Phase 4.1):** Per-item RLS transactions when rls_pool is available,
/// preserving per-item partial-success semantics. Each action is processed in its own
/// RLS transaction. If one action fails (concurrency conflict, invalid transition, etc.),
/// other actions still proceed in their own transactions.
#[cfg(feature = "jwt-auth")]
pub async fn batch_reapprove_compensation_actions(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5: When JWT is present, validate ALL actions belong to JWT's tenant
    // Fail-closed if ANY action has a different tenant
    if let Some(rls_claims) = optional_rls_claims {
        // Pre-validate: track not_found items but don't fail on them
        let mut not_found = Vec::new();
        for action_id in &request.action_ids {
            match state
                .compensation_action_service
                .get_action(*action_id)
                .await
            {
                Ok(action) => {
                    if action.tenant_id != rls_claims.tenant_id {
                        let msg = format!(
                            "Tenant mismatch: JWT tenant_id ({}) does not match action {} tenant_id ({})",
                            rls_claims.tenant_id, action_id, action.tenant_id
                        );
                        tracing::warn!(
                            "batch_reapprove_compensation_actions: tenant mismatch rejection for action {}",
                            action_id
                        );
                        return Err(ApiErrorResponse(
                            intent_rebase_types::IntentRebaseError::Unauthorized(msg),
                        ));
                    }
                }
                Err(intent_rebase_types::IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(*action_id);
                }
                Err(e) => {
                    return Err(ApiErrorResponse(e));
                }
            }
        }

        // RLS path: per-item transactions preserving partial-success semantics
        let mut outcomes = Vec::new();
        let total = request.action_ids.len();
        let mut succeeded = 0;
        let mut failed = 0;

        for action_id in request.action_ids {
            // Fetch action - if not found, add to not_found and continue
            let action = match state
                .compensation_action_service
                .get_action(action_id)
                .await
            {
                Ok(a) => a,
                Err(intent_rebase_types::IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(action_id);
                    failed += 1;
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id,
                        success: false,
                        result: None,
                        error: Some("Action not found".to_string()),
                    });
                    continue;
                }
                Err(e) => {
                    failed += 1;
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id,
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };

            // Policy gate: Action must be in Failed status to be reapprovable
            // This is validated by reapprove_with_tx SQL query (status = 'failed' check)
            // But we can fail fast if not in Failed status
            if action.status != compensation_service::CompensationStatus::Failed {
                failed += 1;
                outcomes.push(BatchItemOutcomeResponse {
                    action_id,
                    success: false,
                    result: None,
                    error: Some(format!(
                        "Invalid transition: {:?} -> Pending (Only Failed actions can be reapproved)",
                        action.status
                    )),
                });
                continue;
            }

            // Try RLS path if pool + SQL repo available
            if let (Some(rls_pool), Some(sql_repo)) = (
                &state.rls_pool,
                state.compensation_action_service.repo().as_sqlx_repo(),
            ) {
                let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(format!("failed to begin RLS transaction: {}", e)),
                        });
                        continue;
                    }
                };

                let result = sql_repo
                    .reapprove_with_tx(&mut tx, action_id, action.lock_version)
                    .await;

                match result {
                    Ok(updated) => {
                        if let Err(e) = tx.commit().await {
                            failed += 1;
                            outcomes.push(BatchItemOutcomeResponse {
                                action_id,
                                success: false,
                                result: None,
                                error: Some(format!("failed to commit: {}", e)),
                            });
                            continue;
                        }
                        tracing::debug!(
                            "batch_reapprove_compensation_actions: RLS success for action {}",
                            action_id
                        );
                        succeeded += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: true,
                            result: Some(CompensationActionResponse::from(updated)),
                            error: None,
                        });
                    }
                    Err(e) => {
                        // Transaction auto-rollbacks on drop, just record failure
                        tracing::error!(
                            error = %e,
                            "batch_reapprove_compensation_actions: RLS update failed for action {}",
                            action_id
                        );
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            } else {
                // Fallback to non-RLS service method
                match state
                    .compensation_action_service
                    .reapprove_action(action_id, action.lock_version)
                    .await
                {
                    Ok(updated) => {
                        succeeded += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: true,
                            result: Some(CompensationActionResponse::from(updated)),
                            error: None,
                        });
                    }
                    Err(e) => {
                        failed += 1;
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }
        }

        return Ok(Json(BatchOrchestrationResponse {
            outcomes,
            not_found: not_found.clone(),
            summary: BatchOrchestrationSummaryResponse {
                total,
                succeeded,
                failed,
                not_found: not_found.len(),
            },
        }));
    }

    // Non-JWT path (backward compatible): use query param tenant_id
    let result = state
        .compensation_action_service
        .batch_reapprove(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

/// POST /compensation-actions/batch-reapprove - Batch reapprove compensation actions (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler uses the query param tenant_id.
#[cfg(not(feature = "jwt-auth"))]
pub async fn batch_reapprove_compensation_actions(
    State(state): State<AppState>,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_reapprove(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

/// POST /compensation-actions/batch-execute - Batch execute compensation actions
///
/// Phase 3 P1-S5h bounded slice: When valid JWT claims are present, this handler
/// uses per-item RLS transaction wrapping for each action's write phase.
/// Fails closed on tenant mismatch per item; fails open when JWT is absent.
///
/// **Bounded sequential per-item RLS transaction pattern:**
/// - For each action: begin_with_tenant → executor (read-only) → record_result_with_tx + rollback_record create_with_tx → commit
/// - Non-RLS fallback uses service.batch_execute for backward compatibility
///
/// **Bounded partial-success semantics:**
/// - Tenant mismatch per item: fail closed (item rejected, batch continues)
/// - Action not found: recorded as not_found, batch continues
/// - Executor failure: recorded as failed, batch continues
///
/// **Executor gate:** Only Approved + service-executable actions can execute.
#[cfg(feature = "jwt-auth")]
pub async fn batch_execute_compensation_actions(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    // Phase 1 P1-S5h: When JWT is present, use per-item RLS transaction wrapping
    if let Some(rls_claims) = optional_rls_claims {
        let mut outcomes = Vec::new();
        let mut not_found = Vec::new();
        let mut summary = BatchOrchestrationSummaryResponse {
            total: request.action_ids.len(),
            succeeded: 0,
            failed: 0,
            not_found: 0,
        };

        for action_id in &request.action_ids {
            // Fetch action to validate existence and tenant ownership
            let action = match state
                .compensation_action_service
                .get_action(*action_id)
                .await
            {
                Ok(a) => a,
                Err(intent_rebase_types::IntentRebaseError::CompensationActionNotFound(_)) => {
                    not_found.push(*action_id);
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some("Compensation action not found".to_string()),
                    });
                    summary.not_found += 1;
                    summary.failed += 1;
                    continue;
                }
                Err(e) => {
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    });
                    summary.failed += 1;
                    continue;
                }
            };

            // Tenant mismatch check - fail closed per item
            if action.tenant_id != rls_claims.tenant_id {
                tracing::warn!(
                    "batch_execute_compensation_actions: tenant mismatch for action {}",
                    action_id
                );
                outcomes.push(BatchItemOutcomeResponse {
                    action_id: *action_id,
                    success: false,
                    result: None,
                    error: Some("Tenant mismatch: action not found or access denied".to_string()),
                });
                summary.failed += 1;
                continue;
            }

            // Phase 1 P1-S5h: RLS path if pool + SQL repos available
            // Guard condition: rls_pool present AND JWT claims present AND SQL repos available
            if let (Some(rls_pool), Some(sql_action_repo)) = (
                state.rls_pool.as_ref(),
                state.compensation_action_service.repo().as_sqlx_repo(),
            ) {
                // Executor gate: only Approved actions can execute
                if action.status != compensation_service::CompensationStatus::Approved {
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some("Action is not in Approved status".to_string()),
                    });
                    summary.failed += 1;
                    continue;
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
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some("Action is not service-executable".to_string()),
                    });
                    summary.failed += 1;
                    continue;
                }

                // Capture fields needed for RLS tx
                let lock_version = action.lock_version;
                let tenant_id = action.tenant_id;
                let intent_id = action.intent_id;
                let compensation_plan_id = action.id;
                let actor_id = request
                    .initiated_by
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
                            let executor = compensation_service::RollbackExecutor::new(
                                side_effect_repo.clone(),
                            );
                            match executor.execute(&action).await {
                                Ok(r) => r,
                                Err(e) => {
                                    outcomes.push(BatchItemOutcomeResponse {
                                        action_id: *action_id,
                                        success: false,
                                        result: None,
                                        error: Some(e.to_string()),
                                    });
                                    summary.failed += 1;
                                    continue;
                                }
                            }
                        }
                        (
                            compensation_service::StrategyType::CounterAction,
                            compensation_service::CompensationFeasibility::SemiAutomatic,
                        ) => {
                            let executor = compensation_service::CounterActionExecutor::new(
                                side_effect_repo.clone(),
                            );
                            match executor.execute(&action).await {
                                Ok(r) => r,
                                Err(e) => {
                                    outcomes.push(BatchItemOutcomeResponse {
                                        action_id: *action_id,
                                        success: false,
                                        result: None,
                                        error: Some(e.to_string()),
                                    });
                                    summary.failed += 1;
                                    continue;
                                }
                            }
                        }
                        (
                            compensation_service::StrategyType::FollowupNotice,
                            compensation_service::CompensationFeasibility::ManualOnly,
                        ) => {
                            let executor = compensation_service::FollowupNoticeExecutor::new(
                                side_effect_repo.clone(),
                            );
                            match executor.execute(&action).await {
                                Ok(r) => r,
                                Err(e) => {
                                    outcomes.push(BatchItemOutcomeResponse {
                                        action_id: *action_id,
                                        success: false,
                                        result: None,
                                        error: Some(e.to_string()),
                                    });
                                    summary.failed += 1;
                                    continue;
                                }
                            }
                        }
                        (
                            compensation_service::StrategyType::Escalation,
                            compensation_service::CompensationFeasibility::NotPossible,
                        ) => {
                            let executor = compensation_service::EscalationExecutor::new(
                                side_effect_repo.clone(),
                            );
                            match executor.execute(&action).await {
                                Ok(r) => r,
                                Err(e) => {
                                    outcomes.push(BatchItemOutcomeResponse {
                                        action_id: *action_id,
                                        success: false,
                                        result: None,
                                        error: Some(e.to_string()),
                                    });
                                    summary.failed += 1;
                                    continue;
                                }
                            }
                        }
                        _ => {
                            outcomes.push(BatchItemOutcomeResponse {
                                action_id: *action_id,
                                success: false,
                                result: None,
                                error: Some("Unsupported strategy/feasibility combo".to_string()),
                            });
                            summary.failed += 1;
                            continue;
                        }
                    }
                } else {
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some("Side effect repository not available".to_string()),
                    });
                    summary.failed += 1;
                    continue;
                };

                // Phase 1 P1-S5h: RLS tx wrapping for record_result + rollback_record create
                let mut tx = match rls_pool.begin_with_tenant(rls_claims.tenant_id).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        tracing::error!(
                            "batch_execute: failed to begin RLS tx for action {}: {}",
                            action_id,
                            e
                        );
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id: *action_id,
                            success: false,
                            result: None,
                            error: Some(format!("Failed to begin RLS transaction: {}", e)),
                        });
                        summary.failed += 1;
                        continue;
                    }
                };

                // Record execution result within RLS tx
                let record_result = sql_action_repo
                    .record_result_with_tx(
                        &mut tx,
                        *action_id,
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
                            "batch_execute: record_result_with_tx failed for action {}, rolling back",
                            action_id
                        );
                        if let Err(rb_err) = tx.rollback().await {
                            tracing::error!(
                                "batch_execute: rollback failed for action {}: {}",
                                action_id,
                                rb_err
                            );
                        }
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id: *action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                        summary.failed += 1;
                        continue;
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
                            "batch_execute: failed to create rollback record for action {}: {:?}",
                            action_id,
                            e
                        );
                        // Best-effort: continue even if rollback record creation fails
                    }
                }

                // Commit RLS tx
                if let Err(e) = tx.commit().await {
                    tracing::error!(
                        "batch_execute: commit failed for action {}: {}",
                        action_id,
                        e
                    );
                    outcomes.push(BatchItemOutcomeResponse {
                        action_id: *action_id,
                        success: false,
                        result: None,
                        error: Some(format!("Failed to commit RLS transaction: {}", e)),
                    });
                    summary.failed += 1;
                    continue;
                }

                tracing::debug!(
                    "batch_execute: RLS path success for action {} tenant_id={}",
                    action_id,
                    tenant_id
                );

                outcomes.push(BatchItemOutcomeResponse {
                    action_id: *action_id,
                    success: true,
                    result: Some(CompensationActionResponse::from(updated)),
                    error: None,
                });
                summary.succeeded += 1;
            } else {
                // Non-RLS fallback path: use service method for full execution with executor
                // This handles the case where rls_pool is None or SQL repos are unavailable
                match state
                    .compensation_action_service
                    .execute_action(*action_id, request.initiated_by.as_deref())
                    .await
                {
                    Ok(updated) => {
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id: *action_id,
                            success: true,
                            result: Some(CompensationActionResponse::from(updated)),
                            error: None,
                        });
                        summary.succeeded += 1;
                    }
                    Err(e) => {
                        outcomes.push(BatchItemOutcomeResponse {
                            action_id: *action_id,
                            success: false,
                            result: None,
                            error: Some(e.to_string()),
                        });
                        summary.failed += 1;
                    }
                }
            }
        }

        return Ok(Json(BatchOrchestrationResponse {
            outcomes,
            not_found,
            summary,
        }));
    }

    // Non-JWT path (backward compatible): use query param tenant_id
    let result = state
        .compensation_action_service
        .batch_execute(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

/// POST /compensation-actions/batch-execute - Batch execute compensation actions (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler uses the query param tenant_id.
#[cfg(not(feature = "jwt-auth"))]
pub async fn batch_execute_compensation_actions(
    State(state): State<AppState>,
    Query(query): Query<OrchestrationQuery>,
    Json(request): Json<BatchOrchestrationRequest>,
) -> Result<Json<BatchOrchestrationResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .batch_execute(
            query.tenant_id,
            request.action_ids,
            request.initiated_by.as_deref(),
        )
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(map_batch_result_to_response(result)))
}

// ============================================================================
// Tests for Batch Compensation Action Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    use crate::test_helpers::{create_test_optional_rls_claims, create_test_service};
    use crate::types::{BatchOrchestrationRequest, OrchestrationQuery};

    use axum::extract::{Query, State};
    use axum::Json;
    use uuid::Uuid;

    // -------------------------------------------------------------------------
    // batch_approve_compensation_actions Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_approve_compensation_actions_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Try to batch approve with TenantB (mismatch) - request includes the action
        let tenant_b = Uuid::new_v4();
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_approve_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Query(OrchestrationQuery {
                tenant_id: tenant_b,
            }),
            Json(request),
        )
        .await;

        // Should fail with Unauthorized (fail-closed on tenant mismatch)
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.0.to_string();
        assert!(
            err_msg.contains("Tenant mismatch"),
            "Expected tenant mismatch error, got: {}",
            err_msg
        );
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_approve_compensation_actions_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Batch approve with TenantA (matching)
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_approve_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Query(OrchestrationQuery {
                tenant_id: tenant_a,
            }),
            Json(request),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Expected success with matching tenant, got: {:?}",
            result
        );
        let response = result.unwrap();
        assert_eq!(response.summary.succeeded, 1);
        assert_eq!(response.summary.failed, 0);
    }

    // -------------------------------------------------------------------------
    // batch_reapprove_compensation_actions Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_reapprove_compensation_actions_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Manually set to Failed status to make it reapprovable
        use compensation_service::CompensationStatus;
        let _failed_action = state
            .compensation_action_service
            .update_status(created.id, CompensationStatus::Failed, created.lock_version)
            .await
            .unwrap();

        // Try to batch reapprove with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_reapprove_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Query(OrchestrationQuery {
                tenant_id: tenant_b,
            }),
            Json(request),
        )
        .await;

        // Should fail with Unauthorized (fail-closed on tenant mismatch)
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.0.to_string();
        assert!(
            err_msg.contains("Tenant mismatch"),
            "Expected tenant mismatch error, got: {}",
            err_msg
        );
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_reapprove_compensation_actions_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Manually set to Failed status to make it reapprovable
        use compensation_service::CompensationStatus;
        let _failed_action = state
            .compensation_action_service
            .update_status(created.id, CompensationStatus::Failed, created.lock_version)
            .await
            .unwrap();

        // Batch reapprove with TenantA (matching)
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_reapprove_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Query(OrchestrationQuery {
                tenant_id: tenant_a,
            }),
            Json(request),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Expected success with matching tenant, got: {:?}",
            result
        );
        let response = result.unwrap();
        assert_eq!(response.summary.succeeded, 1);
        assert_eq!(response.summary.failed, 0);
    }

    // -------------------------------------------------------------------------
    // batch_execute_compensation_actions Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_execute_compensation_actions_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create an Approved compensation action with TenantA
        // Must be Approved + Automatic feasibility for batch_execute
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execute
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Manually set to Approved status (necessary for batch_execute)
        use compensation_service::CompensationStatus;
        let _approved_action = state
            .compensation_action_service
            .update_status(
                created.id,
                CompensationStatus::Approved,
                created.lock_version,
            )
            .await
            .unwrap();

        // Try to batch execute with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_execute_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Query(OrchestrationQuery {
                tenant_id: tenant_b,
            }),
            Json(request),
        )
        .await;

        // Phase 1 P1-S5h: Per-item fail-closed on tenant mismatch - batch continues
        // but the mismatched item is recorded as failed with error message
        assert!(
            result.is_ok(),
            "Expected Ok response with per-item failure, got: {:?}",
            result
        );
        let response = result.unwrap();
        assert_eq!(response.summary.total, 1);
        assert_eq!(response.summary.failed, 1);
        assert_eq!(response.summary.succeeded, 0);
        // The error message should indicate tenant mismatch / access denied
        let outcome = &response.outcomes[0];
        assert!(!outcome.success);
        assert!(outcome.error.is_some());
        let error_msg = outcome.error.as_ref().unwrap();
        assert!(
            error_msg.contains("Tenant mismatch") || error_msg.contains("access denied"),
            "Expected tenant mismatch or access denied error, got: {}",
            error_msg
        );
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_batch_execute_compensation_actions_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create an Approved compensation action with TenantA
        // Must be Approved + Automatic feasibility for batch_execute
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execute
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Manually set to Approved status (necessary for batch_execute)
        use compensation_service::CompensationStatus;
        let _approved_action = state
            .compensation_action_service
            .update_status(
                created.id,
                CompensationStatus::Approved,
                created.lock_version,
            )
            .await
            .unwrap();

        // Batch execute with TenantA (matching)
        let request = BatchOrchestrationRequest {
            action_ids: vec![created.id],
            initiated_by: Some("test-initiator".to_string()),
        };

        let result = super::batch_execute_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Query(OrchestrationQuery {
                tenant_id: tenant_a,
            }),
            Json(request),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Expected success with matching tenant, got: {:?}",
            result
        );
        let response = result.unwrap();
        assert_eq!(response.summary.succeeded, 1);
        assert_eq!(response.summary.failed, 0);
    }
}
