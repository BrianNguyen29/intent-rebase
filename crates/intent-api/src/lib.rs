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

/// API key authentication scaffold (Phase 1 bounded decomposition slice)
pub mod api_key;

/// Tracing initialization module (Phase 3 Batch 2 Slice 2 bounded slice)
pub mod tracing_init;

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

/// DLQ metric helper functions (Phase 3 DLQ design — extracted helper decomposition)
pub mod dlq_metrics;

// Re-export panic_hardening::init_panic_hook for convenience
pub use panic_hardening::init_panic_hook;

// Re-export API key scaffold for convenience
pub use api_key::{api_key_extractor_middleware, ApiKey, ApiKeyExtensionKey, ApiKeyRejection};

// Re-export tracing_init::init_tracing for convenience
pub use tracing_init::init_tracing;

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

// Re-export DLQ metric helpers for backward compatibility (wired from nats_jetstream.rs)
pub use dlq_metrics::{
    record_dlq_message, record_dlq_message_age_seconds, record_dlq_messages_current,
    record_dlq_replay, record_dlq_replay_failure,
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
