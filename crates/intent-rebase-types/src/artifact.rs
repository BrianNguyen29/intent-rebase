//! Artifact domain types

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
    pub invalidated: bool,
    pub invalidated_reason: Option<String>,
}
