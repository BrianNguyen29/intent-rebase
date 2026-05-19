use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Propagation Status Types (Phase 4+ design-only; bounded stub endpoint)
// =============================================================================

/// Query parameters for propagation status endpoint.
///
/// **Design-only / Phase 4+ deferred.** Bounded stub returns empty downstream
/// systems and zeroed summary. No persistence, no event streaming, no webhook.
#[derive(Debug, Deserialize)]
pub struct PropagationStatusQuery {
    pub tenant_id: Uuid,
}

/// A single downstream system's propagation status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownstreamSystemStatus {
    pub system_id: String,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub status: String,
    pub last_seen_version: i32,
}

/// Summary counts for propagation status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationSummary {
    pub total: usize,
    pub acknowledged: usize,
    pub pending: usize,
    pub failed: usize,
}

/// Response for propagation status endpoint.
///
/// **Bounded stub:** Returns empty `downstream_systems` and zeroed summary.
/// Full implementation (webhook delivery, event streaming acknowledgment,
/// cross-workflow lineage) is Phase 4+ deferred scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationStatusResponse {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub downstream_systems: Vec<DownstreamSystemStatus>,
    pub propagation_summary: PropagationSummary,
    pub unsupported_items: Vec<String>,
}

/// Request body for signal ingestion endpoint (Slice 2 bounded).
///
/// Records that a downstream system has been signaled for an intent change.
/// No actual webhook delivery or event streaming — this is a bounded
/// manual/internal ingestion API.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestPropagationSignalRequest {
    pub tenant_id: Uuid,
    pub downstream_system_id: String,
    pub last_seen_version: i32,
}

/// Response for signal ingestion endpoint (Slice 2 bounded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestPropagationSignalResponse {
    pub record_id: Uuid,
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub downstream_system_id: String,
    pub status: String,
}
