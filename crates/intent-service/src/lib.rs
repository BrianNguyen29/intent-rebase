//! Intent Service — manages intent CRUD and versioning
//!
//! Phase 0: This is a minimal skeleton. Real implementation begins Phase 1.

use intent_rebase_types::{Intent, IntentRebaseError};

/// IntentService handles intent lifecycle operations
pub struct IntentService;

impl IntentService {
    pub fn new() -> Self {
        Self
    }

    /// Create a new intent (Phase 1 — stub for now)
    pub async fn create_intent(&self, _intent: Intent) -> Result<Intent, IntentRebaseError> {
        // Phase 1: implement actual creation
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }

    /// Get an intent by ID (Phase 1 — stub for now)
    pub async fn get_intent(&self, _id: uuid::Uuid) -> Result<Intent, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }
}

impl Default for IntentService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_constructs() {
        let _ = IntentService::new();
    }
}
