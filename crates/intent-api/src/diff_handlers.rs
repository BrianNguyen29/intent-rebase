//! Diff computation handlers.
//!
//! Bounded handler decomposition slice: Contains the compute_diff handler
//! for computing diffs between intent versions.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::DiffRequest;
use uuid::Uuid;

use crate::{ApiErrorResponse, AppState, DiffResponse};

// ============================================================================
// Diff Computation Handler
// ============================================================================

/// Record diff compute duration
pub(crate) fn record_diff_compute_duration(duration_secs: f64) {
    metrics::histogram!("intent_api_diff_compute_duration_seconds").record(duration_secs);
}

/// POST /intents/{intent_id}/diff - Compute diff between two versions
///
/// Request body: { from_version, to_version }
/// Response: version context plus diff and risk analysis
pub async fn compute_diff(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<DiffResponse>, ApiErrorResponse> {
    let start = std::time::Instant::now();
    let result = state
        .service
        .compute_diff(intent_id, request.from_version, request.to_version)
        .await;

    let duration = start.elapsed().as_secs_f64();
    record_diff_compute_duration(duration);

    match result {
        Ok((from_version, to_version, diff, risk)) => Ok(Json(DiffResponse {
            intent_id,
            from_version,
            to_version,
            diff,
            risk,
        })),
        Err(e) => Err(ApiErrorResponse(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, response::IntoResponse};
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
        IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
        IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
    };
    use intent_service::InMemoryIntentRepository;
    use std::sync::Arc;
    use std::time::Instant;

    /// Minimal test service creator for diff handler tests.
    /// Creates an AppState with only the components needed for compute_diff testing.
    fn create_test_service_for_diff() -> AppState {
        let repo = Arc::new(InMemoryIntentRepository::new());
        let service = Arc::new(intent_service::IntentService::new(repo));
        AppState {
            service,
            graph_service: Arc::new(graph_service::GraphService::new(Arc::new(
                graph_service::InMemoryGraphRepository::new(),
            ))),
            orchestrator: Arc::new(rebase_orchestrator::RebaseOrchestrator::new(
                Arc::new(intent_service::InMemoryCheckpointRepository::new()),
                Arc::new(graph_service::GraphService::new(Arc::new(
                    graph_service::InMemoryGraphRepository::new(),
                ))),
                Arc::new(runtime_adapter::MockAdapter::ready()),
            )),
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            event_publisher: None,
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: Arc::new(
                compensation_service::CompensationActionService::new(Arc::new(
                    compensation_service::InMemoryCompensationActionRepository::new(),
                )),
            ),
            orchestration_runtime: Arc::new(compensation_service::OrchestrationRuntime::new(
                Arc::new(compensation_service::CompensationActionService::new(
                    Arc::new(compensation_service::InMemoryCompensationActionRepository::new()),
                )),
                Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new()),
            )),
            forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new()),
            forensic_archive_generator: Arc::new(
                forensic_service::InMemoryForensicArchiveGenerator::new()
                    .with_intent_version_count(5)
                    .with_artifact_count(10)
                    .with_audit_event_count(100)
                    .with_policy_snapshot_count(3),
            ),
            forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
                Arc::new(forensic_service::InMemoryBundleRepository::new()),
                Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
                Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
            )),
            start_time: Instant::now(),
            rls_pool: None,
        }
    }

    fn create_test_payload() -> IntentPayload {
        IntentPayload {
            objective: IntentObjective {
                summary: "Test intent".to_string(),
                success_statement: "Success".to_string(),
                domain: "testing".to_string(),
            },
            scope: IntentScope {
                in_scope: vec!["item1".to_string()],
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
        }
    }

    #[tokio::test]
    async fn test_compute_diff_success() {
        let state = create_test_service_for_diff();

        // Create an intent first
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
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

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Test the compute_diff handler directly
        let diff_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = compute_diff(State(state), Path(intent_id), Json(diff_request))
            .await
            .expect("Diff computation should succeed");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version.version_number, 1);
        assert_eq!(result.to_version.version_number, 2);
    }

    #[tokio::test]
    async fn test_compute_diff_invalid_version_ordering() {
        let state = create_test_service_for_diff();

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
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

        // Test with reversed version order (from_version > to_version)
        let diff_request = DiffRequest {
            from_version: 2,
            to_version: 1,
        };
        let result = compute_diff(State(state), Path(intent_id), Json(diff_request)).await;
        // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
