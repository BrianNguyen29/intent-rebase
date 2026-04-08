//! Apply pipeline — internal low/medium auto-apply with D/E blocking
//!
//! This module implements the internal apply pipeline for Phase 2:
//! - Class A: No-op, return immediately
//! - Class B/C: Auto-proceed with optional notification
//! - Class D/E: Blocked, requires manual review
//!
//! ## Design Principles
//!
//! - **No public endpoints**: Pure internal orchestration
//! - **Class D/E blocked**: Manual review required, no auto-apply
//! - **Risk-tier aware**: Low/Medium risk can auto-apply, High/Critical blocked
//! - **Notification-ready**: Supports notification hooks (Phase 3 integration point)
//!
//! ## Guard Pattern
//!
//! The pipeline uses a guard pattern to enforce decision class policies:
//! - `LowMediumGuard`: Allows Class A/B/C to proceed
//! - `HighCriticalGuard`: Blocks Class D/E, returns manual review required

use rebase_engine::DecisionClass;

/// Outcome of an apply decision
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyOutcome {
    /// No apply needed (Class A - no semantic changes)
    NoOp,
    /// Auto-proceeded without notification (Class B/C with low risk)
    AutoProceeded,
    /// Auto-proceeded with notification sent (Class B/C with medium risk)
    AutoProceededWithNotification,
    /// Blocked, requires manual review (Class D/E)
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
/// Guards evaluate the decision class and determine whether
/// to allow auto-apply or block for manual review.
pub trait ApplyGuard: Send + Sync {
    /// Evaluate the decision class and return the apply decision
    fn evaluate(&self, decision_class: DecisionClass) -> ApplyDecision;
}

/// Guard that allows Class A/B/C (low/medium risk) auto-apply
///
/// Blocks Class D/E for manual review.
#[derive(Default)]
pub struct LowMediumGuard;

impl LowMediumGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for LowMediumGuard {
    fn evaluate(&self, decision_class: DecisionClass) -> ApplyDecision {
        match decision_class {
            DecisionClass::A => ApplyDecision::NoOp,
            DecisionClass::B | DecisionClass::C => {
                // Low/Medium severity changes can auto-apply
                // Class B typically no notification, Class C may need notification
                let notification = decision_class == DecisionClass::C;
                ApplyDecision::Proceed { notification }
            }
            DecisionClass::D | DecisionClass::E => {
                ApplyDecision::Blocked {
                    reason: format!(
                        "Class {:?} requires manual review. High-severity or critical changes detected.",
                        decision_class
                    ),
                }
            }
        }
    }
}

/// Guard that blocks all auto-apply (for high-security environments)
///
/// This guard blocks ALL classes including A/B/C, requiring manual review for all.
/// Useful for environments where human approval is required for all changes.
#[derive(Default)]
pub struct HighCriticalGuard;

impl HighCriticalGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for HighCriticalGuard {
    fn evaluate(&self, decision_class: DecisionClass) -> ApplyDecision {
        match decision_class {
            DecisionClass::A => ApplyDecision::Blocked {
                reason: "All changes require manual review (HighCriticalGuard mode)".to_string(),
            },
            DecisionClass::B | DecisionClass::C | DecisionClass::D | DecisionClass::E => {
                ApplyDecision::Blocked {
                    reason: format!(
                        "Class {:?} requires manual review (HighCriticalGuard mode)",
                        decision_class
                    ),
                }
            }
        }
    }
}

/// Guard that blocks only Class D/E (standard mode)
///
/// This is the default guard for Phase 2:
/// - Class A: No-op
/// - Class B/C: Auto-proceed
/// - Class D/E: Blocked for manual review
#[derive(Default)]
pub struct StandardGuard;

impl StandardGuard {
    pub fn new() -> Self {
        Self
    }
}

impl ApplyGuard for StandardGuard {
    fn evaluate(&self, decision_class: DecisionClass) -> ApplyDecision {
        LowMediumGuard::new().evaluate(decision_class)
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

    /// Evaluate the decision class and return the apply decision
    pub fn evaluate(&self, decision_class: DecisionClass) -> ApplyDecision {
        self.guard.evaluate(decision_class)
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
#[derive(Debug, Clone)]
pub struct ApplyRequest {
    /// The decision class from the rebase plan
    pub decision_class: DecisionClass,
    /// Whether to use strict mode (HighCriticalGuard)
    pub strict: bool,
    /// Optional reason for manual review (populated when blocked)
    pub blocked_reason: Option<String>,
}

impl ApplyRequest {
    /// Create a new ApplyRequest from a decision class
    pub fn from_decision_class(decision_class: DecisionClass) -> Self {
        Self {
            decision_class,
            strict: false,
            blocked_reason: None,
        }
    }

    /// Create a strict mode request
    pub fn strict(decision_class: DecisionClass) -> Self {
        Self {
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

    #[test]
    fn test_low_medium_guard_class_a() {
        let guard = LowMediumGuard::new();
        let decision = guard.evaluate(DecisionClass::A);

        assert!(matches!(decision, ApplyDecision::NoOp));
    }

    #[test]
    fn test_low_medium_guard_class_b() {
        let guard = LowMediumGuard::new();
        let decision = guard.evaluate(DecisionClass::B);

        match decision {
            ApplyDecision::Proceed { notification } => {
                assert!(!notification, "Class B should not require notification");
            }
            _ => panic!("Expected Proceed decision"),
        }
    }

    #[test]
    fn test_low_medium_guard_class_c() {
        let guard = LowMediumGuard::new();
        let decision = guard.evaluate(DecisionClass::C);

        match decision {
            ApplyDecision::Proceed { notification } => {
                assert!(notification, "Class C should require notification");
            }
            _ => panic!("Expected Proceed decision"),
        }
    }

    #[test]
    fn test_low_medium_guard_class_d_blocked() {
        let guard = LowMediumGuard::new();
        let decision = guard.evaluate(DecisionClass::D);

        match decision {
            ApplyDecision::Blocked { reason } => {
                assert!(reason.contains("manual review"));
            }
            _ => panic!("Expected Blocked decision"),
        }
    }

    #[test]
    fn test_low_medium_guard_class_e_blocked() {
        let guard = LowMediumGuard::new();
        let decision = guard.evaluate(DecisionClass::E);

        match decision {
            ApplyDecision::Blocked { reason } => {
                assert!(reason.contains("manual review"));
            }
            _ => panic!("Expected Blocked decision"),
        }
    }

    #[test]
    fn test_high_critical_guard_all_blocked() {
        let guard = HighCriticalGuard::new();

        // Class A should still be blocked
        let decision = guard.evaluate(DecisionClass::A);
        assert!(matches!(decision, ApplyDecision::Blocked { .. }));

        // Class B should be blocked
        let decision = guard.evaluate(DecisionClass::B);
        assert!(matches!(decision, ApplyDecision::Blocked { .. }));

        // Class C should be blocked
        let decision = guard.evaluate(DecisionClass::C);
        assert!(matches!(decision, ApplyDecision::Blocked { .. }));
    }

    #[test]
    fn test_standard_guard_default() {
        let pipeline = ApplyPipeline::new();

        // Standard guard should behave like LowMediumGuard
        assert!(matches!(
            pipeline.evaluate(DecisionClass::A),
            ApplyDecision::NoOp
        ));
        assert!(matches!(
            pipeline.evaluate(DecisionClass::B),
            ApplyDecision::Proceed { .. }
        ));
        assert!(matches!(
            pipeline.evaluate(DecisionClass::C),
            ApplyDecision::Proceed { .. }
        ));
        assert!(matches!(
            pipeline.evaluate(DecisionClass::D),
            ApplyDecision::Blocked { .. }
        ));
        assert!(matches!(
            pipeline.evaluate(DecisionClass::E),
            ApplyDecision::Blocked { .. }
        ));
    }

    #[test]
    fn test_high_critical_mode() {
        let pipeline = ApplyPipeline::with_high_critical_mode();

        // All should be blocked
        for class in [
            DecisionClass::A,
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            assert!(matches!(
                pipeline.evaluate(class),
                ApplyDecision::Blocked { .. }
            ));
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
        let request = ApplyRequest::from_decision_class(DecisionClass::B);
        assert_eq!(request.decision_class, DecisionClass::B);
        assert!(!request.strict);

        let request = ApplyRequest::strict(DecisionClass::D);
        assert_eq!(request.decision_class, DecisionClass::D);
        assert!(request.strict);
    }

    #[test]
    fn test_custom_guard() {
        struct DummyGuard;

        impl ApplyGuard for DummyGuard {
            fn evaluate(&self, _decision_class: DecisionClass) -> ApplyDecision {
                ApplyDecision::NoOp
            }
        }

        let pipeline = ApplyPipeline::with_guard(DummyGuard);

        // All decisions should be no-op with DummyGuard
        for class in [
            DecisionClass::A,
            DecisionClass::B,
            DecisionClass::C,
            DecisionClass::D,
            DecisionClass::E,
        ] {
            assert!(matches!(pipeline.evaluate(class), ApplyDecision::NoOp));
        }
    }
}
