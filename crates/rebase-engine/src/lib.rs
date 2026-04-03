//! Rebase Engine — computes semantic diffs and rebase plans
//!
//! Phase 0: This is a minimal skeleton. Real implementation begins Phase 1.

use intent_rebase_types::IntentRebaseError;

/// RebaseEngine computes semantic diffs and generates rebase plans
pub struct RebaseEngine;

impl RebaseEngine {
    pub fn new() -> Self {
        Self
    }

    /// Compute semantic diff between two intent versions (Phase 1)
    pub async fn compute_diff(
        &self,
        _from_version: i32,
        _to_version: i32,
    ) -> Result<serde_json::Value, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }

    /// Generate a rebase plan (Phase 1)
    pub async fn generate_plan(
        &self,
        _diff: serde_json::Value,
    ) -> Result<serde_json::Value, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }
}

impl Default for RebaseEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_constructs() {
        let _ = RebaseEngine::new();
    }
}
