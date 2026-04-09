//! Artifact domain types
//!
//! Phase 2b bounded slice: Artifact invalidation metadata and quarantine status.
//! Note: This is metadata/status only - real S3 quarantine move is Phase 3.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An artifact produced under a specific intent version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub intent_version: i32,
    pub tenant_id: Uuid,
    pub name: String,
    pub artifact_type: String,
    pub s3_uri: String,
    pub checksum: String,
    pub metadata: ArtifactMetadata,
    pub created_at: DateTime<Utc>,
}

/// Metadata attached to an artifact
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtifactMetadata {
    pub produced_by: Option<String>,
    pub tags: Vec<String>,
    /// Whether this artifact has been invalidated due to intent change.
    /// Phase 2b: This is a status/metadata flag only. Real S3 quarantine move is Phase 3.
    pub invalidated: bool,
    pub invalidated_reason: Option<String>,
    /// Phase 2b: Quarantine status metadata. Actual artifact move to quarantine path is Phase 3.
    /// This field tracks intent-based quarantine signal without S3 integration.
    pub quarantine_signal: Option<QuarantineSignal>,
}

/// Phase 2b: Quarantine signal metadata for artifact invalidation.
///
/// This represents intent-driven quarantine signal, NOT actual S3 move.
/// Real quarantine action (S3 path change, blob deletion) is Phase 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineSignal {
    /// Why the artifact was quarantined
    pub reason: String,
    /// Which intent change triggered this
    pub intent_id: Uuid,
    /// Which version of the intent triggered this
    pub intent_version: i32,
    /// When the quarantine signal was raised
    pub signaled_at: DateTime<Utc>,
    /// Who/what initiated the quarantine (actor_id)
    pub initiated_by: String,
    /// Current status: signaled | acknowledged | released
    #[serde(default)]
    pub status: QuarantineStatus,
}

/// Phase 2b: Quarantine status enum.
///
/// Metadata only - real quarantine action (S3 move/delete) is Phase 3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineStatus {
    /// Quarantine signal raised but not yet acknowledged
    Signaled,
    /// Quarantine signal acknowledged, artifact effectively inaccessible
    Acknowledged,
    /// Artifact released from quarantine (intent resolved, rebase completed)
    Released,
}

impl Default for QuarantineStatus {
    fn default() -> Self {
        QuarantineStatus::Signaled
    }
}

/// Artifact quarantine status response for Phase 2b bounded read API.
///
/// Phase 2b: Returns metadata/status only. Real S3 quarantine move is Phase 3.
/// This allows callers to check artifact quarantine status without S3 integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactQuarantineStatus {
    pub artifact_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version: i32,
    /// Phase 2b: Whether artifact is effectively invalidated (due to intent change)
    pub invalidated: bool,
    pub invalidated_reason: Option<String>,
    /// Phase 2b: Quarantine status if quarantine signal was raised
    pub quarantine_signal: Option<QuarantineSignal>,
    /// Human-readable status description
    pub status_description: String,
}

impl Artifact {
    /// Phase 2b: Check if artifact is effectively invalidated due to intent change.
    ///
    /// Returns true if metadata.invalidated is true OR quarantine status is Acknowledged.
    pub fn is_invalidated(&self) -> bool {
        self.metadata.invalidated
            || self
                .metadata
                .quarantine_signal
                .as_ref()
                .map(|qs| qs.status == QuarantineStatus::Acknowledged)
                .unwrap_or(false)
    }

    /// Phase 2b: Generate quarantine status response from artifact.
    pub fn quarantine_status(&self) -> ArtifactQuarantineStatus {
        let status_description = if self.is_invalidated() {
            if let Some(ref qs) = self.metadata.quarantine_signal {
                format!("Quarantined: {} (status: {:?})", qs.reason, qs.status)
            } else {
                "Invalidated due to intent change".to_string()
            }
        } else {
            "Active - no quarantine signal".to_string()
        };

        ArtifactQuarantineStatus {
            artifact_id: self.id,
            intent_id: self.intent_id,
            intent_version: self.intent_version,
            invalidated: self.metadata.invalidated,
            invalidated_reason: self.metadata.invalidated_reason.clone(),
            quarantine_signal: self.metadata.quarantine_signal.clone(),
            status_description,
        }
    }
}
