//! Side effect model for compensation planning
//!
//! See [../../../../docs/03-spec/05-compensation.md] for full specification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Side effect severity class (S0–S4)
///
/// | Class | Description | Compensation |
/// |-------|-------------|--------------|
/// | S0 | Pure read, no side effect | None needed |
/// | S1 | Internal reversible | Auto if policy allows |
/// | S2 | External reversible | Auto or semi-auto by risk |
/// | S3 | External partially reversible | Operator review default |
/// | S4 | Irreversible | Escalation required |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    S0PureRead,
    S1InternalReversible,
    S2ExternalReversible,
    S3ExternalPartiallyReversible,
    S4Irreversible,
}

/// A recorded side effect from an artifact-producing operation.
///
/// **Batch 1 scope:** persistence and repository layer now implemented.
/// See [side_effect_repo.rs] for storage implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    /// Unique identifier for this side effect record
    pub id: Uuid,
    /// Tenant this side effect belongs to
    pub tenant_id: Uuid,
    /// Intent ID that produced this side effect
    pub intent_id: Uuid,
    /// Intent version at time of effect emission
    pub intent_version: i32,
    /// Class of side effect (S0–S4)
    pub effect_class: SideEffectClass,
    /// Effect type identifier (e.g. "email_sent", "pr_opened", "ticket_created")
    pub effect_type: String,
    /// Target of the effect (e.g. email address, PR URL, ticket ID)
    pub target: String,
    /// Timestamp when the effect occurred
    pub occurred_at: DateTime<Utc>,
    /// Optional idempotency key to prevent duplicate compensation
    pub idempotency_key: Option<String>,
}

impl SideEffect {
    /// Create a new side effect record.
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        effect_class: SideEffectClass,
        effect_type: &str,
        target: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            intent_version,
            effect_class,
            effect_type: effect_type.to_string(),
            target: target.to_string(),
            occurred_at: Utc::now(),
            idempotency_key: None,
        }
    }

    /// Create a new side effect record with an idempotency key.
    pub fn with_idempotency_key(
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        effect_class: SideEffectClass,
        effect_type: &str,
        target: &str,
        idempotency_key: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            intent_version,
            effect_class,
            effect_type: effect_type.to_string(),
            target: target.to_string(),
            occurred_at: Utc::now(),
            idempotency_key: Some(idempotency_key.to_string()),
        }
    }

    /// Returns true if this side effect can be compensated automatically.
    pub fn is_auto_compensatable(&self) -> bool {
        matches!(
            self.effect_class,
            SideEffectClass::S0PureRead | SideEffectClass::S1InternalReversible
        )
    }

    /// Returns true if this side effect requires escalation.
    pub fn requires_escalation(&self) -> bool {
        matches!(self.effect_class, SideEffectClass::S4Irreversible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_effect_construction() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "pr_opened",
            "https://github.com/example/pull/123",
        );

        assert_eq!(effect.tenant_id, tenant_id);
        assert_eq!(effect.intent_id, intent_id);
        assert_eq!(effect.intent_version, 1);
        assert!(!effect.is_auto_compensatable());
        assert!(!effect.requires_escalation());
    }

    #[test]
    fn test_side_effect_auto_compensatable_s0_s1() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let s0 = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S0PureRead,
            "read",
            "noop",
        );
        let s1 = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S1InternalReversible,
            "metadata_write",
            "db-record",
        );

        assert!(s0.is_auto_compensatable());
        assert!(s1.is_auto_compensatable());
    }

    #[test]
    fn test_side_effect_requires_escalation_s4() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S4Irreversible,
            "money_transfer",
            "account-xyz",
        );

        assert!(effect.requires_escalation());
    }

    #[test]
    fn test_side_effect_serialization_round_trip() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::new(
            tenant_id,
            intent_id,
            2,
            SideEffectClass::S3ExternalPartiallyReversible,
            "email_sent",
            "user@example.com",
        );

        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: SideEffect = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, effect.id);
        assert_eq!(deserialized.tenant_id, effect.tenant_id);
        assert_eq!(deserialized.intent_id, effect.intent_id);
        assert_eq!(
            deserialized.effect_class,
            SideEffectClass::S3ExternalPartiallyReversible
        );
        assert_eq!(deserialized.effect_type, "email_sent");
    }

    #[test]
    fn test_side_effect_with_idempotency_key() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let effect = SideEffect::with_idempotency_key(
            tenant_id,
            intent_id,
            1,
            SideEffectClass::S2ExternalReversible,
            "payment_initiated",
            "txn-12345",
            "payment-txn-12345-idempotent",
        );

        assert!(effect.idempotency_key.is_some());
        assert_eq!(
            effect.idempotency_key.unwrap(),
            "payment-txn-12345-idempotent"
        );
    }
}
