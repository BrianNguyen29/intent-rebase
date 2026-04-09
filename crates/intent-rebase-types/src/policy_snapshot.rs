//! Policy snapshot domain types for Phase 2 governance slice
//!
//! Provides point-in-time, immutable records of approval policy at time of intent approval.
//! This is bounded groundwork - S3 upload and revalidation are out of scope.
//! Scope canonicalization is implemented to ensure deterministic hashing.

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
    /// When scope was canonicalized (canonical JSON hashing implemented; S3/revalidation future)
    #[serde(rename = "canonicalized_at")]
    pub canonicalized_at: DateTime<Utc>,
}

impl PolicySnapshot {
    /// Create a new PolicySnapshot with current timestamp.
    ///
    /// The scope_hash is computed internally using canonical JSON serialization
    /// to ensure deterministic hashes regardless of input key ordering.
    ///
    /// Note: snapshot_uri is a placeholder - actual S3 upload is out of scope for this slice.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        intent_id: Uuid,
        intent_version: i32,
        rule_pack_version: String,
        scope_definition: ScopeDefinition,
    ) -> Self {
        let now = Utc::now();
        let scope_hash = Self::compute_scope_hash(&scope_definition);
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
    /// Uses canonical JSON serialization to ensure deterministic hashes:
    /// - Object keys are sorted alphabetically at each level
    /// - Array elements are sorted if they are objects (by canonical form)
    /// This ensures semantically equivalent scopes with different key ordering
    /// produce identical hashes.
    pub fn compute_scope_hash(scope_definition: &ScopeDefinition) -> String {
        use sha2::{Digest, Sha256};
        let canonical = canonicalize_scope_definition(scope_definition);
        let json =
            serde_json::to_string(&canonical).expect("canonical serialization should not fail");
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

/// Canonicalize a ScopeDefinition for deterministic hashing.
///
/// Sorts object keys alphabetically and sorts array elements that are objects
/// to ensure consistent serialization regardless of input key ordering.
///
/// Limits: Only handles JSON object key ordering and array element ordering for objects.
/// Does not handle arbitrary JSON schema canonicalization beyond this.
fn canonicalize_scope_definition(scope: &ScopeDefinition) -> serde_json::Value {
    serde_json::json!({
        "affected_resources": canonicalize_array_sorted(&scope.affected_resources),
        "min_approvals": scope.min_approvals,
        "required_approvers": canonicalize_array_sorted(&scope.required_approvers),
        "scope_type": scope.scope_type,
    })
}

/// Canonicalize an array by sorting object elements by their canonical form.
/// Non-object elements are preserved in original order.
fn canonicalize_array_sorted(arr: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut canonical: Vec<serde_json::Value> = arr.iter().map(canonicalize_json_value).collect();
    // Sort object elements by their canonical form for deterministic ordering
    canonical.sort_by(|a, b| {
        let ca = serde_json::to_string(a).unwrap_or_default();
        let cb = serde_json::to_string(b).unwrap_or_default();
        ca.cmp(&cb)
    });
    canonical
}

/// Recursively canonicalize a JSON value by sorting object keys.
fn canonicalize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let canonical: serde_json::Map<String, serde_json::Value> = sorted
                .into_iter()
                .map(|(k, v)| (k.clone(), canonicalize_json_value(v)))
                .collect();
            serde_json::Value::Object(canonical)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json_value).collect())
        }
        _ => value.clone(),
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
        let expected_hash = PolicySnapshot::compute_scope_hash(&scope);

        let snapshot =
            PolicySnapshot::new(tenant_id, intent_id, 1, "v1.0.0".to_string(), scope.clone());

        assert_eq!(snapshot.tenant_id, tenant_id);
        assert_eq!(snapshot.intent_id, intent_id);
        assert_eq!(snapshot.intent_version, 1);
        assert_eq!(snapshot.rule_pack_version, "v1.0.0");
        assert_eq!(snapshot.scope_hash, expected_hash);
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

    #[test]
    fn test_scope_hash_same_for_ordering_difference_in_resources() {
        // Two scopes with same resources but different JSON key ordering
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![
                serde_json::json!({"type": "artifact", "id": "a"}),
                serde_json::json!({"type": "workflow", "id": "b"}),
            ],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![
                serde_json::json!({"id": "b", "type": "workflow"}),
                serde_json::json!({"id": "a", "type": "artifact"}),
            ],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_eq!(
            hash1, hash2,
            "Same resources with different key ordering should hash identically"
        );
    }

    #[test]
    fn test_scope_hash_same_for_ordering_difference_in_approvers() {
        // Two scopes with same approvers but different JSON key ordering
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![
                serde_json::json!({"type": "role", "id": "admin"}),
                serde_json::json!({"type": "user", "id": "alice"}),
            ],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![
                serde_json::json!({"id": "alice", "type": "user"}),
                serde_json::json!({"id": "admin", "type": "role"}),
            ],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_eq!(
            hash1, hash2,
            "Same approvers with different key ordering should hash identically"
        );
    }

    #[test]
    fn test_scope_hash_same_for_different_array_order() {
        // Two scopes with same elements but different array order
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![
                serde_json::json!({"type": "artifact", "id": "a"}),
                serde_json::json!({"type": "artifact", "id": "b"}),
            ],
            required_approvers: vec![serde_json::json!({"type": "role", "id": "admin"})],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![
                serde_json::json!({"type": "artifact", "id": "b"}),
                serde_json::json!({"type": "artifact", "id": "a"}),
            ],
            required_approvers: vec![serde_json::json!({"type": "role", "id": "admin"})],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_eq!(
            hash1, hash2,
            "Same elements in different array order should hash identically"
        );
    }

    #[test]
    fn test_scope_hash_diff_for_different_content() {
        // Scopes with different content must have different hashes
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![serde_json::json!({"type": "artifact", "id": "a"})],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![serde_json::json!({"type": "artifact", "id": "b"})],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_ne!(
            hash1, hash2,
            "Different resources should have different hashes"
        );
    }

    #[test]
    fn test_scope_hash_different_min_approvals() {
        let scope1 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 1,
        };

        let scope2 = ScopeDefinition {
            scope_type: ScopeType::Partial,
            affected_resources: vec![],
            required_approvers: vec![],
            min_approvals: 2,
        };

        let hash1 = PolicySnapshot::compute_scope_hash(&scope1);
        let hash2 = PolicySnapshot::compute_scope_hash(&scope2);

        assert_ne!(
            hash1, hash2,
            "Different min_approvals should have different hashes"
        );
    }
}
