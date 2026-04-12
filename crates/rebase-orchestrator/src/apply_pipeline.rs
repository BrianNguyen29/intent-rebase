//! Apply pipeline — risk-tier-controlled auto-apply policy
//!
//! Phase 2b: This module implements the internal apply pipeline with `risk_tier`
//! as the controlling apply-policy contract:
//! - LOW risk_tier: Automatic, no approval required
//! - MEDIUM risk_tier: Automatic with notification
//! - HIGH/CRITICAL risk_tier: Blocked, requires manual approval
//!
//! ## Design Principles
//!
//! - **No public endpoints**: Pure internal orchestration
//! - **Risk-tier controlled**: `risk_tier` is the primary policy contract (Phase 2b)
//! - **Decision class secondary**: `decision_class` preserved for audit/display purposes
//! - **Notification-ready**: Supports notification hooks (Phase 3 integration point)
//!
//! ## Guard Pattern
//!
//! The pipeline uses a guard pattern to enforce risk-tier policies:
//! - `LowRiskTierGuard`: Allows LOW risk_tier to auto-apply (no notification)
//! - `MediumRiskTierGuard`: Allows MEDIUM risk_tier to auto-apply (with notification)
//! - `HighCriticalRiskTierGuard`: Blocks HIGH/CRITICAL risk_tier, returns manual review required

use intent_rebase_types::RiskTier;
use rebase_engine::DecisionClass;

/// Outcome of an apply decision
///
/// Phase 2b: Outcomes are driven by risk_tier policy:
/// - NoOp: No apply needed (no semantic changes or Class A)
/// - AutoProceeded: Auto-proceeded without notification (LOW risk_tier)
/// - AutoProceededWithNotification: Auto-proceeded with notification sent (MEDIUM risk_tier)
/// - BlockedManualReview: Blocked, requires manual review (HIGH/CRITICAL risk_tier)
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyOutcome {
    /// No apply needed (no semantic changes or Class A)
    NoOp,
    /// Auto-proceeded without notification (LOW risk_tier)
    AutoProceeded,
    /// Auto-proceeded with notification sent (MEDIUM risk_tier)
    AutoProceededWithNotification,
    /// Blocked, requires manual review (HIGH/CRITICAL risk_tier)
    BlockedManualReview,
}

/// Decision made by the apply pipeline
#[derive(Debug, Clone)]
pub enum ApplyDecision {
    /// Apply should proceed
    Proceed {
        /// Whether a notification should be sent
        notification: bool,
    },
    /// Apply is a no-op
    NoOp,
    /// Apply is blocked, manual review required
    Blocked {
        /// Reason for blocking
        reason: String,
    },
}

/// Guard trait for apply decisions
///
/// Phase 2b: Guards evaluate the risk_tier as the controlling policy contract,
/// with decision_class available for secondary context (e.g., no-op detection).
/// Policy contract:
/// - LOW risk_tier: auto-apply, no notification
/// - MEDIUM risk_tier: auto-apply with notification
/// - HIGH/CRITICAL risk_tier: blocked, manual review required
pub trait ApplyGuard: Send + Sync {
    /// Evaluate the apply decision based on risk_tier and decision_class.
    ///
    /// # Arguments
    /// * `risk_tier` - The controlling policy contract (Phase 2b)
    /// * `decision_class` - Secondary context for audit/display (Class A = no-op)
    fn evaluate(&self, risk_tier: &RiskTier, decision_class: DecisionClass) -> ApplyDecision;
}

/// Guard that enforces risk-tier controlled policy (Phase 2b)
///
/// Policy contract:
/// - LOW risk_tier: auto-apply, no notification
/// - MEDIUM risk_tier: auto-apply with notification
/// - HIGH/CRITICAL risk_tier: blocked, manual review required
///
/// Class A is always NoOp regardless of risk_tier.
#[derive(Default)]
pub struct RiskTierGuard;

impl RiskTierGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for RiskTierGuard {
    fn evaluate(&self, risk_tier: &RiskTier, decision_class: DecisionClass) -> ApplyDecision {
        // Class A is always NoOp regardless of risk_tier
        if decision_class == DecisionClass::A {
            return ApplyDecision::NoOp;
        }

        // Phase 2b risk-tier policy contract
        match risk_tier {
            RiskTier::Low => {
                // LOW risk_tier: automatic, no approval required, no notification
                ApplyDecision::Proceed {
                    notification: false,
                }
            }
            RiskTier::Medium => {
                // MEDIUM risk_tier: automatic with notification
                ApplyDecision::Proceed { notification: true }
            }
            RiskTier::High | RiskTier::Critical => {
                // HIGH/CRITICAL risk_tier: blocked, requires manual approval
                ApplyDecision::Blocked {
                    reason: format!(
                        "{:?} risk_tier requires manual review. Auto-apply not permitted for high/critical risk changes.",
                        risk_tier
                    ),
                }
            }
        }
    }
}

/// Guard that blocks all auto-apply (for high-security environments)
///
/// Phase 2b: This guard blocks ALL risk tiers including LOW/MEDIUM,
/// requiring manual review for all changes. Useful for environments
/// where human approval is required for all changes.
#[derive(Default)]
pub struct HighCriticalGuard;

impl HighCriticalGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for HighCriticalGuard {
    fn evaluate(&self, risk_tier: &RiskTier, decision_class: DecisionClass) -> ApplyDecision {
        // Even Class A is blocked in high-critical mode
        if decision_class == DecisionClass::A {
            return ApplyDecision::Blocked {
                reason: "All changes require manual review (HighCriticalGuard mode)".to_string(),
            };
        }

        ApplyDecision::Blocked {
            reason: format!(
                "{:?} risk_tier requires manual review (HighCriticalGuard mode)",
                risk_tier
            ),
        }
    }
}

/// Guard that enforces standard Phase 2b risk-tier policy (default)
///
/// This is the default guard for Phase 2b:
/// - LOW risk_tier: Auto-proceed, no notification
/// - MEDIUM risk_tier: Auto-proceed with notification
/// - HIGH/CRITICAL risk_tier: Blocked for manual review
/// - Class A: Always NoOp regardless of risk_tier
#[derive(Default)]
pub struct StandardGuard;

impl StandardGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for StandardGuard {
    fn evaluate(&self, risk_tier: &RiskTier, decision_class: DecisionClass) -> ApplyDecision {
        RiskTierGuard::new().evaluate(risk_tier, decision_class)
    }
}

/// The internal apply pipeline
///
/// Coordinates the apply decision process using configurable guards.
/// This is the main entry point for Phase 2 internal apply operations.
pub struct ApplyPipeline {
    guard: Box<dyn ApplyGuard>,
}

impl ApplyPipeline {
    /// Create a new ApplyPipeline with the default guard (StandardGuard)
    pub fn new() -> Self {
        Self {
            guard: Box::new(StandardGuard::new()),
        }
    }

    /// Create an ApplyPipeline with a custom guard
    pub fn with_guard<G: ApplyGuard + 'static>(guard: G) -> Self {
        Self {
            guard: Box::new(guard),
        }
    }

    /// Create an ApplyPipeline that requires manual review for all (HighCriticalGuard)
    pub fn with_high_critical_mode() -> Self {
        Self {
            guard: Box::new(HighCriticalGuard::new()),
        }
    }

    /// Evaluate the apply decision based on risk_tier and decision_class.
    ///
    /// Phase 2b: risk_tier is the controlling policy contract.
    /// decision_class is preserved for secondary context (e.g., Class A = NoOp).
    pub fn evaluate(&self, risk_tier: &RiskTier, decision_class: DecisionClass) -> ApplyDecision {
        self.guard.evaluate(risk_tier, decision_class)
    }

    /// Convert an ApplyDecision to an ApplyOutcome
    ///
    /// This is useful for converting the internal decision to a
    /// result type that can be returned to callers.
    pub fn decision_to_outcome(decision: &ApplyDecision) -> ApplyOutcome {
        match decision {
            ApplyDecision::NoOp => ApplyOutcome::NoOp,
            ApplyDecision::Proceed { notification } => {
                if *notification {
                    ApplyOutcome::AutoProceededWithNotification
                } else {
                    ApplyOutcome::AutoProceeded
                }
            }
            ApplyDecision::Blocked { .. } => ApplyOutcome::BlockedManualReview,
        }
    }
}

impl Default for ApplyPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Request for apply operation
///
/// Phase 2b: risk_tier is the controlling policy contract.
#[derive(Debug, Clone)]
pub struct ApplyRequest {
    /// The controlling risk tier (Phase 2b policy contract)
    pub risk_tier: RiskTier,
    /// The decision class from the rebase plan (secondary context)
    pub decision_class: DecisionClass,
    /// Whether to use strict mode (HighCriticalGuard)
    pub strict: bool,
    /// Optional reason for manual review (populated when blocked)
    pub blocked_reason: Option<String>,
}

impl ApplyRequest {
    /// Create a new ApplyRequest from risk_tier and decision_class
    pub fn new(risk_tier: RiskTier, decision_class: DecisionClass) -> Self {
        Self {
            risk_tier,
            decision_class,
            strict: false,
            blocked_reason: None,
        }
    }

    /// Create a strict mode request
    pub fn strict(risk_tier: RiskTier, decision_class: DecisionClass) -> Self {
        Self {
            risk_tier,
            decision_class,
            strict: true,
            blocked_reason: None,
        }
    }
}

/// Result of an apply operation
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// The outcome of the apply
    pub outcome: ApplyOutcome,
    /// Human-readable rationale
    pub rationale: String,
    /// Whether a notification should be sent
    pub notification_required: bool,
}

impl ApplyResult {
    /// Create a no-op result
    pub fn noop(rationale: impl Into<String>) -> Self {
        Self {
            outcome: ApplyOutcome::NoOp,
            rationale: rationale.into(),
            notification_required: false,
        }
    }

    /// Create an auto-proceed result
    pub fn auto_proceed(notification: bool, rationale: impl Into<String>) -> Self {
        Self {
            outcome: if notification {
                ApplyOutcome::AutoProceededWithNotification
            } else {
                ApplyOutcome::AutoProceeded
            },
            rationale: rationale.into(),
            notification_required: notification,
        }
    }

    /// Create a blocked result
    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            outcome: ApplyOutcome::BlockedManualReview,
            rationale: reason.into(),
            notification_required: true, // Always notify when blocked
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // RiskTierGuard tests — Phase 2b risk-tier policy contract
    // =========================================================================

    #[test]
    fn test_risk_tier_guard_class_a_always_noop() {
        let guard = RiskTierGuard::new();

        // Class A is always NoOp regardless of risk_tier
        assert!(matches!(
            guard.evaluate(&RiskTier::Low, DecisionClass::A),
            ApplyDecision::NoOp
        ));
        assert!(matches!(
            guard.evaluate(&RiskTier::Medium, DecisionClass::A),
            ApplyDecision::NoOp
        ));
        assert!(matches!(
            guard.evaluate(&RiskTier::High, DecisionClass::A),
            ApplyDecision::NoOp
        ));
        assert!(matches!(
            guard.evaluate(&RiskTier::Critical, DecisionClass::A),
            ApplyDecision::NoOp
        ));
    }

    #[test]
    fn test_risk_tier_guard_low_auto_proceed_no_notification() {
        let guard = RiskTierGuard::new();

        // LOW risk_tier: auto-apply, no notification
        for class in [
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            let decision = guard.evaluate(&RiskTier::Low, class);
            match decision {
                ApplyDecision::Proceed { notification } => {
                    assert!(
                        !notification,
                        "LOW risk_tier should not require notification"
                    );
                }
                _ => panic!(
                    "Expected Proceed decision for LOW risk_tier, got {:?}",
                    decision
                ),
            }
        }
    }

    #[test]
    fn test_risk_tier_guard_medium_auto_proceed_with_notification() {
        let guard = RiskTierGuard::new();

        // MEDIUM risk_tier: auto-apply with notification
        for class in [
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            let decision = guard.evaluate(&RiskTier::Medium, class);
            match decision {
                ApplyDecision::Proceed { notification } => {
                    assert!(notification, "MEDIUM risk_tier should require notification");
                }
                _ => panic!(
                    "Expected Proceed decision for MEDIUM risk_tier, got {:?}",
                    decision
                ),
            }
        }
    }

    #[test]
    fn test_risk_tier_guard_high_blocked() {
        let guard = RiskTierGuard::new();

        // HIGH risk_tier: blocked, requires manual approval
        for class in [
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            let decision = guard.evaluate(&RiskTier::High, class);
            match decision {
                ApplyDecision::Blocked { reason } => {
                    assert!(reason.contains("High"), "Should mention High risk_tier");
                    assert!(
                        reason.contains("manual review"),
                        "Should mention manual review"
                    );
                }
                _ => panic!(
                    "Expected Blocked decision for HIGH risk_tier, got {:?}",
                    decision
                ),
            }
        }
    }

    #[test]
    fn test_risk_tier_guard_critical_blocked() {
        let guard = RiskTierGuard::new();

        // CRITICAL risk_tier: blocked, requires manual approval
        for class in [
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            let decision = guard.evaluate(&RiskTier::Critical, class);
            match decision {
                ApplyDecision::Blocked { reason } => {
                    assert!(
                        reason.contains("Critical"),
                        "Should mention Critical risk_tier"
                    );
                    assert!(
                        reason.contains("manual review"),
                        "Should mention manual review"
                    );
                }
                _ => panic!(
                    "Expected Blocked decision for CRITICAL risk_tier, got {:?}",
                    decision
                ),
            }
        }
    }

    #[test]
    fn test_high_critical_guard_all_blocked() {
        let guard = HighCriticalGuard::new();

        // Class A should still be blocked in high-critical mode
        let decision = guard.evaluate(&RiskTier::Low, DecisionClass::A);
        assert!(matches!(decision, ApplyDecision::Blocked { .. }));

        // All risk tiers should be blocked
        for risk_tier in [
            &RiskTier::Low,
            &RiskTier::Medium,
            &RiskTier::High,
            &RiskTier::Critical,
        ] {
            for class in [
                DecisionClass::B,
                DecisionClass::C,
                DecisionClass::D,
                DecisionClass::E,
            ] {
                let decision = guard.evaluate(risk_tier, class);
                assert!(
                    matches!(decision, ApplyDecision::Blocked { .. }),
                    "HighCriticalGuard should block {:?} + {:?}",
                    risk_tier,
                    class
                );
            }
        }
    }

    #[test]
    fn test_standard_guard_risk_tier_policy() {
        let pipeline = ApplyPipeline::new();

        // Standard guard enforces risk-tier policy
        // Class A: always NoOp
        assert!(matches!(
            pipeline.evaluate(&RiskTier::Low, DecisionClass::A),
            ApplyDecision::NoOp
        ));

        // LOW: auto-proceed, no notification
        assert!(matches!(
            pipeline.evaluate(&RiskTier::Low, DecisionClass::B),
            ApplyDecision::Proceed {
                notification: false
            }
        ));

        // MEDIUM: auto-proceed, with notification
        assert!(matches!(
            pipeline.evaluate(&RiskTier::Medium, DecisionClass::B),
            ApplyDecision::Proceed { notification: true }
        ));

        // HIGH: blocked
        assert!(matches!(
            pipeline.evaluate(&RiskTier::High, DecisionClass::B),
            ApplyDecision::Blocked { .. }
        ));

        // CRITICAL: blocked
        assert!(matches!(
            pipeline.evaluate(&RiskTier::Critical, DecisionClass::B),
            ApplyDecision::Blocked { .. }
        ));
    }

    #[test]
    fn test_high_critical_mode_all_blocked() {
        let pipeline = ApplyPipeline::with_high_critical_mode();

        // All should be blocked in high-critical mode
        for risk_tier in [
            &RiskTier::Low,
            &RiskTier::Medium,
            &RiskTier::High,
            &RiskTier::Critical,
        ] {
            for class in [
                DecisionClass::A,
                DecisionClass::B,
                DecisionClass::C,
                DecisionClass::D,
                DecisionClass::E,
            ] {
                assert!(
                    matches!(
                        pipeline.evaluate(risk_tier, class),
                        ApplyDecision::Blocked { .. }
                    ),
                    "HighCriticalGuard should block {:?} + {:?}",
                    risk_tier,
                    class
                );
            }
        }
    }

    #[test]
    fn test_decision_to_outcome() {
        assert_eq!(
            ApplyPipeline::decision_to_outcome(&ApplyDecision::NoOp),
            ApplyOutcome::NoOp
        );

        assert_eq!(
            ApplyPipeline::decision_to_outcome(&ApplyDecision::Proceed {
                notification: false
            }),
            ApplyOutcome::AutoProceeded
        );

        assert_eq!(
            ApplyPipeline::decision_to_outcome(&ApplyDecision::Proceed { notification: true }),
            ApplyOutcome::AutoProceededWithNotification
        );

        assert_eq!(
            ApplyPipeline::decision_to_outcome(&ApplyDecision::Blocked {
                reason: "test".to_string()
            }),
            ApplyOutcome::BlockedManualReview
        );
    }

    #[test]
    fn test_apply_result_factory() {
        let result = ApplyResult::noop("No changes");
        assert_eq!(result.outcome, ApplyOutcome::NoOp);
        assert!(!result.notification_required);

        let result = ApplyResult::auto_proceed(false, "Low risk");
        assert_eq!(result.outcome, ApplyOutcome::AutoProceeded);
        assert!(!result.notification_required);

        let result = ApplyResult::auto_proceed(true, "Medium risk");
        assert_eq!(result.outcome, ApplyOutcome::AutoProceededWithNotification);
        assert!(result.notification_required);

        let result = ApplyResult::blocked("High risk");
        assert_eq!(result.outcome, ApplyOutcome::BlockedManualReview);
        assert!(result.notification_required);
    }

    #[test]
    fn test_apply_request_factory() {
        let request = ApplyRequest::new(RiskTier::Low, DecisionClass::B);
        assert_eq!(request.risk_tier, RiskTier::Low);
        assert_eq!(request.decision_class, DecisionClass::B);
        assert!(!request.strict);

        let request = ApplyRequest::strict(RiskTier::High, DecisionClass::D);
        assert_eq!(request.risk_tier, RiskTier::High);
        assert_eq!(request.decision_class, DecisionClass::D);
        assert!(request.strict);
    }

    #[test]
    fn test_custom_guard() {
        struct DummyGuard;

        impl ApplyGuard for DummyGuard {
            fn evaluate(
                &self,
                _risk_tier: &RiskTier,
                _decision_class: DecisionClass,
            ) -> ApplyDecision {
                ApplyDecision::NoOp
            }
        }

        let pipeline = ApplyPipeline::with_guard(DummyGuard);

        // All decisions should be no-op with DummyGuard
        for risk_tier in [
            &RiskTier::Low,
            &RiskTier::Medium,
            &RiskTier::High,
            &RiskTier::Critical,
        ] {
            for class in [
                DecisionClass::A,
                DecisionClass::B,
                DecisionClass::C,
                DecisionClass::D,
                DecisionClass::E,
            ] {
                assert!(matches!(
                    pipeline.evaluate(risk_tier, class),
                    ApplyDecision::NoOp
                ));
            }
        }
    }
}
