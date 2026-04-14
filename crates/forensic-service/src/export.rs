//! Bounded forensic archive export
//!
//! **Bounded scope:** This module provides in-memory forensic archive generation
//! for export/download. Archives are generated from scaffolded data and returned
//! as JSON-serialized payloads WITHOUT any persistent storage, S3 integration,
//! or real replay engine.
//!
//! **Truthful semantics:**
//! - Generated archives contain scaffolded/fictional data representing what a
//!   real forensic bundle WOULD contain
//! - `archive_id` is a unique identifier for the generated archive
//! - `generated_at` timestamps when the archive was generated
//! - `item_count` reflects the estimated item count from coverage data
//!
//! **NOT claimed:**
//! - Bundle generation (actual data collection from intent/graph/audit services)
//! - Bundle storage (S3 or any persistence layer)
//! - Async job orchestration for bundle generation
//! - Real replay engine (state reproduction from bundle)
//! - Hash chain integrity verification

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Re-export from verification module for coverage types
use super::verification::{
    VerificationPurpose, VerificationTimeRange,
};

/// Request for forensic archive export
///
/// **Bounded semantics:** This triggers in-memory archive generation from the
/// given verification request parameters. The archive contains scaffolded/fictional
/// data representing what a real bundle would contain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportRequest {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID to generate archive for
    pub intent_id: Uuid,
    /// Time range to include in archive
    pub time_range: ExportTimeRange,
    /// Purpose of the archive
    #[serde(default)]
    pub purpose: ExportPurpose,
    /// Whether to include artifact entries
    #[serde(default = "default_include_artifacts")]
    pub include_artifacts: bool,
    /// Whether to include audit event entries
    #[serde(default = "default_include_audit_events")]
    pub include_audit_events: bool,
    /// Whether to include policy snapshot entries
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

/// Time range for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl From<VerificationTimeRange> for ExportTimeRange {
    fn from(vtr: VerificationTimeRange) -> Self {
        Self {
            start: vtr.start,
            end: vtr.end,
        }
    }
}

/// Purpose mapping from verification to export
impl From<VerificationPurpose> for ExportPurpose {
    fn from(vp: VerificationPurpose) -> Self {
        match vp {
            VerificationPurpose::IncidentInvestigation => ExportPurpose::IncidentInvestigation,
            VerificationPurpose::ComplianceAudit => ExportPurpose::ComplianceAudit,
            VerificationPurpose::Legal => ExportPurpose::Legal,
        }
    }
}

/// Purpose of the forensic archive
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPurpose {
    IncidentInvestigation,
    ComplianceAudit,
    Legal,
}

impl Default for ExportPurpose {
    fn default() -> Self {
        ExportPurpose::IncidentInvestigation
    }
}

/// Individual entry within a forensic archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicArchiveEntry {
    /// Unique identifier for this entry
    pub entry_id: Uuid,
    /// Entry type
    pub entry_type: ArchiveEntryType,
    /// When this entry was recorded
    pub timestamp: DateTime<Utc>,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID (if applicable)
    pub intent_id: Option<Uuid>,
    /// Entry data as JSON
    pub data: serde_json::Value,
}

/// Type of archive entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveEntryType {
    IntentVersion,
    Artifact,
    AuditEvent,
    PolicySnapshot,
    BundleManifest,
}

/// In-memory forensic archive
///
/// **Bounded semantics:** This archive is generated entirely in-memory from
/// the given verification coverage data. It contains scaffolded entries that
/// represent what a real forensic bundle would include if fully generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicArchive {
    /// Unique identifier for this archive
    pub archive_id: Uuid,
    /// Archive format version
    pub archive_version: String,
    /// When this archive was generated
    pub generated_at: DateTime<Utc>,
    /// Actor who triggered archive generation (or "system")
    pub generated_by: String,
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Intent ID this archive is for
    pub intent_id: Uuid,
    /// Time range covered by this archive
    pub time_range: ExportTimeRange,
    /// Purpose of this archive
    pub purpose: ExportPurpose,
    /// Entries included in this archive
    pub entries: Vec<ForensicArchiveEntry>,
    /// Summary of contents
    pub contents: ExportContentsSummary,
    /// Total item count in archive
    pub item_count: usize,
    /// Content type for response (MIME type)
    pub content_type: String,
    /// Raw archive data as bytes (JSON serialized)
    #[serde(skip_serializing)]
    pub raw_data: Vec<u8>,
}

/// Summary of contents in an export archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportContentsSummary {
    /// Number of intent version entries
    pub intent_versions: usize,
    /// Number of artifact entries
    pub artifacts: usize,
    /// Number of audit event entries
    pub audit_events: usize,
    /// Number of policy snapshot entries
    pub policy_snapshots: usize,
}

impl Default for ExportContentsSummary {
    fn default() -> Self {
        Self {
            intent_versions: 0,
            artifacts: 0,
            audit_events: 0,
            policy_snapshots: 0,
        }
    }
}

/// Response for forensic archive export
///
/// Returns the generated archive metadata and raw data as a JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExportResponse {
    /// Unique identifier for this archive
    pub archive_id: Uuid,
    /// When archive was generated
    pub generated_at: DateTime<Utc>,
    /// Verification status at time of generation
    pub status: ExportStatus,
    /// Human-readable status reason
    pub status_reason: String,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Intent ID
    pub intent_id: Uuid,
    /// Time range covered
    pub time_range: ExportTimeRange,
    /// Purpose of archive
    pub purpose: ExportPurpose,
    /// Summary of archive contents
    pub contents: ExportContentsSummary,
    /// Total item count
    pub item_count: usize,
    /// Content type (application/json)
    pub content_type: String,
    /// Archive size in bytes
    pub archive_size_bytes: usize,
}

impl ForensicExportResponse {
    /// Create a new export response from an archive
    pub fn from_archive(archive: &ForensicArchive) -> Self {
        Self {
            archive_id: archive.archive_id,
            generated_at: archive.generated_at,
            status: ExportStatus::Generated,
            status_reason: "Archive generated in-memory from scaffolded data".to_string(),
            tenant_id: archive.tenant_id,
            intent_id: archive.intent_id,
            time_range: archive.time_range.clone(),
            purpose: archive.purpose,
            contents: archive.contents.clone(),
            item_count: archive.item_count,
            content_type: archive.content_type.clone(),
            archive_size_bytes: archive.raw_data.len(),
        }
    }
}

/// Export status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    /// Archive was successfully generated
    Generated,
    /// Archive generation failed
    Failed,
}

/// Archive generator service trait
///
/// **Bounded scope:** Implementations should generate archives entirely in-memory
/// from the given parameters WITHOUT writing to persistent storage or triggering
/// async jobs.
#[async_trait::async_trait]
pub trait ForensicArchiveGenerator: Send + Sync {
    /// Generate a forensic archive from the given request
    async fn generate(&self, request: ForensicExportRequest) -> ForensicExportResponse;
}

/// In-memory forensic archive generator for testing
///
/// **Bounded semantics:** This implementation generates archives with scaffolded/fictional
/// entries based on the request parameters. It does NOT query actual services for
/// real intent versions, artifacts, audit events, or policy snapshots.
pub struct InMemoryForensicArchiveGenerator {
    /// Intent version count to include in generated archive
    intent_version_count: usize,
    /// Artifact count to include
    artifact_count: usize,
    /// Audit event count to include
    audit_event_count: usize,
    /// Policy snapshot count to include
    policy_snapshot_count: usize,
}

impl InMemoryForensicArchiveGenerator {
    /// Create a new in-memory archive generator
    pub fn new() -> Self {
        Self {
            intent_version_count: 0,
            artifact_count: 0,
            audit_event_count: 0,
            policy_snapshot_count: 0,
        }
    }

    /// Configure intent version count
    pub fn with_intent_version_count(mut self, count: usize) -> Self {
        self.intent_version_count = count;
        self
    }

    /// Configure artifact count
    pub fn with_artifact_count(mut self, count: usize) -> Self {
        self.artifact_count = count;
        self
    }

    /// Configure audit event count
    pub fn with_audit_event_count(mut self, count: usize) -> Self {
        self.audit_event_count = count;
        self
    }

    /// Configure policy snapshot count
    pub fn with_policy_snapshot_count(mut self, count: usize) -> Self {
        self.policy_snapshot_count = count;
        self
    }

    /// Generate entries for the archive
    fn generate_entries(&self, request: &ForensicExportRequest) -> Vec<ForensicArchiveEntry> {
        let mut entries = Vec::new();
        let now = Utc::now();
        let tenant_id = request.tenant_id;
        let intent_id = request.intent_id;

        // Generate intent version entries (scaffolded)
        if self.intent_version_count > 0 {
            for i in 0..self.intent_version_count {
                entries.push(ForensicArchiveEntry {
                    entry_id: Uuid::new_v4(),
                    entry_type: ArchiveEntryType::IntentVersion,
                    timestamp: now,
                    tenant_id,
                    intent_id: Some(intent_id),
                    data: serde_json::json!({
                        "version_number": i + 1,
                        "status": "active",
                        "created_at": now,
                        "created_by": "system"
                    }),
                });
            }
        }

        // Generate artifact entries (scaffolded)
        if request.include_artifacts && self.artifact_count > 0 {
            for _ in 0..self.artifact_count {
                entries.push(ForensicArchiveEntry {
                    entry_id: Uuid::new_v4(),
                    entry_type: ArchiveEntryType::Artifact,
                    timestamp: now,
                    tenant_id,
                    intent_id: Some(intent_id),
                    data: serde_json::json!({
                        "artifact_id": Uuid::new_v4(),
                        "artifact_type": "document",
                        "provenance": {
                            "chain_complete": true,
                            "references": []
                        },
                        "created_at": now
                    }),
                });
            }
        }

        // Generate audit event entries (scaffolded)
        if request.include_audit_events && self.audit_event_count > 0 {
            for _ in 0..self.audit_event_count {
                entries.push(ForensicArchiveEntry {
                    entry_id: Uuid::new_v4(),
                    entry_type: ArchiveEntryType::AuditEvent,
                    timestamp: now,
                    tenant_id,
                    intent_id: Some(intent_id),
                    data: serde_json::json!({
                        "event_type": "intent_updated",
                        "actor": "system",
                        "occurred_at": now,
                        "metadata": {}
                    }),
                });
            }
        }

        // Generate policy snapshot entries (scaffolded)
        if request.include_policy_snapshots && self.policy_snapshot_count > 0 {
            for i in 0..self.policy_snapshot_count {
                entries.push(ForensicArchiveEntry {
                    entry_id: Uuid::new_v4(),
                    entry_type: ArchiveEntryType::PolicySnapshot,
                    timestamp: now,
                    tenant_id,
                    intent_id: Some(intent_id),
                    data: serde_json::json!({
                        "snapshot_version": i + 1,
                        "policy_content": {},
                        "created_at": now
                    }),
                });
            }
        }

        entries
    }

    /// Build archive contents summary
    fn build_contents_summary(&self, entries: &[ForensicArchiveEntry]) -> ExportContentsSummary {
        let mut summary = ExportContentsSummary::default();
        for entry in entries {
            match entry.entry_type {
                ArchiveEntryType::IntentVersion => summary.intent_versions += 1,
                ArchiveEntryType::Artifact => summary.artifacts += 1,
                ArchiveEntryType::AuditEvent => summary.audit_events += 1,
                ArchiveEntryType::PolicySnapshot => summary.policy_snapshots += 1,
                ArchiveEntryType::BundleManifest => {}
            }
        }
        summary
    }
}

impl Default for InMemoryForensicArchiveGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ForensicArchiveGenerator for InMemoryForensicArchiveGenerator {
    async fn generate(&self, request: ForensicExportRequest) -> ForensicExportResponse {
        // Generate scaffolded entries based on configured counts
        let entries = self.generate_entries(&request);

        // Build contents summary
        let contents = self.build_contents_summary(&entries);
        let item_count = entries.len();

        // Create the archive
        let archive = ForensicArchive {
            archive_id: Uuid::new_v4(),
            archive_version: "v1".to_string(),
            generated_at: Utc::now(),
            generated_by: "system".to_string(),
            tenant_id: request.tenant_id,
            intent_id: request.intent_id,
            time_range: ExportTimeRange {
                start: request.time_range.start,
                end: request.time_range.end,
            },
            purpose: request.purpose,
            entries,
            contents: contents.clone(),
            item_count,
            content_type: "application/json".to_string(),
            raw_data: Vec::new(), // Will be set after serialization
        };

        // Serialize archive to JSON bytes
        let raw_data = serde_json::to_vec(&archive).unwrap_or_default();
        let archive_size_bytes = raw_data.len();

        // Create response from archive
        let mut response = ForensicExportResponse::from_archive(&archive);
        response.archive_size_bytes = archive_size_bytes;

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_request_defaults() {
        let json = r#"{
            "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
            "intent_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "time_range": {
                "start": "2025-01-01T00:00:00Z",
                "end": "2025-01-31T23:59:59Z"
            }
        }"#;

        let request: ForensicExportRequest =
            serde_json::from_str(json).expect("should deserialize");

        assert_eq!(request.purpose, ExportPurpose::IncidentInvestigation);
        assert!(request.include_artifacts);
        assert!(request.include_audit_events);
        assert!(request.include_policy_snapshots);
    }

    #[test]
    fn test_export_purpose_serialization() {
        assert_eq!(
            serde_json::to_string(&ExportPurpose::IncidentInvestigation).unwrap(),
            "\"incident_investigation\""
        );
        assert_eq!(
            serde_json::to_string(&ExportPurpose::ComplianceAudit).unwrap(),
            "\"compliance_audit\""
        );
        assert_eq!(
            serde_json::to_string(&ExportPurpose::Legal).unwrap(),
            "\"legal\""
        );
    }

    #[test]
    fn test_export_status_serialization() {
        assert_eq!(
            serde_json::to_string(&ExportStatus::Generated).unwrap(),
            "\"generated\""
        );
        assert_eq!(
            serde_json::to_string(&ExportStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    #[test]
    fn test_archive_entry_type_serialization() {
        assert_eq!(
            serde_json::to_string(&ArchiveEntryType::IntentVersion).unwrap(),
            "\"intent_version\""
        );
        assert_eq!(
            serde_json::to_string(&ArchiveEntryType::Artifact).unwrap(),
            "\"artifact\""
        );
        assert_eq!(
            serde_json::to_string(&ArchiveEntryType::AuditEvent).unwrap(),
            "\"audit_event\""
        );
        assert_eq!(
            serde_json::to_string(&ArchiveEntryType::PolicySnapshot).unwrap(),
            "\"policy_snapshot\""
        );
        assert_eq!(
            serde_json::to_string(&ArchiveEntryType::BundleManifest).unwrap(),
            "\"bundle_manifest\""
        );
    }

    #[test]
    fn test_in_memory_generator_empty_archive() {
        let _generator = InMemoryForensicArchiveGenerator::new();
        let _request = ForensicExportRequest {
            tenant_id: Uuid::new_v4(),
            intent_id: Uuid::new_v4(),
            time_range: ExportTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: ExportPurpose::IncidentInvestigation,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        // Generator should create archive with zero entries (counts are 0 by default)
        // This tests the scaffolded nature - real data would require service integration
        // Block on the async generate to verify it completes without panic
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(_generator.generate(_request.clone()));
    }

    #[test]
    fn test_in_memory_generator_with_counts() {
        let generator = InMemoryForensicArchiveGenerator::new()
            .with_intent_version_count(5)
            .with_artifact_count(10)
            .with_audit_event_count(100)
            .with_policy_snapshot_count(3);

        let request = ForensicExportRequest {
            tenant_id: Uuid::new_v4(),
            intent_id: Uuid::new_v4(),
            time_range: ExportTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: ExportPurpose::ComplianceAudit,
            include_artifacts: true,
            include_audit_events: true,
            include_policy_snapshots: true,
        };

        // Block on the async generate
        let rt = tokio::runtime::Runtime::new().unwrap();
        let response = rt.block_on(generator.generate(request.clone()));

        assert_eq!(response.tenant_id, request.tenant_id);
        assert_eq!(response.intent_id, request.intent_id);
        assert_eq!(response.contents.intent_versions, 5);
        assert_eq!(response.contents.artifacts, 10);
        assert_eq!(response.contents.audit_events, 100);
        assert_eq!(response.contents.policy_snapshots, 3);
        assert_eq!(response.item_count, 118); // 5 + 10 + 100 + 3
        assert_eq!(response.status, ExportStatus::Generated);
        assert_eq!(response.content_type, "application/json");
    }

    #[test]
    fn test_export_response_from_archive() {
        let archive = ForensicArchive {
            archive_id: Uuid::new_v4(),
            archive_version: "v1".to_string(),
            generated_at: Utc::now(),
            generated_by: "system".to_string(),
            tenant_id: Uuid::new_v4(),
            intent_id: Uuid::new_v4(),
            time_range: ExportTimeRange {
                start: Utc::now(),
                end: Utc::now(),
            },
            purpose: ExportPurpose::Legal,
            entries: Vec::new(),
            contents: ExportContentsSummary {
                intent_versions: 3,
                artifacts: 5,
                audit_events: 50,
                policy_snapshots: 2,
            },
            item_count: 60,
            content_type: "application/json".to_string(),
            raw_data: b"test archive data".to_vec(),
        };

        let response = ForensicExportResponse::from_archive(&archive);

        assert_eq!(response.archive_id, archive.archive_id);
        assert_eq!(response.status, ExportStatus::Generated);
        assert_eq!(response.contents.intent_versions, 3);
        assert_eq!(response.item_count, 60);
        // archive_size_bytes is the raw_data.len() directly since ForensicExportResponse::from_archive
        // doesn't serialize the archive - it just reads the field. In real usage through generate(),
        // the archive is JSON-serialized and the size reflects the serialized bytes.
        assert_eq!(response.archive_size_bytes, archive.raw_data.len());
    }

    #[test]
    fn test_time_range_conversion_from_verification() {
        let vtr = VerificationTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        };

        let etr: ExportTimeRange = vtr.clone().into();
        assert_eq!(etr.start, vtr.start);
        assert_eq!(etr.end, vtr.end);
    }
}
