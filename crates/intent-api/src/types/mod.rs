//! Intent API response and request types
//!
//! Phase 2: Bounded file decomposition slice. This module contains pure data types
//! for HTTP request/response handling. These types are re-exported from the crate root
//! to maintain API compatibility.
//!
//! **Bounded scope:** This is a first extraction slice. Not all types have been
//! moved here yet. Types remaining in lib.rs include AppState, handlers, middleware,
//! and complex composed types.

pub mod approval;
pub mod artifact;
pub mod compensation;
pub mod error;
pub mod forensic;
pub mod graph;
pub mod health;
pub mod impact;
pub mod intent;
pub mod policy;
pub mod policy_gate;
pub mod propagation;
pub mod simulation;
pub mod webhook;

pub use approval::*;
pub use artifact::*;
pub use compensation::*;
pub use error::*;
pub use forensic::*;
pub use graph::*;
pub use health::*;
pub use impact::*;
pub use intent::*;
pub use policy::*;
pub use policy_gate::*;
pub use propagation::*;
pub use simulation::*;
pub use webhook::*;

use compensation_service::{
    CompensationFeasibility, CompensationStatus, CoordinationStatus as ServiceCoordinationStatus,
    ErrorSeverity, FeasibilityRisk, PolicyGateStatus as ServicePolicyGateStatus,
    RetryExhaustionRisk, StrategySeverity, StrategyType,
};

// Pure formatting helpers for Policy Gate types
pub(crate) fn format_gate_status(status: &ServicePolicyGateStatus) -> String {
    match status {
        ServicePolicyGateStatus::Eligible => "eligible".to_string(),
        ServicePolicyGateStatus::Blocked => "blocked".to_string(),
        ServicePolicyGateStatus::ManualReviewRequired => "manual_review_required".to_string(),
    }
}

pub(crate) fn format_feasibility(f: &CompensationFeasibility) -> String {
    match f {
        CompensationFeasibility::Automatic => "automatic".to_string(),
        CompensationFeasibility::SemiAutomatic => "semi_automatic".to_string(),
        CompensationFeasibility::ManualOnly => "manual_only".to_string(),
        CompensationFeasibility::NotPossible => "not_possible".to_string(),
    }
}

pub(crate) fn format_strategy_type(s: &StrategyType) -> String {
    match s {
        StrategyType::Rollback => "rollback".to_string(),
        StrategyType::CounterAction => "counter_action".to_string(),
        StrategyType::FollowupNotice => "followup_notice".to_string(),
        StrategyType::Quarantine => "quarantine".to_string(),
        StrategyType::Escalation => "escalation".to_string(),
    }
}

pub(crate) fn format_compensation_status(s: &CompensationStatus) -> String {
    match s {
        CompensationStatus::Pending => "pending".to_string(),
        CompensationStatus::Approved => "approved".to_string(),
        CompensationStatus::Executed => "executed".to_string(),
        CompensationStatus::Failed => "failed".to_string(),
        CompensationStatus::Waived => "waived".to_string(),
    }
}

pub(crate) fn format_strategy_severity(s: &StrategySeverity) -> String {
    match s {
        StrategySeverity::Low => "low".to_string(),
        StrategySeverity::Medium => "medium".to_string(),
        StrategySeverity::High => "high".to_string(),
        StrategySeverity::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_retry_exhaustion_risk(r: &RetryExhaustionRisk) -> String {
    match r {
        RetryExhaustionRisk::Low => "low".to_string(),
        RetryExhaustionRisk::Medium => "medium".to_string(),
        RetryExhaustionRisk::High => "high".to_string(),
        RetryExhaustionRisk::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_feasibility_risk(f: &FeasibilityRisk) -> String {
    match f {
        FeasibilityRisk::Low => "low".to_string(),
        FeasibilityRisk::Medium => "medium".to_string(),
        FeasibilityRisk::High => "high".to_string(),
        FeasibilityRisk::Critical => "critical".to_string(),
    }
}

pub(crate) fn format_error_severity(e: &ErrorSeverity) -> String {
    match e {
        ErrorSeverity::None => "none".to_string(),
        ErrorSeverity::Low => "low".to_string(),
        ErrorSeverity::Medium => "medium".to_string(),
        ErrorSeverity::High => "high".to_string(),
    }
}

pub(crate) fn format_coordination_status(status: &ServiceCoordinationStatus) -> String {
    match status {
        ServiceCoordinationStatus::Ready => "ready".to_string(),
        ServiceCoordinationStatus::AwaitingPolicy => "awaiting_policy".to_string(),
        ServiceCoordinationStatus::AwaitingManualReview => "awaiting_manual_review".to_string(),
        ServiceCoordinationStatus::Blocked => "blocked".to_string(),
        ServiceCoordinationStatus::Terminal => "terminal".to_string(),
    }
}
