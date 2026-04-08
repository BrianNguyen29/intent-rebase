//! Policy snapshot domain types for Phase 2 governance slice
//!
//! Provides point-in-time, immutable records of approval policy at time of intent approval.
//! This is bounded groundwork - S3 upload, scope canonicalization, and revalidation are out of scope.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Scope type indicating breadth of approval scope
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeType {
    Full,
    Partial,
    None,
}

/// Scope definition describing what requires approval
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScopeDefinition {
    /// Type of scope
    #[serde(rename = "scope_type")]
    pub scope_type: ScopeType,
    /// Resources affected by the intent
    #[serde(rename = "affected_resources")]
    pub affected_resources: Vec<serde_json::Value>,
    /// Approvers required for approval
    #[serde(rename = "required_approvers")]
    pub required_approvers: Vec<serde_json::Value>,
    /// Minimum number of approvals required
    #[serde(rename = "min_approvals")]
    pub min_approvals: i32,
}

impl Default for ScopeDefinition {
    fn default() -> Self {
        Self {
            scope_type: ScopeType::None,
            affected_resources: Vec::new(),
            required_approvers: Vec::new(),
            min_approvals: 1,
        }
    }
}

/// A policy snapshot - point-in-time record of approval policy at intent approval time.
///
/// Bounded slice: This is the database record. S3 blob storage and integrity verification
/// are out of scope for this slice.
///
/// See: docs/14-governance/03-policy-snapshot-spec.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySnapshot {
    /// Unique identifier for this snapshot
    pub id: Uuid,
    /// Tenant this snapshot belongs to
    #[serde(rename = "tenant_id")]
    pub tenant_id: Uuid,
    /// Intent this snapshot is associated with
    #[serde(rename = "intent_id")]
    pub intent_id: Uuid,
    /// Intent version this snapshot was created for
    #[serde(rename = "intent_version")]
    pub intent_version: i32,
    /// Rule pack version active at snapshot creation time
    #[serde(rename = "rule_pack_version")]
    pub rule_pack_version: String,
    /// Scope definition at snapshot creation time
    #[serde(rename = "scope_definition")]
    pub scope_definition: ScopeDefinition,
    /// SHA256 hash of scope_definition for integrity verification
    #[serde(rename = "scope_hash")]
    pub scope_hash: String,
    /// URI to the immutable snapshot blob (placeholder - S3 upload out of scope)
    #[serde(rename = "snapshot_uri")]
    pub snapshot_uri: String,
    /// When this snapshot was created
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    /// When scope was canonicalized (placeholder - canonicalization out of scope)
    #[serde(rename = "canonicalized_at")]
    pub canonicalized_at: DateTime<Utc>,
}

impl PolicySnapshot {
    /// Create a new PolicySnapshot with current timestamp.
    ///
    /// Note: snapshot_uri is a placeholder - actual S3 upload is out of scope for this slice.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        rule_pack_version: String,
        scope_definition: ScopeDefinition,
        scope_hash: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            intent_id,
            intent_version,
            rule_pack_version,
            scope_definition,
            scope_hash,
            // Placeholder URI - S3 upload is out of scope
            snapshot_uri: format!(
                "memory://policy-snapshots/{}/v{}",
                intent_id, intent_version
            ),
            created_at: now,
            canonicalized_at: now,
        }
    }

    /// Compute SHA256 hash of the scope definition for integrity verification.
    ///
    /// Note: JSON serialization is not canonically formatted (key ordering may vary).
    /// For production integrity verification, JSON should be canonically serialized
    /// before hashing to ensure consistent hashes across serializations.
    pub fn compute_scope_hash(scope_definition: &ScopeDefinition) -> String {
        use sha2::{Digest, Sha256};
        let json = serde_json::to_string(scope_definition).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

/// Request to create a new policy snapshot (used internally, not from API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicySnapshotRequest {
    #[serde(rename = "tenant_id")]
    pub tenant_id: Uuid,
    #[serde(rename = "intent_id")]
    pub intent_id: Uuid,
    #[serde(rename = "intent_version")]
    pub intent_version: i32,
    #[serde(rename = "rule_pack_version")]
    pub rule_pack_version: String,
    #[serde(rename = "scope_definition")]
    pub scope_definition: ScopeDefinition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_definition_default() {
        let scope = ScopeDefinition::default();
        assert_eq!(scope.scope_type, ScopeType::None);
        assert!(scope.affected_resources.is_empty());
        assert!(scope.required_approvers.is_empty());
        assert_eq!(scope.min_approvals, 1);
    }

    #[test]
    fn test_policy_snapshot_new() {
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let scope = ScopeDefinition::default();

        let snapshot = PolicySnapshot::new(
            tenant_id,
            intent_id,
            1,
            "v1.0.0".to_string(),
            scope.clone(),
            "abc123".to_string(),
        );

        assert_eq!(snapshot.tenant_id, tenant_id);
        assert_eq!(snapshot.intent_id, intent_id);
        assert_eq!(snapshot.intent_version, 1);
        assert_eq!(snapshot.rule_pack_version, "v1.0.0");
        assert_eq!(snapshot.scope_hash, "abc123");
        assert!(snapshot.snapshot_uri.contains("v1"));
    }

    #[test]
    fn test_compute_scope_hash() {
        let scope = ScopeDefinition {
            scope_type: ScopeType::Full,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex characters
    }

    #[test]
    fn test_scope_hash_diff_for_different_scopes() {
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Full,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_ne!(hash1, hash2);
    }
}
