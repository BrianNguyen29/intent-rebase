//! Forensic bundle model
//!
//! See [../../../../docs/14-governance/10-forensic-bundle.md] for full specification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::bundle_contents::BundleContents;

/// Generation status of a forensic bundle.
///
/// **Batch 3b (P4 bounded slice):** Status tracking and transitions are implemented.
/// Bundle generation itself is Phase 4 scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    /// Bundle generation has been requested but not yet started
    Pending,
    /// Bundle is currently being generated
    Generating,
    /// Bundle generation completed successfully
    Ready,
    /// Bundle generation failed
    Failed,
}

impl BundleStatus {
    /// Returns true if the status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, BundleStatus::Ready | BundleStatus::Failed)
    }

    /// Returns true if transition from one status to another is valid.
    pub fn can_transition_to(&self, target: BundleStatus) -> bool {
        use BundleStatus::*;
        match (self, target) {
            // Same status is always valid (no-op) - must check first before terminal arms
            (a, b) if *a == b => true,
            // Terminal states cannot transition to any other status
            (Ready, _) => false,
            (Failed, _) => false,
            // Pending can transition to Generating or Failed (if request invalid)
            (Pending, Generating) => true,
            (Pending, Failed) => true,
            // Generating can transition to Ready or Failed
            (Generating, Ready) => true,
            (Generating, Failed) => true,
            // All other transitions are invalid
            _ => false,
        }
    }
}

/// Purpose of the forensic bundle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
///
/// **Bounded replay evidence slice:** Per-section hashes are stored in the manifest
/// so that later replay verification can confirm bundle contents were not modified.
/// This is read-only integrity evidence, not full runtime/state reconstruction replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleIntegrity {
    /// SHA256 hash of the manifest
    pub manifest_hash: String,
    /// Whether the full hash chain was verified successfully
    pub chain_verified: bool,
    /// When verification was performed
    pub verification_timestamp: DateTime<Utc>,
    /// SHA256 hash of the intent versions section
    ///
    /// **Bounded scope:** Stored for replay verification. Empty until generation completes.
    pub intent_versions_hash: String,
    /// SHA256 hash of the artifacts section
    ///
    /// **Bounded scope:** Stored for replay verification. Empty until generation completes.
    pub artifacts_hash: String,
    /// SHA256 hash of the approvals section
    ///
    /// **Bounded scope:** Stored for replay verification. Empty until generation completes.
    pub approvals_hash: String,
    /// SHA256 hash of the audit events section
    ///
    /// **Bounded scope:** Stored for replay verification. Empty until generation completes.
    pub audit_events_hash: String,
    /// SHA256 hash of the policy snapshots section
    ///
    /// **Bounded scope:** Stored for replay verification. Empty until generation completes.
    pub policy_snapshots_hash: String,
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
///
/// **Batch 3b (P4 bounded slice):** Status tracking added.
/// Bundle generation, S3 storage, and integrity verification are Phase 4.
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
    /// Generation status of this bundle
    pub status: BundleStatus,
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
    /// Create a new bundle manifest with Pending status.
    ///
    /// **P4 bounded slice:** Status defaults to Pending.
    /// Actual bundle generation is Phase 4 scope.
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
            status: BundleStatus::Pending,
            contents,
            integrity: BundleIntegrity {
                manifest_hash: String::new(), // Computed during generation
                chain_verified: false,
                verification_timestamp: Utc::now(),
                intent_versions_hash: String::new(), // Computed during generation
                artifacts_hash: String::new(),
                approvals_hash: String::new(),
                audit_events_hash: String::new(),
                policy_snapshots_hash: String::new(),
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
        assert_eq!(bundle.status, BundleStatus::Pending);
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

    #[test]
    fn test_bundle_integrity_section_hashes_round_trip() {
        let tenant_id = Uuid::new_v4();
        let bundle = ForensicBundle::new(
            tenant_id,
            BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            BundlePurpose::IncidentInvestigation,
            BundleContents::default(),
            "test",
            None,
        );

        let mut bundle = bundle;
        bundle.integrity.intent_versions_hash = "hash_intent".to_string();
        bundle.integrity.artifacts_hash = "hash_artifacts".to_string();
        bundle.integrity.approvals_hash = "hash_approvals".to_string();
        bundle.integrity.audit_events_hash = "hash_audit".to_string();
        bundle.integrity.policy_snapshots_hash = "hash_policy".to_string();

        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: ForensicBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.integrity.intent_versions_hash, "hash_intent");
        assert_eq!(deserialized.integrity.artifacts_hash, "hash_artifacts");
        assert_eq!(deserialized.integrity.approvals_hash, "hash_approvals");
        assert_eq!(deserialized.integrity.audit_events_hash, "hash_audit");
        assert_eq!(deserialized.integrity.policy_snapshots_hash, "hash_policy");
    }
}
