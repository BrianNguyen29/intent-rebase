//! Rule pack module for diff governance configuration
//!
//! This module provides versioned rule pack support for deterministic diff risk analysis.
//! Rule packs are immutable snapshots that contain:
//! - Rule pack version metadata
//! - Risk analysis thresholds
//!
//! Rule packs enable:
//! - Audit trail: which rules were used for a given diff
//! - Rollback: revert to previous rule version if issues detected
//! - Deterministic replay: same input + same rule pack = same output

use intent_rebase_types::{EdgeDirection, EdgeType, NodeType, PropagationConfig};
use serde::{Deserialize, Serialize};

/// Rule pack version identifier
///
/// Format: "v{major}.{minor}.{patch}"
/// - Major: Breaking changes to rule logic
/// - Minor: New rules or thresholds, backward compatible
/// - Patch: Bug fixes, documentation changes
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RulePackVersion(pub String);

impl RulePackVersion {
    /// Parse a version string into a RulePackVersion
    pub fn parse(s: &str) -> Option<Self> {
        if s.starts_with('v') && s.len() >= 4 {
            let parts: Vec<&str> = s[1..].split('.').collect();
            if parts.len() == 3 {
                // Validate all parts are numeric
                if parts.iter().all(|p| p.parse::<u32>().is_ok()) {
                    return Some(Self(s.to_string()));
                }
            }
        }
        None
    }

    /// Get the current stable rule pack version for Phase 1
    pub fn current() -> Self {
        Self("v1.0.0".to_string())
    }
}

impl Default for RulePackVersion {
    fn default() -> Self {
        Self::current()
    }
}

impl std::fmt::Display for RulePackVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Rule pack status in the lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RulePackStatus {
    /// Draft version, not for production use
    Draft,
    /// Active version, recommended for use
    Active,
    /// Deprecated, use newer version
    Deprecated,
    /// Superseded by a newer version
    Superseded,
}

/// Core risk configuration within a rule pack
///
/// This determines how risk is computed for diffs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePackRiskConfig {
    /// Minimum confidence threshold (0.0-1.0)
    /// Below this threshold, manual review is recommended
    pub confidence_threshold: f64,

    /// Maximum number of high-severity section changes
    /// before triggering manual review
    pub max_high_severity_sections: usize,
}

/// Propagation configuration within a rule pack (PR #13 baseline)
///
/// This determines how impact propagates through the dependency graph.
///
/// Phase 1 baseline uses deterministic explicit rules matching the prior
/// hardcoded behavior. Future PRs may introduce rule-pack-driven propagation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RulePackPropagationConfig {
    /// Maximum traversal depth (default: 3)
    pub max_depth: Option<usize>,
    /// Edge types to traverse (default: DependsOn, Triggers, GeneratedFrom)
    pub traversable_edge_types: Vec<EdgeType>,
    /// Directions to traverse (default: Both)
    pub traversable_directions: Vec<EdgeDirection>,
    /// Target node types for classification (default: Artifact, Approval, SideEffect, Generic)
    pub target_node_types: Vec<NodeType>,
}

impl Default for RulePackPropagationConfig {
    fn default() -> Self {
        Self {
            max_depth: Some(3),
            traversable_edge_types: vec![
                EdgeType::DependsOn,
                EdgeType::Triggers,
                EdgeType::GeneratedFrom,
            ],
            traversable_directions: vec![EdgeDirection::Both],
            target_node_types: vec![
                NodeType::Artifact,
                NodeType::Approval,
                NodeType::SideEffect,
                NodeType::Generic,
            ],
        }
    }
}

impl Default for RulePackRiskConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.7,
            max_high_severity_sections: 3,
        }
    }
}

impl From<&super::risk::RiskConfig> for RulePackRiskConfig {
    fn from(risk_config: &super::risk::RiskConfig) -> Self {
        Self {
            confidence_threshold: risk_config.confidence_threshold,
            max_high_severity_sections: risk_config.max_high_severity_before_manual_review,
        }
    }
}

impl From<&RulePackRiskConfig> for super::risk::RiskConfig {
    fn from(pack_config: &RulePackRiskConfig) -> Self {
        Self {
            confidence_threshold: pack_config.confidence_threshold,
            max_high_severity_before_manual_review: pack_config.max_high_severity_sections,
        }
    }
}

/// A versioned rule pack containing diff governance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePack {
    /// Rule pack version identifier
    pub version: RulePackVersion,

    /// Human-readable pack name
    pub name: String,

    /// Pack status
    pub status: RulePackStatus,

    /// Risk analysis configuration
    pub risk: RulePackRiskConfig,

    /// Propagation configuration (PR #13 baseline)
    /// Note: #[serde(default)] allows loading packs without this field (backward compat)
    #[serde(default)]
    pub propagation: RulePackPropagationConfig,

    /// Pack description
    pub description: Option<String>,
}

impl RulePack {
    /// Create a new rule pack with default configuration
    pub fn new(name: &str) -> Self {
        Self {
            version: RulePackVersion::current(),
            name: name.to_string(),
            status: RulePackStatus::Active,
            risk: RulePackRiskConfig::default(),
            propagation: RulePackPropagationConfig::default(),
            description: Some(format!("Default rule pack v{}", RulePackVersion::current())),
        }
    }

    /// Create a rule pack with custom risk configuration
    pub fn with_risk_config(name: &str, risk: RulePackRiskConfig) -> Self {
        Self {
            version: RulePackVersion::current(),
            name: name.to_string(),
            status: RulePackStatus::Active,
            risk,
            propagation: RulePackPropagationConfig::default(),
            description: Some(format!("Custom rule pack v{}", RulePackVersion::current())),
        }
    }

    /// Serialize rule pack to JSON bytes
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    /// Deserialize rule pack from JSON bytes
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Load rule pack from a JSON file path
    pub fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let pack: RulePack = serde_json::from_str(&contents)?;
        Ok(pack)
    }

    /// Get the RiskConfig for use with diff risk analysis
    pub fn risk_config(&self) -> super::risk::RiskConfig {
        (&self.risk).into()
    }

    /// Get the PropagationConfig for use with impact propagation (PR #13)
    pub fn propagation_config(&self) -> PropagationConfig {
        PropagationConfig {
            max_depth: self.propagation.max_depth,
            traversable_edge_types: self.propagation.traversable_edge_types.clone(),
            traversable_directions: self.propagation.traversable_directions.clone(),
            target_node_types: self.propagation.target_node_types.clone(),
        }
    }
}

impl Default for RulePack {
    fn default() -> Self {
        Self::new("default")
    }
}

/// Rule pack for deterministic diff risk analysis
///
/// This is the Phase 1 default rule pack. It provides:
/// - Confidence threshold: 0.7 (70%)
/// - Max high-severity sections: 3
pub static DEFAULT_RULE_PACK: once_cell::sync::Lazy<RulePack> =
    once_cell::sync::Lazy::new(|| RulePack::new("default-diff-v1"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_pack_version_parse() {
        let v = RulePackVersion::parse("v1.0.0").unwrap();
        assert_eq!(v.0, "v1.0.0");

        let v = RulePackVersion::parse("v0.1.0").unwrap();
        assert_eq!(v.0, "v0.1.0");
    }

    #[test]
    fn test_rule_pack_version_parse_invalid() {
        assert!(RulePackVersion::parse("1.0.0").is_none());
        assert!(RulePackVersion::parse("v1.0").is_none());
        assert!(RulePackVersion::parse("v1.a.0").is_none());
        assert!(RulePackVersion::parse("").is_none());
    }

    #[test]
    fn test_rule_pack_version_ordering() {
        let v1 = RulePackVersion::parse("v1.0.0").unwrap();
        let v2 = RulePackVersion::parse("v1.0.1").unwrap();
        let v3 = RulePackVersion::parse("v2.0.0").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }

    #[test]
    fn test_rule_pack_default() {
        let pack = RulePack::default();
        assert_eq!(pack.name, "default");
        assert_eq!(pack.status, RulePackStatus::Active);
        assert_eq!(pack.risk.confidence_threshold, 0.7);
        assert_eq!(pack.risk.max_high_severity_sections, 3);
    }

    #[test]
    fn test_rule_pack_serialization() {
        let pack = RulePack::new("test-pack");
        let json = pack.to_json().unwrap();
        let deserialized = RulePack::from_json(&json).unwrap();

        assert_eq!(deserialized.name, pack.name);
        assert_eq!(deserialized.version, pack.version);
        assert_eq!(
            deserialized.risk.confidence_threshold,
            pack.risk.confidence_threshold
        );
    }

    #[test]
    fn test_rule_pack_risk_config_conversion() {
        let risk_config = super::super::risk::RiskConfig {
            confidence_threshold: 0.8,
            max_high_severity_before_manual_review: 5,
        };

        let pack_config: RulePackRiskConfig = (&risk_config).into();
        assert_eq!(pack_config.confidence_threshold, 0.8);
        assert_eq!(pack_config.max_high_severity_sections, 5);

        let back_to_risk: super::super::risk::RiskConfig = (&pack_config).into();
        assert_eq!(back_to_risk.confidence_threshold, 0.8);
        assert_eq!(back_to_risk.max_high_severity_before_manual_review, 5);
    }

    #[test]
    fn test_rule_pack_to_json_bytes() {
        let pack = RulePack::new("test");
        let bytes = pack.to_json().unwrap();
        assert!(!bytes.is_empty());
        assert!(String::from_utf8_lossy(&bytes).contains("test"));
    }

    #[test]
    fn test_default_rule_pack_static() {
        // Verify the DEFAULT_RULE_PACK static is properly initialized
        let pack = crate::DEFAULT_RULE_PACK.clone();
        assert_eq!(pack.name, "default-diff-v1");
        assert_eq!(pack.status, RulePackStatus::Active);
        assert_eq!(pack.risk.confidence_threshold, 0.7);
    }

    #[test]
    fn test_rule_pack_custom_threshold() {
        // Create a rule pack with custom thresholds for strict review
        let pack = RulePack::with_risk_config(
            "strict",
            RulePackRiskConfig {
                confidence_threshold: 0.9,     // Very high threshold
                max_high_severity_sections: 1, // Any high-severity triggers review
            },
        );

        let risk_config = pack.risk_config();
        assert_eq!(risk_config.confidence_threshold, 0.9);
        assert_eq!(risk_config.max_high_severity_before_manual_review, 1);
    }

    #[test]
    fn test_rule_pack_version_current() {
        let version = RulePackVersion::current();
        assert_eq!(version.0, "v1.0.0");
    }

    #[test]
    fn test_rule_pack_version_display() {
        let version = RulePackVersion::parse("v2.3.4").unwrap();
        assert_eq!(format!("{}", version), "v2.3.4");
    }
}

/// Integration tests for fixture loading
#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::diff::{
        AcceptanceCriteriaDiff, AuthorityDiff, ConstraintsDiff, ScopeDiff, ScopeItemsDiff,
    };
    use crate::risk::Severity;
    use crate::rules::analyze_diff_risk_with_config;
    use std::path::PathBuf;

    /// Fixture corpus test types - these deserialize the fixture JSON files
    #[derive(Debug, Clone, serde::Deserialize)]
    struct FixtureCorpusDiff {
        scope: ScopeDiff,
        constraints: ConstraintsDiff,
        acceptance_criteria: AcceptanceCriteriaDiff,
        authority: AuthorityDiff,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    struct FixtureCorpusExpected {
        severity: String,
        confidence: f64,
        manual_review: bool,
        section_count: usize,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    #[allow(dead_code)]
    struct FixtureCorpusEntry {
        name: String,
        description: String,
        rule_pack: String,
        input: FixtureCorpusDiff,
        expected: FixtureCorpusExpected,
    }

    /// Get the path to a fixture file
    fn fixture_path(name: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("fixtures");
        path.push(name);
        path.set_extension("json");
        path
    }

    /// Load a rule pack from fixtures directory
    fn load_rule_pack(name: &str) -> RulePack {
        let path = fixture_path(name);
        RulePack::from_file(&path).unwrap_or_else(|_| {
            // If file doesn't exist, return default pack
            RulePack::default()
        })
    }

    /// Load a fixture corpus entry from the fixtures directory
    fn load_fixture_corpus(name: &str) -> FixtureCorpusEntry {
        let path = fixture_path(name);
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read fixture: {:?}", path));
        serde_json::from_str(&contents)
            .unwrap_or_else(|e| panic!("Failed to parse fixture {:?}: {}", path, e))
    }

    #[test]
    fn test_load_default_rule_pack_fixture() {
        // The default pack fixture should exist (created during build)
        let pack = load_rule_pack("default");
        // Fixture has name "default-diff-v1" and version "v1.0.0"
        assert_eq!(pack.name, "default-diff-v1");
        assert_eq!(pack.version.0, "v1.0.0");
        assert_eq!(pack.status, RulePackStatus::Active);
        assert_eq!(pack.risk.confidence_threshold, 0.7);
        assert_eq!(pack.risk.max_high_severity_sections, 3);
    }

    #[test]
    fn test_no_semantic_change_fixture_corpus() {
        // Load and execute the no-semantic-change regression fixture
        // This fixture verifies that identical content produces low severity, high confidence
        let fixture = load_fixture_corpus("no-semantic-change");

        // Verify fixture metadata
        assert_eq!(fixture.name, "no-semantic-change");
        assert_eq!(fixture.rule_pack, "default");

        // Load the default rule pack for threshold configuration
        let pack = load_rule_pack(&fixture.rule_pack);
        let risk_config = pack.risk_config();

        // Run the risk analysis on the fixture input
        let analysis = analyze_diff_risk_with_config(
            &fixture.input.scope,
            &fixture.input.constraints,
            &fixture.input.acceptance_criteria,
            &fixture.input.authority,
            &risk_config,
        );

        // Assert results match expected from fixture file
        assert_eq!(
            format!("{:?}", analysis.severity).to_lowercase(),
            fixture.expected.severity,
            "Severity mismatch for {}: expected {:?}",
            fixture.name,
            fixture.expected.severity
        );
        assert_eq!(
            analysis.confidence, fixture.expected.confidence,
            "Confidence mismatch for {}: expected {}",
            fixture.name, fixture.expected.confidence
        );
        assert_eq!(
            analysis.manual_review, fixture.expected.manual_review,
            "Manual review mismatch for {}: expected {}",
            fixture.name, fixture.expected.manual_review
        );
        let actual_section_count = analysis.section_risks.len();
        assert_eq!(
            actual_section_count, fixture.expected.section_count,
            "Section count mismatch for {}: expected {}",
            fixture.name, fixture.expected.section_count
        );
    }

    #[test]
    fn test_scope_add_medium_fixture_corpus() {
        // Load and execute the scope-add-medium regression fixture
        // This fixture verifies that adding a scope item produces medium severity
        let fixture = load_fixture_corpus("scope-add-medium");

        // Verify fixture metadata
        assert_eq!(fixture.name, "scope-add-medium");
        assert_eq!(fixture.rule_pack, "default");

        // Load the default rule pack for threshold configuration
        let pack = load_rule_pack(&fixture.rule_pack);
        let risk_config = pack.risk_config();

        // Verify the scope diff input shows item2 was added
        assert_eq!(fixture.input.scope.in_scope.added, vec!["item2"]);
        assert!(fixture.input.scope.in_scope.removed.is_empty());

        // Run the risk analysis on the fixture input
        let analysis = analyze_diff_risk_with_config(
            &fixture.input.scope,
            &fixture.input.constraints,
            &fixture.input.acceptance_criteria,
            &fixture.input.authority,
            &risk_config,
        );

        // Assert results match expected from fixture file
        assert_eq!(
            format!("{:?}", analysis.severity).to_lowercase(),
            fixture.expected.severity,
            "Severity mismatch for {}: expected {:?}",
            fixture.name,
            fixture.expected.severity
        );
        assert_eq!(
            analysis.confidence, fixture.expected.confidence,
            "Confidence mismatch for {}: expected {}",
            fixture.name, fixture.expected.confidence
        );
        assert_eq!(
            analysis.manual_review, fixture.expected.manual_review,
            "Manual review mismatch for {}: expected {}",
            fixture.name, fixture.expected.manual_review
        );
        let actual_section_count = analysis.section_risks.len();
        assert_eq!(
            actual_section_count, fixture.expected.section_count,
            "Section count mismatch for {}: expected {}",
            fixture.name, fixture.expected.section_count
        );
    }

    #[test]
    fn test_all_fixture_corpus_entries() {
        // Regression test: ensure ALL fixture corpus files can be loaded and analyzed
        // This test iterates over all known fixture files and verifies they produce
        // deterministic, expected outputs
        let fixture_names = vec!["no-semantic-change", "scope-add-medium"];

        for fixture_name in fixture_names {
            let fixture = load_fixture_corpus(fixture_name);
            let pack = load_rule_pack(&fixture.rule_pack);
            let risk_config = pack.risk_config();

            let analysis = analyze_diff_risk_with_config(
                &fixture.input.scope,
                &fixture.input.constraints,
                &fixture.input.acceptance_criteria,
                &fixture.input.authority,
                &risk_config,
            );

            // Verify deterministic output
            let analysis2 = analyze_diff_risk_with_config(
                &fixture.input.scope,
                &fixture.input.constraints,
                &fixture.input.acceptance_criteria,
                &fixture.input.authority,
                &risk_config,
            );

            assert_eq!(
                analysis.severity, analysis2.severity,
                "Non-deterministic severity for {}",
                fixture_name
            );
            assert_eq!(
                analysis.confidence, analysis2.confidence,
                "Non-deterministic confidence for {}",
                fixture_name
            );
            assert_eq!(
                analysis.manual_review, analysis2.manual_review,
                "Non-deterministic manual_review for {}",
                fixture_name
            );

            // Log for visibility
            println!(
                "Fixture '{}': severity={:?}, confidence={}, manual_review={}, sections={}",
                fixture_name,
                analysis.severity,
                analysis.confidence,
                analysis.manual_review,
                analysis.section_risks.len()
            );
        }
    }

    #[test]
    fn test_rule_pack_with_fixture_integration() {
        // Test that a rule pack can be created and used with the risk engine
        let pack = RulePack::with_risk_config(
            "test-pack",
            RulePackRiskConfig {
                confidence_threshold: 0.5, // Low threshold to avoid LowConfidence trigger
                max_high_severity_sections: 2,
            },
        );

        let risk_config = pack.risk_config();

        // Create a simple scope diff
        let scope_diff = ScopeDiff {
            in_scope: ScopeItemsDiff {
                added: vec!["new_item".to_string()],
                removed: vec![],
            },
            out_of_scope: ScopeItemsDiff {
                added: vec![],
                removed: vec![],
            },
        };

        let constraints_diff = ConstraintsDiff::default();
        let ac_diff = AcceptanceCriteriaDiff::default();
        let authority_diff = AuthorityDiff::default();

        // Run analysis with custom config
        let analysis = analyze_diff_risk_with_config(
            &scope_diff,
            &constraints_diff,
            &ac_diff,
            &authority_diff,
            &risk_config,
        );

        // Should be medium severity for scope addition
        assert_eq!(analysis.severity, Severity::Medium);
        // Confidence should be 0.5 for scope items (ambiguous match)
        assert_eq!(analysis.confidence, 0.5);
        // Manual review: confidence=0.5 >= threshold=0.5, only 1 medium section, no policy change
        // => no manual review
        assert!(!analysis.manual_review);
    }

    #[test]
    fn test_strict_rule_pack_triggers_manual_review() {
        // Create a strict rule pack where even medium changes trigger review
        let strict_pack = RulePack::with_risk_config(
            "strict",
            RulePackRiskConfig {
                confidence_threshold: 0.5,     // Very low - almost everything triggers
                max_high_severity_sections: 0, // Zero tolerance
            },
        );

        let risk_config = strict_pack.risk_config();

        // Create a simple scope diff
        let scope_diff = ScopeDiff {
            in_scope: ScopeItemsDiff {
                added: vec!["new_item".to_string()],
                removed: vec![],
            },
            out_of_scope: ScopeItemsDiff {
                added: vec![],
                removed: vec![],
            },
        };

        let constraints_diff = ConstraintsDiff::default();
        let ac_diff = AcceptanceCriteriaDiff::default();
        let authority_diff = AuthorityDiff::default();

        let analysis = analyze_diff_risk_with_config(
            &scope_diff,
            &constraints_diff,
            &ac_diff,
            &authority_diff,
            &risk_config,
        );

        // Medium severity section + confidence below threshold = manual review
        assert!(analysis.manual_review);
    }

    #[test]
    fn test_fixture_determinism() {
        // Verify that the same input always produces the same output
        let pack = RulePack::new("determinism-test");
        let risk_config = pack.risk_config();

        let scope_diff = ScopeDiff {
            in_scope: ScopeItemsDiff {
                added: vec!["item_a".to_string(), "item_b".to_string()],
                removed: vec!["item_c".to_string()],
            },
            out_of_scope: ScopeItemsDiff {
                added: vec![],
                removed: vec![],
            },
        };

        let constraints_diff = ConstraintsDiff::default();
        let ac_diff = AcceptanceCriteriaDiff::default();
        let authority_diff = AuthorityDiff::default();

        // Run analysis twice with same input
        let analysis1 = analyze_diff_risk_with_config(
            &scope_diff,
            &constraints_diff,
            &ac_diff,
            &authority_diff,
            &risk_config,
        );

        let analysis2 = analyze_diff_risk_with_config(
            &scope_diff,
            &constraints_diff,
            &ac_diff,
            &authority_diff,
            &risk_config,
        );

        // Results should be identical
        assert_eq!(analysis1.severity, analysis2.severity);
        assert_eq!(analysis1.confidence, analysis2.confidence);
        assert_eq!(analysis1.manual_review, analysis2.manual_review);
        assert_eq!(
            analysis1.manual_review_reasons.len(),
            analysis2.manual_review_reasons.len()
        );
    }

    #[test]
    fn test_rule_pack_json_roundtrip() {
        // Create a pack, serialize to JSON, deserialize, verify equality
        let original = RulePack::with_risk_config(
            "roundtrip-test",
            RulePackRiskConfig {
                confidence_threshold: 0.8,
                max_high_severity_sections: 4,
            },
        );

        let json = original.to_json().unwrap();
        let deserialized = RulePack::from_json(&json).unwrap();

        assert_eq!(deserialized.name, original.name);
        assert_eq!(deserialized.version, original.version);
        assert_eq!(
            deserialized.risk.confidence_threshold,
            original.risk.confidence_threshold
        );
        assert_eq!(
            deserialized.risk.max_high_severity_sections,
            original.risk.max_high_severity_sections
        );
    }
}
