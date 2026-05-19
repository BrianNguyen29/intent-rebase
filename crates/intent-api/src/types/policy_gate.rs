use compensation_service::{
    CoordinationRecord as ServiceCoordinationRecord,
    CoordinationResult as ServiceCoordinationResult,
    CoordinationSummary as ServiceCoordinationSummary,
};
use compensation_service::{
    ErrorClassification, PolicyGateEvaluation as ServicePolicyGateEvaluation,
    PolicyGateEvaluationResult as ServicePolicyGateEvaluationResult,
    PolicyGateMetadata as ServicePolicyGateMetadata, PolicyGateSummary as ServicePolicyGateSummary,
    RiskMetadata as ServiceRiskMetadata,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{
    format_compensation_status, format_coordination_status, format_error_severity,
    format_feasibility, format_feasibility_risk, format_gate_status, format_retry_exhaustion_risk,
    format_strategy_severity, format_strategy_type,
};

// =============================================================================
// Policy Gate Evaluation Types (Phase 3 Batch 1 bounded read-only slice)
// =============================================================================

/// Query parameters for tenant-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct CompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped policy gate evaluation
#[derive(Debug, Deserialize)]
pub struct IntentCompensationPolicyGateQuery {
    pub tenant_id: Uuid,
}

/// API response for policy gate evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationPolicyGateResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub evaluations: Vec<PolicyGateEvaluationResponse>,
    pub summary: PolicyGateSummaryResponse,
}

/// Policy gate evaluation for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateEvaluationResponse {
    pub action: compensation_service::CompensationAction,
    pub gate_status: String,
    pub gate_reason: String,
    pub policy_metadata: PolicyGateMetadataResponse,
    pub risk_metadata: RiskMetadataResponse,
}

/// Policy gate metadata for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateMetadataResponse {
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub has_non_retryable_error: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Risk metadata for a single action (API version)
///
/// Phase 3 Batch 1 (bounded read-only slice): Derived from existing action state fields.
/// Provides risk-relevant signals: strategy severity, retry exhaustion risk, feasibility risk,
/// error severity, remaining retry budget, error classification, terminal state flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetadataResponse {
    pub strategy_severity: String,
    pub retry_exhaustion_risk: String,
    pub feasibility_risk: String,
    pub error_severity: String,
    pub retry_budget_remaining: i32,
    pub error_classification: Option<ErrorClassificationResponse>,
    pub is_terminal: bool,
    pub requires_manual_intervention: bool,
}

/// Error classification response (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorClassificationResponse {
    pub error_code: String,
    pub retryable: bool,
    pub reason: String,
}

/// Summary of policy gate evaluations (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGateSummaryResponse {
    pub total_actions: usize,
    pub eligible_count: usize,
    pub blocked_count: usize,
    pub manual_review_required_count: usize,
    pub dlq_candidate_count: usize,
    pub pending_approval_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServicePolicyGateEvaluationResult> for CompensationPolicyGateResponse {
    fn from(result: ServicePolicyGateEvaluationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            evaluations: result
                .evaluations
                .into_iter()
                .map(PolicyGateEvaluationResponse::from)
                .collect(),
            summary: PolicyGateSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServicePolicyGateEvaluation> for PolicyGateEvaluationResponse {
    fn from(eval: ServicePolicyGateEvaluation) -> Self {
        Self {
            action: eval.action,
            gate_status: format_gate_status(&eval.gate_status),
            gate_reason: eval.gate_reason,
            policy_metadata: PolicyGateMetadataResponse::from(eval.policy_metadata),
            risk_metadata: RiskMetadataResponse::from(eval.risk_metadata),
        }
    }
}

impl From<ServicePolicyGateMetadata> for PolicyGateMetadataResponse {
    fn from(meta: ServicePolicyGateMetadata) -> Self {
        Self {
            auto_executable: meta.auto_executable,
            is_dlq_candidate: meta.is_dlq_candidate,
            can_reapprove: meta.can_reapprove,
            retry_budget_exhausted: meta.retry_budget_exhausted,
            has_non_retryable_error: meta.has_non_retryable_error,
            feasibility: format_feasibility(&meta.feasibility),
            strategy_type: format_strategy_type(&meta.strategy_type),
            status: format_compensation_status(&meta.status),
            attempt_count: meta.attempt_count,
            max_retries: meta.max_retries,
        }
    }
}

impl From<ServicePolicyGateSummary> for PolicyGateSummaryResponse {
    fn from(summary: ServicePolicyGateSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            eligible_count: summary.eligible_count,
            blocked_count: summary.blocked_count,
            manual_review_required_count: summary.manual_review_required_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            pending_approval_count: summary.pending_approval_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}

impl From<ServiceRiskMetadata> for RiskMetadataResponse {
    fn from(risk: ServiceRiskMetadata) -> Self {
        Self {
            strategy_severity: format_strategy_severity(&risk.strategy_severity),
            retry_exhaustion_risk: format_retry_exhaustion_risk(&risk.retry_exhaustion_risk),
            feasibility_risk: format_feasibility_risk(&risk.feasibility_risk),
            error_severity: format_error_severity(&risk.error_severity),
            retry_budget_remaining: risk.retry_budget_remaining,
            error_classification: risk
                .error_classification
                .map(ErrorClassificationResponse::from),
            is_terminal: risk.is_terminal,
            requires_manual_intervention: risk.requires_manual_intervention,
        }
    }
}

impl From<ErrorClassification> for ErrorClassificationResponse {
    fn from(ec: ErrorClassification) -> Self {
        Self {
            error_code: ec.error_code,
            retryable: ec.retryable,
            reason: ec.reason,
        }
    }
}

// =============================================================================
// Orchestration Coordination Types (Phase 3 Batch 1 bounded read-only view)
// =============================================================================

/// Query parameters for tenant-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct OrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for intent-scoped orchestration coordination status
#[derive(Debug, Deserialize)]
pub struct IntentOrchestrationCoordinationQuery {
    pub tenant_id: Uuid,
}

/// API response for orchestration coordination status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationCoordinationResponse {
    pub tenant_id: Uuid,
    pub intent_id: Option<Uuid>,
    pub records: Vec<CoordinationRecordResponse>,
    pub summary: CoordinationSummaryResponse,
}

/// Coordination record for a single action (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationRecordResponse {
    pub action: compensation_service::CompensationAction,
    pub coordination_status: String,
    pub coordination_reason: String,
    pub auto_executable: bool,
    pub is_dlq_candidate: bool,
    pub can_reapprove: bool,
    pub retry_budget_exhausted: bool,
    pub feasibility: String,
    pub strategy_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_retries: i32,
}

/// Summary of coordination records (API version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationSummaryResponse {
    pub total_actions: usize,
    pub ready_count: usize,
    pub awaiting_policy_count: usize,
    pub awaiting_manual_review_count: usize,
    pub blocked_count: usize,
    pub terminal_count: usize,
    pub dlq_candidate_count: usize,
    pub auto_executable_count: usize,
}

impl From<ServiceCoordinationResult> for OrchestrationCoordinationResponse {
    fn from(result: ServiceCoordinationResult) -> Self {
        Self {
            tenant_id: Uuid::nil(), // Will be set by caller
            intent_id: None,        // Will be set by caller
            records: result
                .records
                .into_iter()
                .map(CoordinationRecordResponse::from)
                .collect(),
            summary: CoordinationSummaryResponse::from(result.summary),
        }
    }
}

impl From<ServiceCoordinationRecord> for CoordinationRecordResponse {
    fn from(record: ServiceCoordinationRecord) -> Self {
        Self {
            action: record.action,
            coordination_status: format_coordination_status(&record.coordination_status),
            coordination_reason: record.coordination_reason,
            auto_executable: record.auto_executable,
            is_dlq_candidate: record.is_dlq_candidate,
            can_reapprove: record.can_reapprove,
            retry_budget_exhausted: record.retry_budget_exhausted,
            feasibility: format_feasibility(&record.feasibility),
            strategy_type: format_strategy_type(&record.strategy_type),
            status: format_compensation_status(&record.status),
            attempt_count: record.attempt_count,
            max_retries: record.max_retries,
        }
    }
}

impl From<ServiceCoordinationSummary> for CoordinationSummaryResponse {
    fn from(summary: ServiceCoordinationSummary) -> Self {
        Self {
            total_actions: summary.total_actions,
            ready_count: summary.ready_count,
            awaiting_policy_count: summary.awaiting_policy_count,
            awaiting_manual_review_count: summary.awaiting_manual_review_count,
            blocked_count: summary.blocked_count,
            terminal_count: summary.terminal_count,
            dlq_candidate_count: summary.dlq_candidate_count,
            auto_executable_count: summary.auto_executable_count,
        }
    }
}
