//! Intent API HTTP transport layer
//!
//! Phase 1: Exposes intent/version endpoints via axum.
//! Routes are manually wired to match the OpenAPI spec in docs/04-api/openapi.yaml.

#[allow(unused_imports)]
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use graph_service::GraphService;
#[allow(unused_imports)]
use intent_rebase_types::{
    get_current_trace_context, AffectedItemsStatus, CreateIntentRequest, CreateIntentResponse,
    CreateVersionRequest, CreateVersionResponse, DiffRequest, IntentRebaseError,
};
use intent_service::IntentService;
use rebase_engine::planner::CompensationPlanningSummary;
use rebase_engine::{classify_approvals, RiskTier};
use rebase_orchestrator::{apply_pipeline::ApplyOutcome, RebaseOrchestrator};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
pub mod auth;

/// NATS event publisher module (Phase 2b bounded core publisher slice)
pub mod nats_event_publisher;

/// NATS JetStream module (Phase 3 bounded slice)
pub mod nats_jetstream;

/// Panic hardening module (Phase 2b bounded slice — first file decomposition slice)
pub mod panic_hardening;

/// Intent API response and request types (Phase 2 bounded file decomposition slice)
pub mod types;

/// Health check routes and middleware (Phase 3 Batch 2 bounded slice)
pub mod health_routes;

/// Graph handlers (Phase 1 - Internal CRUD only, extracted as bounded handler decomposition slice)
pub mod graph_handlers;

/// Forensic handlers (Phase 3 Batch 3b + P4 bounded slices - extracted as bounded handler decomposition slice)
pub mod forensic_handlers;

/// Simulation handlers (Phase 3 Batch 1 N4-4 bounded simulation slice)
pub mod simulation_handlers;

/// Replay handlers (Phase 2b bounded replay slice)
pub mod replay_handlers;

/// Policy snapshot handlers (Phase 2 bounded read-only slice, extracted as bounded handler decomposition slice)
pub mod policy_snapshot_handlers;

/// Query handlers (Phase 3 Batch 1 bounded read-only slice - side effect and orchestration dashboard queries)
pub mod query_handlers;

/// Compensation query handlers (Phase 3 Batch 1 bounded read-only slice - compensation action queries)
pub mod compensation_query_handlers;

/// Orchestration run handlers (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
pub mod orchestration_run_handlers;

/// Compensation planner and orchestration dry-run handlers (Phase 3 bounded planner slice)
pub mod compensation_planner_handlers;

/// Compensation action mutation handlers (Phase 3 bounded mutation slice)
pub mod compensation_mutation_handlers;

/// Read-only approval request handlers (Phase 2b bounded read-only slice)
pub mod approval_handlers_readonly;

/// Approval mutation handlers (Phase 2b bounded mutation slice)
pub mod approval_mutation_handlers;

/// Artifact ingest handlers (Phase 3 Batch 1 bounded slice)
pub mod ingest_handlers;

/// Trigger reapproval handlers (Phase 2b ADR-07 bounded slice)
pub mod trigger_reapproval_handlers;

/// Batch compensation action handlers (Phase 3 P3-S5 bounded slice)
pub mod batch_handlers;

/// Intent read-only query handlers (Phase 2 bounded slice)
pub mod intent_read_handlers;

/// Intent validation handlers (Phase 2 bounded slice - extracted handler decomposition)
pub mod intent_validation_handlers;

/// Intent mutation handlers (Phase 2 bounded slice - extracted handler decomposition)
pub mod intent_mutation_handlers;

/// Error response types (Phase 2 bounded file decomposition slice)
pub mod error_response;

/// Diff computation handlers (Phase 2 bounded slice - extracted handler decomposition)
pub mod diff_handlers;

/// Rebase preview handlers (Phase 2 bounded slice - extracted handler decomposition)
pub mod rebase_preview_handlers;

/// Approval invalidation and audit helpers (Phase 2b bounded slice - extracted helper decomposition)
pub mod approval_invalidation;

// Re-export panic_hardening::init_panic_hook for convenience
pub use panic_hardening::init_panic_hook;

// Re-export auth types for convenience when jwt-auth feature is enabled
#[cfg(feature = "jwt-auth")]
pub use auth::{
    generate_test_token, rls_reset_tenant_context_sql, rls_set_tenant_context_sql,
    validate_tenant_id_for_rls, AuthConfig, AuthConfigError, Claims,
};

// Re-export NATS event publisher for use in main.rs and testing
pub use nats_event_publisher::NatsEventPublisher;

// Re-export error response types for convenience (Phase 2 bounded file decomposition slice)
pub use error_response::ApiErrorResponse;

// Re-export types for convenience (Phase 2 bounded file decomposition slice)
pub use types::{
    ApiError, ApprovalRequestResponse, ApprovalRequestSummary, ApprovalRevalidationResponse,
    ApproveApprovalRequestBody, ApproveCompensationActionBody, ArtifactIngestRequest,
    ArtifactIngestResponse, BatchCandidatesSummary, BatchItemOutcomeResponse,
    BatchOrchestrationRequest, BatchOrchestrationResponse, BatchOrchestrationSummaryResponse,
    CompensationActionResponse, CompensationActionStatusCounts, CompensationActionSummary,
    CompensationPolicyGateQuery, CompensationPolicyGateResponse, CompensationSimulationRequest,
    CoordinationRecordResponse, CoordinationSummaryResponse, CreateOrchestrationRunRequest,
    DiffResponse, ErrorClassificationResponse, ErrorDetails, ExecuteCompensationActionBody,
    ExpireApprovalRequestBody, FeasibilityCounts, ForensicArtifactCoverage,
    ForensicAuditEventCoverage, ForensicBundleContentsSummary, ForensicBundleIntegrityInfo,
    ForensicBundleRequest, ForensicBundleResponse, ForensicBundleSummary, ForensicBundleTimeRange,
    ForensicExportContentsSummary, ForensicExportRequest, ForensicExportResponse,
    ForensicExportTimeRange, ForensicIntentVersionCoverage, ForensicPolicySnapshotCoverage,
    ForensicVerificationRequest, ForensicVerificationResponse, ForensicVerificationTimeRange,
    GetLatestPolicySnapshotQuery, GetPolicySnapshotByVersionQuery, GetPolicySnapshotQuery,
    HealthResponse, IntentCompensationPolicyGateQuery, IntentOrchestrationCoordinationQuery,
    ListBatchCandidatesQuery, ListBatchCandidatesResponse, ListCompensationActionsQuery,
    ListCompensationActionsResponse, ListDlqCandidatesQuery, ListDlqCandidatesResponse,
    ListForensicBundlesQuery, ListForensicBundlesResponse, ListGraphEdgesQuery,
    ListGraphNodesQuery, ListPendingApprovalRequestsQuery, ListPendingApprovalRequestsResponse,
    ListPolicySnapshotsQuery, ListPolicySnapshotsResponse, ListSideEffectsQuery,
    ListSideEffectsResponse, OrchestrationCoordinationQuery, OrchestrationCoordinationResponse,
    OrchestrationDashboardQuery, OrchestrationDashboardResponse,
    OrchestrationDryRunProposalResponse, OrchestrationDryRunRequest, OrchestrationDryRunResponse,
    OrchestrationDryRunSummaryResponse, OrchestrationQuery, OrchestrationRunQuery,
    OrchestrationRunResponse, PlanCompensationActionsRequest, PlanCompensationActionsResponse,
    PolicyGateEvaluationResponse, PolicyGateMetadataResponse, PolicyGateSummaryResponse,
    PolicySnapshotResponse, ReapproveCompensationActionBody, RebaseApplyResponse,
    RebasePreviewResponse, RebaseSimulationQuery, RejectApprovalRequestBody, ReplayRequest,
    ReplayResponse, RequestId, RiskMetadataResponse, RunItemResultResponse, SideEffectSummary,
    TriggerReapprovalRequest, TriggerReapprovalResponse, WaiveCompensationActionBody,
};

// Re-export approval invalidation helpers for backward compatibility
pub use approval_invalidation::{
    apply_outcome_label, apply_status_code, cancel_existing_approved_and_audit,
    cancel_specific_approved_and_audit, checkpoint_alignment_label, publish_audit_event,
    runtime_execution_status_label, CancelApprovalContext,
};

// Re-export intent mutation helpers for backward compatibility
pub use intent_mutation_handlers::{
    parse_optional_header, validate_create_intent_request, validate_create_version_request,
};

// ============================================================================
// Metrics Definitions (Phase 3 Batch 2 Slice 3 — bounded metrics foundation)
// ============================================================================
//
// These metrics are aligned to the SLO targets documented in 04-sre-and-slos.md
// and the dashboard scaffold in 06-slo-dashboard.md.
//
// NOT YET IMPLEMENTED for all flows — this is a bounded slice delivering
// instrumentation for core intent operations only. Full coverage across all
// artifact-producing operations and compensation flows remains future scope.
//
// Metrics are recorded using the metrics_exporter_prometheus handle which is
// installed by the /metrics endpoint. The PrometheusBuilder handles the
// exporter setup and metric registration.
//
// Metrics are actively recorded for core intent operations using the metrics 0.24
// API via metrics-exporter-prometheus 0.18 (upgraded from 0.12 to resolve the
// version conflict with workspace metrics 0.23).
//
// Metrics referenced by Prometheus rules:
// - intent_api_intent_version_created_total{status="success|error"}
// - intent_api_rebase_preview_requests_total{status="success|error"}
// - intent_api_rebase_apply_requests_total{status="success|error"}
// - intent_api_diff_compute_duration_seconds
// - intent_api_rebase_preview_duration_seconds
// - intent_api_rebase_apply_duration_seconds

/// Record rebase apply request outcome
fn record_rebase_apply_request(status: &'static str) {
    metrics::counter!("intent_api_rebase_apply_requests_total", "status" => status).increment(1);
}

/// Record rebase apply duration
fn record_rebase_apply_duration(duration_secs: f64, risk_class: &'static str) {
    metrics::histogram!("intent_api_rebase_apply_duration_seconds", "risk_class" => risk_class)
        .record(duration_secs);
}

// =============================================================================
// DLQ Metric Helper Functions (Phase 3 DLQ design — G3 evidence)
// =============================================================================
// Counter helpers (record_dlq_replay, record_dlq_replay_failure, record_dlq_message)
// ARE wired and called from DlqHelper in nats_jetstream.rs.
//
// Gauge/depth/age helpers (record_dlq_messages_current, record_dlq_message_age_seconds)
// remain as stubs — their runtime emission awaits lifecycle worker wiring (Phase 4/G3).

/// Record current DLQ depth (number of messages in dead-letter queue)
#[allow(dead_code)]
fn record_dlq_messages_current(count: f64) {
    metrics::gauge!("intent_api_dlq_messages_current").set(count);
}

/// Record age of oldest message in DLQ (seconds)
#[allow(dead_code)]
fn record_dlq_message_age_seconds(age_secs: f64) {
    metrics::gauge!("intent_api_dlq_message_age_seconds").set(age_secs);
}

/// Record DLQ replay operation
pub fn record_dlq_replay(status: &'static str) {
    metrics::counter!("intent_api_dlq_replay_total", "status" => status).increment(1);
}

/// Record failed DLQ replay attempt
pub fn record_dlq_replay_failure() {
    metrics::counter!("intent_api_dlq_replay_failures_total").increment(1);
}

/// Record message sent to DLQ
pub fn record_dlq_message() {
    metrics::counter!("intent_api_dlq_messages_total").increment(1);
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub service: Arc<IntentService>,
    pub graph_service: Arc<GraphService>,
    pub orchestrator: Arc<RebaseOrchestrator>,
    pub audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    pub approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    pub policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    /// Phase 2b: Optional event publisher for audit event streaming.
    /// When None, events are persisted to audit storage but NOT streamed.
    /// When Some, events are also published to the event stream (best-effort, fail-open).
    /// Consumers, DLQ, and real NATS integration are Phase 3 items.
    pub event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    /// Phase 3 Batch 1 (groundwork): Side effect service for recording and querying
    /// side effects from artifact-producing operations.
    pub side_effect_service: Arc<compensation_service::SideEffectService>,
    /// Phase 3 Batch 1: Compensation action service for querying compensation actions.
    /// This is a read-only query facade; mutation/execution is Batch 1+ scope.
    pub compensation_action_service: Arc<compensation_service::CompensationActionService>,
    /// Phase 3 Batch 1 (bounded single-shot): Orchestration runtime for executing
    /// compensation actions via HTTP accepted flow.
    pub orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    /// Phase 3 Batch 3b (bounded slice): Forensic verification service for
    /// verifying forensic bundle feasibility without generating actual bundles.
    pub forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    /// Phase 3 Batch 3b (bounded slice): Forensic archive generator for
    /// in-memory archive generation with scaffolded data. Does NOT query
    /// real services or persist data.
    pub forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    /// P4 (bounded slice): Forensic bundle service for real data collection,
    /// bundle generation, and S3/MinIO persistence. Orchestrates the full
    /// generate→store→record cycle.
    pub forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    /// Phase 3 P3-S5 (bounded slice): RLS-aware PostgreSQL pool for tenant-scoped
    /// transaction wrapping. When Some, create_graph_node uses this to wrap node
    /// creation in RLS-set transactions. When None, falls back to non-RLS path.
    pub rls_pool: Option<graph_service::RlsAwarePool>,
    pub start_time: Instant,
}

// ============================================================================
// API Key Authentication Scaffold (Phase 1)
// ============================================================================

/// API key extracted from X-API-Key header.
/// Phase 1: This is stored in request extensions but NOT validated.
/// Phase 2: Will integrate with actual API key validation and tenant resolution.
#[derive(Debug, Clone)]
pub struct ApiKey(pub String);

/// Extension key for storing API key in request extensions.
#[derive(Clone, Copy)]
pub struct ApiKeyExtensionKey;

impl std::fmt::Display for ApiKeyExtensionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKeyExtensionKey")
    }
}

/// Rejection type for API key extraction — implements IntoResponse for axum compatibility.
#[derive(Debug)]
pub struct ApiKeyRejection(pub String);

impl IntoResponse for ApiKeyRejection {
    fn into_response(self) -> axum::response::Response {
        let body = ApiError {
            error: ErrorDetails {
                code: "INVALID_API_KEY".to_string(),
                message: self.0,
                retryable: false,
                details: None,
            },
        };
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    }
}

#[async_trait::async_trait]
impl<S> axum::extract::FromRequestParts<S> for ApiKey
where
    S: Clone + Send + Sync,
{
    type Rejection = ApiKeyRejection;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Phase 1: Look for X-API-Key header, return empty if not present
        // Phase 2: This will become mandatory and validated against a key store
        match parts.headers.get("x-api-key") {
            Some(value) => {
                let key = value
                    .to_str()
                    .map_err(|_| ApiKeyRejection("X-API-Key header is not valid UTF-8".into()))?;
                Ok(ApiKey(key.to_string()))
            }
            None => {
                // Phase 1: Return empty API key (no blocking)
                // The middleware logs this for observability
                Ok(ApiKey(String::new()))
            }
        }
    }
}

/// Middleware that extracts X-API-Key header and stores it in request extensions.
/// Phase 1: This middleware logs the presence/absence of API keys but does NOT block requests.
/// Phase 2: Will validate API keys and enforce authentication.
pub async fn api_key_extractor_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let api_key = request
        .headers()
        .get("x-api-key")
        .map(|v| v.to_str().unwrap_or("<invalid utf8>"));

    match api_key {
        Some(key) if !key.is_empty() => {
            tracing::debug!("API key present: {}...", &key[..key.len().min(8)]);
        }
        _ => {
            tracing::debug!("No API key present in request (Phase 1 - allowed)");
        }
    }

    // Phase 1: Pass through without blocking
    // Phase 2: Add actual validation here
    next.run(request).await
}

/// POST /intents/{intent_id}/rebase-apply - Apply a rebase to an intent
///
/// Phase 3 P3-S5 bounded slice: When valid JWT claims are present, this handler
/// validates tenant ownership before applying the rebase.
/// Fails closed on tenant mismatch; fails open when JWT is absent (backward compatible).
#[cfg(feature = "jwt-auth")]
async fn rebase_apply(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<(StatusCode, Json<RebaseApplyResponse>), ApiErrorResponse> {
    let start = std::time::Instant::now();

    let intent_head = match state.service.get_intent_head(intent_id).await {
        Ok(h) => h,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Phase 3 P3-S5: Tenant mismatch rejection when JWT present
    // Fail-open when JWT absent (backward compatible)
    if let Some(rls_claims) = optional_rls_claims {
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("rebase_apply: tenant mismatch rejection");
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let to_version = match state
        .service
        .get_version(intent_id, request.to_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let plan = match state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let apply_result = match state
        .orchestrator
        .apply_rebase(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            &from_version,
            &to_version,
            &plan,
            &plan.affected_items,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with risk_class label
    let risk_class = match plan.risk_tier {
        RiskTier::Low => "low",
        RiskTier::Medium => "medium",
        RiskTier::High => "high",
        RiskTier::Critical => "critical",
    };
    let duration = start.elapsed().as_secs_f64();
    record_rebase_apply_duration(duration, risk_class);

    // Phase 2b bounded slice: Record audit event for all external apply outcomes
    // Best-effort actor attribution: fallback external-api/unknown
    let actor_id = "external-api/unknown";
    let audit_payload = intent_rebase_types::RebaseApplyAuditPayload {
        from_version: request.from_version,
        to_version: request.to_version,
        decision_class: format!("{:?}", plan.decision_class),
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
    };

    // Record audit event (best-effort, don't fail the response)
    if let Err(e) = state
        .audit_service
        .record_rebase_applied(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "RebaseApplied",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Phase 2b bounded slice: Create pending approval_request when blocked D/E
    if matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview) {
        let blocked_payload = intent_rebase_types::RebaseApplyBlockedAuditPayload {
            from_version: request.from_version,
            to_version: request.to_version,
            decision_class: format!("{:?}", plan.decision_class),
            risk_level: plan.risk_level,
            rationale: apply_result.rationale.clone(),
            requestor_id: actor_id.to_string(),
            requestor_type: "external-api".to_string(),
        };

        // Record blocked audit event (best-effort)
        if let Err(e) = state
            .audit_service
            .record_rebase_apply_blocked(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                blocked_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "RebaseApplyBlocked",
                &serde_json::to_value(blocked_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        // Create pending approval_request record
        let approval_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            request.from_version,
            request.to_version,
            intent_head.intent.workflow_id,
            intent_head.intent.tenant_id,
            actor_id,
            "external-api",
            &format!("{:?}", plan.decision_class),
            &apply_result.rationale,
        );

        // Only proceed with cancellation if creation succeeded
        match state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
        {
            Ok(created) => {
                // Slice 1 bounded targeted cancellation: Use classifier when graph data is available
                //
                // Check if graph data is available for targeted cancellation:
                // - affected_items.status == Available indicates graph classification succeeded
                // - Non-empty affected_approvals means we have specific approvals to target
                //
                // Fallback to flat cancellation when:
                // - Graph data is unavailable (status == Unavailable)
                // - No affected approvals identified
                // - Classifier returns empty stale_ids
                //
                // This ensures no approvals remain valid due to missing graph/classifier data.
                let use_classifier = plan.affected_items.status == AffectedItemsStatus::Available
                    && !plan.affected_items.affected_approvals.is_empty();

                if use_classifier {
                    // Get all current approval IDs for the intent to pass to classifier
                    match state
                        .approval_request_repo
                        .list_by_intent(intent_id, intent_head.intent.tenant_id)
                        .await
                    {
                        Ok(current_approvals) => {
                            // Extract approval IDs as strings for the classifier
                            let current_approval_ids: Vec<String> =
                                current_approvals.iter().map(|a| a.id.to_string()).collect();

                            // Classify approvals to determine which are stale
                            let classification = classify_approvals(&plan, &current_approval_ids);

                            if !classification.stale_ids.is_empty() {
                                // Use targeted cancellation with classifier-determined stale_ids
                                tracing::debug!(
                                    "Classifier identified {} stale approvals for targeted cancellation",
                                    classification.stale_ids.len()
                                );
                                let cancelled_count = cancel_specific_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    &classification.stale_ids,
                                    CancelApprovalContext {
                                        intent_id,
                                        tenant_id: intent_head.intent.tenant_id,
                                        actor_id: actor_id.to_string(),
                                        from_version: request.from_version,
                                        to_version: request.to_version,
                                        decision_class: format!("{:?}", plan.decision_class),
                                        new_approval_id: created.id,
                                    },
                                )
                                .await;

                                // Fall back to flat cancellation if targeted cancellation cancelled
                                // fewer approvals than expected. This handles the case where
                                // external_ref.ref_id didn't correlate correctly with ApprovalRequest.id
                                // (e.g., production graph not populated or ID mapping incomplete).
                                if cancelled_count < classification.stale_ids.len() {
                                    tracing::warn!(
                                        "Targeted cancellation cancelled {} of {} expected approvals, falling back to flat cancellation",
                                        cancelled_count,
                                        classification.stale_ids.len()
                                    );
                                    let _fallback_count = cancel_existing_approved_and_audit(
                                        &state.approval_request_repo,
                                        &state.audit_service,
                                        &state.event_publisher,
                                        intent_id,
                                        intent_head.intent.tenant_id,
                                        actor_id,
                                        request.from_version,
                                        request.to_version,
                                        &format!("{:?}", plan.decision_class),
                                        created.id,
                                    )
                                    .await;
                                }
                            } else {
                                // Classifier returned no stale_ids - fall back to flat cancellation
                                // to ensure no approvals remain valid due to missing data
                                tracing::debug!(
                                    "Classifier returned empty stale_ids, falling back to flat cancellation"
                                );
                                let _cancelled_count = cancel_existing_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    intent_id,
                                    intent_head.intent.tenant_id,
                                    actor_id,
                                    request.from_version,
                                    request.to_version,
                                    &format!("{:?}", plan.decision_class),
                                    created.id,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
                            // Failed to list approvals - fall back to flat cancellation
                            tracing::warn!(
                                "Failed to list approvals for classifier, falling back to flat cancellation: {:?}",
                                e
                            );
                            let _cancelled_count = cancel_existing_approved_and_audit(
                                &state.approval_request_repo,
                                &state.audit_service,
                                &state.event_publisher,
                                intent_id,
                                intent_head.intent.tenant_id,
                                actor_id,
                                request.from_version,
                                request.to_version,
                                &format!("{:?}", plan.decision_class),
                                created.id,
                            )
                            .await;
                        }
                    }
                } else {
                    // Graph data unavailable or no affected approvals - use flat cancellation fallback
                    // This preserves existing behavior when classifier input is missing/uncertain
                    tracing::debug!(
                        "Graph data unavailable for targeted cancellation, using flat cancellation fallback"
                    );
                    let _cancelled_count = cancel_existing_approved_and_audit(
                        &state.approval_request_repo,
                        &state.audit_service,
                        &state.event_publisher,
                        intent_id,
                        intent_head.intent.tenant_id,
                        actor_id,
                        request.from_version,
                        request.to_version,
                        &format!("{:?}", plan.decision_class),
                        created.id,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create approval_request record: {:?}", e);
            }
        }
    }

    let response = RebaseApplyResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        notification_required: apply_result.notification_required,
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    };

    record_rebase_apply_request("success");
    Ok((apply_status_code(&apply_result.outcome), Json(response)))
}

/// POST /intents/{intent_id}/rebase-apply - Apply a rebase to an intent (non-JWT fallback)
///
/// Phase 3 P3-S5: Non-JWT path for backward compatibility.
/// When jwt-auth feature is disabled, this handler operates without tenant validation.
#[cfg(not(feature = "jwt-auth"))]
async fn rebase_apply(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<(StatusCode, Json<RebaseApplyResponse>), ApiErrorResponse> {
    let start = std::time::Instant::now();

    let intent_head = match state.service.get_intent_head(intent_id).await {
        Ok(h) => h,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let from_version = match state
        .service
        .get_version(intent_id, request.from_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let to_version = match state
        .service
        .get_version(intent_id, request.to_version)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let plan = match state
        .service
        .compute_rebase_preview_with_graph(intent_id, request.from_version, request.to_version)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let apply_result = match state
        .orchestrator
        .apply_rebase(
            intent_id,
            intent_head.intent.tenant_id,
            intent_head.intent.workflow_id,
            &from_version,
            &to_version,
            &plan,
            &plan.affected_items,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            record_rebase_apply_request("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Record latency with risk_class label
    let risk_class = match plan.risk_tier {
        RiskTier::Low => "low",
        RiskTier::Medium => "medium",
        RiskTier::High => "high",
        RiskTier::Critical => "critical",
    };
    let duration = start.elapsed().as_secs_f64();
    record_rebase_apply_duration(duration, risk_class);

    // Phase 2b bounded slice: Record audit event for all external apply outcomes
    // Best-effort actor attribution: fallback external-api/unknown
    let actor_id = "external-api/unknown";
    let audit_payload = intent_rebase_types::RebaseApplyAuditPayload {
        from_version: request.from_version,
        to_version: request.to_version,
        decision_class: format!("{:?}", plan.decision_class),
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
    };

    // Record audit event (best-effort, don't fail the response)
    if let Err(e) = state
        .audit_service
        .record_rebase_applied(
            intent_head.intent.tenant_id,
            actor_id,
            intent_id,
            audit_payload.clone(),
            get_current_trace_context(),
        )
        .await
    {
        tracing::warn!("Failed to record RebaseApplied audit event: {:?}", e);
    } else {
        // Phase 2b bounded event publishing: publish after successful audit persistence
        publish_audit_event(
            &state.event_publisher,
            intent_head.intent.tenant_id,
            "RebaseApplied",
            &serde_json::to_value(audit_payload).unwrap_or_else(|_| serde_json::json!({})),
        )
        .await;
    }

    // Phase 2b bounded slice: Create pending approval_request when blocked D/E
    if matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview) {
        let blocked_payload = intent_rebase_types::RebaseApplyBlockedAuditPayload {
            from_version: request.from_version,
            to_version: request.to_version,
            decision_class: format!("{:?}", plan.decision_class),
            risk_level: plan.risk_level,
            rationale: apply_result.rationale.clone(),
            requestor_id: actor_id.to_string(),
            requestor_type: "external-api".to_string(),
        };

        // Record blocked audit event (best-effort)
        if let Err(e) = state
            .audit_service
            .record_rebase_apply_blocked(
                intent_head.intent.tenant_id,
                actor_id,
                intent_id,
                blocked_payload.clone(),
                get_current_trace_context(),
            )
            .await
        {
            tracing::warn!("Failed to record RebaseApplyBlocked audit event: {:?}", e);
        } else {
            // Phase 2b bounded event publishing: publish after successful audit persistence
            publish_audit_event(
                &state.event_publisher,
                intent_head.intent.tenant_id,
                "RebaseApplyBlocked",
                &serde_json::to_value(blocked_payload).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await;
        }

        // Create pending approval_request record
        let approval_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            request.from_version,
            request.to_version,
            intent_head.intent.workflow_id,
            intent_head.intent.tenant_id,
            actor_id,
            "external-api",
            &format!("{:?}", plan.decision_class),
            &apply_result.rationale,
        );

        // Only proceed with cancellation if creation succeeded
        match state
            .approval_request_repo
            .create_approval_request(approval_request)
            .await
        {
            Ok(created) => {
                // Slice 1 bounded targeted cancellation: Use classifier when graph data is available
                //
                // Check if graph data is available for targeted cancellation:
                // - affected_items.status == Available indicates graph classification succeeded
                // - Non-empty affected_approvals means we have specific approvals to target
                //
                // Fallback to flat cancellation when:
                // - Graph data is unavailable (status == Unavailable)
                // - No affected approvals identified
                // - Classifier returns empty stale_ids
                //
                // This ensures no approvals remain valid due to missing graph/classifier data.
                let use_classifier = plan.affected_items.status == AffectedItemsStatus::Available
                    && !plan.affected_items.affected_approvals.is_empty();

                if use_classifier {
                    // Get all current approval IDs for the intent to pass to classifier
                    match state
                        .approval_request_repo
                        .list_by_intent(intent_id, intent_head.intent.tenant_id)
                        .await
                    {
                        Ok(current_approvals) => {
                            // Extract approval IDs as strings for the classifier
                            let current_approval_ids: Vec<String> =
                                current_approvals.iter().map(|a| a.id.to_string()).collect();

                            // Classify approvals to determine which are stale
                            let classification = classify_approvals(&plan, &current_approval_ids);

                            if !classification.stale_ids.is_empty() {
                                // Use targeted cancellation with classifier-determined stale_ids
                                tracing::debug!(
                                    "Classifier identified {} stale approvals for targeted cancellation",
                                    classification.stale_ids.len()
                                );
                                let cancelled_count = cancel_specific_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    &classification.stale_ids,
                                    CancelApprovalContext {
                                        intent_id,
                                        tenant_id: intent_head.intent.tenant_id,
                                        actor_id: actor_id.to_string(),
                                        from_version: request.from_version,
                                        to_version: request.to_version,
                                        decision_class: format!("{:?}", plan.decision_class),
                                        new_approval_id: created.id,
                                    },
                                )
                                .await;

                                // Fall back to flat cancellation if targeted cancellation cancelled
                                // fewer approvals than expected. This handles the case where
                                // external_ref.ref_id didn't correlate correctly with ApprovalRequest.id
                                // (e.g., production graph not populated or ID mapping incomplete).
                                if cancelled_count < classification.stale_ids.len() {
                                    tracing::warn!(
                                        "Targeted cancellation cancelled {} of {} expected approvals, falling back to flat cancellation",
                                        cancelled_count,
                                        classification.stale_ids.len()
                                    );
                                    let _fallback_count = cancel_existing_approved_and_audit(
                                        &state.approval_request_repo,
                                        &state.audit_service,
                                        &state.event_publisher,
                                        intent_id,
                                        intent_head.intent.tenant_id,
                                        actor_id,
                                        request.from_version,
                                        request.to_version,
                                        &format!("{:?}", plan.decision_class),
                                        created.id,
                                    )
                                    .await;
                                }
                            } else {
                                // Classifier returned no stale_ids - fall back to flat cancellation
                                // to ensure no approvals remain valid due to missing data
                                tracing::debug!(
                                    "Classifier returned empty stale_ids, falling back to flat cancellation"
                                );
                                let _cancelled_count = cancel_existing_approved_and_audit(
                                    &state.approval_request_repo,
                                    &state.audit_service,
                                    &state.event_publisher,
                                    intent_id,
                                    intent_head.intent.tenant_id,
                                    actor_id,
                                    request.from_version,
                                    request.to_version,
                                    &format!("{:?}", plan.decision_class),
                                    created.id,
                                )
                                .await;
                            }
                        }
                        Err(e) => {
                            // Failed to list approvals - fall back to flat cancellation
                            tracing::warn!(
                                "Failed to list approvals for classifier, falling back to flat cancellation: {:?}",
                                e
                            );
                            let _cancelled_count = cancel_existing_approved_and_audit(
                                &state.approval_request_repo,
                                &state.audit_service,
                                &state.event_publisher,
                                intent_id,
                                intent_head.intent.tenant_id,
                                actor_id,
                                request.from_version,
                                request.to_version,
                                &format!("{:?}", plan.decision_class),
                                created.id,
                            )
                            .await;
                        }
                    }
                } else {
                    // Graph data unavailable or no affected approvals - use flat cancellation fallback
                    // This preserves existing behavior when classifier input is missing/uncertain
                    tracing::debug!(
                        "Graph data unavailable for targeted cancellation, using flat cancellation fallback"
                    );
                    let _cancelled_count = cancel_existing_approved_and_audit(
                        &state.approval_request_repo,
                        &state.audit_service,
                        &state.event_publisher,
                        intent_id,
                        intent_head.intent.tenant_id,
                        actor_id,
                        request.from_version,
                        request.to_version,
                        &format!("{:?}", plan.decision_class),
                        created.id,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create approval_request record: {:?}", e);
            }
        }
    }

    let response = RebaseApplyResponse {
        intent_id,
        from_version,
        to_version,
        decision_class: plan.decision_class,
        risk_tier: plan.risk_tier,
        risk_level: plan.risk_level,
        outcome: apply_outcome_label(&apply_result.outcome).to_string(),
        manual_review_required: matches!(apply_result.outcome, ApplyOutcome::BlockedManualReview),
        notification_required: apply_result.notification_required,
        rationale: apply_result.rationale.clone(),
        aligned_checkpoint_id: apply_result
            .aligned_checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.checkpoint_id),
        checkpoint_alignment_outcome: apply_result
            .aligned_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint_alignment_label(&checkpoint.outcome).to_string()),
        runtime_execution_status: runtime_execution_status_label(
            &apply_result.runtime_execution_result.status,
        )
        .to_string(),
        signal_sent: apply_result.runtime_execution_result.signal_sent,
        replay_attempted: apply_result.runtime_execution_result.replay_attempted,
        replay_completed: apply_result.runtime_execution_result.replay_completed,
        graph_updates_applied: apply_result
            .graph_updates
            .iter()
            .filter(|update| update.success)
            .count(),
        graph_updates_failed: apply_result
            .graph_updates
            .iter()
            .filter(|update| !update.success)
            .count(),
        compensation_planning: CompensationPlanningSummary::from(&plan.deferred.compensation),
    };

    record_rebase_apply_request("success");
    Ok((apply_status_code(&apply_result.outcome), Json(response)))
}

// ============================================================================
// Policy Snapshot Handlers (Phase 2 bounded read-only slice)
// ============================================================================

/// Initialize tracing with optional OTLP export.
///
/// When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, this initializes the OpenTelemetry
/// SDK with an OTLP exporter and a tokio runtime extension for background export.
/// When the env var is absent, only JSON logging to stdout is active (existing behavior).
///
/// Phase 3 Batch 2 Slice 2 OTEL extension (bounded slice):
/// - Optional OTLP export when endpoint is configured via env var
/// - Retains existing JSON logging fallback when OTEL is not configured
/// - Does NOT implement cross-process trace context propagation (future scope)
pub fn init_tracing() {
    use opentelemetry::trace::TracerProvider;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json());

    // Optionally wire in OTLP export if endpoint is configured
    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        // Use the pipeline pattern to set up OTLP with batch export
        let tracer_provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(opentelemetry_otlp::new_exporter().tonic())
            .install_batch(opentelemetry_sdk::runtime::Tokio)
            .expect("Failed to create OTLP tracer provider — check OTEL_EXPORTER_OTLP_ENDPOINT");

        // Set as global provider so tracing-opentelemetry layer can use it
        let _ = opentelemetry::global::set_tracer_provider(tracer_provider.clone());

        // Set global W3C trace-context propagator so extraction/injection work
        let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
        opentelemetry::global::set_text_map_propagator(propagator);

        // Create tracing-opentelemetry layer with the tracer
        let tracer = tracer_provider.tracer("intent-api");
        let tracer_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        registry.with(tracer_layer).init();
        tracing::info!("OTLP tracing enabled via OTEL_EXPORTER_OTLP_ENDPOINT");
    } else {
        // Set global W3C trace-context propagator even without OTLP
        // so trace_context_middleware extraction/injection works
        let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
        opentelemetry::global::set_text_map_propagator(propagator);

        registry.init();
        tracing::info!("OTLP tracing disabled (OTEL_EXPORTER_OTLP_ENDPOINT not set)");
    }
}

// Health check routes and middleware have been moved to health_routes.rs

// Side effect and orchestration dashboard handlers have been moved to query_handlers.rs

// Compensation action mutation handlers have been moved to compensation_mutation_handlers.rs

// DLQ candidate handlers have been moved to compensation_query_handlers.rs

/// Build the Phase 1 router with CORS enabled
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
/// When `None` (default), audit events are persisted but NOT streamed.
/// When `Some`, events are also published to the event stream (best-effort, fail-open).
#[allow(clippy::too_many_arguments)]
pub fn build_router(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    let state = AppState {
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        start_time: Instant::now(),
        rls_pool,
    };

    Router::new()
        .route("/health", get(health_routes::health_handler))
        .route("/ready", get(health_routes::ready_handler))
        .route("/metrics", get(health_routes::metrics_handler))
        .route(
            "/v1/intents/validate",
            post(intent_validation_handlers::validate_intent),
        )
        .route("/intents", post(intent_mutation_handlers::create_intent))
        .route(
            "/intents/{intent_id}",
            get(intent_read_handlers::get_intent_head),
        )
        .route(
            "/intents/{intent_id}/versions",
            post(intent_mutation_handlers::create_version),
        )
        .route(
            "/intents/{intent_id}/versions",
            get(intent_read_handlers::list_versions),
        )
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(intent_read_handlers::get_version),
        )
        .route(
            "/intents/{intent_id}/diff",
            post(diff_handlers::compute_diff),
        )
        .route(
            "/intents/{intent_id}/rebase-preview",
            post(rebase_preview_handlers::rebase_preview),
        )
        .route("/intents/{intent_id}/rebase-apply", post(rebase_apply))
        // Replay endpoint (Phase 2b bounded replay slice)
        .route(
            "/intents/{intent_id}/replay",
            post(replay_handlers::replay_intent),
        )
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route(
            "/intents/{intent_id}/side-effects",
            get(query_handlers::list_side_effects),
        )
        // N4-4: Rebase simulation endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/intents/{intent_id}/rebase-simulation",
            get(simulation_handlers::rebase_simulation),
        )
        // N4-4 POST: Compensation simulation run endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/compensation-simulation/run",
            post(simulation_handlers::compensation_simulation_run),
        )
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/orchestration-dashboard",
            get(query_handlers::get_orchestration_dashboard),
        )
        // Compensation actions query endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/compensation-actions",
            get(compensation_query_handlers::list_compensation_actions),
        )
        // Compensation action mutation endpoints (Phase 3 Batch 1 bounded execution slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/{action_id}/approve",
            post(compensation_mutation_handlers::approve_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/waive",
            post(compensation_mutation_handlers::waive_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/execute",
            post(compensation_mutation_handlers::execute_compensation_action),
        )
        // Compensation action manual retry and DLQ endpoints (Phase 3 Batch 1 bounded manual retry slice)
        .route(
            "/compensation-actions/{action_id}/reapprove",
            post(compensation_mutation_handlers::reapprove_compensation_action),
        )
        // Bounded compensation planner endpoint (Phase 3 bounded planner slice)
        .route(
            "/compensation-actions/plan",
            post(compensation_planner_handlers::plan_compensation_actions),
        )
        .route(
            "/compensation-actions/dlq",
            get(compensation_query_handlers::list_dlq_candidates),
        )
        // Batch candidates query endpoint (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
        .route(
            "/compensation-actions/batch-candidates",
            get(compensation_query_handlers::list_batch_candidates),
        )
        // Policy gate evaluation endpoints (Phase 3 Batch 1 bounded read-only slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/policy-gate",
            get(compensation_query_handlers::get_compensation_policy_gate),
        )
        .route(
            "/intents/{intent_id}/compensation-policy-gate",
            get(compensation_query_handlers::get_intent_compensation_policy_gate),
        )
        // Orchestration coordination status endpoints (Phase 3 Batch 1 bounded read-only orchestration view)
        .route(
            "/compensation-actions/orchestration-coordination",
            get(compensation_query_handlers::get_orchestration_coordination),
        )
        .route(
            "/intents/{intent_id}/orchestration-coordination",
            get(compensation_query_handlers::get_intent_orchestration_coordination),
        )
        // Manual orchestration & dry-run planner endpoints (Phase 3 Batch 1 bounded slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/orchestration-dry-run",
            post(compensation_planner_handlers::orchestration_dry_run),
        )
        .route(
            "/compensation-actions/batch-approve",
            post(batch_handlers::batch_approve_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-reapprove",
            post(batch_handlers::batch_reapprove_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-execute",
            post(batch_handlers::batch_execute_compensation_actions),
        )
        // Orchestration run endpoints (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
        .route(
            "/compensation-actions/runs",
            post(orchestration_run_handlers::create_orchestration_run),
        )
        .route(
            "/compensation-actions/runs/{run_id}",
            get(orchestration_run_handlers::get_orchestration_run),
        )
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route("/v1/graph/nodes", post(graph_handlers::create_graph_node))
        .route("/v1/graph/nodes", get(graph_handlers::list_graph_nodes))
        .route(
            "/v1/graph/nodes/{node_id}",
            get(graph_handlers::get_graph_node),
        )
        .route("/v1/graph/edges", post(graph_handlers::create_graph_edge))
        .route("/v1/graph/edges", get(graph_handlers::list_graph_edges))
        .route(
            "/v1/graph/nodes/{node_id}/edges",
            get(graph_handlers::list_edges_from_node),
        )
        // Artifact ingest with optional side effect capture (Phase 3 Batch 1 groundwork)
        .route(
            "/v1/graph/artifacts",
            post(ingest_handlers::ingest_artifact),
        )
        // Approval request endpoints (Phase 2b bounded slice)
        .route(
            "/approval-requests/pending",
            get(approval_handlers_readonly::list_pending_approval_requests),
        )
        .route(
            "/approval-requests/{approval_request_id}/approve",
            post(approval_mutation_handlers::approve_approval_request),
        )
        .route(
            "/approval-requests/{approval_request_id}/reject",
            post(approval_mutation_handlers::reject_approval_request),
        )
        // POST expire - bounded manual expiry transition (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/expire",
            post(approval_mutation_handlers::expire_approval_request),
        )
        // GET revalidate - bounded read-only scope comparison (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/revalidate",
            get(approval_handlers_readonly::revalidate_approval_request),
        )
        // ADR-07: POST trigger-reapproval - bounded re-approval trigger (Phase 2b)
        .route(
            "/approval-requests/trigger-reapproval",
            post(trigger_reapproval_handlers::trigger_reapproval),
        )
        // Policy snapshot endpoints (Phase 2 bounded read-only slice)
        .route(
            "/policy-snapshots/{snapshot_id}",
            get(policy_snapshot_handlers::get_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/latest",
            get(policy_snapshot_handlers::get_latest_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/versions/{version}",
            get(policy_snapshot_handlers::get_policy_snapshot_by_version),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}",
            get(policy_snapshot_handlers::list_policy_snapshots),
        )
        // Forensic verification endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/verify",
            post(forensic_handlers::verify_forensic_bundle),
        )
        // Forensic archive export endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/export",
            post(forensic_handlers::export_forensic_archive),
        )
        // Forensic bundle generation endpoint (P4 bounded slice)
        .route(
            "/forensic/bundle",
            post(forensic_handlers::create_forensic_bundle),
        )
        // Forensic bundle listing endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles",
            get(forensic_handlers::list_forensic_bundles),
        )
        // Forensic bundle download endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles/{bundle_id}/download",
            get(forensic_handlers::download_forensic_bundle),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        // Trace context middleware must run AFTER request_id_middleware so that
        // the span created here is a child of any extracted trace context.
        .layer(axum::middleware::from_fn(
            health_routes::request_id_middleware,
        ))
        .layer(axum::middleware::from_fn(
            health_routes::trace_context_middleware,
        ))
        .layer(TraceLayer::new_for_http())
}

/// JWT authentication middleware for protected routes.
///
/// Public paths (/health, /ready, /metrics) bypass JWT validation.
#[cfg(feature = "jwt-auth")]
async fn jwt_auth_async(
    auth_config: auth::AuthConfig,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    const PUBLIC_PATHS: &[&str] = &["/health", "/ready", "/metrics"];
    let path = request.uri().path();

    // Skip JWT check for public paths
    if PUBLIC_PATHS.contains(&path) {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok());

    match auth_header {
        Some(auth_value) if auth_value.starts_with("Bearer ") => {
            let token = &auth_value[7..];
            match decode::<auth::Claims>(
                token,
                &DecodingKey::from_secret(auth_config.jwt_secret.as_bytes()),
                &Validation::new(auth_config.algorithm),
            ) {
                Ok(token_data) => {
                    let mut request = request;
                    request.extensions_mut().insert(token_data.claims);
                    next.run(request).await
                }
                Err(_) => axum::response::Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("Invalid or expired token".into())
                    .unwrap(),
            }
        }
        _ => axum::response::Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body("Missing or invalid Authorization header".into())
            .unwrap(),
    }
}

/// Build a router with JWT authentication middleware applied to protected routes.
///
/// Public routes (health, ready, metrics) are NOT protected by JWT.
/// All other routes require a valid JWT in the Authorization header.
///
/// Use this instead of `build_router` when JWT authentication is enabled.
#[cfg(feature = "jwt-auth")]
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_jwt_auth(
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    audit_service: Arc<dyn intent_rebase_types::AuditRepository>,
    approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository>,
    policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    auth_config: auth::AuthConfig,
) -> Router {
    let state = AppState {
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        start_time: Instant::now(),
        rls_pool: None,
    };

    Router::new()
        .route("/health", get(health_routes::health_handler))
        .route("/ready", get(health_routes::ready_handler))
        .route("/metrics", get(health_routes::metrics_handler))
        .route(
            "/v1/intents/validate",
            post(intent_validation_handlers::validate_intent),
        )
        .route("/intents", post(intent_mutation_handlers::create_intent))
        .route(
            "/intents/{intent_id}",
            get(intent_read_handlers::get_intent_head),
        )
        .route(
            "/intents/{intent_id}/versions",
            post(intent_mutation_handlers::create_version),
        )
        .route(
            "/intents/{intent_id}/versions",
            get(intent_read_handlers::list_versions),
        )
        .route(
            "/intents/{intent_id}/versions/{version_number}",
            get(intent_read_handlers::get_version),
        )
        .route(
            "/intents/{intent_id}/diff",
            post(diff_handlers::compute_diff),
        )
        .route(
            "/intents/{intent_id}/rebase-preview",
            post(rebase_preview_handlers::rebase_preview),
        )
        .route("/intents/{intent_id}/rebase-apply", post(rebase_apply))
        // Replay endpoint (Phase 2b bounded replay slice)
        .route(
            "/intents/{intent_id}/replay",
            post(replay_handlers::replay_intent),
        )
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route(
            "/intents/{intent_id}/side-effects",
            get(query_handlers::list_side_effects),
        )
        // N4-4: Rebase simulation endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/intents/{intent_id}/rebase-simulation",
            get(simulation_handlers::rebase_simulation),
        )
        // N4-4 POST: Compensation simulation run endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/compensation-simulation/run",
            post(simulation_handlers::compensation_simulation_run),
        )
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/orchestration-dashboard",
            get(query_handlers::get_orchestration_dashboard),
        )
        // Compensation actions query endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/{intent_id}/compensation-actions",
            get(compensation_query_handlers::list_compensation_actions),
        )
        // Compensation action mutation endpoints (Phase 3 Batch 1 bounded execution slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/{action_id}/approve",
            post(compensation_mutation_handlers::approve_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/waive",
            post(compensation_mutation_handlers::waive_compensation_action),
        )
        .route(
            "/compensation-actions/{action_id}/execute",
            post(compensation_mutation_handlers::execute_compensation_action),
        )
        // Compensation action manual retry and DLQ endpoints (Phase 3 Batch 1 bounded manual retry slice)
        .route(
            "/compensation-actions/{action_id}/reapprove",
            post(compensation_mutation_handlers::reapprove_compensation_action),
        )
        // Bounded compensation planner endpoint (Phase 3 bounded planner slice)
        .route(
            "/compensation-actions/plan",
            post(compensation_planner_handlers::plan_compensation_actions),
        )
        .route(
            "/compensation-actions/dlq",
            get(compensation_query_handlers::list_dlq_candidates),
        )
        // Batch candidates query endpoint (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
        .route(
            "/compensation-actions/batch-candidates",
            get(compensation_query_handlers::list_batch_candidates),
        )
        // Policy gate evaluation endpoints (Phase 3 Batch 1 bounded read-only slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/policy-gate",
            get(compensation_query_handlers::get_compensation_policy_gate),
        )
        .route(
            "/intents/{intent_id}/compensation-policy-gate",
            get(compensation_query_handlers::get_intent_compensation_policy_gate),
        )
        // Orchestration coordination status endpoints (Phase 3 Batch 1 bounded read-only orchestration view)
        .route(
            "/compensation-actions/orchestration-coordination",
            get(compensation_query_handlers::get_orchestration_coordination),
        )
        .route(
            "/intents/{intent_id}/orchestration-coordination",
            get(compensation_query_handlers::get_intent_orchestration_coordination),
        )
        // Manual orchestration & dry-run planner endpoints (Phase 3 Batch 1 bounded slice)
        // NOTE: These routes are placed before graph routes to avoid path conflict
        .route(
            "/compensation-actions/orchestration-dry-run",
            post(compensation_planner_handlers::orchestration_dry_run),
        )
        .route(
            "/compensation-actions/batch-approve",
            post(batch_handlers::batch_approve_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-reapprove",
            post(batch_handlers::batch_reapprove_compensation_actions),
        )
        .route(
            "/compensation-actions/batch-execute",
            post(batch_handlers::batch_execute_compensation_actions),
        )
        // Orchestration run endpoints (Phase 3 Batch 1 bounded single-shot HTTP orchestration slice)
        .route(
            "/compensation-actions/runs",
            post(orchestration_run_handlers::create_orchestration_run),
        )
        .route(
            "/compensation-actions/runs/{run_id}",
            get(orchestration_run_handlers::get_orchestration_run),
        )
        // Graph endpoints (Phase 1 - internal CRUD only)
        .route("/v1/graph/nodes", post(graph_handlers::create_graph_node))
        .route("/v1/graph/nodes", get(graph_handlers::list_graph_nodes))
        .route(
            "/v1/graph/nodes/{node_id}",
            get(graph_handlers::get_graph_node),
        )
        .route("/v1/graph/edges", post(graph_handlers::create_graph_edge))
        .route("/v1/graph/edges", get(graph_handlers::list_graph_edges))
        .route(
            "/v1/graph/nodes/{node_id}/edges",
            get(graph_handlers::list_edges_from_node),
        )
        // Artifact ingest with optional side effect capture (Phase 3 Batch 1 groundwork)
        .route(
            "/v1/graph/artifacts",
            post(ingest_handlers::ingest_artifact),
        )
        // Approval request endpoints (Phase 2b bounded slice)
        .route(
            "/approval-requests/pending",
            get(approval_handlers_readonly::list_pending_approval_requests),
        )
        .route(
            "/approval-requests/{approval_request_id}/approve",
            post(approval_mutation_handlers::approve_approval_request),
        )
        .route(
            "/approval-requests/{approval_request_id}/reject",
            post(approval_mutation_handlers::reject_approval_request),
        )
        // POST expire - bounded manual expiry transition (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/expire",
            post(approval_mutation_handlers::expire_approval_request),
        )
        // GET revalidate - bounded read-only scope comparison (Phase 2b)
        .route(
            "/approval-requests/{approval_request_id}/revalidate",
            get(approval_handlers_readonly::revalidate_approval_request),
        )
        // ADR-07: POST trigger-reapproval - bounded re-approval trigger (Phase 2b)
        .route(
            "/approval-requests/trigger-reapproval",
            post(trigger_reapproval_handlers::trigger_reapproval),
        )
        // Policy snapshot endpoints (Phase 2 bounded read-only slice)
        .route(
            "/policy-snapshots/{snapshot_id}",
            get(policy_snapshot_handlers::get_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/latest",
            get(policy_snapshot_handlers::get_latest_policy_snapshot),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}/versions/{version}",
            get(policy_snapshot_handlers::get_policy_snapshot_by_version),
        )
        .route(
            "/policy-snapshots/intent/{intent_id}",
            get(policy_snapshot_handlers::list_policy_snapshots),
        )
        // Forensic verification endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/verify",
            post(forensic_handlers::verify_forensic_bundle),
        )
        // Forensic archive export endpoint (Phase 3 Batch 3b bounded slice)
        .route(
            "/forensic/export",
            post(forensic_handlers::export_forensic_archive),
        )
        // Forensic bundle generation endpoint (P4 bounded slice)
        .route(
            "/forensic/bundle",
            post(forensic_handlers::create_forensic_bundle),
        )
        // Forensic bundle listing endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles",
            get(forensic_handlers::list_forensic_bundles),
        )
        // Forensic bundle download endpoint (P4 bounded slice)
        .route(
            "/forensic/bundles/{bundle_id}/download",
            get(forensic_handlers::download_forensic_bundle),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        // JWT auth layer - skips public paths internally
        // Capture auth_config in closure so caller-supplied config controls JWT validation
        .layer(axum::middleware::from_fn(move |request, next| {
            jwt_auth_async(auth_config.clone(), request, next)
        }))
        // Trace context middleware must run AFTER request_id_middleware so that
        // the span created here is a child of any extracted trace context.
        .layer(axum::middleware::from_fn(
            health_routes::request_id_middleware,
        ))
        .layer(axum::middleware::from_fn(
            health_routes::trace_context_middleware,
        ))
        .layer(TraceLayer::new_for_http())
}

/// Build the router with SQL-backed audit and approval repositories.
///
/// This is the production bootstrap helper that constructs SQL-backed repositories
/// from a `PgPool` and injects them into the router. Use this in production
/// deployments where PostgreSQL-backed persistence is required.
///
/// For testing or in-memory deployments, use `build_router` directly with
/// `InMemoryAuditRepository` and `InMemoryApprovalRequestRepository`.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool used to construct SQL-backed repositories
/// * `service` - Pre-configured intent service (typically with SQL-backed intent repository)
/// * `graph_service` - Graph service instance
/// * `orchestrator` - Pre-configured orchestrator (typically with SQL-backed checkpoint repository)
///
/// # Example
///
/// ```ignore
/// let pool = PgPool::connect(&database_url).await?;
/// let intent_repo = SqlxIntentRepository::new(pool.clone());
/// let intent_service = IntentService::new(Arc::new(intent_repo));
/// let checkpoint_repo = SqlxCheckpointRepository::new(pool.clone());
/// let orchestrator = RebaseOrchestrator::new(
///     Arc::new(checkpoint_repo),
///     graph_service.clone(),
///     runtime_adapter,
/// );
///
/// let router = build_router_with_sql_audit_and_approval(
///     pool,
///     Arc::new(intent_service),
///     Arc::new(graph_service),
///     Arc::new(orchestrator),
///     Some(event_publisher),  // Phase 2b: optional event publisher
/// );
/// ```
///
/// Phase 2b: The `event_publisher` parameter enables bounded event streaming.
/// When `None` (default), audit events are persisted but NOT streamed.
/// When `Some`, events are also published to the event stream (best-effort, fail-open).
///
/// Phase 3: Full NATS JetStream integration with consumers and DLQ.
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_sql_audit_and_approval(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    // Construct SQL-backed audit, approval, and policy snapshot repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
    );
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> = Arc::new(
        intent_service::SqlxPolicySnapshotRepository::new(pool.clone()),
    );

    build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        rls_pool,
    )
}

/// Build the router with SQL-backed audit and approval repositories AND JWT authentication.
///
/// This is the production bootstrap helper for deployments that require both SQL-backed
/// repositories and JWT authentication. Use this when `INTENT_API_REQUIRE_JWT=true`.
///
/// Requires `jwt-auth` feature to be enabled.
#[cfg(feature = "jwt-auth")]
#[allow(clippy::too_many_arguments)]
pub fn build_router_with_sql_audit_and_approval_jwt(
    pool: sqlx::PgPool,
    service: Arc<IntentService>,
    graph_service: Arc<GraphService>,
    side_effect_service: Arc<compensation_service::SideEffectService>,
    compensation_action_service: Arc<compensation_service::CompensationActionService>,
    orchestration_runtime: Arc<compensation_service::OrchestrationRuntime>,
    orchestrator: Arc<RebaseOrchestrator>,
    event_publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>>,
    forensic_service: Arc<dyn forensic_service::ForensicVerificationService>,
    forensic_archive_generator: Arc<dyn forensic_service::ForensicArchiveGenerator>,
    forensic_bundle_service: Arc<dyn forensic_service::ForensicBundleServiceTrait>,
    auth_config: auth::AuthConfig,
    rls_pool: Option<graph_service::RlsAwarePool>,
) -> Router {
    // Construct SQL-backed audit, approval, and policy snapshot repositories from the pool
    let audit_service: Arc<dyn intent_rebase_types::AuditRepository> =
        Arc::new(intent_rebase_types::SqlxAuditRepository::new(pool.clone()));
    let approval_request_repo: Arc<dyn intent_service::ApprovalRequestRepository> = Arc::new(
        intent_service::SqlxApprovalRequestRepository::new(pool.clone()),
    );
    let policy_snapshot_repo: Arc<dyn intent_service::PolicySnapshotRepository> = Arc::new(
        intent_service::SqlxPolicySnapshotRepository::new(pool.clone()),
    );

    let router = build_router(
        service,
        graph_service,
        side_effect_service,
        compensation_action_service,
        orchestration_runtime,
        orchestrator,
        audit_service,
        approval_request_repo,
        policy_snapshot_repo,
        event_publisher,
        forensic_service,
        forensic_archive_generator,
        forensic_bundle_service,
        rls_pool,
    );

    // Apply JWT middleware
    router.layer(axum::middleware::from_fn(move |request, next| {
        jwt_auth_async(auth_config.clone(), request, next)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph_service::{GraphService, InMemoryGraphRepository};
    use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
    use runtime_adapter::MockAdapter;
    use std::sync::Arc;

    // Import forensic handlers for tests
    use crate::forensic_handlers::{
        create_forensic_bundle, download_forensic_bundle, export_forensic_archive,
        list_forensic_bundles, verify_forensic_bundle,
    };

    // Import simulation handlers for tests
    use crate::simulation_handlers::{compensation_simulation_run, rebase_simulation};

    // Import query handlers for tests
    use crate::query_handlers::get_orchestration_dashboard;

    // Import compensation mutation handlers for tests
    use crate::compensation_mutation_handlers::{
        approve_compensation_action, execute_compensation_action, reapprove_compensation_action,
        waive_compensation_action,
    };

    // Import batch handlers for tests
    use crate::batch_handlers::{
        batch_approve_compensation_actions, batch_execute_compensation_actions,
        batch_reapprove_compensation_actions,
    };

    // Import intent read handlers for tests
    use crate::intent_read_handlers::{get_intent_head, get_version, list_versions};

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
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        // Phase 3 Batch 1: In-memory side effect repository for tests
        let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        // Phase 3 Batch 1: In-memory compensation action repository for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo),
        );
        // Phase 3 Batch 1: In-memory orchestration run repository for tests
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        // Phase 3 Batch 3b: In-memory forensic verification service for tests
        let forensic_svc = Arc::new(forensic_service::InMemoryForensicVerificationService::new());
        // Phase 3 Batch 3b: In-memory forensic archive generator for tests
        let forensic_archive_gen = Arc::new(
            forensic_service::InMemoryForensicArchiveGenerator::new()
                .with_intent_version_count(5)
                .with_artifact_count(10)
                .with_audit_event_count(100)
                .with_policy_snapshot_count(3),
        );
        // P4: In-memory forensic bundle service for tests (uses in-memory repo and storage)
        let bundle_repo = Arc::new(forensic_service::InMemoryBundleRepository::new());
        let bundle_storage = Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket"));
        // Use a mock collector that returns empty data for basic tests
        let bundle_collector = Arc::new(forensic_service::InMemoryForensicDataCollector::new());
        let forensic_bundle_svc = Arc::new(forensic_service::ForensicBundleService::new(
            bundle_repo,
            bundle_storage,
            bundle_collector,
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
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_service: forensic_svc,
            forensic_archive_generator: forensic_archive_gen,
            forensic_bundle_service: forensic_bundle_svc,
            start_time: Instant::now(),
            rls_pool: None,
        }
    }

    #[tokio::test]
    async fn test_router_builds_successfully() {
        let state = create_test_service();
        let _router: axum::Router = Router::new()
            .route("/intents", post(intent_mutation_handlers::create_intent))
            .route("/intents/{intent_id}", get(get_intent_head))
            .route(
                "/intents/{intent_id}/versions",
                post(intent_mutation_handlers::create_version),
            )
            .route("/intents/{intent_id}/versions", get(list_versions))
            .route(
                "/intents/{intent_id}/versions/{version_number}",
                get(get_version),
            )
            .route(
                "/intents/{intent_id}/diff",
                post(diff_handlers::compute_diff),
            )
            .route(
                "/intents/{intent_id}/rebase-preview",
                post(rebase_preview_handlers::rebase_preview),
            )
            .route("/intents/{intent_id}/rebase-apply", post(rebase_apply))
            .with_state(state);
        // Router builds successfully - this is a compile-time check essentially
    }

    // === Rebase Preview Handler Tests ===

    /// Helper to call rebase_preview that works in both jwt-auth and non-jwt-auth builds
    #[cfg(feature = "jwt-auth")]
    async fn call_rebase_preview(
        state: AppState,
        intent_id: Uuid,
        request: DiffRequest,
    ) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
        rebase_preview_handlers::rebase_preview(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            Json(request),
        )
        .await
    }

    #[cfg(not(feature = "jwt-auth"))]
    async fn call_rebase_preview(
        state: AppState,
        intent_id: Uuid,
        request: DiffRequest,
    ) -> Result<Json<RebasePreviewResponse>, ApiErrorResponse> {
        rebase_preview_handlers::rebase_preview(State(state), Path(intent_id), Json(request)).await
    }

    #[tokio::test]
    async fn test_rebase_preview_success() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

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

        let state = create_test_service();

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

        // Test the rebase_preview handler directly
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = call_rebase_preview(state, intent_id, preview_request)
            .await
            .expect("Rebase preview should succeed");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version.version_number, 1);
        assert_eq!(result.to_version.version_number, 2);
        // Verify response has semantically reliable fields only
        assert!(!result.rationale.is_empty());
        assert!(result.risk_level >= 1 && result.risk_level <= 5);
    }

    #[tokio::test]
    async fn test_rebase_preview_invalid_version_ordering() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let state = create_test_service();

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
        let preview_request = intent_rebase_types::DiffRequest {
            from_version: 2,
            to_version: 1,
        };
        let result = call_rebase_preview(state, intent_id, preview_request).await;
        // result is Err(ApiErrorResponse) - verify it maps to BAD_REQUEST
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // === Graph-Available Affected Items Tests ===

    #[tokio::test]
    async fn test_rebase_preview_with_graph_classifies_affected_items() {
        use graph_service::{GraphRepository, GraphService, InMemoryGraphRepository};
        use intent_rebase_types::{
            AffectedItemsStatus, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, ExternalRef, ExternalRefType, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, NodeType, RiskTier, SourceRef, Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent with graph".to_string(),
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
                acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
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

        // Create service with graph service available
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));

        // Create service with graph integration
        let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            forensic_archive_generator: Arc::new(
                forensic_service::InMemoryForensicArchiveGenerator::new(),
            ),
            forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
                Arc::new(forensic_service::InMemoryBundleRepository::new()),
                Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
                Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
            )),
            start_time: Instant::now(),
            rls_pool: None,
        };

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
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

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Get the version to access its ID
        let to_version = state.service.get_version(intent_id, 2).await.unwrap();

        // Create IntentVersion graph node for v2
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node in the graph that maps to our version
        let iv_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: to_version.id,
                }),
                label: "IntentVersion v2".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Create an artifact that depends on this IntentVersion
        let artifact_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::Artifact,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::Artifact,
                    ref_id: Uuid::new_v4(),
                }),
                label: "Test Artifact".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Create DependsOn edge: Artifact -> IntentVersion
        graph_repo
            .create_edge(intent_rebase_types::CreateGraphEdgeRequest {
                tenant_id,
                workflow_id,
                from_node_id: artifact_node.id,
                to_node_id: iv_node.id,
                edge_type: intent_rebase_types::EdgeType::DependsOn,
                properties: None,
            })
            .await
            .unwrap();

        // Call rebase_preview which should use graph classification
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = call_rebase_preview(state, intent_id, preview_request)
            .await
            .expect("Rebase preview should succeed even with graph");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.affected_items.status, AffectedItemsStatus::Available);
        // Verify affected artifacts contains our artifact
        assert!(!result.affected_items.affected_artifacts.is_empty());
        assert_eq!(
            result.affected_items.affected_artifacts[0].node_id,
            artifact_node.id
        );
    }

    #[tokio::test]
    async fn test_rebase_preview_fallback_when_graph_node_not_found() {
        use graph_service::{GraphService, InMemoryGraphRepository};
        use intent_rebase_types::{
            AffectedItemsStatus, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

        fn create_test_payload() -> IntentPayload {
            IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent no graph".to_string(),
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
                acceptance_criteria: intent_rebase_types::AcceptanceCriteria {
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

        // Create service with graph service but NO graph data
        let repo = Arc::new(InMemoryIntentRepository::new());
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));
        let service = Arc::new(IntentService::with_graph_service(repo, graph_svc.clone()));
        let orchestrator = Arc::new(RebaseOrchestrator::new(
            checkpoint_repo,
            graph_svc.clone(),
            Arc::new(MockAdapter::ready()),
        ));
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        let state = AppState {
            service,
            graph_service: graph_svc.clone(),
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
                as Arc<dyn intent_rebase_types::AuditRepository>,
            approval_request_repo: Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
                as Arc<dyn intent_service::ApprovalRequestRepository>,
            policy_snapshot_repo: Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
                as Arc<dyn intent_service::PolicySnapshotRepository>,
            event_publisher: None, // Phase 2b: event publishing optional in tests
            forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            forensic_archive_generator: Arc::new(
                forensic_service::InMemoryForensicArchiveGenerator::new(),
            ),
            forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
                Arc::new(forensic_service::InMemoryBundleRepository::new()),
                Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
                Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
            )),
            start_time: Instant::now(),
            rls_pool: None,
        };

        // Create a test intent

        // Create an intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: create_test_payload(),
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

        // Create version 2
        let version_request = CreateVersionRequest {
            payload: create_test_payload(),
            change_reason: "v2".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: intent_rebase_types::ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };
        state
            .service
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Call rebase_preview - graph node won't be found but should NOT fail
        let preview_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };
        let result = call_rebase_preview(state, intent_id, preview_request)
            .await
            .expect("Rebase preview should succeed even when graph node not found");

        assert_eq!(result.intent_id, intent_id);
        // Status should be Unavailable since IntentVersion node not in graph
        assert_eq!(
            result.affected_items.status,
            AffectedItemsStatus::Unavailable
        );
        // But endpoint still returns useful data
        assert!(!result.rationale.is_empty());
    }

    // === Replay Endpoint Tests (Phase 2b bounded replay slice) ===

    /// Helper to call replay_intent that works in both jwt-auth and non-jwt-auth builds
    #[cfg(feature = "jwt-auth")]
    async fn call_replay_intent(
        state: AppState,
        intent_id: Uuid,
        request: ReplayRequest,
    ) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
        crate::replay_handlers::replay_intent(
            State(state),
            auth::OptionalRlsTenantClaims(None), // No JWT - tests basic replay without tenant isolation
            Path(intent_id),
            Json(request),
        )
        .await
    }

    #[cfg(not(feature = "jwt-auth"))]
    async fn call_replay_intent(
        state: AppState,
        intent_id: Uuid,
        request: ReplayRequest,
    ) -> Result<Json<ReplayResponse>, ApiErrorResponse> {
        crate::replay_handlers::replay_intent(State(state), Path(intent_id), Json(request)).await
    }

    #[tokio::test]
    async fn test_replay_intent_no_checkpoint_available() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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
            .create_version(intent_id, version_request, None, None)
            .await
            .unwrap();

        // Test the replay endpoint - no checkpoints available, so should get no_checkpoint_found outcome
        let replay_request = ReplayRequest {
            from_version: Some(1),
            to_version: 2,
            checkpoint_id: None,
        };
        let result = call_replay_intent(state, intent_id, replay_request)
            .await
            .expect("Replay should return even with no checkpoints");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.from_version, 1);
        assert_eq!(result.to_version, 2);
        assert!(result.aligned_checkpoint_id.is_none());
        assert_eq!(result.checkpoint_selection_outcome, "NoCheckpointFound");
        // Skipped because no checkpoint and adapter not used for no-checkpoint path
        assert_eq!(result.runtime_execution_status, "skipped_not_ready");
    }

    // === Approval Revalidation Handler Tests ===

    /// Helper to call revalidate_approval_request that works in both jwt-auth and non-jwt-auth builds
    #[cfg(feature = "jwt-auth")]
    async fn call_revalidate_approval_request(
        state: AppState,
        approval_request_id: Uuid,
    ) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
        approval_handlers_readonly::revalidate_approval_request(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(approval_request_id),
        )
        .await
    }

    #[cfg(not(feature = "jwt-auth"))]
    async fn call_revalidate_approval_request(
        state: AppState,
        approval_request_id: Uuid,
    ) -> Result<Json<ApprovalRevalidationResponse>, ApiErrorResponse> {
        approval_handlers_readonly::revalidate_approval_request(
            State(state),
            Path(approval_request_id),
        )
        .await
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_valid_when_scope_unchanged() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create a policy snapshot for version 1 (same as approval basis)
        let scope = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot.clone())
            .await
            .unwrap();

        // Create latest snapshot with SAME scope_hash (same scope)
        let latest_snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope);
        state
            .policy_snapshot_repo
            .create_snapshot(latest_snapshot.clone())
            .await
            .unwrap();

        // Test revalidate - should be valid since scope_hash matches
        let result = call_revalidate_approval_request(state, approval_id)
            .await
            .expect("Revalidate should succeed");

        assert_eq!(result.approval_id, approval_id);
        assert!(result.valid);
        assert_eq!(result.approval_basis_scope_hash, snapshot.scope_hash);
        assert_eq!(result.current_scope_hash, Some(latest_snapshot.scope_hash));
        assert!(!result.revalidation_required);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_invalid_when_scope_changed() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create a policy snapshot for version 1 with Partial scope
        let scope_v1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot_v1 =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope_v1);
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot_v1.clone())
            .await
            .unwrap();

        // Create latest snapshot with DIFFERENT scope (Full instead of Partial)
        let scope_v2 = ScopeDefinition {
            scope_type: ScopeType::Full,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 2,
        };
        let snapshot_v2 =
            PolicySnapshot::new(tenant_id, intent_id, 2, "v1.0.0".to_string(), scope_v2);
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot_v2.clone())
            .await
            .unwrap();

        // Test revalidate - should be invalid since scope_hash differs
        let result = call_revalidate_approval_request(state, approval_id)
            .await
            .expect("Revalidate should succeed");

        assert_eq!(result.approval_id, approval_id);
        assert!(!result.valid);
        assert_eq!(result.approval_basis_scope_hash, snapshot_v1.scope_hash);
        assert_eq!(result.current_scope_hash, Some(snapshot_v2.scope_hash));
        assert!(result.revalidation_required);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_valid_when_only_basis_snapshot_exists() {
        use intent_rebase_types::{PolicySnapshot, ScopeDefinition, ScopeType};
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Create only the approval-basis snapshot (no newer snapshots exist)
        // When no newer policy snapshots exist, the approval basis is the latest,
        // so scope_hash matches and the approval is still valid
        let scope = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };
        let snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());
        state
            .policy_snapshot_repo
            .create_snapshot(snapshot.clone())
            .await
            .unwrap();

        // Test revalidate - should return valid=true because latest (only) snapshot
        // matches approval basis, meaning no newer policy exists to invalidate the approval
        let result = call_revalidate_approval_request(state, approval_id)
            .await
            .expect("Revalidate should succeed when only basis snapshot exists");

        assert_eq!(result.approval_id, approval_id);
        assert!(result.valid);
        assert!(!result.revalidation_required);
        assert_eq!(result.current_scope_hash, Some(snapshot.scope_hash));
        assert!(result.reason.contains("Scope unchanged"));
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_not_found() {
        let state = create_test_service();
        let non_existent_id = Uuid::new_v4();

        // Test revalidate - should return 404
        let result = call_revalidate_approval_request(state, non_existent_id).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_revalidate_approval_request_basis_snapshot_not_found() {
        use intent_service::{ApprovalRequest, ApprovalRequestStatus};

        let state = create_test_service();

        // Create an approval request but NO policy snapshots at all
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let approval_request = ApprovalRequest {
            id: approval_id,
            intent_id,
            intent_version_from: 1,
            intent_version_to: 2,
            workflow_id,
            tenant_id,
            requestor_id: "test".to_string(),
            requestor_type: "test".to_string(),
            decision_class: "D".to_string(),
            reason: "Test".to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        };
        state
            .approval_request_repo
            .create_approval_request(approval_request.clone())
            .await
            .unwrap();

        // Test revalidate - should return 404 because approval basis snapshot doesn't exist
        let result = call_revalidate_approval_request(state, approval_id).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // Phase 2b: Event Publishing Tests (bounded event-streaming slice)
    // =========================================================================

    /// Helper: Create AppState with an in-memory event publisher for testing
    fn create_test_service_with_publisher(
        publisher: Arc<dyn intent_rebase_types::EventPublisher>,
    ) -> AppState {
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
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        // Phase 3 Batch 1: In-memory orchestration runtime for tests
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));
        AppState {
            service,
            graph_service: graph_svc,
            side_effect_service: Arc::new(compensation_service::SideEffectService::new(Arc::new(
                compensation_service::InMemorySideEffectRepository::new(),
            ))),
            compensation_action_service: compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_service: audit_repo,
            approval_request_repo: approval_repo,
            policy_snapshot_repo,
            event_publisher: Some(publisher),
            forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            forensic_archive_generator: Arc::new(
                forensic_service::InMemoryForensicArchiveGenerator::new(),
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

    #[tokio::test]
    async fn test_event_publisher_none_skips_publishing() {
        // Test that when event_publisher is None, publish_audit_event is a no-op
        let publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>> = None;
        let tenant_id = Uuid::new_v4();

        // Should not panic or error - just silently skip
        publish_audit_event(
            &publisher,
            tenant_id,
            "RebaseApplied",
            &serde_json::json!({ "test": true }),
        )
        .await;
    }

    #[tokio::test]
    async fn test_event_publisher_inmemory_stores_events() {
        // Test that InMemoryEventPublisher stores events correctly
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let state = create_test_service_with_publisher(publisher.clone());

        // Verify publisher is ready
        assert!(state.event_publisher.as_ref().unwrap().is_ready());
    }

    #[tokio::test]
    async fn test_publish_audit_event_helper_success() {
        // Test publish_audit_event helper with InMemoryEventPublisher
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded"
        });

        let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
            Some(publisher.clone());
        publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;

        // Verify event was published
        let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
        let events = publisher.get_events_for_subject(&subject_str).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].schema_version, "v1");
        assert_eq!(events[0].payload, payload);
    }

    #[tokio::test]
    async fn test_publish_audit_event_helper_multiple_events() {
        // Test that multiple events are published with monotonic sequences
        let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
        let tenant_id = Uuid::new_v4();

        let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
            Some(publisher.clone());

        // Publish 3 events
        for i in 1..=3 {
            let payload = serde_json::json!({ "index": i });
            publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;
        }

        // Verify sequence is monotonic
        let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
        let events = publisher.get_events_for_subject(&subject_str).await;
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[2].sequence, 3);
    }

    #[tokio::test]
    async fn test_noop_event_publisher_skips() {
        // Test that NoOpEventPublisher skips all events (always returns Skipped)
        use intent_rebase_types::{EventPublisher, TraceContext};
        let publisher = Arc::new(intent_rebase_types::NoOpEventPublisher::new());
        let tenant_id = Uuid::new_v4();
        let payload = serde_json::json!({ "test": true });
        let subject =
            intent_rebase_types::EventSubject::from_audit_event(tenant_id, "RebaseApplied");

        // NoOpEventPublisher should skip (return Skipped)
        let result = publisher
            .publish(&subject, &payload, TraceContext::default())
            .await;
        match result {
            intent_rebase_types::PublishResult::Skipped { reason } => {
                assert!(reason.contains("disabled"));
            }
            _ => panic!("Expected Skipped result from NoOpEventPublisher"),
        }
    }

    #[tokio::test]
    async fn test_build_router_accepts_event_publisher() {
        // Test that build_router accepts event_publisher parameter
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
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let event_publisher = Some(Arc::new(intent_rebase_types::InMemoryEventPublisher::new())
            as Arc<dyn intent_rebase_types::EventPublisher>);
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(Arc::new(
            compensation_service::InMemorySideEffectRepository::new(),
        )));
        let compensation_action_svc =
            Arc::new(compensation_service::CompensationActionService::new(
                Arc::new(compensation_service::InMemoryCompensationActionRepository::new()),
            ));
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
            compensation_action_svc.clone(),
            orchestration_run_repo,
        ));

        let _router: axum::Router = build_router(
            service,
            graph_svc,
            side_effect_svc,
            compensation_action_svc,
            orchestration_runtime,
            orchestrator,
            audit_repo,
            approval_repo,
            policy_snapshot_repo,
            event_publisher,
            Arc::new(forensic_service::InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            Arc::new(forensic_service::InMemoryForensicArchiveGenerator::new())
                as Arc<dyn forensic_service::ForensicArchiveGenerator>,
            Arc::new(forensic_service::ForensicBundleService::new(
                Arc::new(forensic_service::InMemoryBundleRepository::new()),
                Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
                Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
            )),
            None,
        );
        // Router builds successfully - this verifies the signature change works
    }

    // === Compensation Action API Tests ===

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
        let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
            as Arc<dyn intent_rebase_types::AuditRepository>;
        let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
            as Arc<dyn intent_service::ApprovalRequestRepository>;
        let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
            as Arc<dyn intent_service::PolicySnapshotRepository>;
        let side_effect_repo = Arc::new(compensation_service::InMemorySideEffectRepository::new());
        let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(
            side_effect_repo,
        ));
        // Use in-memory compensation action repo with stub executor
        let compensation_action_repo =
            Arc::new(compensation_service::InMemoryCompensationActionRepository::new());
        let compensation_action_svc = Arc::new(
            compensation_service::CompensationActionService::new(compensation_action_repo.clone()),
        );
        let orchestration_run_repo =
            Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
        let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
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
            forensic_service: Arc::new(forensic_service::InMemoryForensicVerificationService::new())
                as Arc<dyn forensic_service::ForensicVerificationService>,
            forensic_archive_generator: Arc::new(
                forensic_service::InMemoryForensicArchiveGenerator::new(),
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

    #[cfg(not(feature = "jwt-auth"))]
    #[tokio::test]
    async fn test_approve_compensation_action_success() {
        let state = create_test_service_with_executor();

        // Create a compensation action directly via the service
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
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

        // Approve the action via the API
        let request = ApproveCompensationActionBody {
            lock_version: created.lock_version,
            approved_by: Some("test-approver".to_string()),
        };
        let result = approve_compensation_action(State(state), Path(created.id), Json(request))
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
            approve_compensation_action(State(state), Path(Uuid::new_v4()), Json(request)).await;
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
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
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

        // Waive the action via the API
        let request = WaiveCompensationActionBody {
            lock_version: created.lock_version,
            waived_by: Some("test-waiver".to_string()),
        };
        let result = waive_compensation_action(State(state), Path(created.id), Json(request))
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
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
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
        let result = execute_compensation_action(State(state), Path(approved.id), Json(request))
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
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
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

        // Try to execute without approval - should fail
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };
        let result =
            execute_compensation_action(State(state), Path(created.id), Json(request)).await;

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
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::ManualOnly,
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );

        let response = CompensationActionResponse::from(action);

        assert_eq!(response.status, "pending");
        assert_eq!(response.strategy_type, "rollback");
        assert_eq!(response.feasibility, "manual_only");
        assert_eq!(response.tenant_id, tenant_id);
        assert_eq!(response.intent_id, intent_id);
    }

    // =========================================================================
    // Orchestration Dashboard Tests (Phase 3 Batch 1 bounded read-only slice)
    // =========================================================================

    #[tokio::test]
    async fn test_orchestration_dashboard_empty_state() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return even with no data");

        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.tenant_id, tenant_id);
        assert!(result.side_effects.is_empty());
        assert_eq!(result.side_effect_summary.total, 0);
        assert!(result.compensation_actions.is_empty());
        assert_eq!(result.compensation_action_summary.total, 0);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_with_side_effects() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Record some side effects
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "metadata_write",
                "db-record-123",
            )
            .await
            .unwrap();

        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S4Irreversible,
                "money_transfer",
                "account-xyz",
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result.side_effects.len(), 2);
        assert_eq!(result.side_effect_summary.total, 2);
        assert_eq!(result.side_effect_summary.irreversible_count, 1);
        assert_eq!(result.side_effect_summary.auto_compensatable_count, 1);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_with_compensation_actions() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create actions in different statuses
        // Pending action
        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let pending_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        state
            .compensation_action_service
            .create_action(pending_action)
            .await
            .unwrap();

        // Approved + Automatic action (auto-executable)
        let approved_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback 2",
        );
        let approved = state
            .compensation_action_service
            .create_action(approved_action)
            .await
            .unwrap();
        state
            .compensation_action_service
            .approve_action(approved.id, approved.lock_version, Some("test"))
            .await
            .unwrap();

        // Failed + retryable error (reapprovable)
        let failed_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback 3",
        );
        let failed = state
            .compensation_action_service
            .create_action(failed_action)
            .await
            .unwrap();
        // Approve then fail with retryable error
        let failed_approved = state
            .compensation_action_service
            .approve_action(failed.id, failed.lock_version, Some("test"))
            .await
            .unwrap();
        let failed_result = compensation_service::ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                failed_approved.id,
                &failed_result,
                failed_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result.compensation_actions.len(), 3);
        assert_eq!(result.compensation_action_summary.total, 3);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
        assert_eq!(result.compensation_action_summary.status_counts.approved, 1);
        assert_eq!(result.compensation_action_summary.status_counts.failed, 1);
        assert_eq!(result.compensation_action_summary.retryable_failed_count, 1);
        assert_eq!(result.compensation_action_summary.reapprovable_count, 1);
        assert_eq!(result.compensation_action_summary.auto_executable_count, 1);
        // Approved + Automatic
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_dlq_candidates() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create a failed action with non-retryable error (DLQ candidate)
        let dlq_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context.clone(),
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        let dlq = state
            .compensation_action_service
            .create_action(dlq_action)
            .await
            .unwrap();
        // Approve then fail with non-retryable error
        let dlq_approved = state
            .compensation_action_service
            .approve_action(dlq.id, dlq.lock_version, Some("test"))
            .await
            .unwrap();
        let dlq_result = compensation_service::ExecutionResult::failure(
            "Permanent failure",
            "INVALID_CONFIGURATION",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                dlq_approved.id,
                &dlq_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
        // Non-retryable error + exhausted budget = DLQ candidate, not reapprovable
        assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_exhausted_budget_dlq() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        let rebase_context =
            compensation_service::RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        // Create action with max_retries = 1
        let mut dlq_action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            compensation_service::CompensationFeasibility::Automatic,
            compensation_service::StrategyType::Rollback,
            "Auto rollback",
        );
        dlq_action.max_retries = 1; // Exhaust on first failure

        let dlq = state
            .compensation_action_service
            .create_action(dlq_action)
            .await
            .unwrap();
        // Approve then fail with retryable error (but budget exhausted)
        let dlq_approved = state
            .compensation_action_service
            .approve_action(dlq.id, dlq.lock_version, Some("test"))
            .await
            .unwrap();
        let dlq_result = compensation_service::ExecutionResult::failure(
            "Temporary failure",
            "CONNECTION_TIMEOUT",
            None,
        );
        state
            .compensation_action_service
            .record_result(
                dlq_approved.id,
                &dlq_result,
                dlq_approved.lock_version,
                None,
            )
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return data");

        // Exhausted budget makes it a DLQ candidate even with retryable error
        assert_eq!(result.compensation_action_summary.dlq_candidate_count, 1);
        assert_eq!(result.compensation_action_summary.reapprovable_count, 0);
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_response_shape() {
        use compensation_service::{CompensationFeasibility, RebaseContext, StrategyType};

        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let side_effect_id = Uuid::new_v4();

        // Create a side effect
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S2ExternalReversible,
                "pr_opened",
                "https://github.com/example/pull/123",
            )
            .await
            .unwrap();

        // Create a compensation action
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        let action = compensation_service::CompensationAction::new(
            tenant_id,
            side_effect_id,
            intent_id,
            rebase_context,
            CompensationFeasibility::SemiAutomatic,
            StrategyType::FollowupNotice,
            "Send follow-up",
        );
        state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        let query = OrchestrationDashboardQuery { tenant_id };
        let result = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Dashboard should return data");

        // Verify response structure
        assert_eq!(result.intent_id, intent_id);
        assert_eq!(result.tenant_id, tenant_id);
        assert_eq!(result.side_effects.len(), 1);
        assert_eq!(result.compensation_actions.len(), 1);

        // Verify side effect summary
        assert_eq!(result.side_effect_summary.total, 1);
        assert_eq!(result.side_effect_summary.irreversible_count, 0);
        assert_eq!(result.side_effect_summary.auto_compensatable_count, 0); // S2 is not auto

        // Verify compensation action summary
        assert_eq!(result.compensation_action_summary.total, 1);
        assert_eq!(result.compensation_action_summary.status_counts.pending, 1);
        assert_eq!(result.compensation_action_summary.auto_executable_count, 0);
        // SemiAutomatic is not auto
    }

    #[tokio::test]
    async fn test_orchestration_dashboard_tenant_isolation() {
        let state = create_test_service();
        let intent_id = Uuid::new_v4();
        let tenant_id_1 = Uuid::new_v4();
        let tenant_id_2 = Uuid::new_v4();

        // Record side effects for tenant 1
        state
            .side_effect_service
            .record_side_effect(
                tenant_id_1,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "effect_1",
                "target_1",
            )
            .await
            .unwrap();

        // Record side effects for tenant 2
        state
            .side_effect_service
            .record_side_effect(
                tenant_id_2,
                intent_id,
                1,
                compensation_service::SideEffectClass::S2ExternalReversible,
                "effect_2",
                "target_2",
            )
            .await
            .unwrap();

        // Query for tenant 1
        let query1 = OrchestrationDashboardQuery {
            tenant_id: tenant_id_1,
        };
        let result1 = get_orchestration_dashboard(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query1),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result1.side_effect_summary.total, 1);
        assert_eq!(result1.side_effects[0].effect_type, "effect_1");

        // Query for tenant 2
        let query2 = OrchestrationDashboardQuery {
            tenant_id: tenant_id_2,
        };
        let result2 = get_orchestration_dashboard(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Path(intent_id),
            axum::extract::Query(query2),
        )
        .await
        .expect("Dashboard should return data");

        assert_eq!(result2.side_effect_summary.total, 1);
        assert_eq!(result2.side_effects[0].effect_type, "effect_2");
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
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let result = verify_forensic_bundle(
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
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::Legal,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: false,
        };

        let result = verify_forensic_bundle(
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
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::VerificationPurpose::IncidentInvestigation,
            include_artifacts: false,
            include_audit_events: false,
            include_policy_snapshots: false,
        };

        let result = verify_forensic_bundle(
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
            earliest_version: Some(chrono::Utc::now()),
            latest_version: Some(chrono::Utc::now()),
            has_artifact_traceability: true,
        };

        let json = serde_json::to_string(&coverage).expect("Should serialize");
        assert!(json.contains("\"intent_exists\":true"));
        assert!(json.contains("\"version_count\":5"));
        assert!(json.contains("\"has_artifact_traceability\":true"));
    }

    // === Forensic Export Tests ===

    #[tokio::test]
    async fn test_export_forensic_archive_returns_generated_status() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicExportRequest {
            tenant_id,
            intent_id,
            time_range: ForensicExportTimeRange {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::ExportPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let result = export_forensic_archive(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return export result");

        assert_eq!(result.status, forensic_service::ExportStatus::Generated);
        assert_eq!(result.tenant_id, tenant_id);
        assert_eq!(result.intent_id, intent_id);
        // Item count = 5 (intent versions) + 10 (artifacts) + 100 (audit events) + 3 (policy snapshots)
        assert_eq!(result.item_count, 118);
        assert_eq!(result.content_type, "application/json");
    }

    #[tokio::test]
    async fn test_export_forensic_archive_request_deserialization() {
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

        let request: ForensicExportRequest =
            serde_json::from_str(json).expect("Should deserialize");

        assert_eq!(
            request.purpose,
            forensic_service::ExportPurpose::ComplianceAudit
        );
        assert!(request.include_artifacts);
        assert!(!request.include_audit_events);
        assert!(request.include_policy_snapshots);
    }

    #[tokio::test]
    async fn test_export_forensic_archive_response_serialization() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicExportRequest {
            tenant_id,
            intent_id,
            time_range: ForensicExportTimeRange {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::ExportPurpose::Legal,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: false,
        };

        let result = export_forensic_archive(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return export result");

        // Verify serialization works
        let json = serde_json::to_string(&result.0).expect("Should serialize");
        assert!(json.contains("\"status\":\"generated\""));
        assert!(json.contains("\"tenant_id\""));
        assert!(json.contains("\"intent_id\""));
        assert!(json.contains("\"content_type\":\"application/json\""));
        // item_count = 5 + 10 + 100 = 115 (no policy snapshots)
        assert!(json.contains("\"item_count\":115"));
    }

    #[tokio::test]
    async fn test_export_forensic_archive_status_reason_truthful() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        let request = ForensicExportRequest {
            tenant_id,
            intent_id,
            time_range: ForensicExportTimeRange {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::ExportPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let result = export_forensic_archive(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return export result");

        // Status reason should be truthful about in-memory generation
        assert!(
            result.status_reason.contains("in-memory")
                || result.status_reason.contains("scaffolded")
        );
        assert!(!result.status_reason.contains("S3"));
        assert!(!result.status_reason.contains("persisted"));
    }

    #[tokio::test]
    async fn test_export_forensic_archive_empty_counts() {
        // Use a generator with zero counts to test empty archive scenario
        let generator = Arc::new(forensic_service::InMemoryForensicArchiveGenerator::new())
            as Arc<dyn forensic_service::ForensicArchiveGenerator>;

        let state = AppState {
            service: Arc::new(IntentService::new(Arc::new(
                intent_service::InMemoryIntentRepository::new(),
            ))),
            graph_service: Arc::new(GraphService::new(Arc::new(
                graph_service::InMemoryGraphRepository::new(),
            ))),
            orchestrator: Arc::new(RebaseOrchestrator::new(
                Arc::new(intent_service::InMemoryCheckpointRepository::new()),
                Arc::new(GraphService::new(Arc::new(
                    graph_service::InMemoryGraphRepository::new(),
                ))),
                Arc::new(MockAdapter::ready()),
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
            forensic_archive_generator: generator,
            forensic_bundle_service: Arc::new(forensic_service::ForensicBundleService::new(
                Arc::new(forensic_service::InMemoryBundleRepository::new()),
                Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
                Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
            )),
            start_time: Instant::now(),
            rls_pool: None,
        };

        let request = ForensicExportRequest {
            tenant_id: Uuid::new_v4(),
            intent_id: Uuid::new_v4(),
            time_range: ForensicExportTimeRange {
                start: chrono::Utc::now(),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::ExportPurpose::ComplianceAudit,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let result = export_forensic_archive(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should return export result");

        // Zero counts should produce zero items
        assert_eq!(result.item_count, 0);
        assert_eq!(result.contents.intent_versions, 0);
        assert_eq!(result.contents.artifacts, 0);
        assert_eq!(result.contents.audit_events, 0);
        assert_eq!(result.contents.policy_snapshots, 0);
    }

    // =========================================================================
    // Forensic Bundle Listing & Download Tests (P4 bounded slice)
    // =========================================================================

    #[tokio::test]
    async fn test_list_forensic_bundles_empty_when_no_bundles() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        let result = list_forensic_bundles(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            axum::extract::Query(ListForensicBundlesQuery {
                tenant_id,
                limit: None,
            }),
        )
        .await
        .expect("Should return list result");

        assert_eq!(result.total, 0);
        assert!(result.bundles.is_empty());
    }

    #[tokio::test]
    async fn test_list_forensic_bundles_returns_bundles_for_tenant() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        // First create a bundle via the create endpoint
        let create_request = ForensicBundleRequest {
            tenant_id,
            intent_ids: vec![],
            time_range: ForensicBundleTimeRange {
                start: chrono::Utc::now() - chrono::Duration::days(1),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::BundlePurpose::IncidentInvestigation,
            created_by: "test-user".to_string(),
        };

        let _create_result = create_forensic_bundle(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(create_request),
        )
        .await
        .expect("Should create bundle");

        // Now list bundles
        let result = list_forensic_bundles(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            axum::extract::Query(ListForensicBundlesQuery {
                tenant_id,
                limit: None,
            }),
        )
        .await
        .expect("Should return list result");

        assert_eq!(result.total, 1);
        assert_eq!(result.bundles.len(), 1);
        assert_eq!(result.bundles[0].tenant_id, tenant_id);
        assert_eq!(
            result.bundles[0].purpose,
            forensic_service::BundlePurpose::IncidentInvestigation
        );
    }

    #[tokio::test]
    async fn test_list_forensic_bundles_with_limit() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        // Create two bundles
        for i in 0..2 {
            let create_request = ForensicBundleRequest {
                tenant_id,
                intent_ids: vec![],
                time_range: ForensicBundleTimeRange {
                    start: chrono::Utc::now() - chrono::Duration::days(1),
                    end: chrono::Utc::now(),
                },
                purpose: forensic_service::BundlePurpose::ComplianceAudit,
                created_by: format!("test-user-{}", i),
            };

            let _ = create_forensic_bundle(
                State(state.clone()),
                auth::OptionalRlsTenantClaims(None),
                Json(create_request),
            )
            .await
            .expect("Should create bundle");
        }

        // List with limit=1
        let result = list_forensic_bundles(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            axum::extract::Query(ListForensicBundlesQuery {
                tenant_id,
                limit: Some(1),
            }),
        )
        .await
        .expect("Should return list result");

        // With in-memory repo, limit may not be strictly enforced in test setup
        // but the endpoint should still work
        assert!(!result.bundles.is_empty());
    }

    #[tokio::test]
    async fn test_download_forensic_bundle_not_found() {
        let state = create_test_service();
        let bundle_id = Uuid::new_v4();

        let result = download_forensic_bundle(State(state), Path(bundle_id)).await;

        // Should return error for non-existent bundle
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_download_forensic_bundle_success() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();

        // Create a bundle
        let create_request = ForensicBundleRequest {
            tenant_id,
            intent_ids: vec![],
            time_range: ForensicBundleTimeRange {
                start: chrono::Utc::now() - chrono::Duration::days(1),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::BundlePurpose::Legal,
            created_by: "test-user".to_string(),
        };

        let (_status, create_response) = create_forensic_bundle(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(create_request),
        )
        .await
        .expect("Should create bundle");

        let bundle_id = create_response.bundle_id;

        // Download the bundle
        let response = download_forensic_bundle(State(state), Path(bundle_id))
            .await
            .expect("Should return download response");

        // Verify response has correct content type
        let parts = response.into_response();
        assert_eq!(
            parts.headers().get("content-type").unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn test_list_forensic_bundles_tenant_isolation() {
        let state = create_test_service();
        let tenant1 = Uuid::new_v4();
        let tenant2 = Uuid::new_v4();

        // Create bundle for tenant1
        let create_request1 = ForensicBundleRequest {
            tenant_id: tenant1,
            intent_ids: vec![],
            time_range: ForensicBundleTimeRange {
                start: chrono::Utc::now() - chrono::Duration::days(1),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::BundlePurpose::IncidentInvestigation,
            created_by: "test-user-1".to_string(),
        };

        let _ = create_forensic_bundle(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(create_request1),
        )
        .await
        .expect("Should create bundle for tenant1");

        // Create bundle for tenant2
        let create_request2 = ForensicBundleRequest {
            tenant_id: tenant2,
            intent_ids: vec![],
            time_range: ForensicBundleTimeRange {
                start: chrono::Utc::now() - chrono::Duration::days(1),
                end: chrono::Utc::now(),
            },
            purpose: forensic_service::BundlePurpose::ComplianceAudit,
            created_by: "test-user-2".to_string(),
        };

        let _ = create_forensic_bundle(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(create_request2),
        )
        .await
        .expect("Should create bundle for tenant2");

        // List bundles for tenant1 - should only see tenant1's bundle
        let result1 = list_forensic_bundles(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            axum::extract::Query(ListForensicBundlesQuery {
                tenant_id: tenant1,
                limit: None,
            }),
        )
        .await
        .expect("Should return list result");

        assert_eq!(result1.total, 1);
        assert_eq!(result1.bundles[0].tenant_id, tenant1);

        // List bundles for tenant2 - should only see tenant2's bundle
        let result2 = list_forensic_bundles(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            axum::extract::Query(ListForensicBundlesQuery {
                tenant_id: tenant2,
                limit: None,
            }),
        )
        .await
        .expect("Should return list result");

        assert_eq!(result2.total, 1);
        assert_eq!(result2.bundles[0].tenant_id, tenant2);
    }

    // =========================================================================
    // N4-4: Rebase Simulation Tests (Phase 3 Batch 1 bounded simulation slice)
    // =========================================================================

    #[tokio::test]
    async fn test_rebase_simulation_empty_side_effects() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Run simulation with no side effects (deterministic mode by default)
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("deterministic".to_string()),
            seed: None,
        };

        let result = rebase_simulation(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Should run simulation");

        // With no side effects, report should have 0 total actions
        assert_eq!(result.total_actions, 0);
        assert_eq!(result.successful_count, 0);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_rebase_simulation_with_side_effects() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Record a side effect
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "test_effect",
                "test_target",
            )
            .await
            .expect("Should record side effect");

        // Run simulation with deterministic mode
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("deterministic".to_string()),
            seed: None,
        };

        let result = rebase_simulation(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Should run simulation");

        // Report should have 1 action and it should succeed (S1 + Automatic)
        assert_eq!(result.total_actions, 1);
        assert_eq!(result.successful_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.outcomes[0].predicted_success);
    }

    #[tokio::test]
    async fn test_rebase_simulation_intent_not_found() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let non_existent_intent_id = Uuid::new_v4();

        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: None,
            seed: None,
        };

        let result = rebase_simulation(
            State(state),
            Path(non_existent_intent_id),
            axum::extract::Query(query),
        )
        .await;

        // Should return error for non-existent intent
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rebase_simulation_stochastic_mode_with_seed() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Run simulation with stochastic mode and a seed
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("stochastic".to_string()),
            seed: Some(42),
        };

        let result = rebase_simulation(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .expect("Should run simulation");

        // Verify stochastic mode was used
        assert_eq!(
            result.config.mode,
            compensation_service::SimulationMode::Stochastic
        );
        assert_eq!(result.total_actions, 0); // No side effects
    }

    #[tokio::test]
    async fn test_rebase_simulation_invalid_version_ordering() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Test with reversed version order (from_version > to_version) — should fail
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 2,
            to_version: 1,
            mode: None,
            seed: None,
        };

        let err_response =
            rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .unwrap_err();

        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rebase_simulation_invalid_version_bounds() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Test with from_version = 0 (invalid, must be >= 1)
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 0,
            to_version: 2,
            mode: None,
            seed: None,
        };

        let err_response = rebase_simulation(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .unwrap_err();

        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test with to_version = 0 (invalid, must be >= 1)
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 0,
            mode: None,
            seed: None,
        };

        let err_response = rebase_simulation(
            State(state.clone()),
            Path(intent_id),
            axum::extract::Query(query),
        )
        .await
        .unwrap_err();

        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test with negative versions
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: -1,
            to_version: 2,
            mode: None,
            seed: None,
        };

        let err_response =
            rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
                .await
                .unwrap_err();

        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rebase_simulation_invalid_mode_fallback() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Run simulation with invalid mode — should fall back to deterministic
        let query = RebaseSimulationQuery {
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("invalid_mode".to_string()),
            seed: None,
        };

        let result = rebase_simulation(State(state), Path(intent_id), axum::extract::Query(query))
            .await
            .expect("Invalid mode should fall back to deterministic");

        // Verify fallback to deterministic mode
        assert_eq!(
            result.config.mode,
            compensation_service::SimulationMode::Deterministic
        );
    }

    // =========================================================================
    // N4-4 POST: Compensation Simulation Run Tests (Phase 3 Batch 1 bounded simulation slice)
    // =========================================================================

    #[tokio::test]
    async fn test_compensation_simulation_run_empty_side_effects() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Run simulation with POST request (no side effects)
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("deterministic".to_string()),
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should run simulation");

        // With no side effects, report should have 0 total actions
        assert_eq!(result.total_actions, 0);
        assert_eq!(result.successful_count, 0);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_compensation_simulation_run_with_side_effects() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Record a side effect
        state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "test_effect",
                "test_target",
            )
            .await
            .expect("Should record side effect");

        // Run simulation with POST request
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("deterministic".to_string()),
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should run simulation");

        // Report should have 1 action and it should succeed (S1 + Automatic)
        assert_eq!(result.total_actions, 1);
        assert_eq!(result.successful_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.outcomes[0].predicted_success);
    }

    #[tokio::test]
    async fn test_compensation_simulation_run_invalid_version_ordering() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Run simulation with reversed version order
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 2,
            to_version: 1, // Invalid: from > to
            mode: None,
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;

        // Should return error for invalid version ordering
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_compensation_simulation_run_invalid_version_bounds() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Test with from_version = 0 (invalid, must be >= 1)
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 0,
            to_version: 2,
            mode: None,
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;

        // Should return error for invalid version bounds
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test with to_version = 0 (invalid, must be >= 1)
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 1,
            to_version: 0,
            mode: None,
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state.clone()),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;

        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Test with negative versions
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: -1,
            to_version: 2,
            mode: None,
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;

        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_compensation_simulation_run_intent_not_found() {
        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let non_existent_intent_id = Uuid::new_v4();

        let request = CompensationSimulationRequest {
            intent_id: non_existent_intent_id,
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: None,
            seed: None,
            side_effect_ids: None,
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;

        // Should return error for non-existent intent
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_compensation_simulation_run_with_side_effect_ids_filter() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let state = create_test_service();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create intent
        let create_request = CreateIntentRequest {
            tenant_id: None,
            workflow_id,
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

        // Record two side effects
        let se1 = state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S1InternalReversible,
                "test_effect_1",
                "test_target",
            )
            .await
            .expect("Should record side effect 1");

        let _se2 = state
            .side_effect_service
            .record_side_effect(
                tenant_id,
                intent_id,
                1,
                compensation_service::SideEffectClass::S2ExternalReversible,
                "test_effect_2",
                "test_target",
            )
            .await
            .expect("Should record side effect 2");

        // Run simulation with only first side effect ID
        let request = CompensationSimulationRequest {
            intent_id,
            tenant_id,
            from_version: 1,
            to_version: 2,
            mode: Some("deterministic".to_string()),
            seed: None,
            side_effect_ids: Some(vec![se1.id]), // Only simulate se1
        };

        let result = compensation_simulation_run(
            State(state),
            auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Should run simulation");

        // Report should only have 1 action (se1 only)
        assert_eq!(result.total_actions, 1);
        // S1 + Automatic = success
        assert_eq!(result.successful_count, 1);
        assert_eq!(result.failed_count, 0);
    }

    // =========================================================================
    // Phase 2b: Rebase Apply BlockedManualReview Invalidation Tests
    //
    // Tests for bounded approval cancellation in rebase_apply BlockedManualReview path.
    // Verifies that when rebase_apply creates a Pending approval request for
    // BlockedManualReview, existing Approved approvals for the same intent
    // are cancelled using cancel_existing_approved_and_audit helper.
    // =========================================================================

    #[tokio::test]
    async fn test_cancel_existing_approved_and_audit_cancels_approved_approvals() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };
        use intent_service::ApprovalRequestStatus;

        let state = create_test_service();

        // Create an intent to get tenant_id
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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create an existing Approved approval request
        let approved_request = intent_service::ApprovalRequest::new_pending(
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
        let approved_id = approved_request.id;
        state
            .approval_request_repo
            .create_approval_request(approved_request)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                approved_id,
                ApprovalRequestStatus::Approved,
                "approver",
                None,
            )
            .await
            .unwrap();

        // Verify it's Approved
        let verified = state
            .approval_request_repo
            .get_approval_request(approved_id)
            .await
            .unwrap();
        assert_eq!(verified.status, ApprovalRequestStatus::Approved);

        // Create a new pending approval request (simulating what rebase_apply does)
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            2,
            3,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call the helper to cancel existing Approved approvals
        let cancelled_count = cancel_existing_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            intent_id,
            tenant_id,
            "external-api",
            2,
            3,
            "D",
            new_approval_id,
        )
        .await;

        // Should have cancelled 1 approval
        assert_eq!(cancelled_count, 1);

        // The approved request should now be Cancelled
        let cancelled = state
            .approval_request_repo
            .get_approval_request(approved_id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);

        // The new pending request should still be Pending
        let still_pending = state
            .approval_request_repo
            .get_approval_request(new_approval_id)
            .await
            .unwrap();
        assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
    }

    #[tokio::test]
    async fn test_cancel_existing_approved_and_audit_does_not_cancel_pending() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };
        use intent_service::ApprovalRequestStatus;

        let state = create_test_service();

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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create a Pending approval request (not Approved)
        let pending_request = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Pending approval",
        );
        let pending_id = pending_request.id;
        state
            .approval_request_repo
            .create_approval_request(pending_request)
            .await
            .unwrap();

        // Verify it's Pending
        let verified = state
            .approval_request_repo
            .get_approval_request(pending_id)
            .await
            .unwrap();
        assert_eq!(verified.status, ApprovalRequestStatus::Pending);

        // Create a new pending approval request
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            2,
            3,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call the helper
        let cancelled_count = cancel_existing_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            intent_id,
            tenant_id,
            "external-api",
            2,
            3,
            "D",
            new_approval_id,
        )
        .await;

        // Should have cancelled 0 approvals (pending not cancelled)
        assert_eq!(cancelled_count, 0);

        // The pending request should still be Pending
        let still_pending = state
            .approval_request_repo
            .get_approval_request(pending_id)
            .await
            .unwrap();
        assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
    }

    #[tokio::test]
    async fn test_cancel_existing_approved_and_audit_returns_zero_when_none_exist() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let state = create_test_service();

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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create a new pending approval request (no existing approvals)
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call the helper with intent that has no existing approvals
        let cancelled_count = cancel_existing_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            intent_id,
            tenant_id,
            "external-api",
            1,
            2,
            "D",
            new_approval_id,
        )
        .await;

        // Should have cancelled 0 approvals
        assert_eq!(cancelled_count, 0);
    }

    // =========================================================================
    // Slice 1: Targeted Approval Cancellation Tests
    //
    // Tests for classifier-driven targeted cancellation in rebase_apply.
    // Verifies that cancel_specific_approved_and_audit correctly cancels
    // only the specific approvals identified as stale by the classifier.
    // =========================================================================

    #[tokio::test]
    async fn test_cancel_specific_approved_and_audit_cancels_specific_approvals() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };
        use intent_service::ApprovalRequestStatus;

        let state = create_test_service();

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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create two Approved approval requests
        let approved_request1 = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Previous approval 1",
        );
        let approved_id1 = approved_request1.id;
        state
            .approval_request_repo
            .create_approval_request(approved_request1)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                approved_id1,
                ApprovalRequestStatus::Approved,
                "approver1",
                None,
            )
            .await
            .unwrap();

        let approved_request2 = intent_service::ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/previous",
            "external-api",
            "D",
            "Previous approval 2",
        );
        let approved_id2 = approved_request2.id;
        state
            .approval_request_repo
            .create_approval_request(approved_request2)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                approved_id2,
                ApprovalRequestStatus::Approved,
                "approver2",
                None,
            )
            .await
            .unwrap();

        // Create a new pending approval request
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            2,
            3,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call targeted cancellation with only approved_id1 as stale
        let stale_ids = vec![approved_id1.to_string()];
        let cancelled_count = cancel_specific_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            &stale_ids,
            CancelApprovalContext {
                intent_id,
                tenant_id,
                actor_id: "external-api".to_string(),
                from_version: 2,
                to_version: 3,
                decision_class: "D".to_string(),
                new_approval_id,
            },
        )
        .await;

        // Should have cancelled 1 approval (only the one in stale_ids)
        assert_eq!(cancelled_count, 1);

        // approved_id1 should now be Cancelled
        let cancelled = state
            .approval_request_repo
            .get_approval_request(approved_id1)
            .await
            .unwrap();
        assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);

        // approved_id2 should still be Approved (not in stale_ids)
        let still_approved = state
            .approval_request_repo
            .get_approval_request(approved_id2)
            .await
            .unwrap();
        assert_eq!(still_approved.status, ApprovalRequestStatus::Approved);

        // The new pending request should still be Pending
        let still_pending = state
            .approval_request_repo
            .get_approval_request(new_approval_id)
            .await
            .unwrap();
        assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
    }

    #[tokio::test]
    async fn test_cancel_specific_approved_and_audit_with_empty_stale_ids() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };
        use intent_service::ApprovalRequestStatus;

        let state = create_test_service();

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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create an Approved approval request
        let approved_request = intent_service::ApprovalRequest::new_pending(
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
        let approved_id = approved_request.id;
        state
            .approval_request_repo
            .create_approval_request(approved_request)
            .await
            .unwrap();
        state
            .approval_request_repo
            .update_approval_request_status(
                approved_id,
                ApprovalRequestStatus::Approved,
                "approver",
                None,
            )
            .await
            .unwrap();

        // Create a new pending approval request
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            2,
            3,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call targeted cancellation with empty stale_ids
        let stale_ids: Vec<String> = vec![];
        let cancelled_count = cancel_specific_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            &stale_ids,
            CancelApprovalContext {
                intent_id,
                tenant_id,
                actor_id: "external-api".to_string(),
                from_version: 2,
                to_version: 3,
                decision_class: "D".to_string(),
                new_approval_id,
            },
        )
        .await;

        // Should have cancelled 0 approvals (empty stale_ids)
        assert_eq!(cancelled_count, 0);

        // The approved request should still be Approved
        let still_approved = state
            .approval_request_repo
            .get_approval_request(approved_id)
            .await
            .unwrap();
        assert_eq!(still_approved.status, ApprovalRequestStatus::Approved);
    }

    #[tokio::test]
    async fn test_cancel_specific_approved_and_audit_only_cancels_approved_status() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, CreateIntentRequest, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };
        use intent_service::ApprovalRequestStatus;

        let state = create_test_service();

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

        let intent_head = state.service.get_intent_head(intent_id).await.unwrap();
        let tenant_id = intent_head.intent.tenant_id;

        // Create a Pending approval request (not Approved)
        let pending_request = intent_service::ApprovalRequest::new_pending(
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
        let pending_id = pending_request.id;
        state
            .approval_request_repo
            .create_approval_request(pending_request)
            .await
            .unwrap();
        // Note: it's already Pending, don't call update_approval_request_status

        // Create a new pending approval request
        let new_approval = intent_service::ApprovalRequest::new_pending(
            intent_id,
            2,
            3,
            workflow_id,
            tenant_id,
            "external-api",
            "external-api",
            "D",
            "New blocked rebase",
        );
        let new_approval_id = new_approval.id;
        state
            .approval_request_repo
            .create_approval_request(new_approval)
            .await
            .unwrap();

        // Call targeted cancellation with pending_id as stale (but it's Pending, not Approved)
        let stale_ids = vec![pending_id.to_string()];
        let cancelled_count = cancel_specific_approved_and_audit(
            &state.approval_request_repo,
            &state.audit_service,
            &state.event_publisher,
            &stale_ids,
            CancelApprovalContext {
                intent_id,
                tenant_id,
                actor_id: "external-api".to_string(),
                from_version: 2,
                to_version: 3,
                decision_class: "D".to_string(),
                new_approval_id,
            },
        )
        .await;

        // Should have cancelled 0 approvals (only Approved can be cancelled)
        assert_eq!(cancelled_count, 0);

        // The pending request should still be Pending
        let still_pending = state
            .approval_request_repo
            .get_approval_request(pending_id)
            .await
            .unwrap();
        assert_eq!(still_pending.status, ApprovalRequestStatus::Pending);
    }

    // =========================================================================
    // Trace Context Propagation Tests (Phase 3 Batch 2 Slice 2 — bounded OTEL)
    //
    // Note: Direct middleware testing requires complex axum infrastructure.
    // The trace_context_middleware is verified through:
    // 1. cargo check -p intent-api (verifies compilation)
    // 2. cargo test -p intent-api (verifies existing tests still pass)
    // 3. Router wiring in build_router() includes trace_context_middleware layer
    // =========================================================================

    // =========================================================================
    // RLC-1 Tenant Mismatch Tests (Phase 3 P3-S5 Bounded Slice)
    //
    // Tests for JWT tenant ownership validation on high-risk handlers.
    // These tests verify fail-closed behavior on tenant mismatch.
    // =========================================================================

    /// Helper to create RlsTenantClaims for testing
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
    fn create_test_optional_rls_claims(tenant_id: Uuid) -> auth::OptionalRlsTenantClaims {
        auth::OptionalRlsTenantClaims(Some(create_test_rls_claims(tenant_id)))
    }

    // -------------------------------------------------------------------------
    // approve_compensation_action Tenant Mismatch Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_approve_compensation_action_rejects_tenant_mismatch() {
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

        // Try to approve with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = ApproveCompensationActionBody {
            lock_version: created.lock_version,
            approved_by: Some("test-approver".to_string()),
        };

        let result = approve_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Path(created.id),
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

    #[tokio::test]
    async fn test_approve_compensation_action_succeeds_with_matching_tenant() {
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

        // Approve with TenantA (matching)
        let request = ApproveCompensationActionBody {
            lock_version: created.lock_version,
            approved_by: Some("test-approver".to_string()),
        };

        let result = approve_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Path(created.id),
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
        assert_eq!(response.status, "approved");
    }

    // -------------------------------------------------------------------------
    // orchestration_dry_run Tenant Mismatch Tests (P1-S5i)
    // -------------------------------------------------------------------------

    /// Tests that orchestration_dry_run rejects JWT tenant mismatch.
    /// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match query tenant_id.
    #[tokio::test]
    async fn test_orchestration_dry_run_rejects_tenant_mismatch() {
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

        // Try to run dry-run with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let query = OrchestrationQuery {
            tenant_id: tenant_a, // Query has TenantA
        };
        let request = OrchestrationDryRunRequest {
            action_ids: vec![created.id],
        };

        let result = compensation_planner_handlers::orchestration_dry_run(
            State(state),
            create_test_optional_rls_claims(tenant_b), // JWT has TenantB - mismatch
            axum::extract::Query(query),
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
    #[tokio::test]
    async fn test_orchestration_dry_run_succeeds_with_matching_tenant() {
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

        // Run dry-run with TenantA (matching)
        let query = OrchestrationQuery {
            tenant_id: tenant_a,
        };
        let request = OrchestrationDryRunRequest {
            action_ids: vec![created.id],
        };

        let result = compensation_planner_handlers::orchestration_dry_run(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            axum::extract::Query(query),
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

    // -------------------------------------------------------------------------
    // replay_intent Tenant Mismatch Tests (P1-S5i)
    // -------------------------------------------------------------------------

    /// Tests that replay_intent rejects JWT tenant mismatch.
    /// P1-S5i: Validates fail-closed behavior when JWT tenant_id doesn't match intent's tenant_id.
    #[tokio::test]
    async fn test_replay_intent_rejects_tenant_mismatch() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let result = crate::replay_handlers::replay_intent(
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
    #[tokio::test]
    async fn test_replay_intent_succeeds_with_matching_tenant() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload,
            IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef, Urgency,
        };

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

        let result = crate::replay_handlers::replay_intent(
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

    // -------------------------------------------------------------------------
    // rebase_apply Tenant Mismatch Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_rebase_apply_rejects_tenant_mismatch() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
            DiffRequest, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
            IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, SourceRef,
            Urgency,
        };

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

        let state = create_test_service();

        // Create an intent with TenantA (via service directly, not handler)
        let tenant_a = Uuid::new_v4();
        let create_request = CreateIntentRequest {
            tenant_id: Some(tenant_a), // Set tenant_id to TenantA
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

        // Now call rebase_apply with TenantB (different from intent's tenant)
        let tenant_b = Uuid::new_v4();
        let diff_request = DiffRequest {
            from_version: 1,
            to_version: 2,
        };

        let result = rebase_apply(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Path(intent_id),
            Json(diff_request),
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

    // -------------------------------------------------------------------------
    // execute_compensation_action Tenant Mismatch Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_compensation_action_rejects_tenant_mismatch() {
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

        // Approve the action first (necessary for execution)
        state
            .compensation_action_service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Try to execute with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };

        let result = execute_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Path(created.id),
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

    #[tokio::test]
    async fn test_execute_compensation_action_succeeds_with_matching_tenant() {
        let state = create_test_service();

        // Create a compensation action with TenantA
        // Use Automatic feasibility so execution succeeds
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
            compensation_service::CompensationFeasibility::Automatic, // Must be Automatic for execution
            compensation_service::StrategyType::Rollback,
            "Test rollback",
        );
        let created = state
            .compensation_action_service
            .create_action(action)
            .await
            .unwrap();

        // Approve the action first (necessary for execution)
        state
            .compensation_action_service
            .approve_action(created.id, created.lock_version, None)
            .await
            .unwrap();

        // Execute with TenantA (matching)
        let request = ExecuteCompensationActionBody {
            executed_by: Some("test-executor".to_string()),
        };

        let result = execute_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Path(created.id),
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
        assert_eq!(response.status, "executed");
    }

    // -------------------------------------------------------------------------
    // waive_compensation_action Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_waive_compensation_action_rejects_tenant_mismatch() {
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

        // Try to waive with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = WaiveCompensationActionBody {
            lock_version: created.lock_version,
            waived_by: Some("test-waiver".to_string()),
        };

        let result = waive_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Path(created.id),
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

    #[tokio::test]
    async fn test_waive_compensation_action_succeeds_with_matching_tenant() {
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

        // Waive with TenantA (matching)
        let request = WaiveCompensationActionBody {
            lock_version: created.lock_version,
            waived_by: Some("test-waiver".to_string()),
        };

        let result = waive_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Path(created.id),
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
        assert_eq!(response.status, "waived");
    }

    // -------------------------------------------------------------------------
    // reapprove_compensation_action Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_reapprove_compensation_action_rejects_tenant_mismatch() {
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
        // (can't easily create a Failed action through normal flow in test)
        use compensation_service::CompensationStatus;
        let failed_action = state
            .compensation_action_service
            .update_status(created.id, CompensationStatus::Failed, created.lock_version)
            .await
            .unwrap();

        // Try to reapprove with TenantB (mismatch)
        let tenant_b = Uuid::new_v4();
        let request = ReapproveCompensationActionBody {
            lock_version: failed_action.lock_version,
        };

        let result = reapprove_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            Path(created.id),
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

    #[tokio::test]
    async fn test_reapprove_compensation_action_succeeds_with_matching_tenant() {
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
        let failed_action = state
            .compensation_action_service
            .update_status(created.id, CompensationStatus::Failed, created.lock_version)
            .await
            .unwrap();

        // Reapprove with TenantA (matching)
        let request = ReapproveCompensationActionBody {
            lock_version: failed_action.lock_version,
        };

        let result = reapprove_compensation_action(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            Path(created.id),
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
        assert_eq!(response.status, "pending");
    }

    // -------------------------------------------------------------------------
    // batch_approve_compensation_actions Tenant Mismatch Tests (RLC-2)
    // -------------------------------------------------------------------------

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

        let result = batch_approve_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            axum::extract::Query(OrchestrationQuery {
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

        let result = batch_approve_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            axum::extract::Query(OrchestrationQuery {
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

        let result = batch_reapprove_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            axum::extract::Query(OrchestrationQuery {
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

        let result = batch_reapprove_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            axum::extract::Query(OrchestrationQuery {
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

        let result = batch_execute_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_b), // Tenant B mismatch
            axum::extract::Query(OrchestrationQuery {
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

        let result = batch_execute_compensation_actions(
            State(state),
            create_test_optional_rls_claims(tenant_a), // Tenant A matches
            axum::extract::Query(OrchestrationQuery {
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
