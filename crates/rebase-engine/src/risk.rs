//! Risk analysis module for semantic diff results
//!
//! This module implements deterministic risk rules for the structured diff output:
//! - severity: low/medium/high/critical based on change type and affected section
//! - confidence: 0.0-1.0 based on matching quality (clause_id uniqueness)
//! - manual_review: triggers when human review is recommended
//!
//! Rules are deterministic and replayable under the same rule pack version.

use serde::{Deserialize, Serialize};

/// Severity levels for diff changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// No semantic change or clarification only
    Low,
    /// Minor functional change with limited impact
    Medium,
    /// Significant change affecting authority, constraints, or priority items
    High,
    /// Critical change affecting policy, compliance, or security
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Low
    }
}

/// Manual review trigger reason
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualReviewReason {
    /// Critical severity change detected
    CriticalSeverity,
    /// High severity change affecting authority
    HighSeverityAuthorityChange,
    /// Confidence score below threshold
    LowConfidence { confidence: f64, threshold: f64 },
    /// Policy constraint was changed
    PolicyConstraintChanged,
    /// Multiple high-severity changes in single diff
    MultipleHighSeverityChanges { count: usize },
    /// Approval requirement removed
    ApprovalRequirementRemoved,
}

impl Default for ManualReviewReason {
    fn default() -> Self {
        ManualReviewReason::LowConfidence {
            confidence: 1.0,
            threshold: 0.7,
        }
    }
}

/// Result of risk analysis on a diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffRiskAnalysis {
    /// Overall severity (highest severity across all changes)
    pub severity: Severity,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Whether manual review is recommended
    pub manual_review: bool,
    /// Reasons for manual review decision
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub manual_review_reasons: Vec<ManualReviewReason>,
    /// Detailed risk breakdown by section
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub section_risks: Vec<SectionRisk>,
    /// Rationale for the overall assessment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// Risk assessment for a single section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionRisk {
    pub section: String,
    pub severity: Severity,
    pub change_count: usize,
    pub high_priority_changes: usize,
}

/// Configuration for risk analysis thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Minimum confidence threshold below which manual review is triggered
    pub confidence_threshold: f64,
    /// Maximum number of high-severity changes before triggering manual review
    pub max_high_severity_before_manual_review: usize,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.7,
            max_high_severity_before_manual_review: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_default_is_low() {
        assert_eq!(Severity::default(), Severity::Low);
    }

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.confidence_threshold, 0.7);
        assert_eq!(config.max_high_severity_before_manual_review, 3);
    }

    #[test]
    fn test_manual_review_reason_serialization() {
        let reason = ManualReviewReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        };
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("low_confidence"));
    }

    #[test]
    fn test_diff_risk_analysis_serialization() {
        let analysis = DiffRiskAnalysis {
            severity: Severity::High,
            confidence: 0.85,
            manual_review: true,
            manual_review_reasons: vec![ManualReviewReason::HighSeverityAuthorityChange],
            section_risks: vec![SectionRisk {
                section: "authority".to_string(),
                severity: Severity::High,
                change_count: 2,
                high_priority_changes: 1,
            }],
            rationale: Some("Authority changes detected".to_string()),
        };
        let json = serde_json::to_string(&analysis).unwrap();
        assert!(json.contains("high"));
        assert!(json.contains("0.85"));
        assert!(json.contains("authority"));
    }
}
