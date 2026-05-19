use chrono::{DateTime, Utc};
use intent_rebase_types::{PolicySnapshot, ScopeDefinition};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Policy Snapshot Types
// =============================================================================

/// Query parameters for getting policy snapshot by ID
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting latest policy snapshot by intent
#[derive(Debug, Deserialize)]
pub struct GetLatestPolicySnapshotQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for getting policy snapshot by intent version
#[derive(Debug, Deserialize)]
pub struct GetPolicySnapshotByVersionQuery {
    pub tenant_id: Uuid,
}

/// Query parameters for listing policy snapshots by intent
#[derive(Debug, Deserialize)]
pub struct ListPolicySnapshotsQuery {
    pub tenant_id: Uuid,
}

/// Response type for a single policy snapshot
#[derive(Debug, Serialize)]
pub struct PolicySnapshotResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub intent_version: i32,
    pub rule_pack_version: String,
    pub scope_definition: ScopeDefinition,
    pub scope_hash: String,
    pub snapshot_uri: String,
    pub created_at: DateTime<Utc>,
    pub canonicalized_at: DateTime<Utc>,
}

impl From<PolicySnapshot> for PolicySnapshotResponse {
    fn from(s: PolicySnapshot) -> Self {
        Self {
            id: s.id,
            tenant_id: s.tenant_id,
            intent_id: s.intent_id,
            intent_version: s.intent_version,
            rule_pack_version: s.rule_pack_version,
            scope_definition: s.scope_definition,
            scope_hash: s.scope_hash,
            snapshot_uri: s.snapshot_uri,
            created_at: s.created_at,
            canonicalized_at: s.canonicalized_at,
        }
    }
}

/// Response for listing policy snapshots
#[derive(Debug, Serialize)]
pub struct ListPolicySnapshotsResponse {
    pub policy_snapshots: Vec<PolicySnapshotResponse>,
    pub total: usize,
}
