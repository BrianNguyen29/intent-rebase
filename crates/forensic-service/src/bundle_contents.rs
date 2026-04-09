//! Bundle contents summary
//!
//! See [../../../../docs/14-governance/10-forensic-bundle.md] for full specification.

use serde::{Deserialize, Serialize};

/// Summary of contents included in a forensic bundle.
///
/// **Batch 0 scope:** type scaffold only. Actual content collection is Batch 3.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleContents {
    /// Number of intent versions included
    pub intent_versions: usize,
    /// Number of artifact metadata entries included
    pub artifacts: usize,
    /// Number of approval records included
    pub approvals: usize,
    /// Number of audit events included
    pub audit_events: usize,
    /// Number of policy snapshots included
    pub policy_snapshots: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_contents_default() {
        let contents = BundleContents::default();
        assert_eq!(contents.intent_versions, 0);
        assert_eq!(contents.artifacts, 0);
        assert_eq!(contents.approvals, 0);
        assert_eq!(contents.audit_events, 0);
        assert_eq!(contents.policy_snapshots, 0);
    }

    #[test]
    fn test_bundle_contents_serialization() {
        let contents = BundleContents {
            intent_versions: 10,
            artifacts: 25,
            approvals: 5,
            audit_events: 5000,
            policy_snapshots: 3,
        };

        let json = serde_json::to_string(&contents).unwrap();
        let deserialized: BundleContents = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.intent_versions, 10);
        assert_eq!(deserialized.artifacts, 25);
    }
}
