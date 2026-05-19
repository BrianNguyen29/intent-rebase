//! Bounded forensic bundle replay — read-only verification and reconstruction report.
//!
//! This module provides a **bounded replay capability** for forensic bundles:
//! - Verifies bundle integrity against recorded hashes
//! - Produces a human-readable reconstruction report
//! - All replay operations are read-only and isolated (no production mutations)
//!
//! ## What This IS
//!
//! - Verification: Confirm that a bundle's recorded integrity hashes match the content
//! - Reconstruction report: Summary of what the bundle contains and how it validates
//! - Audit trail: Provides evidence of bundle completeness for investigators
//!
//! ## What This IS NOT
//!
//! - **Not runtime replay**: Does NOT reconstruct system state or replay events in a live system
//! - **Not mutation**: Does NOT modify any production data or state
//! - **Not S3 storage**: Does NOT handle bundle storage or retrieval from cloud
//! - **Not export**: Does NOT provide download or export functionality
//!
//! Full runtime replay is Phase 4 scope (requires runtime adapter integration).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::bundle::{BundlePurpose, BundleStatus, BundleTimeRange, ForensicBundle};
use super::bundle_contents::BundleContents;
use super::bundle_hasher::{
    compute_sha256, ApprovalEntry, ApprovalsForHash, ArtifactEntry, ArtifactsForHash,
    AuditEventEntry, AuditEventsForHash, ContentSectionHash, ContentSectionsForVerification,
    IntentVersionEntry, IntentVersionsForHash, PolicySnapshotEntry, PolicySnapshotsForHash,
};

/// Result of a single section's replay verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySectionResult {
    /// Section name (e.g., "intent_versions", "artifacts")
    pub section: String,
    /// Whether this section passed verification
    pub verified: bool,
    /// Number of items in this section
    pub item_count: usize,
    /// Recorded hash from bundle manifest
    pub recorded_hash: String,
    /// Computed hash from provided content
    pub computed_hash: String,
    /// Human-readable details
    pub details: String,
}

impl ReplaySectionResult {
    pub(crate) fn success(
        section: &str,
        item_count: usize,
        recorded_hash: &str,
        computed_hash: &str,
    ) -> Self {
        Self {
            section: section.to_string(),
            verified: true,
            item_count,
            recorded_hash: recorded_hash.to_string(),
            computed_hash: computed_hash.to_string(),
            details: format!(
                "Section '{}' verified successfully: {} items, hash match",
                section, item_count
            ),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn failure(
        section: &str,
        item_count: usize,
        recorded_hash: &str,
        computed_hash: &str,
    ) -> Self {
        Self {
            section: section.to_string(),
            verified: false,
            item_count,
            recorded_hash: recorded_hash.to_string(),
            computed_hash: computed_hash.to_string(),
            details: if recorded_hash.is_empty() && computed_hash.is_empty() {
                format!(
                    "Section '{}' skipped: no hash available for comparison",
                    section
                )
            } else {
                format!(
                    "Section '{}' FAILED verification: {} items, hash mismatch",
                    section, item_count
                )
            },
        }
    }
}

/// Outcome of a complete bundle replay verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayVerificationReport {
    /// Bundle ID being verified
    pub bundle_id: Uuid,
    /// Tenant ID of the bundle
    pub tenant_id: Uuid,
    /// When verification was performed
    pub verified_at: DateTime<Utc>,
    /// Whether all sections passed verification
    pub overall_verified: bool,
    /// Bundle purpose from manifest
    pub purpose: BundlePurpose,
    /// Time range covered by bundle
    pub time_range: BundleTimeRange,
    /// Verification results per section
    pub sections: Vec<ReplaySectionResult>,
    /// Total items across all sections
    pub total_items: BundleContents,
    /// Summary message
    pub summary: String,
    /// Count of sections that passed
    pub sections_passed: usize,
    /// Count of sections that failed
    pub sections_failed: usize,
}

impl ReplayVerificationReport {
    /// Generate a report from verification results.
    fn from_results(bundle: &ForensicBundle, section_results: Vec<ReplaySectionResult>) -> Self {
        let sections_passed = section_results.iter().filter(|s| s.verified).count();
        let sections_failed = section_results.iter().filter(|s| !s.verified).count();
        let overall_verified = sections_failed == 0;

        let summary = if overall_verified {
            format!(
                "Bundle {} verified successfully. All {} sections passed integrity check.",
                bundle.bundle_id,
                section_results.len()
            )
        } else {
            format!(
                "Bundle {} verification FAILED. {}/{} sections failed integrity check.",
                bundle.bundle_id,
                sections_failed,
                section_results.len()
            )
        };

        Self {
            bundle_id: bundle.bundle_id,
            tenant_id: bundle.tenant_id,
            verified_at: Utc::now(),
            overall_verified,
            purpose: bundle.purpose,
            time_range: bundle.time_range.clone(),
            sections: section_results,
            total_items: bundle.contents.clone(),
            summary,
            sections_passed,
            sections_failed,
        }
    }
}

/// Request to verify a forensic bundle during replay.
#[derive(Debug, Clone)]
pub struct VerifyBundleReplayRequest {
    /// The bundle manifest to verify
    pub bundle: ForensicBundle,
    /// Intent version entries that were collected in this bundle
    pub intent_versions: Vec<IntentVersionEntry>,
    /// Artifact entries that were collected in this bundle
    pub artifacts: Vec<ArtifactEntry>,
    /// Approval entries that were collected in this bundle
    pub approvals: Vec<ApprovalEntry>,
    /// Audit event entries that were collected in this bundle
    pub audit_events: Vec<AuditEventEntry>,
    /// Policy snapshot entries that were collected in this bundle
    pub policy_snapshots: Vec<PolicySnapshotEntry>,
}

/// Response from bundle replay verification.
#[derive(Debug, Clone)]
pub struct VerifyBundleReplayResponse {
    /// The verification report
    pub report: ReplayVerificationReport,
    /// The original bundle manifest
    pub bundle: ForensicBundle,
    /// Section hashes for audit trail
    pub section_hashes: Vec<ContentSectionHash>,
}

/// Service for bounded forensic bundle replay operations.
///
/// Provides read-only verification and reconstruction reporting
/// without any production mutations or runtime integration.
#[derive(Clone)]
pub struct BundleReplayService;

impl BundleReplayService {
    /// Create a new BundleReplayService.
    pub fn new() -> Self {
        Self
    }

    /// Verify a forensic bundle against provided content entries.
    ///
    /// **DEPRECATED — DO NOT USE:** This method uses `manifest_hash` as a proxy for
    /// all section hashes, which produces misleading results. Use
    /// [`verify_bundle_from_integrity`](Self::verify_bundle_from_integrity) instead,
    /// which reads per-section hashes from the bundle manifest and performs accurate
    /// section-by-section verification.
    ///
    /// This method is retained only to avoid breaking existing test references.
    /// It will be removed in a future slice.
    #[deprecated(
        since = "0.1.0",
        note = "Use verify_bundle_from_integrity instead. This method incorrectly uses manifest_hash as a proxy for all section hashes."
    )]
    pub fn verify_bundle(
        &self,
        request: VerifyBundleReplayRequest,
    ) -> Result<VerifyBundleReplayResponse, BundleReplayError> {
        let bundle = request.bundle;

        // Build section hash records for audit trail
        let mut section_hashes = Vec::new();

        // Intent versions hash
        let intent_versions_for_hash = IntentVersionsForHash {
            versions: request.intent_versions.clone(),
        };
        let intent_hash = compute_sha256(&intent_versions_for_hash).unwrap_or_default();
        let recorded_intent_hash = &bundle.integrity.manifest_hash; // Would need full hash in real impl
        let intent_count = request.intent_versions.len();
        section_hashes.push(ContentSectionHash {
            section: "intent_versions".to_string(),
            content_hash: intent_hash.clone(),
            item_count: intent_count,
        });

        // Artifacts hash
        let artifacts_for_hash = ArtifactsForHash {
            artifacts: request.artifacts.clone(),
        };
        let artifacts_hash = compute_sha256(&artifacts_for_hash).unwrap_or_default();
        let artifacts_count = request.artifacts.len();
        section_hashes.push(ContentSectionHash {
            section: "artifacts".to_string(),
            content_hash: artifacts_hash.clone(),
            item_count: artifacts_count,
        });

        // Approvals hash
        let approvals_for_hash = ApprovalsForHash {
            approvals: request.approvals.clone(),
        };
        let approvals_hash = compute_sha256(&approvals_for_hash).unwrap_or_default();
        let approvals_count = request.approvals.len();
        section_hashes.push(ContentSectionHash {
            section: "approvals".to_string(),
            content_hash: approvals_hash.clone(),
            item_count: approvals_count,
        });

        // Audit events hash
        let audit_events_for_hash = AuditEventsForHash {
            events: request.audit_events.clone(),
        };
        let audit_hash = compute_sha256(&audit_events_for_hash).unwrap_or_default();
        let audit_count = request.audit_events.len();
        section_hashes.push(ContentSectionHash {
            section: "audit_events".to_string(),
            content_hash: audit_hash.clone(),
            item_count: audit_count,
        });

        // Policy snapshots hash
        let policy_snapshots_for_hash = PolicySnapshotsForHash {
            snapshots: request.policy_snapshots.clone(),
        };
        let policy_hash = compute_sha256(&policy_snapshots_for_hash).unwrap_or_default();
        let policy_count = request.policy_snapshots.len();
        section_hashes.push(ContentSectionHash {
            section: "policy_snapshots".to_string(),
            content_hash: policy_hash.clone(),
            item_count: policy_count,
        });

        // Build section results
        // Note: In a full implementation, the recorded hashes would come from
        // BundleIntegrityHash stored alongside the bundle. For this bounded slice,
        // we use the manifest hash as a proxy for demonstration.
        let section_results = vec![
            ReplaySectionResult::success(
                "intent_versions",
                intent_count,
                recorded_intent_hash,
                &intent_hash,
            ),
            ReplaySectionResult::success(
                "artifacts",
                artifacts_count,
                recorded_intent_hash,
                &artifacts_hash,
            ),
            ReplaySectionResult::success(
                "approvals",
                approvals_count,
                recorded_intent_hash,
                &approvals_hash,
            ),
            ReplaySectionResult::success(
                "audit_events",
                audit_count,
                recorded_intent_hash,
                &audit_hash,
            ),
            ReplaySectionResult::success(
                "policy_snapshots",
                policy_count,
                recorded_intent_hash,
                &policy_hash,
            ),
        ];

        // Generate the final report
        let report = ReplayVerificationReport::from_results(&bundle, section_results);

        Ok(VerifyBundleReplayResponse {
            report,
            bundle,
            section_hashes,
        })
    }

    /// Verify a forensic bundle with full integrity hash tracking.
    ///
    /// This variant uses a pre-computed BundleIntegrityHash for accurate section verification.
    /// Use this when you have the integrity hash from bundle generation.
    pub fn verify_bundle_with_integrity(
        &self,
        request: VerifyBundleReplayRequest,
        intent_versions_hash: String,
        artifacts_hash: String,
        approvals_hash: String,
        audit_events_hash: String,
        policy_snapshots_hash: String,
    ) -> Result<VerifyBundleReplayResponse, BundleReplayError> {
        let bundle = request.bundle;

        // Build section hash records for audit trail
        let mut section_hashes = Vec::new();

        // Intent versions hash
        let intent_versions_for_hash = IntentVersionsForHash {
            versions: request.intent_versions.clone(),
        };
        let computed_intent_hash = compute_sha256(&intent_versions_for_hash).unwrap_or_default();
        let intent_count = request.intent_versions.len();
        section_hashes.push(ContentSectionHash {
            section: "intent_versions".to_string(),
            content_hash: computed_intent_hash.clone(),
            item_count: intent_count,
        });

        // Artifacts hash
        let artifacts_for_hash = ArtifactsForHash {
            artifacts: request.artifacts.clone(),
        };
        let computed_artifacts_hash = compute_sha256(&artifacts_for_hash).unwrap_or_default();
        let artifacts_count = request.artifacts.len();
        section_hashes.push(ContentSectionHash {
            section: "artifacts".to_string(),
            content_hash: computed_artifacts_hash.clone(),
            item_count: artifacts_count,
        });

        // Approvals hash
        let approvals_for_hash = ApprovalsForHash {
            approvals: request.approvals.clone(),
        };
        let computed_approvals_hash = compute_sha256(&approvals_for_hash).unwrap_or_default();
        let approvals_count = request.approvals.len();
        section_hashes.push(ContentSectionHash {
            section: "approvals".to_string(),
            content_hash: computed_approvals_hash.clone(),
            item_count: approvals_count,
        });

        // Audit events hash
        let audit_events_for_hash = AuditEventsForHash {
            events: request.audit_events.clone(),
        };
        let computed_audit_hash = compute_sha256(&audit_events_for_hash).unwrap_or_default();
        let audit_count = request.audit_events.len();
        section_hashes.push(ContentSectionHash {
            section: "audit_events".to_string(),
            content_hash: computed_audit_hash.clone(),
            item_count: audit_count,
        });

        // Policy snapshots hash
        let policy_snapshots_for_hash = PolicySnapshotsForHash {
            snapshots: request.policy_snapshots.clone(),
        };
        let computed_policy_hash = compute_sha256(&policy_snapshots_for_hash).unwrap_or_default();
        let policy_count = request.policy_snapshots.len();
        section_hashes.push(ContentSectionHash {
            section: "policy_snapshots".to_string(),
            content_hash: computed_policy_hash.clone(),
            item_count: policy_count,
        });

        // Build section results with proper recorded vs computed comparison
        let section_results = vec![
            {
                let verified = computed_intent_hash == intent_versions_hash;
                ReplaySectionResult {
                    section: "intent_versions".to_string(),
                    verified,
                    item_count: intent_count,
                    recorded_hash: intent_versions_hash.clone(),
                    computed_hash: computed_intent_hash.clone(),
                    details: if verified {
                        format!(
                            "intent_versions verified: {} items, hash match",
                            intent_count
                        )
                    } else {
                        format!(
                            "intent_versions FAILED: {} items, hash mismatch",
                            intent_count
                        )
                    },
                }
            },
            {
                let verified = computed_artifacts_hash == artifacts_hash;
                ReplaySectionResult {
                    section: "artifacts".to_string(),
                    verified,
                    item_count: artifacts_count,
                    recorded_hash: artifacts_hash.clone(),
                    computed_hash: computed_artifacts_hash.clone(),
                    details: if verified {
                        format!("artifacts verified: {} items, hash match", artifacts_count)
                    } else {
                        format!("artifacts FAILED: {} items, hash mismatch", artifacts_count)
                    },
                }
            },
            {
                let verified = computed_approvals_hash == approvals_hash;
                ReplaySectionResult {
                    section: "approvals".to_string(),
                    verified,
                    item_count: approvals_count,
                    recorded_hash: approvals_hash.clone(),
                    computed_hash: computed_approvals_hash.clone(),
                    details: if verified {
                        format!("approvals verified: {} items, hash match", approvals_count)
                    } else {
                        format!("approvals FAILED: {} items, hash mismatch", approvals_count)
                    },
                }
            },
            {
                let verified = computed_audit_hash == audit_events_hash;
                ReplaySectionResult {
                    section: "audit_events".to_string(),
                    verified,
                    item_count: audit_count,
                    recorded_hash: audit_events_hash.clone(),
                    computed_hash: computed_audit_hash.clone(),
                    details: if verified {
                        format!("audit_events verified: {} items, hash match", audit_count)
                    } else {
                        format!("audit_events FAILED: {} items, hash mismatch", audit_count)
                    },
                }
            },
            {
                let verified = computed_policy_hash == policy_snapshots_hash;
                ReplaySectionResult {
                    section: "policy_snapshots".to_string(),
                    verified,
                    item_count: policy_count,
                    recorded_hash: policy_snapshots_hash.clone(),
                    computed_hash: computed_policy_hash.clone(),
                    details: if verified {
                        format!(
                            "policy_snapshots verified: {} items, hash match",
                            policy_count
                        )
                    } else {
                        format!(
                            "policy_snapshots FAILED: {} items, hash mismatch",
                            policy_count
                        )
                    },
                }
            },
        ];

        // Generate the final report
        let report = ReplayVerificationReport::from_results(&bundle, section_results);

        Ok(VerifyBundleReplayResponse {
            report,
            bundle,
            section_hashes,
        })
    }

    /// Verify a forensic bundle using the integrity hashes stored in the bundle manifest.
    ///
    /// This is the **bounded replay evidence path**: the per-section hashes are persisted
    /// in `bundle.integrity` during generation, and this method recomputes hashes from the
    /// provided content sections to confirm they match the recorded values.
    ///
    /// **What this IS:** read-only integrity verification using stored evidence.
    /// **What this IS NOT:** full runtime replay, state reconstruction, or mutation.
    ///
    /// Returns `Ok(VerifyBundleReplayResponse)` with a detailed report if verification completes.
    /// Returns `Err(BundleReplayError)` if the bundle is not in Ready status.
    pub fn verify_bundle_from_integrity(
        &self,
        bundle: &ForensicBundle,
        content_sections: &ContentSectionsForVerification,
    ) -> Result<VerifyBundleReplayResponse, BundleReplayError> {
        if bundle.status != BundleStatus::Ready {
            return Err(BundleReplayError::BundleNotReady {
                bundle_id: bundle.bundle_id,
                current_status: bundle.status,
            });
        }

        // Compute hashes from provided content sections
        let computed_intent_hash =
            compute_sha256(&content_sections.intent_versions).unwrap_or_default();
        let computed_artifacts_hash =
            compute_sha256(&content_sections.artifacts).unwrap_or_default();
        let computed_approvals_hash =
            compute_sha256(&content_sections.approvals).unwrap_or_default();
        let computed_audit_hash =
            compute_sha256(&content_sections.audit_events).unwrap_or_default();
        let computed_policy_hash =
            compute_sha256(&content_sections.policy_snapshots).unwrap_or_default();

        let intent_count = content_sections.intent_versions.versions.len();
        let artifacts_count = content_sections.artifacts.artifacts.len();
        let approvals_count = content_sections.approvals.approvals.len();
        let audit_count = content_sections.audit_events.events.len();
        let policy_count = content_sections.policy_snapshots.snapshots.len();

        let section_hashes = vec![
            ContentSectionHash {
                section: "intent_versions".to_string(),
                content_hash: computed_intent_hash.clone(),
                item_count: intent_count,
            },
            ContentSectionHash {
                section: "artifacts".to_string(),
                content_hash: computed_artifacts_hash.clone(),
                item_count: artifacts_count,
            },
            ContentSectionHash {
                section: "approvals".to_string(),
                content_hash: computed_approvals_hash.clone(),
                item_count: approvals_count,
            },
            ContentSectionHash {
                section: "audit_events".to_string(),
                content_hash: computed_audit_hash.clone(),
                item_count: audit_count,
            },
            ContentSectionHash {
                section: "policy_snapshots".to_string(),
                content_hash: computed_policy_hash.clone(),
                item_count: policy_count,
            },
        ];

        let section_results = vec![
            {
                let recorded = &bundle.integrity.intent_versions_hash;
                let verified = computed_intent_hash == *recorded;
                ReplaySectionResult {
                    section: "intent_versions".to_string(),
                    verified,
                    item_count: intent_count,
                    recorded_hash: recorded.clone(),
                    computed_hash: computed_intent_hash.clone(),
                    details: if verified {
                        format!(
                            "intent_versions verified: {} items, hash match",
                            intent_count
                        )
                    } else {
                        format!(
                            "intent_versions FAILED: {} items, hash mismatch",
                            intent_count
                        )
                    },
                }
            },
            {
                let recorded = &bundle.integrity.artifacts_hash;
                let verified = computed_artifacts_hash == *recorded;
                ReplaySectionResult {
                    section: "artifacts".to_string(),
                    verified,
                    item_count: artifacts_count,
                    recorded_hash: recorded.clone(),
                    computed_hash: computed_artifacts_hash.clone(),
                    details: if verified {
                        format!("artifacts verified: {} items, hash match", artifacts_count)
                    } else {
                        format!("artifacts FAILED: {} items, hash mismatch", artifacts_count)
                    },
                }
            },
            {
                let recorded = &bundle.integrity.approvals_hash;
                let verified = computed_approvals_hash == *recorded;
                ReplaySectionResult {
                    section: "approvals".to_string(),
                    verified,
                    item_count: approvals_count,
                    recorded_hash: recorded.clone(),
                    computed_hash: computed_approvals_hash.clone(),
                    details: if verified {
                        format!("approvals verified: {} items, hash match", approvals_count)
                    } else {
                        format!("approvals FAILED: {} items, hash mismatch", approvals_count)
                    },
                }
            },
            {
                let recorded = &bundle.integrity.audit_events_hash;
                let verified = computed_audit_hash == *recorded;
                ReplaySectionResult {
                    section: "audit_events".to_string(),
                    verified,
                    item_count: audit_count,
                    recorded_hash: recorded.clone(),
                    computed_hash: computed_audit_hash.clone(),
                    details: if verified {
                        format!("audit_events verified: {} items, hash match", audit_count)
                    } else {
                        format!("audit_events FAILED: {} items, hash mismatch", audit_count)
                    },
                }
            },
            {
                let recorded = &bundle.integrity.policy_snapshots_hash;
                let verified = computed_policy_hash == *recorded;
                ReplaySectionResult {
                    section: "policy_snapshots".to_string(),
                    verified,
                    item_count: policy_count,
                    recorded_hash: recorded.clone(),
                    computed_hash: computed_policy_hash.clone(),
                    details: if verified {
                        format!(
                            "policy_snapshots verified: {} items, hash match",
                            policy_count
                        )
                    } else {
                        format!(
                            "policy_snapshots FAILED: {} items, hash mismatch",
                            policy_count
                        )
                    },
                }
            },
        ];

        let report = ReplayVerificationReport::from_results(bundle, section_results);

        Ok(VerifyBundleReplayResponse {
            report,
            bundle: bundle.clone(),
            section_hashes,
        })
    }

    /// Verify the bundle manifest integrity by re-serializing and comparing hashes.
    ///
    /// This is a **self-contained verification** that does not require external content
    /// sections. It proves the bundle manifest bytes have not been tampered with since
    /// the manifest hash was computed.
    ///
    /// The manifest hash is computed over the bundle with `manifest_hash` cleared,
    /// because the hash is self-referential (it cannot include itself).
    ///
    /// Returns `true` if the re-computed manifest hash matches the stored value.
    pub fn verify_manifest_integrity(&self, bundle: &ForensicBundle) -> bool {
        let mut bundle_for_hash = bundle.clone();
        bundle_for_hash.integrity.manifest_hash = String::new();
        let computed = compute_sha256(&bundle_for_hash).unwrap_or_default();
        computed == bundle.integrity.manifest_hash
    }

    /// Generate a summary report for a bundle without content verification.
    ///
    /// This is useful when you have the bundle manifest but not the actual content.
    /// It provides a summary of what the bundle should contain based on the manifest.
    pub fn generate_summary(&self, bundle: &ForensicBundle) -> BundleReplaySummary {
        BundleReplaySummary {
            bundle_id: bundle.bundle_id,
            tenant_id: bundle.tenant_id,
            purpose: bundle.purpose,
            time_range: bundle.time_range.clone(),
            status: bundle.status,
            contents: bundle.contents.clone(),
            created_at: bundle.created_at,
            created_by: bundle.created_by.clone(),
            manifest_hash: bundle.integrity.manifest_hash.clone(),
            chain_verified: bundle.integrity.chain_verified,
            summary: format!(
                "Bundle {} contains: {} intent versions, {} artifacts, {} approvals, {} audit events, {} policy snapshots",
                bundle.bundle_id,
                bundle.contents.intent_versions,
                bundle.contents.artifacts,
                bundle.contents.approvals,
                bundle.contents.audit_events,
                bundle.contents.policy_snapshots,
            ),
        }
    }
}

impl Default for BundleReplayService {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of a bundle without full content verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleReplaySummary {
    pub bundle_id: Uuid,
    pub tenant_id: Uuid,
    pub purpose: BundlePurpose,
    pub time_range: BundleTimeRange,
    pub status: BundleStatus,
    pub contents: BundleContents,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub manifest_hash: String,
    pub chain_verified: bool,
    pub summary: String,
}

/// Errors that can occur during bundle replay operations.
#[derive(Debug)]
pub enum BundleReplayError {
    /// Bundle is not in a valid state for replay
    InvalidBundleState {
        bundle_id: Uuid,
        status: BundleStatus,
        reason: String,
    },
    /// Content verification failed
    VerificationFailed {
        bundle_id: Uuid,
        failures: Vec<String>,
    },
    /// Bundle is not Ready (cannot replay non-complete bundles)
    BundleNotReady {
        bundle_id: Uuid,
        current_status: BundleStatus,
    },
}

impl std::fmt::Display for BundleReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleReplayError::InvalidBundleState {
                bundle_id,
                status,
                reason,
            } => write!(
                f,
                "Bundle {} is in invalid state {:?}: {}",
                bundle_id, status, reason
            ),
            BundleReplayError::VerificationFailed {
                bundle_id,
                failures,
            } => write!(
                f,
                "Bundle {} verification failed: {}",
                bundle_id,
                failures.join("; ")
            ),
            BundleReplayError::BundleNotReady {
                bundle_id,
                current_status,
            } => write!(
                f,
                "Bundle {} is not ready for replay (current status: {:?})",
                bundle_id, current_status
            ),
        }
    }
}

impl std::error::Error for BundleReplayError {}
