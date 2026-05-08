//! Trigger reapproval handlers.
//!
//! Phase 2b ADR-07: Contains POST handler for triggering re-approval when scope changes.

use axum::{extract::State, Json};
use intent_rebase_types::get_current_trace_context;

use crate::{publish_audit_event, types::TriggerReapprovalResponse, ApiErrorResponse, AppState};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// Test imports
#[cfg(test)]
use crate::RebaseOrchestrator;
#[cfg(test)]
use axum::http::StatusCode;
#[cfg(test)]
use axum::response::IntoResponse;
#[cfg(test)]
use compensation_service::{
    CompensationActionService, InMemoryCompensationActionRepository,
    InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
};
#[cfg(test)]
use forensic_service::{
    ForensicBundleService, InMemoryBundleRepository, InMemoryBundleStorage,
    InMemoryForensicArchiveGenerator, InMemoryForensicDataCollector,
    InMemoryForensicVerificationService,
};
#[cfg(test)]
use graph_service::{GraphService, InMemoryGraphRepository};
#[cfg(test)]
use intent_rebase_types::InMemoryAuditRepository;
#[cfg(test)]
use intent_service::{
    ApprovalRequestStatus, InMemoryApprovalRequestRepository, InMemoryCheckpointRepository,
    InMemoryIntentRepository, InMemoryPolicySnapshotRepository, IntentService,
};
#[cfg(test)]
use runtime_adapter::MockAdapter;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Instant;
#[cfg(test)]
use uuid::Uuid;

// ============================================================================
// Trigger Reapproval Handlers (Phase 2b ADR-07 bounded slice)
// ============================================================================

/// POST /approval-requests/trigger-reapproval - Trigger re-approval for scope change
///
/// **ADR-07 bounded slice**: Creates a pending approval request when scope hashes differ.
///
/// **Behavior**:
/// - If `original_scope_hash != current_scope_hash`: Creates new pending approval request
/// - If `original_scope_hash == current_scope_hash`: Returns 400 Bad Request (no scope drift)
/// - If intent not found: Returns 404
///
/// **Phase 3 P3-S5 bounded RLS slice**: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler validates tenant ownership before creating the approval request.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
///
/// **Scope limitations**:
/// - Does NOT send notifications (Phase 3 external notification system)
/// - Cancels existing Approved approvals for same tenant+intent (non-Approved statuses unaffected)
/// - Does NOT trigger rebase or orchestration
/// - Does NOT claim production readiness
///
/// **Use case**: Called by external systems that detect scope drift and need to
/// trigger a new approval cycle while preserving audit trail.
#[cfg(feature = "jwt-auth")]
pub async fn trigger_reapproval(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<crate::types::TriggerReapprovalRequest>,
) -> Result<(axum::http::StatusCode, Json<TriggerReapprovalResponse>), ApiErrorResponse> {
    // Step 1: Check if scope hashes match — if so, return 400 (no reapproval needed)
    if request.original_scope_hash == request.current_scope_hash {
        return Err(ApiErrorResponse(
            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                "Scope hashes match — no re-approval required".into(),
            ),
        ));
    }

    // Step 2: Verify intent exists to get workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2b: Phase 3 P3-S5 tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(ref rls_claims) = optional_rls_claims {
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("trigger_reapproval: tenant mismatch rejection");
            return Err(ApiErrorResponse(
                intent_rebase_types::IntentRebaseError::Unauthorized(msg),
            ));
        }
    }

    // Actor attribution: external-api/trigger-reapproval
    let actor_id = "external-api/trigger-reapproval";

    // Step 3: Create new pending approval request using existing primitives
    let approval_request = intent_service::ApprovalRequest::new_pending(
        request.intent_id,
        request.original_version_from,
        request.current_version_to,
        intent_head.intent.workflow_id,
        intent_head.intent.tenant_id,
        actor_id,
        "external-api",
        "ScopeChange",
        &request.reapproval_reason,
    );

    // Step 3b: P1-S5f/P1-S5i RLS transaction wrapping for create+cancel
    // Check if RLS path is available (pool exists AND JWT claims present AND SQL repo)
    let created_approval;
    if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        if let Some(sql_repo) = state.approval_request_repo.as_sqlx_approval_repo() {
            // Use RLS-aware transaction for create+cancel
            let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
            let mut tx = match tx_result {
                Ok(tx) => tx,
                Err(e) => {
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::Internal(format!(
                            "trigger_reapproval: failed to begin RLS transaction: {}",
                            e
                        )),
                    ));
                }
            };

            // Create approval request within transaction
            match sql_repo
                .create_approval_request_with_tx(&mut tx, &approval_request)
                .await
            {
                Ok(created) => created_approval = created,
                Err(e) => {
                    tracing::warn!("trigger_reapproval: RLS create failed, rolling back: {}", e);
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::StorageError(format!(
                            "trigger_reapproval: RLS approval creation failed: {}",
                            e
                        )),
                    ));
                }
            };

            // Cancel existing Approved approvals within the same transaction
            let cancellation_reason = format!(
                "Superseded by new approval request {} due to scope change",
                created_approval.id
            );
            match sql_repo
                .cancel_approved_by_intent_with_tx(
                    &mut tx,
                    request.intent_id,
                    intent_head.intent.tenant_id,
                    actor_id,
                    &cancellation_reason,
                )
                .await
            {
                Ok(_cancelled_count) => {
                    tracing::debug!(
                        "trigger_reapproval: cancelled {} existing approved approvals within RLS tx",
                        _cancelled_count
                    );
                }
                Err(e) => {
                    tracing::warn!("trigger_reapproval: RLS cancel failed, rolling back: {}", e);
                    return Err(ApiErrorResponse(
                        intent_rebase_types::IntentRebaseError::StorageError(format!(
                            "trigger_reapproval: RLS cancellation failed: {}",
                            e
                        )),
                    ));
                }
            }

            // Commit the RLS transaction
            if let Err(e) = tx.commit().await {
                return Err(ApiErrorResponse(
                    intent_rebase_types::IntentRebaseError::StorageError(format!(
                        "trigger_reapproval: failed to commit RLS transaction: {}",
                        e
                    )),
                ));
            }

            tracing::debug!(
                "trigger_reapproval: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );
        } else {
            // Fallback: non-SQL repo, use bare pool create+cancel
            tracing::debug!(
                "trigger_reapproval: rls_pool set but repo doesn't support SQL, falling back to bare pool"
            );
            created_approval = state
                .approval_request_repo
                .create_approval_request(approval_request)
                .await
                .map_err(ApiErrorResponse)?;

            // Cancel any existing Approved approvals for this intent+tenant
            let _cancelled_count = crate::cancel_existing_approved_and_audit(
                &state.approval_request_repo,
                &state.audit_service,
                &state.event_publisher,
                request.intent_id,
                intent_head.intent.tenant_id,
                actor_id,
                request.original_version_from,
                request.current_version_to,
                "ScopeChange",
                created_approval.id,
            )
            .await;
        }
    } else {
        // Non-RLS path: use bare pool operations
        created_approval = state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
            .map_err(ApiErrorResponse)?;

        // Cancel any existing Approved approvals for this intent+tenant
        let _cancelled_count = crate::cancel_existing_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            request.intent_id,
            intent_head.intent.tenant_id,
            actor_id,
            request.original_version_from,
            request.current_version_to,
            "ScopeChange",
            created_approval.id,
        )
        .await;
    }

    // Step 4: Emit audit event (best-effort, post-commit)
    let audit_payload = intent_rebase_types::ApprovalRequestedAuditPayload {
        approval_request_id: created_approval.id,
        intent_id: request.intent_id,
        intent_version_from: request.original_version_from,
        intent_version_to: request.current_version_to,
        decision_class: "ScopeChange".to_string(),
        reapproval_reason: request.reapproval_reason.clone(),
        original_scope_hash: request.original_scope_hash.clone(),
        current_scope_hash: request.current_scope_hash.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_requested(
            intent_head.intent.tenant_id,
            actor_id,
            request.intent_id,
            audit_payload,
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRequested audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ApprovalRequested",
            &serde_json::to_value(serde_json::json!({
                "approval_request_id": created_approval.id,
                "intent_id": request.intent_id,
                "intent_version_from": request.original_version_from,
                "intent_version_to": request.current_version_to,
                "reason": request.reapproval_reason
            }))
            .unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Step 5: Return response
    Ok((
        axum::http::StatusCode::CREATED,
        Json(TriggerReapprovalResponse {
            approval_request_id: created_approval.id,
            intent_id: request.intent_id,
            intent_version_from: request.original_version_from,
            intent_version_to: request.current_version_to,
            status: format!("{:?}", created_approval.status),
            notification_intent: true, // Advisory only — Phase 3 handles actual delivery
            reason: request.reapproval_reason,
        }),
    ))
}

/// POST /approval-requests/trigger-reapproval - Trigger re-approval for scope change (non-JWT fallback)
///
/// **ADR-07 bounded slice**: Creates a pending approval request when scope hashes differ.
/// Non-JWT path for backward compatibility when jwt-auth feature is disabled.
///
/// **Behavior**:
/// - If `original_scope_hash != current_scope_hash`: Creates new pending approval request
/// - If `original_scope_hash == current_scope_hash`: Returns 400 Bad Request (no scope drift)
/// - If intent not found: Returns 404
///
/// **Scope limitations**:
/// - Does NOT send notifications (Phase 3 external notification system)
/// - Cancels existing Approved approvals for same tenant+intent (non-Approved statuses unaffected)
/// - Does NOT trigger rebase or orchestration
/// - Does NOT claim production readiness
#[cfg(not(feature = "jwt-auth"))]
pub async fn trigger_reapproval(
    State(state): State<AppState>,
    Json(request): Json<crate::types::TriggerReapprovalRequest>,
) -> Result<(axum::http::StatusCode, Json<TriggerReapprovalResponse>), ApiErrorResponse> {
    // Step 1: Check if scope hashes match — if so, return 400 (no reapproval needed)
    if request.original_scope_hash == request.current_scope_hash {
        return Err(ApiErrorResponse(
            intent_rebase_types::IntentRebaseError::InvalidIngestRequest(
                "Scope hashes match — no re-approval required".into(),
            ),
        ));
    }

    // Step 2: Verify intent exists to get workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 3: Create new pending approval request using existing primitives
    // Actor attribution: external-api/trigger-reapproval
    let actor_id = "external-api/trigger-reapproval";

    let approval_request = intent_service::ApprovalRequest::new_pending(
        request.intent_id,
        request.original_version_from,
        request.current_version_to,
        intent_head.intent.workflow_id,
        intent_head.intent.tenant_id,
        actor_id,
        "external-api",
        "ScopeChange",
        &request.reapproval_reason,
    );

    // Step 4: Persist the approval request
    let created = state
        .approval_request_repo
        .create_approval_request(approval_request)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 4b: Cancel any existing Approved approvals for this intent+tenant
    // Uses cancel_existing_approved_and_audit helper to handle both cancellation and audit.
    // Only Approved approvals are cancelled; Pending/Rejected/Expired are not affected.
    let _cancelled_count = crate::cancel_existing_approved_and_audit(
        &state.approval_request_repo,
        &state.audit_service,
        &state.event_publisher,
        request.intent_id,
        intent_head.intent.tenant_id,
        actor_id,
        request.original_version_from,
        request.current_version_to,
        "ScopeChange",
        created.id,
    )
    .await;

    // Step 5: Emit audit event (best-effort)
    let audit_payload = intent_rebase_types::ApprovalRequestedAuditPayload {
        approval_request_id: created.id,
        intent_id: request.intent_id,
        intent_version_from: request.original_version_from,
        intent_version_to: request.current_version_to,
        decision_class: "ScopeChange".to_string(),
        reapproval_reason: request.reapproval_reason.clone(),
        original_scope_hash: request.original_scope_hash.clone(),
        current_scope_hash: request.current_scope_hash.clone(),
    };

    if let Err(e) = state
        .audit_service
        .record_approval_requested(
            intent_head.intent.tenant_id,
            actor_id,
            request.intent_id,
            audit_payload,
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ApprovalRequested audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ApprovalRequested",
            &serde_json::to_value(serde_json::json!({
                "approval_request_id": created.id,
                "intent_id": request.intent_id,
                "intent_version_from": request.original_version_from,
                "intent_version_to": request.current_version_to,
                "reason": request.reapproval_reason
            }))
            .unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Step 6: Return response
    Ok((
        axum::http::StatusCode::CREATED,
        Json(TriggerReapprovalResponse {
            approval_request_id: created.id,
            intent_id: request.intent_id,
            intent_version_from: request.original_version_from,
            intent_version_to: request.current_version_to,
            status: format!("{:?}", created.status),
            notification_intent: true, // Advisory only — Phase 3 handles actual delivery
            reason: request.reapproval_reason,
        }),
    ))
}

// ============================================================================
// Tests for Trigger Reapproval Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TriggerReapprovalRequest;
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
        IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
        IntentScope, RiskTier, Urgency,
    };

    /// Create minimal AppState for trigger reapproval handler tests
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

    /// Helper to create OptionalRlsTenantClaims for testing
    #[cfg(feature = "jwt-auth")]
    fn create_test_optional_rls_claims(tenant_id: Uuid) -> auth::OptionalRlsTenantClaims {
        auth::OptionalRlsTenantClaims(Some(create_test_rls_claims(tenant_id)))
    }

    // =====================================================================
    // ADR-07: Approval Revalidation/Re-approval Trigger Tests (bounded slice)
    // =====================================================================

    #[tokio::test]
    async fn test_trigger_reapproval_creates_pending_approval_when_scope_differs() {
        let state = create_test_service();

        // Create an intent first (we need it to exist for get_intent_head to work)
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Call trigger_reapproval with different scope hashes
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "hash_v1".to_string(),
            current_scope_hash: "hash_v2".to_string(), // Different hash
            reapproval_reason: "Scope has changed since approval was granted".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("trigger_reapproval should succeed when scope hashes differ");

        // Verify response
        assert_eq!(result.1.intent_id, intent_id);
        assert_eq!(result.1.intent_version_from, 1);
        assert_eq!(result.1.intent_version_to, 2);
        assert_eq!(result.1.status, "Pending");
        assert!(result.1.notification_intent); // Always true (advisory only)
        assert_eq!(
            result.1.reason,
            "Scope has changed since approval was granted"
        );

        // Verify the approval request was created in the repository
        let created_approval = state
            .approval_request_repo
            .get_approval_request(result.1.approval_request_id)
            .await
            .unwrap();
        assert_eq!(created_approval.status, ApprovalRequestStatus::Pending);
        assert_eq!(created_approval.intent_version_from, 1);
        assert_eq!(created_approval.intent_version_to, 2);
    }

    #[tokio::test]
    async fn test_trigger_reapproval_returns_bad_request_when_scope_matches() {
        let state = create_test_service();

        // Create an intent
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Call trigger_reapproval with SAME scope hashes (no drift)
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "same_hash".to_string(),
            current_scope_hash: "same_hash".to_string(), // Same hash
            reapproval_reason: "Should not trigger".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_trigger_reapproval_returns_not_found_when_intent_missing() {
        let state = create_test_service();

        let request = TriggerReapprovalRequest {
            intent_id: Uuid::new_v4(), // Non-existent intent
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "hash_v1".to_string(),
            current_scope_hash: "hash_v2".to_string(),
            reapproval_reason: "Test".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_trigger_reapproval_cancels_existing_approved_approvals() {
        let state = create_test_service();

        // Create an intent
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get intent head to get tenant_id
        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create an existing approved approval request
        let existing_approved = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Previous approval",
        );
        let existing_approved_id = existing_approved.id;
        state
            .approval_request_repo
            .create_approval_request(existing_approved)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                existing_approved_id,
                ApprovalRequestStatus::Approved,
                "approver",
                None,
            )
            .await
            .unwrap();

        // Verify the existing approval is Approved
        let verified_approved = state
            .approval_request_repo
            .get_approval_request(existing_approved_id)
            .await
            .unwrap();
        assert_eq!(verified_approved.status, ApprovalRequestStatus::Approved);

        // Call trigger_reapproval with different scope hashes
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "hash_v1".to_string(),
            current_scope_hash: "hash_v2".to_string(), // Different hash
            reapproval_reason: "Scope has changed since approval was granted".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("trigger_reapproval should succeed when scope hashes differ");

        // Verify a new pending approval was created
        assert_eq!(result.1.status, "Pending");

        // Verify the existing approved approval was cancelled
        let cancelled_approved = state
            .approval_request_repo
            .get_approval_request(existing_approved_id)
            .await
            .unwrap();
        assert_eq!(
            cancelled_approved.status,
            ApprovalRequestStatus::Cancelled,
            "Existing approved approval should be cancelled"
        );
    }

    #[tokio::test]
    async fn test_trigger_reapproval_does_not_cancel_pending_approvals() {
        let state = create_test_service();

        // Create an intent
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get intent head to get tenant_id
        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create an existing pending approval request
        let existing_pending = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Previous pending approval",
        );
        let existing_pending_id = existing_pending.id;
        state
            .approval_request_repo
            .create_approval_request(existing_pending)
            .await
            .unwrap();

        // Verify the existing approval is Pending
        let verified_pending = state
            .approval_request_repo
            .get_approval_request(existing_pending_id)
            .await
            .unwrap();
        assert_eq!(verified_pending.status, ApprovalRequestStatus::Pending);

        // Call trigger_reapproval with different scope hashes
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "hash_v1".to_string(),
            current_scope_hash: "hash_v2".to_string(), // Different hash
            reapproval_reason: "Scope has changed since approval was granted".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("trigger_reapproval should succeed when scope hashes differ");

        // Verify a new pending approval was created
        assert_eq!(result.1.status, "Pending");

        // Verify the existing pending approval is still Pending (not cancelled)
        let still_pending = state
            .approval_request_repo
            .get_approval_request(existing_pending_id)
            .await
            .unwrap();
        assert_eq!(
            still_pending.status,
            ApprovalRequestStatus::Pending,
            "Existing pending approval should NOT be cancelled"
        );
    }

    #[tokio::test]
    async fn test_trigger_reapproval_does_not_create_or_cancel_when_scope_matches() {
        let state = create_test_service();

        // Create an intent
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get intent head to get tenant_id
        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create an existing approved approval request
        let existing_approved = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Previous approval",
        );
        let existing_approved_id = existing_approved.id;
        state
            .approval_request_repo
            .create_approval_request(existing_approved)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                existing_approved_id,
                ApprovalRequestStatus::Approved,
                "approver",
                None,
            )
            .await
            .unwrap();

        // Verify the existing approval is Approved
        let verified_approved = state
            .approval_request_repo
            .get_approval_request(existing_approved_id)
            .await
            .unwrap();
        assert_eq!(verified_approved.status, ApprovalRequestStatus::Approved);

        // Call trigger_reapproval with SAME scope hashes (should return 400)
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "same_hash".to_string(),
            current_scope_hash: "same_hash".to_string(), // Same hash - no drift
            reapproval_reason: "Should not trigger".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        assert!(result.is_err());

        // Verify error is BAD_REQUEST
        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Verify the existing approved approval is still Approved (not cancelled)
        let still_approved = state
            .approval_request_repo
            .get_approval_request(existing_approved_id)
            .await
            .unwrap();
        assert_eq!(
            still_approved.status,
            ApprovalRequestStatus::Approved,
            "Existing approved approval should NOT be cancelled when scope hashes match"
        );
    }

    // =====================================================================
    // ADR-07: trigger_reapproval JWT Tenant Mismatch Tests (Phase 3 P3-S5)
    // =====================================================================

    #[tokio::test]
    #[cfg(feature = "jwt-auth")]
    async fn test_trigger_reapproval_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create an intent first (we need it to exist for get_intent_head to work)
        let workflow_id = Uuid::new_v4();

        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.0,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get intent head to find the tenant_id (TenantA)
        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_a = intent_head.intent.tenant_id;

        // Create JWT claims for a different tenant (TenantB)
        let tenant_b = Uuid::new_v4();

        // Call trigger_reapproval with tenant mismatch (JWT has TenantB, intent has TenantA)
        let request = TriggerReapprovalRequest {
            intent_id,
            original_version_from: 1,
            current_version_to: 2,
            original_scope_hash: "hash_v1".to_string(),
            current_scope_hash: "hash_v2".to_string(), // Different hash - would normally succeed
            reapproval_reason: "Scope has changed since approval was granted".to_string(),
        };

        let result = super::trigger_reapproval(
            State(state.clone()),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Json(request),
        )
        .await;

        // Verify the request was rejected with Unauthorized
        assert!(
            result.is_err(),
            "trigger_reapproval should fail on tenant mismatch"
        );
        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Tenant mismatch should return 401 Unauthorized"
        );

        // Verify no approval request was created (fail-closed before mutation)
        let approvals = state
            .approval_request_repo
            .list_by_intent(intent_id, tenant_a)
            .await
            .unwrap();
        assert!(
            approvals.is_empty(),
            "No approval should be created when tenant mismatch is detected"
        );
    }
}
