//! Replay handlers (Phase 2b bounded replay slice)
//!
//! This module contains the replay_intent handler(s) for initiating bounded
//! replay operations on intent versions.
//!
//! Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
//! seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::{get_current_trace_context, IntentRebaseError, ReplayAuditPayload};
use uuid::Uuid;

use crate::types::{ReplayRequest, ReplayResponse};
use crate::{runtime_execution_status_label, ApiErrorResponse, AppState};

/// POST /intents/{intent_id}/replay - Initiate a bounded replay operation
///
/// Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
/// seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided in request, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Returns bounded replay outcome with checkpoint alignment details.
///
/// Phase 3 P1-S5i: When valid JWT claims are present, this handler validates
/// tenant ownership before initiating replay. Fails closed on tenant mismatch;
/// fails open when JWT is absent (backward compatible).
#[cfg(feature = "jwt-auth")]
pub(crate) async fn replay_intent(
    State(state): State<AppState>,
    crate::auth::OptionalRlsTenantClaims(optional_rls_claims): crate::auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    // Phase 3 P1-S5i: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        // Get intent head to find workflow_id and tenant_id
        let intent_head = state
            .service
            .get_intent_head(intent_id)
            .await
            .map_err(ApiErrorResponse)?;

        // Tenant mismatch rejection: JWT tenant must match the intent's tenant
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("replay_intent: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        let from_version = request
            .from_version
            .unwrap_or(intent_head.version.version_number);
        let to_version = request.to_version;

        // Execute bounded replay via orchestrator
        let replay_result = state
            .orchestrator
            .replay(
                intent_id,
                intent_head.intent.tenant_id,
                intent_head.intent.workflow_id,
                from_version,
                to_version,
                request.checkpoint_id,
            )
            .await
            .map_err(ApiErrorResponse)?;

        // Record ReplayInitiated audit event (best-effort)
        let actor_id = "external-api/replay";
        let audit_payload = ReplayAuditPayload {
            from_version: Some(from_version),
            to_version: Some(to_version),
            checkpoint_id: replay_result.aligned_checkpoint_id,
            checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
            replay_initiated_via: "post-intents-intent-id-replay".to_string(),
            rationale: format!(
                "Bounded replay initiated from v{} to v{} via public replay endpoint",
                from_version, to_version
            ),
        };

        if let Err(e) = state
            .audit_service
            .record_replay_initiated(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                audit_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            crate::publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "ReplayInitiated",
                &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        return Ok(Json(ReplayResponse {
            intent_id,
            from_version,
            to_version,
            aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
            checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
            runtime_execution_status: runtime_execution_status_label(
                &replay_result.runtime_execution_result.status,
            )
            .to_string(),
            signal_sent: replay_result.runtime_execution_result.signal_sent,
            replay_attempted: replay_result.runtime_execution_result.replay_attempted,
            replay_completed: replay_result.runtime_execution_result.replay_completed,
        }));
    }

    // Non-JWT path (no JWT claims) - proceed without tenant validation
    // Get intent head to find workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    let from_version = request
        .from_version
        .unwrap_or(intent_head.version.version_number);
    let to_version = request.to_version;

    // Phase 2b: Validate target version exists before attempting replay
    state
        .service
        .get_version(intent_id, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b: Validate source version exists if explicitly specified
    if request.from_version.is_some() {
        state
            .service
            .get_version(intent_id, from_version)
            .await
            .map_err(ApiErrorResponse)?;
    }

    // Execute bounded replay via orchestrator
    let replay_result = state
        .orchestrator
        .replay(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            from_version,
            to_version,
            request.checkpoint_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Record ReplayInitiated audit event (best-effort)
    let actor_id = "external-api/replay";
    let audit_payload = ReplayAuditPayload {
        from_version: Some(from_version),
        to_version: Some(to_version),
        checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
        replay_initiated_via: "post-intents-intent-id-replay".to_string(),
        rationale: format!(
            "Bounded replay initiated from v{} to v{} via public replay endpoint",
            from_version, to_version
        ),
    };

    if let Err(e) = state
        .audit_service
        .record_replay_initiated(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        crate::publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ReplayInitiated",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ReplayResponse {
        intent_id,
        from_version,
        to_version,
        aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
        runtime_execution_status: runtime_execution_status_label(
            &replay_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: replay_result.runtime_execution_result.signal_sent,
        replay_attempted: replay_result.runtime_execution_result.replay_attempted,
        replay_completed: replay_result.runtime_execution_result.replay_completed,
    }))
}

/// POST /intents/{intent_id}/replay - Initiate a bounded replay operation (non-JWT fallback)
///
/// Phase 2b bounded replay slice: Uses existing cooperative signal-based replay
/// seam via RebaseOrchestrator::replay(). This is NOT native Temporal reset.
///
/// Bounded checkpoint selection strategy:
/// - If `checkpoint_id` is provided in request, use that specific checkpoint
/// - Otherwise, use the most recent active checkpoint for the workflow
///
/// Returns bounded replay outcome with checkpoint alignment details.
#[cfg(not(feature = "jwt-auth"))]
pub(crate) async fn replay_intent(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
    // Non-JWT path - proceed without tenant validation
    // Get intent head to find workflow_id and tenant_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    let from_version = request
        .from_version
        .unwrap_or(intent_head.version.version_number);
    let to_version = request.to_version;

    // Phase 2b: Validate target version exists before attempting replay
    state
        .service
        .get_version(intent_id, to_version)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 2b: Validate source version exists if explicitly specified
    if request.from_version.is_some() {
        state
            .service
            .get_version(intent_id, from_version)
            .await
            .map_err(ApiErrorResponse)?;
    }

    // Execute bounded replay via orchestrator
    let replay_result = state
        .orchestrator
        .replay(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            from_version,
            to_version,
            request.checkpoint_id,
        )
        .await
        .map_err(ApiErrorResponse)?;

    // Record ReplayInitiated audit event (best-effort)
    let actor_id = "external-api/replay";
    let audit_payload = ReplayAuditPayload {
        from_version: Some(from_version),
        to_version: Some(to_version),
        checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome.clone(),
        replay_initiated_via: "post-intents-intent-id-replay".to_string(),
        rationale: format!(
            "Bounded replay initiated from v{} to v{} via public replay endpoint",
            from_version, to_version
        ),
    };

    if let Err(e) = state
        .audit_service
        .record_replay_initiated(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record ReplayInitiated audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        crate::publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "ReplayInitiated",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    Ok(Json(ReplayResponse {
        intent_id,
        from_version,
        to_version,
        aligned_checkpoint_id: replay_result.aligned_checkpoint_id,
        checkpoint_selection_outcome: replay_result.checkpoint_selection_outcome,
        runtime_execution_status: runtime_execution_status_label(
            &replay_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: replay_result.runtime_execution_result.signal_sent,
        replay_attempted: replay_result.runtime_execution_result.replay_attempted,
        replay_completed: replay_result.runtime_execution_result.replay_completed,
    }))
}

// ============================================================================
// Tests for Replay Handlers
// ============================================================================

#[cfg(test)]
mod tests {
    #[cfg(feature = "jwt-auth")]
    use crate::auth;
    use crate::types::ReplayRequest;
    use crate::AppState;
    use axum::extract::{Path, State};
    use axum::Json;
    use compensation_service::{
        CompensationActionService, InMemoryCompensationActionRepository,
        InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
        SideEffectService,
    };
    use forensic_service::{
        ForensicBundleService, InMemoryBundleRepository, InMemoryBundleStorage,
        InMemoryForensicArchiveGenerator, InMemoryForensicDataCollector,
        InMemoryForensicVerificationService,
    };
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_rebase_types::InMemoryAuditRepository;
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
        IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
        IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
    };
    use intent_service::{
        InMemoryApprovalRequestRepository, InMemoryCheckpointRepository, InMemoryIntentRepository,
        InMemoryPolicySnapshotRepository, IntentService,
    };
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;
    use std::time::Instant;
    use uuid::Uuid;

    /// Create minimal AppState for replay handler tests
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
    // replay_intent Tenant Mismatch Tests (P1-S5i)
    // -------------------------------------------------------------------------

    /// Tests that replay_intent rejects JWT tenant mismatch.
    /// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match intent's tenant_id.
    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_replay_intent_rejects_tenant_mismatch() {
        let state = create_test_service();

        // Create an intent (tenant is assigned by the service)
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
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
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get the intent head (tenant_a not used in this test - we test mismatch with tenant_b)
        let _intent_head = state.service.get_intent_head(intent_id).await.unwrap();

        // Create version 2 to enable replay from v1 to v2
        let version_request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent v2".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
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
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, Some(1), None)
            .await
            .unwrap();

        // Try to replay with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let replay_request = ReplayRequest {
            from_version: Some(1),
            to_version: 2,
            checkpoint_id: None,
        };

        let result = super::replay_intent(
            State(state),
            create_test_optional_rls_claims(tenant_b), // JWT has TenantB - mismatch
            Path(intent_id),
            Json(replay_request),
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

    /// Tests that replay_intent succeeds when JWT tenant matches intent's tenant.
    /// P1-S5i: Validates the happy path for tenant-matched requests.
    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_replay_intent_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
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
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec!["test".to_string()],
        };

        let intent_id = state
            .service
            .create_intent(create_request)
            .await
            .unwrap()
            .intent_id;

        // Get the intent head to find the assigned tenant
        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_a = intent_head.intent.tenant_id;

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent v2".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
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
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, Some(1), None)
            .await
            .unwrap();

        // Replay with TenantA (matching)
        let replay_request = ReplayRequest {
            from_version: Some(1),
            to_version: 2,
            checkpoint_id: None,
        };

        let result = super::replay_intent(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Path(intent_id),
            Json(replay_request),
        )
        .await;

        // Should succeed (returns NoCheckpointFound since no checkpoints available)
        assert!(
            result.is_ok(),
            "Expected success with matching tenant, got: {:?}",
            result
        );
    }
}
