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
}

impl ForensicBundle {
    /// Create a new bundle manifest with Pending status.
    ///
    /// **P4 bounded slice:** Status defaults to Pending.
    /// Actual bundle generation is Phase 4 scope.
    pub fn new(
        tenant_id: Uuid,
        time_range: BundleTimeRange,
        purpose: BundlePurpose,
        contents: BundleContents,
        created_by: &str,
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
                manifest_hash: String::new(), // Computed during generation (Phase 4)
                chain_verified: false,
                verification_timestamp: Utc::now(),
            },
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
        );

        assert_eq!(bundle.tenant_id, tenant_id);
        assert_eq!(bundle.bundle_version, "v1");
        assert_eq!(bundle.created_by, "system");
        assert_eq!(bundle.status, BundleStatus::Pending);
        assert!(!bundle.integrity.chain_verified);
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
        );

        let json = serde_json::to_string(&bundle).unwrap();
        let deserialized: ForensicBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bundle_id, bundle.bundle_id);
        assert_eq!(deserialized.tenant_id, tenant_id);
        assert_eq!(deserialized.contents.intent_versions, 5);
        assert_eq!(deserialized.contents.audit_events, 1000);
    }
}
