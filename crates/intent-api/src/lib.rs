//! Intent API HTTP transport layer
//!
//! Phase 1: Exposes intent/version endpoints via axum.
//! Routes are manually wired to match the OpenAPI spec in docs/04-api/openapi.yaml.

#[allow(unused_imports)]
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use graph_service::GraphService;
#[allow(unused_imports)]
use intent_rebase_types::{AffectedItemsStatus, DiffRequest};
use intent_service::IntentService;
use rebase_orchestrator::RebaseOrchestrator;
use std::sync::Arc;
use std::time::Instant;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
#[allow(unused_imports)]
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

/// Rebase apply handlers (Phase 2 bounded slice - extracted handler decomposition)
pub mod rebase_apply_handlers;

/// Approval invalidation and audit helpers (Phase 2b bounded slice - extracted helper decomposition)
pub mod approval_invalidation;

/// Router building and authentication middleware (Phase 3 bounded router extraction slice)
pub mod router;

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

// Re-export IntentRebaseError from intent_rebase_types for backward compatibility
// (used by forensic_handlers, rebase_apply_handlers, rebase_preview_handlers)
pub use intent_rebase_types::IntentRebaseError;

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

// Re-export router builders for backward compatibility
pub use router::{
    build_router, build_router_with_jwt_auth, build_router_with_sql_audit_and_approval,
    build_router_with_sql_audit_and_approval_jwt,
};

/// Shared test helpers for intent-api handler tests (Phase 3 bounded slice)
#[cfg(test)]
pub mod test_helpers;

#[cfg(test)]
mod handler_tests;
