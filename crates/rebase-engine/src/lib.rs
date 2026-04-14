//! Rebase Engine — computes semantic diffs and rebase plans
//!
//! Phase 1: Structured diff core implemented for scope, constraints,
//! acceptance_criteria, and authority sections. Risk analysis (severity,
//! confidence, manual-review triggers) is implemented in the `risk` and `rules` modules.
//!
//! Rebase planning (preview-only baseline) is implemented in the `planner` module.

pub mod approval_revalidation;
pub mod diff;
pub mod planner;
pub mod risk;
pub mod rule_pack;
pub mod rule_pack_registry;
pub mod rules;

use intent_rebase_types::{IntentRebaseError, IntentVersion};

pub use approval_revalidation::{classify_approvals, ApprovalRevalidationResult};
pub use diff::{
    AcceptanceCriteriaDiff, AuthorityDiff, ConstraintsDiff, IntentVersionDiff, ScopeDiff,
};
pub use planner::{
    AffectedItemsPreview, ApprovalNeedingRevalidation, ApprovalRevalidation, CheckpointCandidate,
    CheckpointSelection, CompensationAction, CompensationPlanningSummary, CompensationReadiness,
    DecisionClass, DeferredFields, RebasePlan, RevalidationStrategy, RiskTier, SectionDecision,
};
pub use risk::{DiffRiskAnalysis, ManualReviewReason, RiskConfig, Severity};
pub use rule_pack::{RulePack, RulePackVersion, DEFAULT_RULE_PACK};
pub use rule_pack_registry::{
    InMemoryTenantRulePackRepository, RulePackRegistryError, TenantRulePackRepository,
};
pub use rules::{analyze_diff_risk, analyze_diff_risk_with_config};

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

    /// Compute semantic diff with risk analysis between two intent versions
    ///
    /// This is the primary entry point for Phase 1 diff operations.
    /// Returns both the structured diff and risk analysis (severity, confidence, manual-review).
    pub async fn compute_diff_with_risk(
        &self,
        from_version: &IntentVersion,
        to_version: &IntentVersion,
    ) -> Result<(IntentVersionDiff, DiffRiskAnalysis), IntentRebaseError> {
        let diff = self.compute_diff(from_version, to_version).await?;
        let risk = rules::analyze_diff_risk(
            &diff.scope,
            &diff.constraints,
            &diff.acceptance_criteria,
            &diff.authority,
        );
        Ok((diff, risk))
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

    /// Compute diff with risk from raw JSON inputs
    ///
    /// Convenience wrapper that deserializes versions and calls compute_diff_with_risk.
    pub async fn compute_diff_with_risk_raw(
        &self,
        from_version_json: serde_json::Value,
        to_version_json: serde_json::Value,
    ) -> Result<(IntentVersionDiff, DiffRiskAnalysis), IntentRebaseError> {
        let from_version: IntentVersion = serde_json::from_value(from_version_json)
            .map_err(|e| IntentRebaseError::SerializationError(e.to_string()))?;
        let to_version: IntentVersion = serde_json::from_value(to_version_json)
            .map_err(|e| IntentRebaseError::SerializationError(e.to_string()))?;

        self.compute_diff_with_risk(&from_version, &to_version)
            .await
    }

    /// Generate a rebase plan (Phase 1 — preview-only baseline)
    ///
    /// Phase 1 baseline implements preview-only planning that maps diff+risk
    /// analysis to decision classes A-E without graph integration.
    ///
    /// The planner uses:
    /// - `IntentVersionDiff` for structured diff output
    /// - `DiffRiskAnalysis` (via `compute_diff_with_risk`) for risk metrics
    ///
    /// Returns a typed `RebasePlan` with:
    /// - Decision class (A-E)
    /// - Rationale and section decisions
    /// - Affected items preview (empty in Phase 1 baseline — TODO for Phase 2)
    /// - Deferred fields (TODO markers for Phase 2)
    ///
    /// Note: This is a preview-only implementation. Full rebase planning with
    /// graph integration, checkpoint selection, and approval revalidation is
    /// deferred to Phase 2.
    pub async fn generate_plan(
        &self,
        diff: IntentVersionDiff,
    ) -> Result<RebasePlan, IntentRebaseError> {
        // Run risk analysis to get severity and confidence
        let risk = rules::analyze_diff_risk(
            &diff.scope,
            &diff.constraints,
            &diff.acceptance_criteria,
            &diff.authority,
        );

        // Generate typed rebase plan from diff and risk analysis
        Ok(RebasePlan::from_diff_and_risk(&diff, &risk))
    }

    /// Generate a rebase plan from diff with explicit risk analysis
    ///
    /// This is a convenience wrapper that accepts pre-computed risk analysis.
    pub async fn generate_plan_with_risk(
        &self,
        diff: IntentVersionDiff,
        risk: DiffRiskAnalysis,
    ) -> Result<RebasePlan, IntentRebaseError> {
        Ok(RebasePlan::from_diff_and_risk(&diff, &risk))
    }
}

impl Default for RebaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute diff directly without using RebaseEngine instance
#[tracing::instrument(skip(from_version, to_version), fields(intent_id = %from_version.intent_id, from_version = from_version.version_number, to_version = to_version.version_number))]
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

/// Compute diff with risk analysis directly without using RebaseEngine instance
#[tracing::instrument(skip(from_version, to_version), fields(intent_id = %from_version.intent_id, from_version = from_version.version_number, to_version = to_version.version_number))]
pub fn compute_diff_with_risk_sync(
    from_version: &IntentVersion,
    to_version: &IntentVersion,
) -> Result<(IntentVersionDiff, DiffRiskAnalysis), IntentRebaseError> {
    let diff = compute_diff_sync(from_version, to_version)?;
    let risk = rules::analyze_diff_risk(
        &diff.scope,
        &diff.constraints,
        &diff.acceptance_criteria,
        &diff.authority,
    );
    Ok((diff, risk))
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

    #[test]
    fn test_compute_diff_with_risk_sync_no_change() {
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let v2 = create_test_version(intent_id, 2);

        let (diff, risk) = compute_diff_with_risk_sync(&v1, &v2).unwrap();
        assert!(diff.scope.in_scope.added.is_empty());
        assert!(risk.severity == Severity::Low);
        assert_eq!(risk.confidence, 1.0);
        assert!(!risk.manual_review);
    }

    #[test]
    fn test_compute_diff_with_risk_sync_scope_change() {
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope = vec!["item1".to_string(), "item2".to_string()];

        let (diff, risk) = compute_diff_with_risk_sync(&v1, &v2).unwrap();
        assert_eq!(diff.scope.in_scope.added, vec!["item2"]);
        assert!(risk.severity == Severity::Medium); // Scope additions are medium
        assert!(risk.confidence < 1.0); // Scope items don't have clause_ids
    }

    #[tokio::test]
    async fn test_compute_diff_with_risk_async() {
        let engine = RebaseEngine::new();
        let intent_id = Uuid::new_v4();
        let v1 = create_test_version(intent_id, 1);
        let mut v2 = create_test_version(intent_id, 2);
        v2.payload.scope.in_scope = vec!["item2".to_string()];

        let (diff, risk) = engine.compute_diff_with_risk(&v1, &v2).await.unwrap();
        assert_eq!(diff.scope.in_scope.added, vec!["item2"]);
        assert_eq!(risk.severity, Severity::Medium);
    }

    #[tokio::test]
    async fn test_compute_diff_with_risk_raw_async() {
        let engine = RebaseEngine::new();
        let v1_json = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "intent_id": Uuid::new_v4().to_string(),
            "version_number": 1,
            "parent_version_id": null,
            "created_at": "2024-01-01T00:00:00Z",
            "created_by": { "actor_type": "user", "actor_id": "test" },
            "change_reason": "test",
            "change_channel": "user_edit",
            "status": "active",
            "hash": "hash1",
            "payload": {
                "objective": { "summary": "Test", "success_statement": "Success", "domain": "test" },
                "scope": { "in_scope": [], "out_of_scope": [] },
                "constraints": { "functional": [], "non_functional": [], "policy": [], "budget": [], "time": [] },
                "acceptance_criteria": { "required": [], "optional": [] },
                "authority": { "allowed_actions": [], "forbidden_actions": [], "approval_requirements": [] },
                "preferences": { "tradeoffs": [] },
                "references": { "specs": [], "tickets": [], "repos": [], "policies": [] },
                "assumptions": { "explicit": [] },
                "metadata": { "risk_tier": "low", "urgency": "low", "confidence": 0.9 }
            }
        });
        let v2_json = v1_json.clone();

        // With same versions (both version 1), should fail with "must be greater than"
        let result = engine.compute_diff_with_risk_raw(v1_json, v2_json).await;
        assert!(result.is_err()); // version 1 is not greater than version 1
        let err = result.unwrap_err();
        assert!(err.to_string().contains("greater than"));
    }
}
