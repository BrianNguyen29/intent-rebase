//! Rebase Engine — computes semantic diffs and rebase plans
//!
//! Phase 1: Structured diff core implemented for scope, constraints,
//! acceptance_criteria, and authority sections.

pub mod diff;

use intent_rebase_types::{IntentRebaseError, IntentVersion};

pub use diff::{
    AcceptanceCriteriaDiff, AuthorityDiff, ConstraintsDiff, IntentVersionDiff, ScopeDiff,
};

/// RebaseEngine computes semantic diffs and generates rebase plans
pub struct RebaseEngine;

impl RebaseEngine {
    pub fn new() -> Self {
        Self
    }

    /// Compute semantic diff between two intent versions (Phase 1)
    ///
    /// Covers only the following sections:
    /// - scope
    /// - constraints
    /// - acceptance_criteria
    /// - authority
    ///
    /// Returns structured diff with deterministic output ordering.
    pub async fn compute_diff(
        &self,
        from_version: &IntentVersion,
        to_version: &IntentVersion,
    ) -> Result<IntentVersionDiff, IntentRebaseError> {
        // Validate version ordering
        if to_version.version_number <= from_version.version_number {
            return Err(IntentRebaseError::InvalidIntentVersion(format!(
                "to_version ({}) must be greater than from_version ({})",
                to_version.version_number, from_version.version_number
            )));
        }

        if from_version.intent_id != to_version.intent_id {
            return Err(IntentRebaseError::InvalidIntentVersion(
                "Cannot diff versions from different intents".into(),
            ));
        }

        Ok(diff::diff_intent_version(from_version, to_version))
    }

    /// Compute diff from raw JSON inputs (for backward compatibility)
    ///
    /// This is a convenience wrapper that deserializes the versions and calls compute_diff.
    pub async fn compute_diff_raw(
        &self,
        from_version_json: serde_json::Value,
        to_version_json: serde_json::Value,
    ) -> Result<IntentVersionDiff, IntentRebaseError> {
        let from_version: IntentVersion = serde_json::from_value(from_version_json)
            .map_err(|e| IntentRebaseError::SerializationError(e.to_string()))?;
        let to_version: IntentVersion = serde_json::from_value(to_version_json)
            .map_err(|e| IntentRebaseError::SerializationError(e.to_string()))?;

        self.compute_diff(&from_version, &to_version).await
    }

    /// Generate a rebase plan (Phase 1 — stub, returns not_yet_implemented)
    ///
    /// Note: Rebase planning is not yet implemented. This will return an error
    /// until Phase 2 when the graph model and planner are added.
    pub async fn generate_plan(
        &self,
        _diff: IntentVersionDiff,
    ) -> Result<serde_json::Value, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Rebase planning not yet implemented (deferred to Phase 2)".into(),
        ))
    }
}

impl Default for RebaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute diff directly without using RebaseEngine instance
pub fn compute_diff_sync(
    from_version: &IntentVersion,
    to_version: &IntentVersion,
) -> Result<IntentVersionDiff, IntentRebaseError> {
    if to_version.version_number <= from_version.version_number {
        return Err(IntentRebaseError::InvalidIntentVersion(format!(
            "to_version ({}) must be greater than from_version ({})",
            to_version.version_number, from_version.version_number
        )));
    }

    if from_version.intent_id != to_version.intent_id {
        return Err(IntentRebaseError::InvalidIntentVersion(
            "Cannot diff versions from different intents".into(),
        ));
    }

    Ok(diff::diff_intent_version(from_version, to_version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::*;
    use uuid::Uuid;

    fn create_test_version(intent_id: Uuid, version_num: i32) -> IntentVersion {
        IntentVersion {
            id: Uuid::new_v4(),
            intent_id,
            version_number: version_num,
            parent_version_id: None,
            created_at: chrono::Utc::now(),
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            change_reason: "test".to_string(),
            change_channel: ChangeChannel::UserEdit,
            status: VersionStatus::Active,
            hash: "test_hash".to_string(),
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test objective".to_string(),
                    success_statement: "Test success".to_string(),
                    domain: "test".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 0.9,
                },
            },
        }
    }

    #[test]
    fn test_engine_constructs() {
        let _ = RebaseEngine::new();
    }

    #[tokio::test]
    async fn test_compute_diff_no_change() {
        let engine = RebaseEngine::new();
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope = vec!["item1".to_string()]; // Same as v1

        let diff = engine.compute_diff(&v1, &v2).await.unwrap();
        assert!(diff.scope.in_scope.added.is_empty());
        assert!(diff.scope.in_scope.removed.is_empty());
    }

    #[tokio::test]
    async fn test_compute_diff_with_change() {
        let engine = RebaseEngine::new();
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope = vec!["item1".to_string(), "item2".to_string()];

        let diff = engine.compute_diff(&v1, &v2).await.unwrap();
        assert_eq!(diff.scope.in_scope.added, vec!["item2"]);
        assert!(diff.scope.in_scope.removed.is_empty());
    }

    #[tokio::test]
    async fn test_compute_diff_version_order_error() {
        let engine = RebaseEngine::new();
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 2);
        let v2 = create_test_version(intent_id, 1);

        let result = engine.compute_diff(&v1, &v2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compute_diff_different_intent_error() {
        let engine = RebaseEngine::new();
        let v1 = create_test_version(Uuid::new_v4(), 1);
        let v2 = create_test_version(Uuid::new_v4(), 2);

        let result = engine.compute_diff(&v1, &v2).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_diff_sync() {
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope = vec!["item2".to_string()];

        let diff = compute_diff_sync(&v1, &v2).unwrap();
        assert_eq!(diff.scope.in_scope.added, vec!["item2"]);
        assert_eq!(diff.scope.in_scope.removed, vec!["item1"]);
    }
}
