//! Bounded forensic replay verification
//!
//! **Bounded scope:** This module provides request-driven verification of forensic bundle
//! feasibility. It validates parameters and computes coverage estimates WITHOUT generating
//! actual bundles, storing data, or performing replay.
//!
//! **Truthful semantics:**
//! - `VerificationStatus::Ready` means the system COULD generate a bundle with the
//!   given parameters (all referenced entities exist)
//! - `VerificationStatus::Incomplete` means some referenced entities are missing or
//!   the time range has gaps
//! - `VerificationStatus::NotSupported` means the verification mode is not implemented
//!
//! **NOT claimed:**
//! - Bundle generation (actual data collection)
//! - Bundle storage (S3 or any persistence layer)
//! - Bundle retrieval (downloading stored bundles)
//! - Bundle replay (reproducing state from a bundle)
//! - Hash chain integrity verification

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::collector::*;

/// Purpose of the forensic verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPurpose {
    #[default]
    IncidentInvestigation,
    ComplianceAudit,
    Legal,
}

/// Time range for forensic verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Request for forensic verification
///
/// **Bounded semantics:** This is a verification request, NOT a bundle generation request.
/// It validates parameters and reports what a bundle WOULD contain without generating it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to verify forensic coverage for
    pub intent_id: Uuid,
    /// Time range to verify
    pub time_range: VerificationTimeRange,
    /// Purpose of the verification
    #[serde(default)]
    pub purpose: VerificationPurpose,
    /// Whether to verify artifact coverage
    #[serde(default = "default_include_artifacts")]
    pub include_artifacts: bool,
    /// Whether to verify audit event coverage
    #[serde(default = "default_include_audit_events")]
    pub include_audit_events: bool,
    /// Whether to verify policy snapshot coverage
    #[serde(default = "default_include_policy_snapshots")]
    pub include_policy_snapshots: bool,
}

fn default_include_artifacts() -> bool {
    true
}

fn default_include_audit_events() -> bool {
    true
}

fn default_include_policy_snapshots() -> bool {
    true
}

/// Verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// All referenced entities exist and are within the time range
    Ready,
    /// Some referenced entities are missing or time range has gaps
    Incomplete,
    /// Verification mode not supported
    NotSupported,
}

/// Intent version coverage in a verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentVersionCoverage {
    /// Whether intent exists
    pub intent_exists: bool,
    /// Intent ID
    pub intent_id: Uuid,
    /// Number of versions within the time range
    pub version_count: usize,
    /// Earliest version timestamp within range
    pub earliest_version: Option<DateTime<Utc>>,
    /// Latest version timestamp within range
    pub latest_version: Option<DateTime<Utc>>,
    /// Whether all versions have artifact traceability
    pub has_artifact_traceability: bool,
}

/// Artifact coverage in a verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCoverage {
    /// Number of artifacts found for the intent
    pub artifact_count: usize,
    /// Number of artifacts with complete provenance chain
    pub artifacts_with_provenance: usize,
    /// Whether artifact coverage is complete
    pub coverage_complete: bool,
}

/// Audit event coverage in a verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventCoverage {
    /// Number of audit events found for the tenant in time range
    pub event_count: usize,
    /// Whether the time range has full coverage (no gaps)
    pub time_range_complete: bool,
    /// First event timestamp in range
    pub first_event: Option<DateTime<Utc>>,
    /// Last event timestamp in range
    pub last_event: Option<DateTime<Utc>>,
}

/// Policy snapshot coverage in a verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshotCoverage {
    /// Number of policy snapshots found for the intent
    pub snapshot_count: usize,
    /// Whether snapshots cover all versions
    pub coverage_complete: bool,
}

/// Response for forensic verification
///
/// **Bounded semantics:** This reports what a bundle WOULD contain if generated.
/// It does NOT return actual bundle data, stored bundles, or replay state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationResponse {
    /// Unique identifier for this verification
    pub verification_id: Uuid,
    /// When verification was performed
    pub verified_at: DateTime<Utc>,
    /// Verification status
    pub status: VerificationStatus,
    /// Human-readable status reason
    pub status_reason: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID
    pub intent_id: Uuid,
    /// Time range that was verified
    pub time_range: VerificationTimeRange,
    /// Purpose of verification
    pub purpose: VerificationPurpose,
    /// Intent version coverage
    pub intent_version_coverage: IntentVersionCoverage,
    /// Artifact coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_coverage: Option<ArtifactCoverage>,
    /// Audit event coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_event_coverage: Option<AuditEventCoverage>,
    /// Policy snapshot coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_coverage: Option<PolicySnapshotCoverage>,
    /// Estimated total items that would be in a full bundle
    pub estimated_bundle_item_count: usize,
}

impl ForensicVerificationResponse {
    /// Create a new verification response
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        time_range: VerificationTimeRange,
        purpose: VerificationPurpose,
    ) -> Self {
        Self {
            verification_id: Uuid::new_v4(),
            verified_at: Utc::now(),
            status: VerificationStatus::Ready,
            status_reason: String::new(),
            tenant_id,
            intent_id,
            time_range,
            purpose,
            intent_version_coverage: IntentVersionCoverage {
                intent_exists: false,
                intent_id,
                version_count: 0,
                earliest_version: None,
                latest_version: None,
                has_artifact_traceability: false,
            },
            artifact_coverage: None,
            audit_event_coverage: None,
            policy_snapshot_coverage: None,
            estimated_bundle_item_count: 0,
        }
    }

    /// Compute estimated bundle item count from all coverages
    pub fn compute_estimated_count(&mut self) {
        let mut count = self.intent_version_coverage.version_count;

        if let Some(ref artifact) = self.artifact_coverage {
            count += artifact.artifact_count;
        }

        if let Some(ref audit) = self.audit_event_coverage {
            count += audit.event_count;
        }

        if let Some(ref policy) = self.policy_snapshot_coverage {
            count += policy.snapshot_count;
        }

        self.estimated_bundle_item_count = count;
    }
}

/// Verification service trait
///
/// **Bounded scope:** Implementations should validate parameters and compute
/// coverage estimates WITHOUT generating actual bundles or storing data.
#[async_trait::async_trait]
pub trait ForensicVerificationService: Send + Sync {
    /// Perform forensic verification for the given request
    async fn verify(&self, request: ForensicVerificationRequest) -> ForensicVerificationResponse;
}

/// In-memory forensic verification service for testing
///
/// This implementation returns a ready status with placeholder coverage data.
/// It does NOT query actual services - use a real implementation that integrates
/// with intent service, graph service, and audit repository for actual verification.
pub struct InMemoryForensicVerificationService {
    /// Whether to return Ready or Incomplete status
    ready_status: bool,
    /// Intent version count to return
    intent_version_count: usize,
    /// Artifact count to return
    artifact_count: usize,
    /// Audit event count to return
    audit_event_count: usize,
    /// Policy snapshot count to return
    policy_snapshot_count: usize,
}

impl InMemoryForensicVerificationService {
    pub fn new() -> Self {
        Self {
            ready_status: true,
            intent_version_count: 0,
            artifact_count: 0,
            audit_event_count: 0,
            policy_snapshot_count: 0,
        }
    }

    pub fn with_intent_version_count(mut self, count: usize) -> Self {
        self.intent_version_count = count;
        self
    }

    pub fn with_artifact_count(mut self, count: usize) -> Self {
        self.artifact_count = count;
        self
    }

    pub fn with_audit_event_count(mut self, count: usize) -> Self {
        self.audit_event_count = count;
        self
    }

    pub fn with_policy_snapshot_count(mut self, count: usize) -> Self {
        self.policy_snapshot_count = count;
        self
    }

    pub fn with_incomplete_status(mut self) -> Self {
        self.ready_status = false;
        self
    }
}

impl Default for InMemoryForensicVerificationService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ForensicVerificationService for InMemoryForensicVerificationService {
    async fn verify(&self, request: ForensicVerificationRequest) -> ForensicVerificationResponse {
        let mut response = ForensicVerificationResponse::new(
            request.tenant_id,
            request.intent_id,
            request.time_range,
            request.purpose,
        );

        // Set coverage based on configured counts
        response.intent_version_coverage.intent_exists = self.intent_version_count > 0;
        response.intent_version_coverage.version_count = self.intent_version_count;
        if self.intent_version_count > 0 {
            response.intent_version_coverage.earliest_version = Some(Utc::now());
            response.intent_version_coverage.latest_version = Some(Utc::now());
        }

        if request.include_artifacts {
            response.artifact_coverage = Some(ArtifactCoverage {
                artifact_count: self.artifact_count,
                artifacts_with_provenance: self.artifact_count,
                coverage_complete: self.artifact_count > 0,
            });
        }

        if request.include_audit_events {
            response.audit_event_coverage = Some(AuditEventCoverage {
                event_count: self.audit_event_count,
                time_range_complete: self.audit_event_count > 0,
                first_event: Some(Utc::now()),
                last_event: Some(Utc::now()),
            });
        }

        if request.include_policy_snapshots {
            response.policy_snapshot_coverage = Some(PolicySnapshotCoverage {
                snapshot_count: self.policy_snapshot_count,
                coverage_complete: self.policy_snapshot_count > 0,
            });
        }

        // Set status based on configuration
        if self.ready_status {
            response.status = VerificationStatus::Ready;
            response.status_reason =
                "All referenced entities exist and are within time range".to_string();
        } else {
            response.status = VerificationStatus::Incomplete;
            response.status_reason = "Some entities are missing or time range has gaps".to_string();
        }

        response.compute_estimated_count();
        response
    }
}

/// Real forensic verification service using actual collector counts
///
/// This implementation delegates to `ForensicDataCollector::count_available`
/// to get real counts from service repositories. It does NOT fabricate
/// timestamps or artifact counts — these fields remain None when unavailable.
///
/// **Truthful semantics:**
/// - Returns truthful status based on actual entity counts
/// - `earliest_version`/`latest_version` are None (not available from count_available)
/// - `has_artifact_traceability` is false (not available from count_available)
/// - `coverage_complete` is false (not available from count_available)
pub struct RealForensicVerificationService {
    collector: Arc<dyn ForensicDataCollector>,
}

impl RealForensicVerificationService {
    pub fn new(collector: Arc<dyn ForensicDataCollector>) -> Self {
        Self { collector }
    }
}

#[async_trait::async_trait]
impl ForensicVerificationService for RealForensicVerificationService {
    async fn verify(&self, request: ForensicVerificationRequest) -> ForensicVerificationResponse {
        let mut response = ForensicVerificationResponse::new(
            request.tenant_id,
            request.intent_id,
            request.time_range.clone(),
            request.purpose,
        );

        let time_range = (request.time_range.start, request.time_range.end);
        let intent_ids = &[request.intent_id];

        // Delegate to real collector for counts
        match self
            .collector
            .count_available(Some(request.tenant_id), intent_ids, &time_range)
            .await
        {
            Ok(counts) => {
                // Set intent coverage
                response.intent_version_coverage.intent_exists = counts.intent_count > 0;
                response.intent_version_coverage.version_count = counts.version_count;
                // Note: earliest/latest_version not available from count_available — remain None
                // Note: has_artifact_traceability not available from count_available — remain false

                // Set artifact coverage if requested
                if request.include_artifacts {
                    // Artifact counts not available from count_available — use version_count as proxy
                    // but mark coverage_complete as false since we don't have real artifact data
                    response.artifact_coverage = Some(ArtifactCoverage {
                        artifact_count: 0, // Not available from count_available
                        artifacts_with_provenance: 0,
                        coverage_complete: false,
                    });
                }

                // Set audit event coverage if requested
                if request.include_audit_events {
                    response.audit_event_coverage = Some(AuditEventCoverage {
                        event_count: counts.audit_event_count,
                        // time_range_complete not available — remain false
                        time_range_complete: false,
                        first_event: None,
                        last_event: None,
                    });
                }

                // Set policy snapshot coverage if requested
                if request.include_policy_snapshots {
                    response.policy_snapshot_coverage = Some(PolicySnapshotCoverage {
                        snapshot_count: counts.policy_snapshot_count,
                        coverage_complete: false,
                    });
                }

                // Set status based on actual counts
                if counts.intent_count == 0 {
                    response.status = VerificationStatus::Incomplete;
                    response.status_reason =
                        "No matching intents found for tenant in time range".to_string();
                } else {
                    response.status = VerificationStatus::Ready;
                    response.status_reason =
                        "All referenced entities exist and are within time range".to_string();
                }
            }
            Err(e) => {
                response.status = VerificationStatus::Incomplete;
                response.status_reason = format!("Failed to verify: {}", e);
            }
        }

        response.compute_estimated_count();
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_request_defaults() {
        let json = r#"{
            "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
            "intent_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "time_range": {
                "start": "2025-01-01T00:00:00Z",
                "end": "2025-01-31T23:59:59Z"
            }
        }"#;

        let request: ForensicVerificationRequest =
            serde_json::from_str(json).expect("should deserialize");

        assert_eq!(request.purpose, VerificationPurpose::IncidentInvestigation);
        assert!(request.include_artifacts);
        assert!(request.include_audit_events);
        assert!(request.include_policy_snapshots);
    }

    #[test]
    fn test_verification_status_serialization() {
        assert_eq!(
            serde_json::to_string(&VerificationStatus::Ready).unwrap(),
            "\"ready\""
        );
        assert_eq!(
            serde_json::to_string(&VerificationStatus::Incomplete).unwrap(),
            "\"incomplete\""
        );
        assert_eq!(
            serde_json::to_string(&VerificationStatus::NotSupported).unwrap(),
            "\"not_supported\""
        );
    }

    #[test]
    fn test_verification_response_computes_count() {
        let mut response = ForensicVerificationResponse::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            VerificationTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            VerificationPurpose::ComplianceAudit,
        );

        response.intent_version_coverage.version_count = 5;
        response.artifact_coverage = Some(ArtifactCoverage {
            artifact_count: 10,
            artifacts_with_provenance: 8,
            coverage_complete: false,
        });
        response.audit_event_coverage = Some(AuditEventCoverage {
            event_count: 100,
            time_range_complete: true,
            first_event: None,
            last_event: None,
        });
        response.policy_snapshot_coverage = Some(PolicySnapshotCoverage {
            snapshot_count: 3,
            coverage_complete: true,
        });

        response.compute_estimated_count();

        // 5 + 10 + 100 + 3 = 118
        assert_eq!(response.estimated_bundle_item_count, 118);
    }

    #[test]
    fn test_purpose_default() {
        assert_eq!(
            serde_json::from_str::<VerificationPurpose>("\"incident_investigation\"").unwrap(),
            VerificationPurpose::IncidentInvestigation
        );
        assert_eq!(
            serde_json::from_str::<VerificationPurpose>("\"compliance_audit\"").unwrap(),
            VerificationPurpose::ComplianceAudit
        );
        assert_eq!(
            serde_json::from_str::<VerificationPurpose>("\"legal\"").unwrap(),
            VerificationPurpose::Legal
        );
    }

    #[tokio::test]
    async fn test_real_verification_service_no_intents() {
        use crate::collector::InMemoryForensicDataCollector;

        let collector = Arc::new(InMemoryForensicDataCollector::new());
        let service = RealForensicVerificationService::new(collector);

        let request = ForensicVerificationRequest {
            tenant_id: Uuid::new_v4(),
            intent_id: Uuid::new_v4(),
            time_range: VerificationTimeRange {
                start: Utc::now() - chrono::Duration::days(1),
                end: Utc::now(),
            },
            purpose: VerificationPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let response = service.verify(request).await;

        assert_eq!(response.status, VerificationStatus::Incomplete);
        assert!(response.status_reason.contains("No matching intents"));
        assert!(!response.intent_version_coverage.intent_exists);
        assert_eq!(response.intent_version_coverage.version_count, 0);
    }

    #[tokio::test]
    async fn test_real_verification_service_with_intent_but_no_versions() {
        use crate::collector::{
            CollectedIntentData, CollectedVersionData, InMemoryForensicDataCollector,
        };

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create intent data but with versions outside the time range
        let intent = CollectedIntentData {
            intent_id,
            tenant_id: Some(tenant_id),
            name: format!("intent-{}", intent_id),
            versions: vec![CollectedVersionData {
                version_number: 1,
                summary: "Old version".to_string(),
                change_type: "create".to_string(),
                created_at: Utc::now() - chrono::Duration::days(30), // Outside time range
            }],
            policy_snapshots: vec![],
            audit_events: vec![],
        };

        let collector = Arc::new(InMemoryForensicDataCollector::new().with_intents(vec![intent]));
        let service = RealForensicVerificationService::new(collector);

        let request = ForensicVerificationRequest {
            tenant_id,
            intent_id,
            time_range: VerificationTimeRange {
                start: Utc::now() - chrono::Duration::days(1),
                end: Utc::now(),
            },
            purpose: VerificationPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        let response = service.verify(request).await;

        // Intent exists but no versions in time range
        assert_eq!(response.status, VerificationStatus::Ready);
        assert!(response
            .status_reason
            .contains("All referenced entities exist"));
        assert!(response.intent_version_coverage.intent_exists);
        assert_eq!(response.intent_version_coverage.version_count, 0);
    }

    #[tokio::test]
    async fn test_real_verification_service_maps_counts_correctly() {
        use crate::collector::{
            CollectedIntentData, CollectedVersionData, InMemoryForensicDataCollector,
        };

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let now = Utc::now();

        let intent = CollectedIntentData {
            intent_id,
            tenant_id: Some(tenant_id),
            name: format!("intent-{}", intent_id),
            versions: vec![
                CollectedVersionData {
                    version_number: 1,
                    summary: "Version 1".to_string(),
                    change_type: "create".to_string(),
                    created_at: now - chrono::Duration::hours(2),
                },
                CollectedVersionData {
                    version_number: 2,
                    summary: "Version 2".to_string(),
                    change_type: "update".to_string(),
                    created_at: now - chrono::Duration::hours(1),
                },
            ],
            policy_snapshots: vec![],
            audit_events: vec![],
        };

        let collector = Arc::new(InMemoryForensicDataCollector::new().with_intents(vec![intent]));
        let service = RealForensicVerificationService::new(collector);

        let request = ForensicVerificationRequest {
            tenant_id,
            intent_id,
            time_range: VerificationTimeRange {
                start: now - chrono::Duration::days(1),
                end: now,
            },
            purpose: VerificationPurpose::ComplianceAudit,
            include_artifacts: false,
            include_audit_events: true,
            include_policy_snapshots: false,
        };

        let response = service.verify(request).await;

        assert_eq!(response.status, VerificationStatus::Ready);
        assert!(response.intent_version_coverage.intent_exists);
        assert_eq!(response.intent_version_coverage.version_count, 2);
        // Timestamps not available from count_available
        assert!(response.intent_version_coverage.earliest_version.is_none());
        assert!(response.intent_version_coverage.latest_version.is_none());
        // Artifact coverage not requested
        assert!(response.artifact_coverage.is_none());
        // Audit events requested but collector returns 0 (no events in intent)
        assert!(response.audit_event_coverage.is_some());
        assert_eq!(
            response.audit_event_coverage.as_ref().unwrap().event_count,
            0
        );
        // Policy snapshots not requested
        assert!(response.policy_snapshot_coverage.is_none());
    }
}
