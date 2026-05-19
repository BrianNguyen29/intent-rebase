use chrono::{DateTime, Utc};
use forensic_service::{
    BundlePurpose, BundleStatus, ExportPurpose, ExportStatus, ForensicBundle, VerificationPurpose,
    VerificationStatus,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Forensic Bundle Types
// =============================================================================

/// Time range for forensic bundle request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Summary of contents in a forensic bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleContentsSummary {
    pub intent_versions: usize,
    pub artifacts: usize,
    pub approvals: usize,
    pub audit_events: usize,
    pub policy_snapshots: usize,
}

/// Integrity information for a forensic bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleIntegrityInfo {
    /// SHA-256 hash of the bundle manifest
    pub manifest_hash: String,
    /// Whether the hash chain was verified (always false for new bundles)
    pub chain_verified: bool,
    /// When integrity was computed
    pub verification_timestamp: DateTime<Utc>,
}

/// Request body for forensic bundle generation
///
/// **P4 bounded slice:** Collects real data, generates a bundle manifest,
/// persists it to S3/MinIO, and records the bundle in the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent IDs to include in the bundle
    pub intent_ids: Vec<Uuid>,
    /// Time range to collect data for
    pub time_range: ForensicBundleTimeRange,
    /// Purpose of the bundle
    pub purpose: BundlePurpose,
    /// Actor who triggered bundle generation
    #[serde(default = "default_actor")]
    pub created_by: String,
}

fn default_actor() -> String {
    "system".to_string()
}

/// Response for forensic bundle creation
///
/// **P4 bounded slice:** Returns the generated bundle manifest with
/// storage location and size. The bundle bytes are already persisted
/// to S3/MinIO when this response is returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleResponse {
    /// Unique identifier for the generated bundle
    pub bundle_id: Uuid,
    /// When the bundle was created
    pub created_at: DateTime<Utc>,
    /// Actor who triggered bundle generation
    pub created_by: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Time range covered by the bundle
    pub time_range: ForensicBundleTimeRange,
    /// Bundle generation status (always "ready" on success)
    pub status: BundleStatus,
    /// Purpose of the bundle
    pub purpose: BundlePurpose,
    /// Summary of bundle contents
    pub contents: ForensicBundleContentsSummary,
    /// Integrity information
    pub integrity: ForensicBundleIntegrityInfo,
    /// Storage location (S3/MinIO path)
    pub storage_location: String,
    /// Size of stored bundle in bytes
    pub bundle_size_bytes: usize,
    /// Human-readable message
    pub message: String,
}

// =============================================================================
// Forensic Bundle Replay Types
// =============================================================================

/// Request body for forensic bundle replay verification.
///
/// **Bounded replay evidence slice:** Provides content sections to verify against
/// the per-section hashes stored in the bundle manifest. This is read-only
/// integrity verification, not full runtime/state reconstruction replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleReplayRequest {
    /// Tenant ID for access validation
    pub tenant_id: Uuid,
    /// Intent version entries to verify against the bundle
    pub intent_versions: Vec<forensic_service::IntentVersionEntry>,
    /// Artifact entries to verify against the bundle
    pub artifacts: Vec<forensic_service::ArtifactEntry>,
    /// Approval entries to verify against the bundle
    pub approvals: Vec<forensic_service::ApprovalEntry>,
    /// Audit event entries to verify against the bundle
    pub audit_events: Vec<forensic_service::AuditEventEntry>,
    /// Policy snapshot entries to verify against the bundle
    pub policy_snapshots: Vec<forensic_service::PolicySnapshotEntry>,
}

/// Response for forensic bundle replay verification.
///
/// **Bounded replay evidence slice:** Returns the result of verifying provided
/// content sections against the stored per-section integrity hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicBundleReplayResponse {
    /// Bundle ID that was verified
    pub bundle_id: Uuid,
    /// Whether all sections passed verification
    pub overall_verified: bool,
    /// Number of sections that passed
    pub sections_passed: usize,
    /// Number of sections that failed
    pub sections_failed: usize,
    /// Human-readable summary
    pub summary: String,
    /// Per-section verification results
    pub sections: Vec<forensic_service::ReplaySectionResult>,
}

// =============================================================================
// Forensic Export Types
// =============================================================================

/// Request for forensic archive export
///
/// **Phase 3 Batch 3b (bounded slice):** Triggers in-memory archive generation
/// from the given parameters. The archive contains scaffolded/fictional data
/// representing what a real bundle would contain.
///
/// **Truthful semantics:**
/// - Generated archive is entirely in-memory with scaffolded entries
/// - Does NOT query actual services for real intent versions, artifacts, etc.
/// - `item_count` reflects the configured generator counts, not actual data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to generate archive for
    pub intent_id: Uuid,
    /// Time range to include in archive
    pub time_range: ForensicExportTimeRange,
    /// Purpose of the archive
    #[serde(default)]
    pub purpose: ExportPurpose,
    /// Whether to include artifact entries
    #[serde(default = "default_export_include_artifacts")]
    pub include_artifacts: bool,
    /// Whether to include audit event entries
    #[serde(default = "default_export_include_audit_events")]
    pub include_audit_events: bool,
    /// Whether to include policy snapshot entries
    #[serde(default = "default_export_include_policy_snapshots")]
    pub include_policy_snapshots: bool,
}

fn default_export_include_artifacts() -> bool {
    true
}

fn default_export_include_audit_events() -> bool {
    true
}

fn default_export_include_policy_snapshots() -> bool {
    true
}

/// Time range for forensic export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Response for forensic archive export
///
/// **Phase 3 Batch 3b (bounded slice):** Returns archive metadata and size
/// for the generated in-memory archive. The actual archive content is NOT
/// embedded in this response — it is generated on-demand.
///
/// **Truthful semantics:**
/// - `archive_id` is a unique identifier for the generated archive
/// - `generated_at` timestamps when generation was triggered
/// - `item_count` is the count of scaffolded entries generated
/// - `archive_size_bytes` reflects the JSON-serialized size of the archive
///
/// **NOT claimed:**
/// - Actual bundle generation from real services
/// - Bundle storage (S3 or any persistence)
/// - Async job orchestration
/// - Real replay engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportResponse {
    /// Unique identifier for this archive
    pub archive_id: Uuid,
    /// When archive was generated
    pub generated_at: DateTime<Utc>,
    /// Export status
    pub status: ExportStatus,
    /// Human-readable status reason
    pub status_reason: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID
    pub intent_id: Uuid,
    /// Time range covered
    pub time_range: ForensicExportTimeRange,
    /// Purpose of archive
    pub purpose: ExportPurpose,
    /// Summary of archive contents
    pub contents: ForensicExportContentsSummary,
    /// Total item count
    pub item_count: usize,
    /// Content type (application/json)
    pub content_type: String,
    /// Archive size in bytes
    pub archive_size_bytes: usize,
}

/// Summary of contents in an export archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportContentsSummary {
    /// Number of intent version entries
    pub intent_versions: usize,
    /// Number of artifact entries
    pub artifacts: usize,
    /// Number of audit event entries
    pub audit_events: usize,
    /// Number of policy snapshot entries
    pub policy_snapshots: usize,
}

// =============================================================================
// List Forensic Bundles Types
// =============================================================================

/// Query parameters for listing forensic bundles
#[derive(Debug, Deserialize)]
pub struct ListForensicBundlesQuery {
    pub tenant_id: Uuid,
    /// Optional limit for the number of bundles to return
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Response for listing forensic bundles
#[derive(Debug, Serialize)]
pub struct ListForensicBundlesResponse {
    pub bundles: Vec<ForensicBundleSummary>,
    pub total: usize,
}

/// Summary of a forensic bundle for list responses
#[derive(Debug, Serialize)]
pub struct ForensicBundleSummary {
    pub bundle_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub tenant_id: Uuid,
    pub time_range: ForensicBundleTimeRange,
    pub status: BundleStatus,
    pub purpose: BundlePurpose,
    pub contents: ForensicBundleContentsSummary,
    pub integrity: ForensicBundleIntegrityInfo,
}

impl From<ForensicBundle> for ForensicBundleSummary {
    fn from(bundle: ForensicBundle) -> Self {
        Self {
            bundle_id: bundle.bundle_id,
            created_at: bundle.created_at,
            created_by: bundle.created_by,
            tenant_id: bundle.tenant_id,
            time_range: ForensicBundleTimeRange {
                start: bundle.time_range.start,
                end: bundle.time_range.end,
            },
            status: bundle.status,
            purpose: bundle.purpose,
            contents: ForensicBundleContentsSummary {
                intent_versions: bundle.contents.intent_versions,
                artifacts: bundle.contents.artifacts,
                approvals: bundle.contents.approvals,
                audit_events: bundle.contents.audit_events,
                policy_snapshots: bundle.contents.policy_snapshots,
            },
            integrity: ForensicBundleIntegrityInfo {
                manifest_hash: bundle.integrity.manifest_hash,
                chain_verified: bundle.integrity.chain_verified,
                verification_timestamp: bundle.integrity.verification_timestamp,
            },
        }
    }
}

// =============================================================================
// Forensic Verification Types
// =============================================================================

/// Request body for forensic verification
///
/// **Phase 3 Batch 3b (bounded slice):** Verifies forensic bundle feasibility
/// for the given parameters WITHOUT generating actual bundles or storing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to verify forensic coverage for
    pub intent_id: Uuid,
    /// Time range to verify
    pub time_range: ForensicVerificationTimeRange,
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

/// Time range for forensic verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicVerificationTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Response for forensic verification
///
/// **Phase 3 Batch 3b (bounded slice):** Reports what a forensic bundle WOULD contain
/// if generated with the given parameters. This is verification/reporting ONLY.
///
/// **Truthful semantics:**
/// - `status: "ready"` means all referenced entities exist and are within time range
/// - `status: "incomplete"` means some entities are missing or time range has gaps
/// - `estimated_bundle_item_count` is an estimate, NOT actual bundle size
///
/// **NOT claimed:**
/// - Actual bundle generation (no data is collected)
/// - Bundle storage (no S3 or persistence writes)
/// - Bundle retrieval (no stored bundle download)
/// - Bundle replay (no state reproduction)
/// - Hash chain integrity (requires generated bundle)
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
    pub time_range: ForensicVerificationTimeRange,
    /// Purpose of verification
    pub purpose: VerificationPurpose,
    /// Intent version coverage
    pub intent_version_coverage: ForensicIntentVersionCoverage,
    /// Artifact coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_coverage: Option<ForensicArtifactCoverage>,
    /// Audit event coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_event_coverage: Option<ForensicAuditEventCoverage>,
    /// Policy snapshot coverage (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_coverage: Option<ForensicPolicySnapshotCoverage>,
    /// Estimated total items that would be in a full bundle
    pub estimated_bundle_item_count: usize,
}

/// Intent version coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicIntentVersionCoverage {
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

/// Artifact coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicArtifactCoverage {
    /// Number of artifacts found for the intent
    pub artifact_count: usize,
    /// Number of artifacts with complete provenance chain
    pub artifacts_with_provenance: usize,
    /// Whether artifact coverage is complete
    pub coverage_complete: bool,
}

/// Audit event coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicAuditEventCoverage {
    /// Number of audit events found for the tenant in time range
    pub event_count: usize,
    /// Whether the time range has full coverage (no gaps)
    pub time_range_complete: bool,
    /// First event timestamp in range
    pub first_event: Option<DateTime<Utc>>,
    /// Last event timestamp in range
    pub last_event: Option<DateTime<Utc>>,
}

/// Policy snapshot coverage in verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicPolicySnapshotCoverage {
    /// Number of policy snapshots found for the intent
    pub snapshot_count: usize,
    /// Whether snapshots cover all versions
    pub coverage_complete: bool,
}

impl From<forensic_service::ForensicVerificationResponse> for ForensicVerificationResponse {
    fn from(resp: forensic_service::ForensicVerificationResponse) -> Self {
        Self {
            verification_id: resp.verification_id,
            verified_at: resp.verified_at,
            status: resp.status,
            status_reason: resp.status_reason,
            tenant_id: resp.tenant_id,
            intent_id: resp.intent_id,
            time_range: ForensicVerificationTimeRange {
                start: resp.time_range.start,
                end: resp.time_range.end,
            },
            purpose: resp.purpose,
            intent_version_coverage: ForensicIntentVersionCoverage {
                intent_exists: resp.intent_version_coverage.intent_exists,
                intent_id: resp.intent_version_coverage.intent_id,
                version_count: resp.intent_version_coverage.version_count,
                earliest_version: resp.intent_version_coverage.earliest_version,
                latest_version: resp.intent_version_coverage.latest_version,
                has_artifact_traceability: resp.intent_version_coverage.has_artifact_traceability,
            },
            artifact_coverage: resp.artifact_coverage.map(|ac| ForensicArtifactCoverage {
                artifact_count: ac.artifact_count,
                artifacts_with_provenance: ac.artifacts_with_provenance,
                coverage_complete: ac.coverage_complete,
            }),
            audit_event_coverage: resp
                .audit_event_coverage
                .map(|aec| ForensicAuditEventCoverage {
                    event_count: aec.event_count,
                    time_range_complete: aec.time_range_complete,
                    first_event: aec.first_event,
                    last_event: aec.last_event,
                }),
            policy_snapshot_coverage: resp.policy_snapshot_coverage.map(|psc| {
                ForensicPolicySnapshotCoverage {
                    snapshot_count: psc.snapshot_count,
                    coverage_complete: psc.coverage_complete,
                }
            }),
            estimated_bundle_item_count: resp.estimated_bundle_item_count,
        }
    }
}
