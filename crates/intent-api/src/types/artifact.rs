use intent_rebase_types::{ExternalRef, GraphEdge, GraphNode, SideEffectCaptureContext};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Artifact Ingest Types
// =============================================================================

/// Request body for artifact ingest with optional side effect capture
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactIngestRequest {
    /// Tenant scope
    pub tenant_id: Uuid,
    /// Workflow scope
    pub workflow_id: Uuid,
    /// External reference to the artifact (e.g., from artifact service)
    pub external_ref: ExternalRef,
    /// Human-readable label for the artifact
    pub label: String,
    /// IntentVersion node IDs this artifact depends on
    pub depends_on_intent_versions: Vec<Uuid>,
    /// Optional properties to attach to the artifact node
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
    /// Phase 3 Batch 1 (groundwork): Optional context for side effect capture.
    /// When provided with sufficient fields, enables capture-on-write to the
    /// compensation ledger after successful artifact ingest.
    #[serde(default)]
    pub side_effect_context: Option<SideEffectCaptureContext>,
}

/// Response for artifact ingest with side effect capture result
#[derive(Debug, Serialize)]
pub struct ArtifactIngestResponse {
    pub node: GraphNode,
    pub edges: Vec<GraphEdge>,
    /// Phase 3 Batch 1 (groundwork): Indicates whether a side effect was recorded
    pub side_effect_recorded: bool,
    pub side_effect_id: Option<Uuid>,
}
