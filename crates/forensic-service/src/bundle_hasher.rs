//! Integrity hashing for forensic bundles
//!
//! Provides deterministic SHA-256 hashing for bundle manifests and content sections.
//! Hashes are computed over canonical JSON serialization to ensure reproducibility.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SHA-256 hex digest (64 characters)
pub type Sha256Hex = String;

/// Compute SHA-256 hash of a serializable value using canonical JSON serialization.
///
/// Uses `serde_json::to_string` which produces deterministic output for
/// equivalent Rust values (map iteration order is preserved; callers whose
/// types require sorted keys must ensure that themselves).
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn compute_sha256<T: ?Sized>(value: &T) -> Result<Sha256Hex, serde_json::Error>
where
    T: serde::Serialize,
{
    let json = serde_json::to_string(value)?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Content section hash entries recorded in the bundle integrity manifest.
///
/// Each entry records the hash of a collected content section (e.g., intent versions,
/// audit events) so that later verification can confirm the section was not modified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSectionHash {
    /// Logical name of the section (e.g., "intent_versions", "audit_events")
    pub section: String,
    /// SHA-256 hex digest of the canonical JSON serialization of the section content
    pub content_hash: Sha256Hex,
    /// Number of items in this section
    pub item_count: usize,
}

/// Complete integrity hash for a forensic bundle.
///
/// Records hashes of all collected sections plus the manifest hash so that
/// a verification pass can confirm no section was tampered with after generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleIntegrityHash {
    /// SHA-256 hash of the entire manifest JSON (all fields including integrity hash itself
    /// at the moment of generation — callers must be aware this is self-referential and
    /// the hash is recorded in the manifest before computation)
    pub manifest_hash: Sha256Hex,
    /// Hash of intent versions section
    pub intent_versions_hash: Sha256Hex,
    /// Number of intent version records collected
    pub intent_versions_count: usize,
    /// Hash of artifact metadata section
    pub artifacts_hash: Sha256Hex,
    /// Number of artifact records collected
    pub artifacts_count: usize,
    /// Hash of approval records section
    pub approvals_hash: Sha256Hex,
    /// Number of approval records collected
    pub approvals_count: usize,
    /// Hash of audit events section
    pub audit_events_hash: Sha256Hex,
    /// Number of audit events collected
    pub audit_events_count: usize,
    /// Hash of policy snapshots section
    pub policy_snapshots_hash: Sha256Hex,
    /// Number of policy snapshots collected
    pub policy_snapshots_count: usize,
    /// When these hashes were computed
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

/// Input data structure for hashing intent versions collected in a bundle.
///
/// Each entry is a tuple of (intent_id, version_number, content_hash) that uniquely
/// identifies an intent version that will appear in the bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntentVersionsForHash {
    pub versions: Vec<IntentVersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentVersionEntry {
    pub intent_id: uuid::Uuid,
    pub version: i32,
    pub content_hash: Sha256Hex,
}

/// Input data structure for hashing artifact metadata collected in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtifactsForHash {
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub artifact_id: uuid::Uuid,
    pub content_hash: Sha256Hex,
    pub collected_at: chrono::DateTime<chrono::Utc>,
}

/// Input data structure for hashing approval records collected in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalsForHash {
    pub approvals: Vec<ApprovalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalEntry {
    pub approval_id: uuid::Uuid,
    pub content_hash: Sha256Hex,
}

/// Input data structure for hashing audit events collected in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditEventsForHash {
    pub events: Vec<AuditEventEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventEntry {
    pub event_id: uuid::Uuid,
    pub content_hash: Sha256Hex,
    pub event_index: usize,
}

/// Input data structure for hashing policy snapshots collected in a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicySnapshotsForHash {
    pub snapshots: Vec<PolicySnapshotEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshotEntry {
    pub snapshot_id: uuid::Uuid,
    pub scope_hash: Sha256Hex,
}

/// Verify a bundle integrity hash by recomputing all section hashes and comparing.
///
/// Returns Ok(()) if all hashes match, Err(VerificationFailure) with details otherwise.
pub fn verify_bundle_integrity(
    computed: &BundleIntegrityHash,
    sections: &ContentSectionsForVerification,
) -> Result<(), IntegrityVerificationFailure> {
    let mut failures = Vec::new();

    // Recompute each section hash; serialization failures are treated as verification failures
    let intent_hash = match compute_sha256(&sections.intent_versions) {
        Ok(h) => h,
        Err(e) => {
            failures.push(format!("intent_versions serialization failed: {}", e));
            String::new()
        }
    };
    if intent_hash != computed.intent_versions_hash {
        failures.push(format!(
            "intent_versions hash mismatch: expected {}, got {}",
            computed.intent_versions_hash, intent_hash
        ));
    }

    let artifacts_hash = match compute_sha256(&sections.artifacts) {
        Ok(h) => h,
        Err(e) => {
            failures.push(format!("artifacts serialization failed: {}", e));
            String::new()
        }
    };
    if artifacts_hash != computed.artifacts_hash {
        failures.push(format!(
            "artifacts hash mismatch: expected {}, got {}",
            computed.artifacts_hash, artifacts_hash
        ));
    }

    let approvals_hash = match compute_sha256(&sections.approvals) {
        Ok(h) => h,
        Err(e) => {
            failures.push(format!("approvals serialization failed: {}", e));
            String::new()
        }
    };
    if approvals_hash != computed.approvals_hash {
        failures.push(format!(
            "approvals hash mismatch: expected {}, got {}",
            computed.approvals_hash, approvals_hash
        ));
    }

    let audit_events_hash = match compute_sha256(&sections.audit_events) {
        Ok(h) => h,
        Err(e) => {
            failures.push(format!("audit_events serialization failed: {}", e));
            String::new()
        }
    };
    if audit_events_hash != computed.audit_events_hash {
        failures.push(format!(
            "audit_events hash mismatch: expected {}, got {}",
            computed.audit_events_hash, audit_events_hash
        ));
    }

    let policy_hash = match compute_sha256(&sections.policy_snapshots) {
        Ok(h) => h,
        Err(e) => {
            failures.push(format!("policy_snapshots serialization failed: {}", e));
            String::new()
        }
    };
    if policy_hash != computed.policy_snapshots_hash {
        failures.push(format!(
            "policy_snapshots hash mismatch: expected {}, got {}",
            computed.policy_snapshots_hash, policy_hash
        ));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(IntegrityVerificationFailure { failures })
    }
}

/// All content sections provided for integrity verification.
#[derive(Debug, Clone, Default)]
pub struct ContentSectionsForVerification {
    pub intent_versions: IntentVersionsForHash,
    pub artifacts: ArtifactsForHash,
    pub approvals: ApprovalsForHash,
    pub audit_events: AuditEventsForHash,
    pub policy_snapshots: PolicySnapshotsForHash,
}

/// Verification failure with details of each hash mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityVerificationFailure {
    pub failures: Vec<String>,
}

impl std::fmt::Display for IntegrityVerificationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Integrity verification failed: ")?;
        for (i, failure) in self.failures.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}", failure)?;
        }
        Ok(())
    }
}

impl std::error::Error for IntegrityVerificationFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256_deterministic() {
        let data = IntentVersionsForHash {
            versions: vec![
                IntentVersionEntry {
                    intent_id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
                        .unwrap(),
                    version: 1,
                    content_hash: "abc123".to_string(),
                },
                IntentVersionEntry {
                    intent_id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001")
                        .unwrap(),
                    version: 2,
                    content_hash: "def456".to_string(),
                },
            ],
        };

        let hash1 = compute_sha256(&data).unwrap();
        let hash2 = compute_sha256(&data).unwrap();
        assert_eq!(hash1, hash2, "SHA-256 must be deterministic");
        assert_eq!(hash1.len(), 64, "SHA-256 hex is 64 characters");
    }

    #[test]
    fn test_compute_sha256_different_input_different_hash() {
        let data1 = IntentVersionsForHash {
            versions: vec![IntentVersionEntry {
                intent_id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                version: 1,
                content_hash: "abc123".to_string(),
            }],
        };
        let data2 = IntentVersionsForHash {
            versions: vec![IntentVersionEntry {
                intent_id: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
                version: 2,
                content_hash: "abc123".to_string(),
            }],
        };

        let hash1 = compute_sha256(&data1).unwrap();
        let hash2 = compute_sha256(&data2).unwrap();
        assert_ne!(
            hash1, hash2,
            "Different inputs must produce different hashes"
        );
    }

    #[test]
    fn test_bundle_integrity_hash_computation() {
        let sections = ContentSectionsForVerification {
            intent_versions: IntentVersionsForHash {
                versions: vec![IntentVersionEntry {
                    intent_id: uuid::Uuid::new_v4(),
                    version: 1,
                    content_hash: "a".repeat(64),
                }],
            },
            artifacts: ArtifactsForHash {
                artifacts: vec![ArtifactEntry {
                    artifact_id: uuid::Uuid::new_v4(),
                    content_hash: "b".repeat(64),
                    collected_at: chrono::Utc::now(),
                }],
            },
            approvals: ApprovalsForHash {
                approvals: vec![ApprovalEntry {
                    approval_id: uuid::Uuid::new_v4(),
                    content_hash: "c".repeat(64),
                }],
            },
            audit_events: AuditEventsForHash {
                events: vec![AuditEventEntry {
                    event_id: uuid::Uuid::new_v4(),
                    content_hash: "d".repeat(64),
                    event_index: 0,
                }],
            },
            policy_snapshots: PolicySnapshotsForHash {
                snapshots: vec![PolicySnapshotEntry {
                    snapshot_id: uuid::Uuid::new_v4(),
                    scope_hash: "e".repeat(64),
                }],
            },
        };

        let computed = BundleIntegrityHash {
            manifest_hash: "f".repeat(64),
            intent_versions_hash: compute_sha256(&sections.intent_versions).unwrap(),
            intent_versions_count: sections.intent_versions.versions.len(),
            artifacts_hash: compute_sha256(&sections.artifacts).unwrap(),
            artifacts_count: sections.artifacts.artifacts.len(),
            approvals_hash: compute_sha256(&sections.approvals).unwrap(),
            approvals_count: sections.approvals.approvals.len(),
            audit_events_hash: compute_sha256(&sections.audit_events).unwrap(),
            audit_events_count: sections.audit_events.events.len(),
            policy_snapshots_hash: compute_sha256(&sections.policy_snapshots).unwrap(),
            policy_snapshots_count: sections.policy_snapshots.snapshots.len(),
            computed_at: chrono::Utc::now(),
        };

        // Verification should pass
        let result = verify_bundle_integrity(&computed, &sections);
        assert!(result.is_ok(), "Verification should pass: {:?}", result);

        // Tamper with one hash — verification must fail
        let mut tampered = computed;
        tampered.intent_versions_hash = "tampered".to_string();
        let result = verify_bundle_integrity(&tampered, &sections);
        assert!(result.is_err(), "Verification should fail after tampering");
    }

    #[test]
    fn test_verify_bundle_integrity_all_sections() {
        // Verify that all 5 sections contribute to integrity
        let mut all_sections = ContentSectionsForVerification::default();

        all_sections.intent_versions = IntentVersionsForHash {
            versions: vec![IntentVersionEntry {
                intent_id: uuid::Uuid::new_v4(),
                version: 1,
                content_hash: "h1".to_string(),
            }],
        };
        all_sections.artifacts = ArtifactsForHash {
            artifacts: vec![ArtifactEntry {
                artifact_id: uuid::Uuid::new_v4(),
                content_hash: "h2".to_string(),
                collected_at: chrono::Utc::now(),
            }],
        };
        all_sections.approvals = ApprovalsForHash {
            approvals: vec![ApprovalEntry {
                approval_id: uuid::Uuid::new_v4(),
                content_hash: "h3".to_string(),
            }],
        };
        all_sections.audit_events = AuditEventsForHash {
            events: vec![AuditEventEntry {
                event_id: uuid::Uuid::new_v4(),
                content_hash: "h4".to_string(),
                event_index: 0,
            }],
        };
        all_sections.policy_snapshots = PolicySnapshotsForHash {
            snapshots: vec![PolicySnapshotEntry {
                snapshot_id: uuid::Uuid::new_v4(),
                scope_hash: "h5".to_string(),
            }],
        };

        let computed = BundleIntegrityHash {
            manifest_hash: "manifest".to_string(),
            intent_versions_hash: compute_sha256(&all_sections.intent_versions).unwrap(),
            intent_versions_count: 1,
            artifacts_hash: compute_sha256(&all_sections.artifacts).unwrap(),
            artifacts_count: 1,
            approvals_hash: compute_sha256(&all_sections.approvals).unwrap(),
            approvals_count: 1,
            audit_events_hash: compute_sha256(&all_sections.audit_events).unwrap(),
            audit_events_count: 1,
            policy_snapshots_hash: compute_sha256(&all_sections.policy_snapshots).unwrap(),
            policy_snapshots_count: 1,
            computed_at: chrono::Utc::now(),
        };

        let result = verify_bundle_integrity(&computed, &all_sections);
        assert!(result.is_ok(), "All sections must verify successfully");
    }

    #[test]
    fn test_sha256_format() {
        let data = ("test", 42);
        let hash = compute_sha256(&data).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA-256 hex should only contain hex characters"
        );
    }

    #[test]
    fn test_empty_sections_hashing() {
        let empty_sections = ContentSectionsForVerification::default();
        let hash = compute_sha256(&empty_sections.intent_versions).unwrap();
        assert_eq!(hash.len(), 64);

        let hash = compute_sha256(&empty_sections.artifacts).unwrap();
        assert_eq!(hash.len(), 64);

        let hash = compute_sha256(&empty_sections.approvals).unwrap();
        assert_eq!(hash.len(), 64);

        let hash = compute_sha256(&empty_sections.audit_events).unwrap();
        assert_eq!(hash.len(), 64);

        let hash = compute_sha256(&empty_sections.policy_snapshots).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_content_section_hash_serialization() {
        let section_hash = ContentSectionHash {
            section: "intent_versions".to_string(),
            content_hash: "abc123def456".to_string(),
            item_count: 5,
        };

        let json = serde_json::to_string(&section_hash).unwrap();
        let deserialized: ContentSectionHash = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.section, "intent_versions");
        assert_eq!(deserialized.content_hash, "abc123def456");
        assert_eq!(deserialized.item_count, 5);
    }

    #[test]
    fn test_integrity_verification_failure_message() {
        let failure = IntegrityVerificationFailure {
            failures: vec![
                "intent_versions hash mismatch".to_string(),
                "artifacts hash mismatch".to_string(),
            ],
        };

        let msg = failure.to_string();
        assert!(msg.contains("intent_versions"));
        assert!(msg.contains("artifacts"));
        assert!(msg.contains("Integrity verification failed"));
    }

    #[test]
    fn test_multiple_sections_tamper_detection() {
        let mut sections = ContentSectionsForVerification::default();
        sections.intent_versions = IntentVersionsForHash {
            versions: vec![IntentVersionEntry {
                intent_id: uuid::Uuid::new_v4(),
                version: 1,
                content_hash: "correct".to_string(),
            }],
        };

        let correct_hash = compute_sha256(&sections.intent_versions).unwrap();

        let computed = BundleIntegrityHash {
            manifest_hash: "manifest".to_string(),
            intent_versions_hash: correct_hash,
            intent_versions_count: 1,
            artifacts_hash: compute_sha256(&sections.artifacts).unwrap(),
            artifacts_count: 0,
            approvals_hash: compute_sha256(&sections.approvals).unwrap(),
            approvals_count: 0,
            audit_events_hash: compute_sha256(&sections.audit_events).unwrap(),
            audit_events_count: 0,
            policy_snapshots_hash: compute_sha256(&sections.policy_snapshots).unwrap(),
            policy_snapshots_count: 0,
            computed_at: chrono::Utc::now(),
        };

        // Clean verification
        assert!(verify_bundle_integrity(&computed, &sections).is_ok());

        // Tamper with content
        sections.intent_versions.versions[0].content_hash = "tampered".to_string();

        // Verification must fail
        let result = verify_bundle_integrity(&computed, &sections);
        assert!(result.is_err());
    }
}
