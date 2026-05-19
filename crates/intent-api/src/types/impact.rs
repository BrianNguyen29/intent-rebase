use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// ImpactReport Types (Phase 2 bounded MVP — on-demand read-only projection)
// =============================================================================

/// Query parameters for ImpactReport endpoint.
///
/// Bounded MVP: on-demand read-only projection aggregating existing primitives.
/// No persistence, no migration, no DB table.
#[derive(Debug, Deserialize)]
pub struct ImpactReportQuery {
    pub tenant_id: Uuid,
    pub from_version: i32,
    pub to_version: i32,
}

/// Trigger section of the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactTrigger {
    pub change_summary: String,
    pub risk_tier: String,
    pub decision_class: String,
}

/// Scope section of the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactScope {
    pub affected_artifacts_count: usize,
    pub affected_approvals_count: usize,
    pub affected_side_effects_count: usize,
}

/// Invalidation section of the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactInvalidation {
    pub invalidated_artifacts_count: usize,
    pub invalidated_approvals_count: usize,
}

/// Compensation section of the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactCompensation {
    pub total_actions: usize,
    pub eligible_count: usize,
    pub blocked_count: usize,
    pub manual_review_required_count: usize,
    pub dlq_candidate_count: usize,
}

/// Safety gate summary for the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyGateSummary {
    pub open_gates: usize,
    pub blocked_gates: usize,
    pub manual_review_gates: usize,
}

/// Provenance metadata for the ImpactReport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactProvenance {
    pub generated_at: DateTime<Utc>,
    pub from_version: i32,
    pub to_version: i32,
}

/// Response for the ImpactReport endpoint.
///
/// Bounded MVP read-only projection. Aggregates intent diff, graph affected items,
/// side effects, compensation actions, and policy gate evaluation into a single
/// transient snapshot. Not persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReportResponse {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub trigger: ImpactTrigger,
    pub scope: ImpactScope,
    pub invalidation: ImpactInvalidation,
    pub compensation: ImpactCompensation,
    pub safety_gates: SafetyGateSummary,
    pub provenance: ImpactProvenance,
    pub unsupported_items: Vec<String>,
}
