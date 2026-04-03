//! Graph Service — manages dependency graph state
//!
//! Phase 0: This is a minimal skeleton. Real implementation begins Phase 1.

use intent_rebase_types::{GraphNode, IntentRebaseError};

/// GraphService manages the dependency graph
pub struct GraphService;

impl GraphService {
    pub fn new() -> Self {
        Self
    }

    /// Add a node to the graph (Phase 1)
    pub async fn add_node(&self, _node: GraphNode) -> Result<GraphNode, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }

    /// Get all nodes for an intent (Phase 1)
    pub async fn get_intent_nodes(
        &self,
        _intent_id: uuid::Uuid,
    ) -> Result<Vec<GraphNode>, IntentRebaseError> {
        Err(IntentRebaseError::Internal(
            "Phase 1: not yet implemented".into(),
        ))
    }
}

impl Default for GraphService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_constructs() {
        let _ = GraphService::new();
    }
}
