//! Bundle generation service
//!
//! Provides the bounded forensic bundle generation path: collect explicit bundle
//! sections, compute integrity hashes, and produce a generation result ready for
//! storage.
//!
//! **This bounded slice scope:** Content collection primitives and integrity hashing.
//! No S3 storage, no HTTP API, no replay, no background jobs.

use chrono::Utc;
use uuid::Uuid;

use super::bundle::{BundlePurpose, BundleStatus, BundleTimeRange, ForensicBundle};
use super::bundle_contents::BundleContents;
use super::bundle_hasher::{
    verify_bundle_integrity, ApprovalEntry, ApprovalsForHash, ArtifactEntry, ArtifactsForHash,
    AuditEventEntry, AuditEventsForHash, BundleIntegrityHash, ContentSectionHash,
    ContentSectionsForVerification, IntentVersionEntry, IntentVersionsForHash, PolicySnapshotEntry,
    PolicySnapshotsForHash,
};

/// Result of a bounded bundle generation operation.
///
/// Contains the generated bundle manifest and the integrity hash record
/// that can be used to verify the bundle contents at a later time.
#[derive(Debug, Clone)]
pub struct BundleGenerationResult {
    /// The generated bundle manifest (status = Generating initially)
    pub bundle: ForensicBundle,
    /// Integrity hashes for all collected sections
    pub integrity_hash: BundleIntegrityHash,
    /// Detailed section hashes for audit trail
    pub section_hashes: Vec<ContentSectionHash>,
}

/// Request parameters for bundle generation.
#[derive(Debug, Clone)]
pub struct GenerateBundleRequest {
    pub tenant_id: Uuid,
    pub time_range: BundleTimeRange,
    pub purpose: BundlePurpose,
    pub created_by: String,
    /// Collected intent version entries
    pub intent_versions: Vec<IntentVersionEntry>,
    /// Collected artifact entries
    pub artifacts: Vec<ArtifactEntry>,
    /// Collected approval entries
    pub approvals: Vec<ApprovalEntry>,
    /// Collected audit event entries
    pub audit_events: Vec<AuditEventEntry>,
    /// Collected policy snapshot entries
    pub policy_snapshots: Vec<PolicySnapshotEntry>,
}

/// Service for generating forensic bundles with integrity hashing.
///
/// This service provides the bounded generation path that:
/// 1. Collects explicit bundle sections/content summaries
/// 2. Computes integrity hashes for each section
/// 3. Produces a generation result ready for storage (Phase 4 scope)
pub struct BundleGeneratorService;

impl BundleGeneratorService {
    /// Generate a forensic bundle from the provided content entries.
    ///
    /// This is a bounded generation path that:
    /// - Creates a new bundle with a generated UUID
    /// - Collects and hashes all provided content sections
    /// - Computes manifest integrity hash
    /// - Returns the bundle in Generating status (caller transitions to Ready after storage)
    ///
    /// **Phase 4 scope (not in this slice):** Actual persistence, S3 storage,
    /// notification, and the full generation lifecycle management.
    pub fn generate(request: GenerateBundleRequest) -> BundleGenerationResult {
        let contents = BundleContents {
            intent_versions: request.intent_versions.len(),
            artifacts: request.artifacts.len(),
            approvals: request.approvals.len(),
            audit_events: request.audit_events.len(),
            policy_snapshots: request.policy_snapshots.len(),
        };

        // Build hash input structures
        let intent_versions_for_hash = IntentVersionsForHash {
            versions: request.intent_versions.clone(),
        };
        let artifacts_for_hash = ArtifactsForHash {
            artifacts: request.artifacts.clone(),
        };
        let approvals_for_hash = ApprovalsForHash {
            approvals: request.approvals.clone(),
        };
        let audit_events_for_hash = AuditEventsForHash {
            events: request.audit_events.clone(),
        };
        let policy_snapshots_for_hash = PolicySnapshotsForHash {
            snapshots: request.policy_snapshots.clone(),
        };

        // Compute all section hashes
        let intent_versions_hash = super::bundle_hasher::compute_sha256(&intent_versions_for_hash)
            .expect("intent_versions serialization should not fail");
        let artifacts_hash = super::bundle_hasher::compute_sha256(&artifacts_for_hash)
            .expect("artifacts serialization should not fail");
        let approvals_hash = super::bundle_hasher::compute_sha256(&approvals_for_hash)
            .expect("approvals serialization should not fail");
        let audit_events_hash = super::bundle_hasher::compute_sha256(&audit_events_for_hash)
            .expect("audit_events serialization should not fail");
        let policy_snapshots_hash =
            super::bundle_hasher::compute_sha256(&policy_snapshots_for_hash)
                .expect("policy_snapshots serialization should not fail");

        // Build integrity hash
        let integrity_hash = BundleIntegrityHash {
            manifest_hash: String::new(), // Computed below after bundle creation
            intent_versions_hash: intent_versions_hash.clone(),
            intent_versions_count: contents.intent_versions,
            artifacts_hash: artifacts_hash.clone(),
            artifacts_count: contents.artifacts,
            approvals_hash: approvals_hash.clone(),
            approvals_count: contents.approvals,
            audit_events_hash: audit_events_hash.clone(),
            audit_events_count: contents.audit_events,
            policy_snapshots_hash: policy_snapshots_hash.clone(),
            policy_snapshots_count: contents.policy_snapshots,
            computed_at: Utc::now(),
        };

        // Create bundle with pending status
        let mut bundle = ForensicBundle::new(
            request.tenant_id,
            request.time_range,
            request.purpose,
            contents,
            &request.created_by,
            None,
        );
        // Set to Generating since we have content
        bundle.status = BundleStatus::Generating;

        // Build section hash records for audit trail
        let section_hashes = vec![
            ContentSectionHash {
                section: "intent_versions".to_string(),
                content_hash: intent_versions_hash,
                item_count: bundle.contents.intent_versions,
            },
            ContentSectionHash {
                section: "artifacts".to_string(),
                content_hash: artifacts_hash,
                item_count: bundle.contents.artifacts,
            },
            ContentSectionHash {
                section: "approvals".to_string(),
                content_hash: approvals_hash,
                item_count: bundle.contents.approvals,
            },
            ContentSectionHash {
                section: "audit_events".to_string(),
                content_hash: audit_events_hash,
                item_count: bundle.contents.audit_events,
            },
            ContentSectionHash {
                section: "policy_snapshots".to_string(),
                content_hash: policy_snapshots_hash,
                item_count: bundle.contents.policy_snapshots,
            },
        ];

        // Compute manifest hash from the bundle JSON
        let manifest_hash = super::bundle_hasher::compute_sha256(&bundle)
            .expect("bundle serialization should not fail");

        // Write the computed manifest hash back into bundle.integrity and update timestamp
        let verification_timestamp = Utc::now();
        bundle.integrity.manifest_hash = manifest_hash.clone();
        bundle.integrity.verification_timestamp = verification_timestamp;

        let mut result_integrity_hash = integrity_hash;
        result_integrity_hash.manifest_hash = manifest_hash;

        BundleGenerationResult {
            bundle,
            integrity_hash: result_integrity_hash,
            section_hashes,
        }
    }

    /// Verify the integrity of a previously generated bundle against the provided content.
    ///
    /// Returns Ok(()) if all content hashes match the recorded integrity hash.
    /// Returns Err(IntegrityVerificationFailure) with details if any hash mismatches.
    pub fn verify_integrity(
        recorded_integrity: &BundleIntegrityHash,
        content_sections: &ContentSectionsForVerification,
    ) -> Result<(), super::bundle_hasher::IntegrityVerificationFailure> {
        verify_bundle_integrity(recorded_integrity, content_sections)
    }

    /// Build a ContentSectionsForVerification from collected entries.
    pub fn build_verification_sections(
        intent_versions: Vec<IntentVersionEntry>,
        artifacts: Vec<ArtifactEntry>,
        approvals: Vec<ApprovalEntry>,
        audit_events: Vec<AuditEventEntry>,
        policy_snapshots: Vec<PolicySnapshotEntry>,
    ) -> ContentSectionsForVerification {
        ContentSectionsForVerification {
            intent_versions: IntentVersionsForHash {
                versions: intent_versions,
            },
            artifacts: ArtifactsForHash { artifacts },
            approvals: ApprovalsForHash { approvals },
            audit_events: AuditEventsForHash {
                events: audit_events,
            },
            policy_snapshots: PolicySnapshotsForHash {
                snapshots: policy_snapshots,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn make_intent_entry(i: u8) -> IntentVersionEntry {
        IntentVersionEntry {
            intent_id: Uuid::from_bytes([i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            version: 1,
            content_hash: format!("{:032x}", i as u64),
        }
    }

    fn make_artifact_entry(i: u8) -> ArtifactEntry {
        ArtifactEntry {
            artifact_id: Uuid::from_bytes([
                i.wrapping_add(0x10),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]),
            content_hash: format!("{:032x}", (i as u64) + 0x100),
            collected_at: Utc::now(),
        }
    }

    fn make_approval_entry(i: u8) -> ApprovalEntry {
        ApprovalEntry {
            approval_id: Uuid::from_bytes([
                i.wrapping_add(0x20),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]),
            content_hash: format!("{:032x}", (i as u64) + 0x200),
        }
    }

    fn make_audit_entry(i: u8) -> AuditEventEntry {
        AuditEventEntry {
            event_id: Uuid::from_bytes([
                i.wrapping_add(0x30),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]),
            content_hash: format!("{:032x}", (i as u64) + 0x300),
            event_index: i as usize,
        }
    }

    fn make_policy_entry(i: u8) -> PolicySnapshotEntry {
        PolicySnapshotEntry {
            snapshot_id: Uuid::from_bytes([
                i.wrapping_add(0x40),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]),
            scope_hash: format!("{:032x}", (i as u64) + 0x400),
        }
    }

    #[test]
    fn test_generate_bundle_basic() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "test-user".to_string(),
            intent_versions: vec![make_intent_entry(1), make_intent_entry(2)],
            artifacts: vec![make_artifact_entry(1)],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result = BundleGeneratorService::generate(request.clone());

        assert_eq!(result.bundle.status, BundleStatus::Generating);
        assert_eq!(result.bundle.contents.intent_versions, 2);
        assert_eq!(result.bundle.contents.artifacts, 1);
        assert_eq!(result.bundle.contents.approvals, 0);
        assert_eq!(result.bundle.contents.audit_events, 0);
        assert_eq!(result.bundle.contents.policy_snapshots, 0);

        // Verify section hashes are populated
        assert_eq!(result.section_hashes.len(), 5);
        let intent_section = result
            .section_hashes
            .iter()
            .find(|s| s.section == "intent_versions")
            .unwrap();
        assert_eq!(intent_section.item_count, 2);
    }

    #[test]
    fn test_generate_bundle_deterministic_hash() {
        let tenant_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let time_range = BundleTimeRange {
            start: DateTime::parse_from_rfc3339("2025-04-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            end: DateTime::parse_from_rfc3339("2025-04-03T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let request1 = GenerateBundleRequest {
            tenant_id,
            time_range: time_range.clone(),
            purpose: BundlePurpose::ComplianceAudit,
            created_by: "system".to_string(),
            intent_versions: vec![make_intent_entry(1)],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result1 = BundleGeneratorService::generate(request1);

        let request2 = GenerateBundleRequest {
            tenant_id,
            time_range,
            purpose: BundlePurpose::ComplianceAudit,
            created_by: "system".to_string(),
            intent_versions: vec![make_intent_entry(1)],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result2 = BundleGeneratorService::generate(request2);

        // Same input should produce same section hashes (manifest_hash includes created_at which differs)
        assert_eq!(
            result1.integrity_hash.intent_versions_hash,
            result2.integrity_hash.intent_versions_hash,
            "Section hashes must be deterministic for identical input"
        );
        assert_eq!(
            result1.integrity_hash.artifacts_hash,
            result2.integrity_hash.artifacts_hash
        );
    }

    #[test]
    fn test_verify_bundle_integrity_success() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::Legal,
            created_by: "admin".to_string(),
            intent_versions: vec![
                make_intent_entry(1),
                make_intent_entry(2),
                make_intent_entry(3),
            ],
            artifacts: vec![make_artifact_entry(1), make_artifact_entry(2)],
            approvals: vec![make_approval_entry(1)],
            audit_events: vec![make_audit_entry(1), make_audit_entry(2)],
            policy_snapshots: vec![make_policy_entry(1)],
        };

        let result = BundleGeneratorService::generate(request.clone());

        // Build verification sections from same request
        let verification_sections = BundleGeneratorService::build_verification_sections(
            request.intent_versions.clone(),
            request.artifacts.clone(),
            request.approvals.clone(),
            request.audit_events.clone(),
            request.policy_snapshots.clone(),
        );

        let verification_result = BundleGeneratorService::verify_integrity(
            &result.integrity_hash,
            &verification_sections,
        );

        assert!(
            verification_result.is_ok(),
            "Integrity verification should succeed: {:?}",
            verification_result
        );
    }

    #[test]
    fn test_verify_bundle_integrity_tampered_content() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "tester".to_string(),
            intent_versions: vec![make_intent_entry(1)],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result = BundleGeneratorService::generate(request.clone());

        // Build verification sections with TAMPERED intent version content_hash
        let mut tampered_intent_versions = request.intent_versions.clone();
        tampered_intent_versions[0].content_hash = "tampered_content_hash_0000".to_string();

        let verification_sections = BundleGeneratorService::build_verification_sections(
            tampered_intent_versions,
            request.artifacts.clone(),
            request.approvals.clone(),
            request.audit_events.clone(),
            request.policy_snapshots.clone(),
        );

        let verification_result = BundleGeneratorService::verify_integrity(
            &result.integrity_hash,
            &verification_sections,
        );

        assert!(
            verification_result.is_err(),
            "Integrity verification should fail with tampered content"
        );
    }

    #[test]
    fn test_generate_bundle_empty_contents() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "system".to_string(),
            intent_versions: vec![],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result = BundleGeneratorService::generate(request);

        assert_eq!(result.bundle.contents.intent_versions, 0);
        assert_eq!(result.bundle.contents.artifacts, 0);
        assert_eq!(result.bundle.contents.approvals, 0);
        assert_eq!(result.bundle.contents.audit_events, 0);
        assert_eq!(result.bundle.contents.policy_snapshots, 0);

        // All section hashes should be valid even for empty content
        assert_eq!(result.section_hashes.len(), 5);
    }

    #[test]
    fn test_generate_bundle_section_hash_items_match_contents() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::ComplianceAudit,
            created_by: "auditor".to_string(),
            intent_versions: vec![
                make_intent_entry(1),
                make_intent_entry(2),
                make_intent_entry(3),
                make_intent_entry(4),
                make_intent_entry(5),
            ],
            artifacts: vec![make_artifact_entry(1), make_artifact_entry(2)],
            approvals: vec![
                make_approval_entry(1),
                make_approval_entry(2),
                make_approval_entry(3),
            ],
            audit_events: vec![make_audit_entry(1), make_audit_entry(2)],
            policy_snapshots: vec![make_policy_entry(1)],
        };

        let result = BundleGeneratorService::generate(request.clone());

        for section in &result.section_hashes {
            match section.section.as_str() {
                "intent_versions" => assert_eq!(section.item_count, 5),
                "artifacts" => assert_eq!(section.item_count, 2),
                "approvals" => assert_eq!(section.item_count, 3),
                "audit_events" => assert_eq!(section.item_count, 2),
                "policy_snapshots" => assert_eq!(section.item_count, 1),
                _ => panic!("Unexpected section: {}", section.section),
            }
        }
    }

    #[test]
    fn test_bundle_integrity_hash_all_sections_populated() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::Legal,
            created_by: "legal-team".to_string(),
            intent_versions: vec![make_intent_entry(1)],
            artifacts: vec![make_artifact_entry(1)],
            approvals: vec![make_approval_entry(1)],
            audit_events: vec![make_audit_entry(1)],
            policy_snapshots: vec![make_policy_entry(1)],
        };

        let result = BundleGeneratorService::generate(request);

        let ih = &result.integrity_hash;
        assert!(
            !ih.manifest_hash.is_empty(),
            "manifest_hash should be populated"
        );
        assert!(
            !ih.intent_versions_hash.is_empty(),
            "intent_versions_hash should be populated"
        );
        assert_eq!(ih.intent_versions_count, 1);
        assert!(
            !ih.artifacts_hash.is_empty(),
            "artifacts_hash should be populated"
        );
        assert_eq!(ih.artifacts_count, 1);
        assert!(
            !ih.approvals_hash.is_empty(),
            "approvals_hash should be populated"
        );
        assert_eq!(ih.approvals_count, 1);
        assert!(
            !ih.audit_events_hash.is_empty(),
            "audit_events_hash should be populated"
        );
        assert_eq!(ih.audit_events_count, 1);
        assert!(
            !ih.policy_snapshots_hash.is_empty(),
            "policy_snapshots_hash should be populated"
        );
        assert_eq!(ih.policy_snapshots_count, 1);
    }

    #[test]
    fn test_different_content_different_hash() {
        let request1 = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "system".to_string(),
            intent_versions: vec![make_intent_entry(1)],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result1 = BundleGeneratorService::generate(request1.clone());

        let mut request2 = request1.clone();
        request2.intent_versions = vec![make_intent_entry(2)]; // Different intent_id

        let result2 = BundleGeneratorService::generate(request2);

        assert_ne!(
            result1.integrity_hash.intent_versions_hash,
            result2.integrity_hash.intent_versions_hash,
            "Different content must produce different hashes"
        );
    }

    #[test]
    fn test_verify_integrity_empty_sections() {
        let request = GenerateBundleRequest {
            tenant_id: Uuid::new_v4(),
            time_range: BundleTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: BundlePurpose::IncidentInvestigation,
            created_by: "system".to_string(),
            intent_versions: vec![],
            artifacts: vec![],
            approvals: vec![],
            audit_events: vec![],
            policy_snapshots: vec![],
        };

        let result = BundleGeneratorService::generate(request.clone());

        let verification_sections = BundleGeneratorService::build_verification_sections(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );

        let verification_result = BundleGeneratorService::verify_integrity(
            &result.integrity_hash,
            &verification_sections,
        );

        assert!(
            verification_result.is_ok(),
            "Empty sections should verify successfully"
        );
    }
}
