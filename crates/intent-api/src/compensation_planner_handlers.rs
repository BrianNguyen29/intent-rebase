//! Compensation planner and orchestration dry-run handlers.
//!
//! Phase 3: Contains POST /compensation-actions/plan and
//! POST /compensation-actions/orchestration-dry-run handlers for bounded
//! compensation planning and dry-run orchestration.

use axum::{extract::State, Json};
use intent_rebase_types::IntentRebaseError;

use crate::{
    types::{
        CompensationActionResponse, FeasibilityCounts, OrchestrationDryRunProposalResponse,
        OrchestrationDryRunRequest, OrchestrationDryRunResponse,
        OrchestrationDryRunSummaryResponse, OrchestrationQuery, PlanCompensationActionsRequest,
        PlanCompensationActionsResponse,
    },
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

#[cfg(feature = "jwt-auth")]
use compensation_service;

pub(crate) use crate::types::format_compensation_status;

// ============================================================================
// Bounded Compensation Planner (Phase 3 bounded planner slice)
// ============================================================================

/// POST /compensation-actions/plan - Plan compensation actions from side effects
///
/// Phase 3 (bounded planner slice): Fetches side effects for the given intent,
/// classifies them using S0-S4 classification, and generates appropriate
/// compensation actions.
///
/// **S0-S4 classification:**
/// | Class | Strategy | Feasibility | Action |
/// |-------|----------|-------------|--------|
/// | S0PureRead | (none) | NotPossible | Skip - no action needed |
/// | S1InternalReversible | Rollback | Automatic | Auto rollback |
/// | S2ExternalReversible | CounterAction | SemiAutomatic | Counter-action with manual trigger |
/// | S3ExternalPartiallyReversible | FollowupNotice | ManualOnly | Manual followup required |
/// | S4Irreversible | Escalation | NotPossible | Escalation required |
///
/// **Returns:** All generated compensation actions (S0 produces no action).
#[cfg(feature = "jwt-auth")]
pub async fn plan_compensation_actions(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<PlanCompensationActionsRequest>,
) -> Result<Json<PlanCompensationActionsResponse>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("plan_compensation_actions: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let rebase_context = compensation_service::RebaseContext::new(
        request.intent_id,
        request.from_version,
        request.to_version,
        request.workflow_id,
    );

    let actions = state
        .compensation_action_service
        .plan_compensation_actions(request.intent_id, request.tenant_id, rebase_context)
        .await
        .map_err(ApiErrorResponse)?;

    // Count by feasibility
    let mut feasibility_counts = FeasibilityCounts {
        automatic: 0,
        semi_automatic: 0,
        manual_only: 0,
        not_possible: 0,
    };

    for action in &actions {
        match action.feasibility {
            compensation_service::CompensationFeasibility::Automatic => {
                feasibility_counts.automatic += 1
            }
            compensation_service::CompensationFeasibility::SemiAutomatic => {
                feasibility_counts.semi_automatic += 1
            }
            compensation_service::CompensationFeasibility::ManualOnly => {
                feasibility_counts.manual_only += 1
            }
            compensation_service::CompensationFeasibility::NotPossible => {
                feasibility_counts.not_possible += 1
            }
        }
    }

    let total = actions.len();
    let action_responses: Vec<CompensationActionResponse> = actions
        .into_iter()
        .map(CompensationActionResponse::from)
        .collect();

    Ok(Json(PlanCompensationActionsResponse {
        actions: action_responses,
        total,
        feasibility_counts,
    }))
}

/// POST /compensation-actions/plan - Plan compensation actions from side effects (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn plan_compensation_actions(
    State(state): State<AppState>,
    Json(request): Json<PlanCompensationActionsRequest>,
) -> Result<Json<PlanCompensationActionsResponse>, ApiErrorResponse> {
    let rebase_context = compensation_service::RebaseContext::new(
        request.intent_id,
        request.from_version,
        request.to_version,
        request.workflow_id,
    );

    let actions = state
        .compensation_action_service
        .plan_compensation_actions(request.intent_id, request.tenant_id, rebase_context)
        .await
        .map_err(ApiErrorResponse)?;

    // Count by feasibility
    let mut feasibility_counts = FeasibilityCounts {
        automatic: 0,
        semi_automatic: 0,
        manual_only: 0,
        not_possible: 0,
    };

    for action in &actions {
        match action.feasibility {
            compensation_service::CompensationFeasibility::Automatic => {
                feasibility_counts.automatic += 1
            }
            compensation_service::CompensationFeasibility::SemiAutomatic => {
                feasibility_counts.semi_automatic += 1
            }
            compensation_service::CompensationFeasibility::ManualOnly => {
                feasibility_counts.manual_only += 1
            }
            compensation_service::CompensationFeasibility::NotPossible => {
                feasibility_counts.not_possible += 1
            }
        }
    }

    let total = actions.len();
    let action_responses: Vec<CompensationActionResponse> = actions
        .into_iter()
        .map(CompensationActionResponse::from)
        .collect();

    Ok(Json(PlanCompensationActionsResponse {
        actions: action_responses,
        total,
        feasibility_counts,
    }))
}

// ============================================================================
// Manual Orchestration & Dry-Run Planner (Phase 3 Batch 1 bounded slice)
// ============================================================================

/// POST /compensation-actions/orchestration-dry-run - Plan orchestration actions (dry-run)
///
/// Phase 3 Batch 1 (bounded dry-run slice): For each provided compensation_action_id,
/// determines the proposed action (approve | reapprove | execute | no_action) based
/// on the action's current state.
///
/// **This is READ-ONLY** - it does not execute any actions.
///
/// Phase 3 P3-S5 bounded slice (P1-S5i): When valid JWT claims are present, this handler
/// validates tenant ownership before planning. Fails closed on tenant mismatch;
/// fails open when JWT is absent (backward compatible).
///
/// **Action determination logic:**
/// - `approve`: Action is Pending (can transition to Approved)
/// - `reapprove`: Action is Failed AND can_be_reapproved() (retryable error + budget remains)
/// - `execute`: Action is Approved AND is_service_executable() (Rollback+Automatic or CounterAction+SemiAutomatic)
/// - `no_action`: Action is in a terminal state or cannot perform any valid transition
///
/// **Bounded partial-success semantics:**
/// - If an action_id is not found, it's added to `not_found` and does not cause failure
/// - All found actions are processed, even if some have no_action
///
/// **No background worker or queue claiming:**
/// This is a direct query-based planner that reads current state and proposes actions.
#[cfg(feature = "jwt-auth")]
pub async fn orchestration_dry_run(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<OrchestrationDryRunRequest>,
) -> Result<Json<OrchestrationDryRunResponse>, ApiErrorResponse> {
    // Phase 3 P3-S5 (P1-S5i): Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if query.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match query tenant_id ({})",
                rls_claims.tenant_id, query.tenant_id
            );
            tracing::warn!("orchestration_dry_run: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    let result = state
        .compensation_action_service
        .plan_orchestration_actions(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    let proposals = result
        .proposals
        .into_iter()
        .map(|p| OrchestrationDryRunProposalResponse {
            action_id: p.action_id,
            proposed_action: p.proposed_action.as_str().to_string(),
            reason: p.reason,
            current_status: format_compensation_status(&p.current_status),
        })
        .collect();

    let response = OrchestrationDryRunResponse {
        proposals,
        not_found: result.not_found,
        summary: OrchestrationDryRunSummaryResponse {
            total: result.summary.total,
            can_approve: result.summary.can_approve,
            can_reapprove: result.summary.can_reapprove,
            can_execute: result.summary.can_execute,
            no_action: result.summary.no_action,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

/// POST /compensation-actions/orchestration-dry-run - Plan orchestration actions (dry-run) (non-JWT fallback)
///
/// Phase 3 Batch 1 (bounded dry-run slice): Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
///
/// **This is READ-ONLY** - it does not execute any actions.
#[cfg(not(feature = "jwt-auth"))]
pub async fn orchestration_dry_run(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OrchestrationQuery>,
    Json(request): Json<OrchestrationDryRunRequest>,
) -> Result<Json<OrchestrationDryRunResponse>, ApiErrorResponse> {
    let result = state
        .compensation_action_service
        .plan_orchestration_actions(query.tenant_id, request.action_ids)
        .await
        .map_err(ApiErrorResponse)?;

    let proposals = result
        .proposals
        .into_iter()
        .map(|p| OrchestrationDryRunProposalResponse {
            action_id: p.action_id,
            proposed_action: p.proposed_action.as_str().to_string(),
            reason: p.reason,
            current_status: format_compensation_status(&p.current_status),
        })
        .collect();

    let response = OrchestrationDryRunResponse {
        proposals,
        not_found: result.not_found,
        summary: OrchestrationDryRunSummaryResponse {
            total: result.summary.total,
            can_approve: result.summary.can_approve,
            can_reapprove: result.summary.can_reapprove,
            can_execute: result.summary.can_execute,
            no_action: result.summary.no_action,
            not_found: result.summary.not_found,
        },
    };

    Ok(Json(response))
}

// ============================================================================
// Tests for Compensation Planner and Dry-Run Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "jwt-auth")]
    use crate::auth;
    use crate::types::{OrchestrationDryRunRequest, OrchestrationQuery};
    use crate::AppState;
    use axum::extract::{Query, State};
    use axum::Json;
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

    /// Create minimal AppState for compensation planner handler tests
    #[cfg(feature = "jwt-auth")]
    fn create_test_service() -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo));
        let service = Arc::new(IntentService::new(repo));
        let orchestrator = Arc::new(crate::RebaseOrchestrator::new(
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

    /// Helper to create OptionalRlsTenantClaims for testing
    #[cfg(feature = "jwt-auth")]
    fn create_test_optional_rls_claims(tenant_id: Uuid) -> auth::OptionalRlsTenantClaims {
        auth::OptionalRlsTenantClaims(Some(create_test_rls_claims(tenant_id)))
    }

    /// Helper to create RlsTenantClaims for testing
    #[cfg(feature = "jwt-auth")]
    fn create_test_rls_claims(tenant_id: Uuid) -> auth::RlsTenantClaims {
        let claims = auth::Claims {
            sub: "test-user".to_string(),
            tenant_id: tenant_id.to_string(),
            roles: vec!["admin".to_string()],
            exp: 9999999999,
            iat: 0,
        };
        // new_unchecked is #[cfg(test)] so this only works in tests
        auth::RlsTenantClaims::new_unchecked(tenant_id, claims)
    }

    // -------------------------------------------------------------------------
    // orchestration_dry_run Tenant Mismatch Tests (P1-S5i)
    // -------------------------------------------------------------------------

    /// Tests that orchestration_dry_run rejects JWT tenant mismatch.
    /// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match query tenant_id.
    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_orchestration_dry_run_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
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

        // Try to run dry-run with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let query = OrchestrationQuery {
            tenant_id: tenant_a, // Query has TenantA
        };
        let request = OrchestrationDryRunRequest {
            action_ids: vec![created.id],
        };

        let result = super::orchestration_dry_run(
            State(state),
            create_test_optional_rls_claims(tenant_b), // JWT has TenantB - mismatch
            Query(query),
            Json(request),
        )
        .await;

        // Should fail with Unauthorized
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.0.to_string();
        assert!(
            err_msg.contains("Tenant mismatch"),
            "Expected tenant mismatch error, got: {}",
            err_msg
        );
    }

    /// Tests that orchestration_dry_run succeeds when JWT tenant matches query tenant.
    /// P1-S5i: Validates the happy path for tenant-matched requests.
    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_orchestration_dry_run_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        let tenant_a = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_a,
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

        // Run dry-run with TenantA (matching)
        let query = OrchestrationQuery {
            tenant_id: tenant_a,
        };
        let request = OrchestrationDryRunRequest {
            action_ids: vec![created.id],
        };

        let result = super::orchestration_dry_run(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Query(query),
            Json(request),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Expected success with matching tenant, got: {:?}",
            result
        );
    }
}
