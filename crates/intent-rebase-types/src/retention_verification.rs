//! Data retention and deletion verification types
//!
//! This module provides bounded retention verification primitives for audit data.
//! **Scope of this slice:**
//! - Retention policy types (per data category, per storage tier)
//! - Retention period queries (can query "is X within retention period Y")
//! - Deletion request tracking types
//! - S3 lifecycle configuration artifact (local config template, not live enforcement)
//!
//! **NOT in scope for this slice:**
//! - Live S3 enforcement / actual deletion execution
//! - Cloud policy enforcement (AWS/GCP native controls)
//! - Backup rotation enforcement
//!
//! All cloud-side deletion enforcement requires out-of-band cloud tooling,
//! IAM policies, and AWS Config/CloudTrail rules that are outside this codebase.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Retention period specification for a data category
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPeriod {
    /// Human-readable name of the data category
    pub category: String,
    /// Retention in hot storage (Postgres) - days
    pub hot_retention_days: u32,
    /// Retention in cold storage (S3) - days
    pub cold_retention_days: u32,
    /// Total retention period (hot + cold) - days
    pub total_retention_days: u32,
}

impl RetentionPeriod {
    /// Create a new retention period specification
    pub fn new(category: &str, hot_retention_days: u32, cold_retention_days: u32) -> Self {
        let total = hot_retention_days.saturating_add(cold_retention_days);
        Self {
            category: category.to_string(),
            hot_retention_days,
            cold_retention_days,
            total_retention_days: total,
        }
    }

    /// Check if a timestamp is within the hot retention period
    pub fn is_within_hot_retention(&self, occurred_at: DateTime<Utc>) -> bool {
        let cutoff = Utc::now() - chrono::Duration::days(self.hot_retention_days as i64);
        occurred_at > cutoff
    }

    /// Check if a timestamp is within the total retention period
    pub fn is_within_total_retention(&self, occurred_at: DateTime<Utc>) -> bool {
        let cutoff = Utc::now() - chrono::Duration::days(self.total_retention_days as i64);
        occurred_at > cutoff
    }

    /// Get the cutoff date for hot storage (events older than this should be in cold storage)
    pub fn hot_storage_cutoff(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(self.hot_retention_days as i64)
    }

    /// Get the cutoff date for total retention (events older than this may be deletable)
    pub fn total_retention_cutoff(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(self.total_retention_days as i64)
    }
}

/// Standard retention periods per data category (from governance docs)
pub mod standard_retention {
    use super::*;

    /// Audit events: 90 days hot, 7 years cold = 7 years total
    pub fn audit_events() -> RetentionPeriod {
        RetentionPeriod::new("audit_events", 90, 2555)
    }

    /// Policy snapshots: 90 days hot, 10 years cold = 10 years total
    pub fn policy_snapshots() -> RetentionPeriod {
        RetentionPeriod::new("policy_snapshots", 90, 3650)
    }

    /// Provenance records: 90 days hot, 10 years cold = 10 years total
    pub fn provenance_records() -> RetentionPeriod {
        RetentionPeriod::new("provenance_records", 90, 3650)
    }

    /// Forensic bundles: 90 days hot, 7 years cold = 7 years total
    pub fn forensic_bundles() -> RetentionPeriod {
        RetentionPeriod::new("forensic_bundles", 90, 2555)
    }

    /// Rule pack history: 90 days hot, 5 years cold = 5 years total
    pub fn rule_pack_history() -> RetentionPeriod {
        RetentionPeriod::new("rule_pack_history", 90, 1825)
    }
}

/// Status of a data deletion request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeletionRequestStatus {
    /// Deletion request has been received and is pending processing
    Pending,
    /// Deletion is in progress
    Processing,
    /// Deletion has been completed
    Completed,
    /// Deletion failed
    Failed,
}

impl DeletionRequestStatus {
    /// Check if a transition from this status to another is valid.
    ///
    /// Valid transitions:
    /// - `Pending` → `Processing`
    /// - `Processing` → `Completed`
    /// - `Processing` → `Failed`
    /// - `Failed` → `Processing` (retry allowed)
    pub fn can_transition_to(&self, next: &DeletionRequestStatus) -> bool {
        matches!(
            (self, next),
            (
                DeletionRequestStatus::Pending,
                DeletionRequestStatus::Processing
            ) | (
                DeletionRequestStatus::Processing,
                DeletionRequestStatus::Completed
            ) | (
                DeletionRequestStatus::Processing,
                DeletionRequestStatus::Failed
            ) | (
                DeletionRequestStatus::Failed,
                DeletionRequestStatus::Processing
            )
        )
    }
}

/// Target of a data deletion request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeletionTargetType {
    /// Delete all data for a user
    User,
    /// Delete all data for a tenant
    Tenant,
    /// Delete a specific intent and its artifacts
    Intent,
    /// Delete a specific artifact
    Artifact,
}

/// Request for data deletion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionRequest {
    /// Unique identifier for this deletion request
    pub id: Uuid,
    /// Type of data being deleted
    pub target_type: DeletionTargetType,
    /// ID of the target being deleted
    pub target_id: Uuid,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Reason for deletion (user request, compliance, legal, etc.)
    pub reason: String,
    /// Who authorized this deletion
    pub authorized_by: String,
    /// When the deletion was requested
    pub requested_at: DateTime<Utc>,
    /// Current status of the deletion
    pub status: DeletionRequestStatus,
    /// When the deletion was completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
    /// Notes about the deletion (verification results, errors, etc.)
    pub notes: Option<String>,
}

/// Verification result for a retention check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionVerificationResult {
    /// Whether the data is within its retention period
    pub is_within_retention: bool,
    /// The retention period that was checked
    pub retention_period: RetentionPeriod,
    /// The timestamp that was checked
    pub checked_timestamp: DateTime<Utc>,
    /// How many days until the data reaches hot storage cutoff (if still in hot)
    pub days_until_hot_cutoff: Option<i64>,
    /// How many days until the data reaches total retention cutoff
    pub days_until_total_cutoff: Option<i64>,
    /// Human-readable description
    pub description: String,
}

impl RetentionVerificationResult {
    /// Verify if a timestamp is within the retention period
    pub fn verify(retention_period: &RetentionPeriod, occurred_at: DateTime<Utc>) -> Self {
        let hot_cutoff = retention_period.hot_storage_cutoff();
        let total_cutoff = retention_period.total_retention_cutoff();

        let is_within_retention = occurred_at > total_cutoff;
        let days_until_hot_cutoff = Some((occurred_at - hot_cutoff).num_days());
        let days_until_total_cutoff = Some((occurred_at - total_cutoff).num_days());

        let description = if is_within_retention {
            format!(
                "{} is within retention (hot: {} days, total: {} days)",
                retention_period.category,
                retention_period.hot_retention_days,
                retention_period.total_retention_days
            )
        } else {
            format!(
                "{} is OUTSIDE retention period and may be eligible for deletion",
                retention_period.category
            )
        };

        Self {
            is_within_retention,
            retention_period: retention_period.clone(),
            checked_timestamp: occurred_at,
            days_until_hot_cutoff,
            days_until_total_cutoff,
            description,
        }
    }
}

/// S3 lifecycle configuration for a bucket
/// This is a local configuration artifact that can be used to configure S3 lifecycle policies.
/// **NOTE:** This is a CONFIGURATION TEMPLATE only. Actual S3 enforcement requires:
/// - AWS IAM policies that restrict bucket access
/// - S3 Object Lock configuration via AWS console/API
/// - AWS Config rules for compliance monitoring
/// - CloudTrail for audit logging of S3 operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3LifecycleConfig {
    /// Bucket name this config applies to
    pub bucket_name: String,
    /// Lifecycle rules
    pub rules: Vec<S3LifecycleRule>,
}

/// A single S3 lifecycle rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3LifecycleRule {
    /// Rule ID
    pub id: String,
    /// Rule status (Enabled/Disabled)
    pub status: String,
    /// Prefix to filter objects (empty = all objects)
    pub prefix: String,
    /// Transitions to cold storage
    pub transitions: Vec<S3StorageTransition>,
    /// When to expire objects (None = no expiration)
    pub expiration_days: Option<u32>,
}

/// Transition to a different storage class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3StorageTransition {
    /// Days after creation to transition
    pub days: u32,
    /// Target storage class
    pub storage_class: String,
}

impl S3LifecycleConfig {
    /// Create S3 lifecycle config for governance data buckets
    pub fn governance_bucket_config(bucket_name: &str) -> Self {
        Self {
            bucket_name: bucket_name.to_string(),
            rules: vec![
                S3LifecycleRule {
                    id: "move-to-glacier-after-90-days".to_string(),
                    status: "Enabled".to_string(),
                    prefix: "audit-events/".to_string(),
                    transitions: vec![S3StorageTransition {
                        days: 90,
                        storage_class: "GLACIER".to_string(),
                    }],
                    expiration_days: Some(2555), // 7 years for audit events
                },
                S3LifecycleRule {
                    id: "move-to-glacier-after-90-days-policy".to_string(),
                    status: "Enabled".to_string(),
                    prefix: "policy-snapshots/".to_string(),
                    transitions: vec![S3StorageTransition {
                        days: 90,
                        storage_class: "GLACIER".to_string(),
                    }],
                    expiration_days: Some(3650), // 10 years for policy snapshots
                },
                S3LifecycleRule {
                    id: "move-to-glacier-after-90-days-provenance".to_string(),
                    status: "Enabled".to_string(),
                    prefix: "provenance/".to_string(),
                    transitions: vec![S3StorageTransition {
                        days: 90,
                        storage_class: "GLACIER".to_string(),
                    }],
                    expiration_days: Some(3650), // 10 years for provenance
                },
                S3LifecycleRule {
                    id: "move-to-glacier-after-90-days-forensic".to_string(),
                    status: "Enabled".to_string(),
                    prefix: "forensic-bundles/".to_string(),
                    transitions: vec![S3StorageTransition {
                        days: 90,
                        storage_class: "GLACIER".to_string(),
                    }],
                    expiration_days: Some(2555), // 7 years for forensic bundles
                },
                S3LifecycleRule {
                    id: "move-to-glacier-after-90-days-rule-pack".to_string(),
                    status: "Enabled".to_string(),
                    prefix: "rule-pack-history/".to_string(),
                    transitions: vec![S3StorageTransition {
                        days: 90,
                        storage_class: "GLACIER".to_string(),
                    }],
                    expiration_days: Some(1825), // 5 years for rule pack history
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_period_new() {
        let rp = RetentionPeriod::new("test", 90, 2555);
        assert_eq!(rp.category, "test");
        assert_eq!(rp.hot_retention_days, 90);
        assert_eq!(rp.cold_retention_days, 2555);
        assert_eq!(rp.total_retention_days, 2645);
    }

    #[test]
    fn test_retention_period_within_hot() {
        let rp = standard_retention::audit_events();
        let recent = Utc::now() - chrono::Duration::days(30);
        assert!(rp.is_within_hot_retention(recent));
    }

    #[test]
    fn test_retention_period_outside_hot() {
        let rp = standard_retention::audit_events();
        let old = Utc::now() - chrono::Duration::days(100);
        assert!(!rp.is_within_hot_retention(old));
    }

    #[test]
    fn test_retention_period_within_total() {
        let rp = standard_retention::audit_events();
        let recent = Utc::now() - chrono::Duration::days(365);
        assert!(rp.is_within_total_retention(recent));
    }

    #[test]
    fn test_retention_period_outside_total() {
        let rp = standard_retention::audit_events();
        let very_old = Utc::now() - chrono::Duration::days(3000);
        assert!(!rp.is_within_total_retention(very_old));
    }

    #[test]
    fn test_retention_verification_result_within() {
        let rp = standard_retention::audit_events();
        let recent = Utc::now() - chrono::Duration::days(30);
        let result = RetentionVerificationResult::verify(&rp, recent);
        assert!(result.is_within_retention);
        assert!(result.days_until_hot_cutoff.unwrap() > 0);
        assert!(result.days_until_total_cutoff.unwrap() > 0);
    }

    #[test]
    fn test_retention_verification_result_outside() {
        let rp = standard_retention::audit_events();
        let very_old = Utc::now() - chrono::Duration::days(3000);
        let result = RetentionVerificationResult::verify(&rp, very_old);
        assert!(!result.is_within_retention);
    }

    #[test]
    fn test_s3_lifecycle_config_governance_bucket() {
        let config = S3LifecycleConfig::governance_bucket_config("test-bucket");
        assert_eq!(config.bucket_name, "test-bucket");
        assert_eq!(config.rules.len(), 5);

        // Check audit events rule
        let audit_rule = config
            .rules
            .iter()
            .find(|r| r.prefix.contains("audit-events"))
            .unwrap();
        assert_eq!(audit_rule.expiration_days, Some(2555));
        assert_eq!(audit_rule.transitions.len(), 1);
        assert_eq!(audit_rule.transitions[0].storage_class, "GLACIER");
    }

    #[test]
    fn test_deletion_request_status_transitions() {
        let request = DeletionRequest {
            id: Uuid::new_v4(),
            target_type: DeletionTargetType::User,
            target_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            reason: "GDPR request".to_string(),
            authorized_by: "admin@example.com".to_string(),
            requested_at: Utc::now(),
            status: DeletionRequestStatus::Pending,
            completed_at: None,
            notes: None,
        };
        assert_eq!(request.status, DeletionRequestStatus::Pending);
    }

    #[test]
    fn test_deletion_request_status_can_transition() {
        // Valid transitions
        assert!(
            DeletionRequestStatus::Pending.can_transition_to(&DeletionRequestStatus::Processing)
        );
        assert!(
            DeletionRequestStatus::Processing.can_transition_to(&DeletionRequestStatus::Completed)
        );
        assert!(DeletionRequestStatus::Processing.can_transition_to(&DeletionRequestStatus::Failed));
        assert!(DeletionRequestStatus::Failed.can_transition_to(&DeletionRequestStatus::Processing));

        // Invalid transitions
        assert!(
            !DeletionRequestStatus::Pending.can_transition_to(&DeletionRequestStatus::Completed)
        );
        assert!(!DeletionRequestStatus::Pending.can_transition_to(&DeletionRequestStatus::Failed));
        assert!(
            !DeletionRequestStatus::Completed.can_transition_to(&DeletionRequestStatus::Processing)
        );
        assert!(!DeletionRequestStatus::Failed.can_transition_to(&DeletionRequestStatus::Pending));
        assert!(!DeletionRequestStatus::Completed.can_transition_to(&DeletionRequestStatus::Failed));
    }

    #[test]
    fn test_s3_lifecycle_config_has_rule_pack_history() {
        let config = S3LifecycleConfig::governance_bucket_config("test-bucket");
        let rule_pack_rule = config
            .rules
            .iter()
            .find(|r| r.prefix.contains("rule-pack-history"))
            .expect("rule-pack-history rule should exist");
        assert_eq!(rule_pack_rule.expiration_days, Some(1825));
        assert_eq!(rule_pack_rule.transitions.len(), 1);
        assert_eq!(rule_pack_rule.transitions[0].storage_class, "GLACIER");
    }
}
