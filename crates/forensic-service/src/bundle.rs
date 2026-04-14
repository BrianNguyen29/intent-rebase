//! Forensic bundle model
//!
//! See [../../../../docs/14-governance/10-forensic-bundle.md] for full specification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::bundle_contents::BundleContents;

/// Purpose of the forensic bundle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundlePurpose {
    IncidentInvestigation,
    ComplianceAudit,
    Legal,
}

/// Retention policy for a forensic bundle
///
/// **Model-level retention evidence only — no S3 enforcement, no scheduler.**
/// This struct records the intended retention policy and expiry timestamp
/// as metadata on the bundle. Actual S3 lifecycle enforcement, background
/// deletion jobs, and automatic expiry are NOT implemented in this slice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRetention {
    /// Retention policy tier (maps to storage class in full S3 implementation)
    pub policy: RetentionPolicy,
    /// When this bundle expires / auto-purges (if policy has one)
    pub expires_at: Option<DateTime<Utc>>,
    /// When the retention period was set
    pub retention_set_at: DateTime<Utc>,
    /// Actor who set the retention policy (or "system")
    pub retention_set_by: String,
}

/// Retention policy tier
///
/// **Truthful scope:** These are label values for the intended retention tier.
/// The actual S3 lifecycle rules (GLACIER after 30d, DEEP_ARCHIVE after 3650d)
/// are NOT implemented. Only model/schema-level labels are provided here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicy {
    /// Hot storage tier — shortest retention
    Hot,
    /// Cold storage tier — medium retention
    Cold,
    /// Archive tier — longest retention
    Archive,
}

impl BundleRetention {
    /// Create a new retention record for a bundle
    ///
    /// **Bounded scope:** This only sets metadata. No S3 lifecycle, no scheduler.
    pub fn new(policy: RetentionPolicy, set_by: &str) -> Self {
        let now = Utc::now();
        Self {
            policy,
            expires_at: None, // Computed by S3 lifecycle in full impl — not here
            retention_set_at: now,
            retention_set_by: set_by.to_string(),
        }
    }

    /// Set expiry from policy (placeholder — actual calc happens in S3 layer)
    pub fn with_expiry(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
}

/// Integrity verification result embedded in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleIntegrity {
    /// SHA256 hash of the manifest
    pub manifest_hash: String,
    /// Whether the full hash chain was verified successfully
    pub chain_verified: bool,
    /// When verification was performed
    pub verification_timestamp: DateTime<Utc>,
}

/// Time range covered by the bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Top-level forensic bundle manifest
///
/// **Batch 0 scope:** type scaffold with construction helpers only.
/// Bundle generation, S3 storage, and integrity verification are Batch 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundle {
    /// Unique identifier for this bundle
    pub bundle_id: Uuid,
    /// Bundle format version
    pub bundle_version: String,
    /// When this bundle was created
    pub created_at: DateTime<Utc>,
    /// Actor who triggered bundle generation (or "system")
    pub created_by: String,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Time range covered by this bundle
    pub time_range: BundleTimeRange,
    /// Purpose of this bundle
    pub purpose: BundlePurpose,
    /// Summary of contents included in this bundle
    pub contents: BundleContents,
    /// Integrity verification result
    pub integrity: BundleIntegrity,
    /// Retention policy metadata
    ///
    /// **Truthful scope:** This is model-level retention evidence only.
    /// Actual S3 lifecycle enforcement, background deletion jobs, and
    /// automatic expiry are NOT implemented.
    pub retention: Option<BundleRetention>,
}

impl ForensicBundle {
    /// Create a new bundle manifest (scaffold — actual generation is Batch 3).
    ///
    /// **Bounded scope:** retention is model-level metadata only.
    /// No S3 lifecycle, scheduler, or automatic deletion.
    pub fn new(
        tenant_id: Uuid,
        time_range: BundleTimeRange,
        purpose: BundlePurpose,
        contents: BundleContents,
        created_by: &str,
        retention: Option<BundleRetention>,
    ) -> Self {
        Self {
            bundle_id: Uuid::new_v4(),
            bundle_version: "v1".to_string(),
            created_at: Utc::now(),
            created_by: created_by.to_string(),
            tenant_id,
            time_range,
            purpose,
            contents,
            integrity: BundleIntegrity {
                manifest_hash: String::new(), // Computed during generation (Batch 3)
                chain_verified: false,
                verification_timestamp: Utc::now(),
            },
            retention,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forensic_bundle_construction() {
        let tenant_id = Uuid::new_v4();
        let time_range = BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        };
        let contents = BundleContents::default();
        let bundle = ForensicBundle::new(
            tenant_id,
            time_range,
            BundlePurpose::IncidentInvestigation,
            contents,
            "system",
            None,
        );

        assert_eq!(bundle.tenant_id, tenant_id);
        assert_eq!(bundle.bundle_version, "v1");
        assert_eq!(bundle.created_by, "system");
        assert!(!bundle.integrity.chain_verified);
        assert!(bundle.retention.is_none());
    }

    #[test]
    fn test_forensic_bundle_with_retention() {
        let tenant_id = Uuid::new_v4();
        let time_range = BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        };
        let contents = BundleContents::default();
        let retention = BundleRetention::new(RetentionPolicy::Cold, "admin@example.com");
        let bundle = ForensicBundle::new(
            tenant_id,
            time_range,
            BundlePurpose::ComplianceAudit,
            contents,
            "admin@example.com",
            Some(retention),
        );

        assert!(bundle.retention.is_some());
        let retention = bundle.retention.as_ref().unwrap();
        assert_eq!(retention.policy, RetentionPolicy::Cold);
        assert_eq!(retention.retention_set_by, "admin@example.com");
        assert!(retention.expires_at.is_none()); // Not computed until S3 layer
    }

    #[test]
    fn test_retention_policy_serialization() {
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Hot).unwrap(),
            "\"hot\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Cold).unwrap(),
            "\"cold\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Archive).unwrap(),
            "\"archive\""
        );
    }

    #[test]
    fn test_bundle_retention_new() {
        let retention = BundleRetention::new(RetentionPolicy::Archive, "admin@example.com");
        assert_eq!(retention.policy, RetentionPolicy::Archive);
        assert_eq!(retention.retention_set_by, "admin@example.com");
        assert!(retention.expires_at.is_none());
    }

    #[test]
    fn test_bundle_retention_with_expiry() {
        let retention =
            BundleRetention::new(RetentionPolicy::Cold, "system").with_expiry(Utc::now());
        assert!(retention.expires_at.is_some());
    }

    #[test]
    fn test_forensic_bundle_serialization_round_trip() {
        let tenant_id = Uuid::new_v4();
        let time_range = BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        };
        let contents = BundleContents {
            intent_versions: 5,
            artifacts: 12,
            approvals: 3,
            audit_events: 1000,
            policy_snapshots: 2,
        };
        let bundle = ForensicBundle::new(
            tenant_id,
            time_range.clone(),
            BundlePurpose::ComplianceAudit,
            contents,
            "admin@example.com",
            None,
        );

        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: ForensicBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bundle_id, bundle.bundle_id);
        assert_eq!(deserialized.tenant_id, tenant_id);
        assert_eq!(deserialized.contents.intent_versions, 5);
        assert_eq!(deserialized.contents.audit_events, 1000);
    }
}
