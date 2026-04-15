//! Forensic data collector trait and implementations
//!
//! Provides the interface for collecting real data from service repositories
//! for forensic bundle generation.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::{
    AuditEvent, PolicySnapshot,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Data collected for a single intent's forensic bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedIntentData {
    pub intent_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub name: String,
    pub versions: Vec<CollectedVersionData>,
    pub policy_snapshots: Vec<PolicySnapshot>,
    pub audit_events: Vec<AuditEvent>,
}

/// Collected version data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedVersionData {
    pub version_number: u64,
    pub summary: String,
    pub change_type: String,
    pub created_at: DateTime<Utc>,
}

/// Result of forensic data collection for a bundle request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionResult {
    pub tenant_id: Option<Uuid>,
    pub intent_id: Uuid,
    pub collected_at: DateTime<Utc>,
    pub intents: Vec<CollectedIntentData>,
    pub total_audit_events: usize,
    pub total_policy_snapshots: usize,
}

/// Trait for collecting forensic data from service repositories
#[async_trait]
pub trait ForensicDataCollector: Send + Sync {
    /// Collect all data for a forensic bundle within the given time range
    async fn collect(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionResult, CollectorError>;
    
    /// Count available records without collecting full data (for verification)
    async fn count_available(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionCounts, CollectorError>;
}

/// Available record counts for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionCounts {
    pub intent_count: usize,
    pub version_count: usize,
    pub audit_event_count: usize,
    pub policy_snapshot_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("Repository error: {0}")]
    Repository(String),
    #[error("Tenant access denied: {0}")]
    TenantAccessDenied(String),
    #[error("Invalid time range: {0}")]
    InvalidTimeRange(String),
    #[error("Collection timeout: {0}")]
    Timeout(String),
}

// =============================================================================
// In-memory implementation (for tests)
// =============================================================================

/// In-memory forensic data collector for unit tests.
///
/// Returns configurable intent data for testing the bundle generation flow
/// without requiring real repositories.
pub struct InMemoryForensicDataCollector {
    intents: RwLock<Vec<CollectedIntentData>>,
}

impl InMemoryForensicDataCollector {
    pub fn new() -> Self {
        Self {
            intents: RwLock::new(Vec::new()),
        }
    }

    /// Add intent data to be returned during collection.
    pub fn with_intents(mut self, intents: Vec<CollectedIntentData>) -> Self {
        self.intents = RwLock::new(intents);
        self
    }
}

impl Default for InMemoryForensicDataCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ForensicDataCollector for InMemoryForensicDataCollector {
    async fn collect(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionResult, CollectorError> {
        if time_range.0 > time_range.1 {
            return Err(CollectorError::InvalidTimeRange(
                "start must be before end".to_string(),
            ));
        }

        let intents = self.intents.read().await;
        let filtered: Vec<CollectedIntentData> = intents
            .iter()
            .filter(|i| intent_ids.contains(&i.intent_id))
            .cloned()
            .collect();

        let total_audit_events = filtered
            .iter()
            .map(|i| i.audit_events.len())
            .sum();

        let total_policy_snapshots = filtered
            .iter()
            .map(|i| i.policy_snapshots.len())
            .sum();

        Ok(CollectionResult {
            tenant_id,
            intent_id: intent_ids.first().copied().unwrap_or(Uuid::nil()),
            collected_at: Utc::now(),
            intents: filtered,
            total_audit_events,
            total_policy_snapshots,
        })
    }

    async fn count_available(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionCounts, CollectorError> {
        if time_range.0 > time_range.1 {
            return Err(CollectorError::InvalidTimeRange(
                "start must be before end".to_string(),
            ));
        }

        let intents = self.intents.read().await;
        let filtered: Vec<&CollectedIntentData> = intents
            .iter()
            .filter(|i| intent_ids.contains(&i.intent_id))
            .collect();

        Ok(CollectionCounts {
            intent_count: filtered.len(),
            version_count: filtered.iter().map(|i| i.versions.len()).sum(),
            audit_event_count: filtered
                .iter()
                .map(|i| i.audit_events.len())
                .sum(),
            policy_snapshot_count: filtered
                .iter()
                .map(|i| i.policy_snapshots.len())
                .sum(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_intent(tenant_id: Uuid, intent_id: Uuid) -> CollectedIntentData {
        CollectedIntentData {
            intent_id,
            tenant_id: Some(tenant_id),
            name: format!("intent-{}", intent_id),
            versions: vec![CollectedVersionData {
                version_number: 1,
                summary: "Test version".to_string(),
                change_type: "create".to_string(),
                created_at: Utc::now(),
            }],
            policy_snapshots: vec![],
            audit_events: vec![],
        }
    }

    #[tokio::test]
    async fn test_in_memory_collector_empty() {
        let collector = InMemoryForensicDataCollector::new();
        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());

        let result = collector
            .collect(None, &[], &time_range)
            .await
            .unwrap();

        assert!(result.intents.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_collector_with_intents() {
        let intent = create_test_intent(Uuid::nil(), Uuid::new_v4());
        let collector = InMemoryForensicDataCollector::new().with_intents(vec![intent]);
        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());

        let result = collector
            .collect(None, &[Uuid::new_v4()], &time_range)
            .await
            .unwrap();

        assert!(result.intents.is_empty()); // Intent ID not in the list
    }

    #[tokio::test]
    async fn test_in_memory_collector_invalid_time_range() {
        let collector = InMemoryForensicDataCollector::new();
        let time_range = (Utc::now(), Utc::now() - chrono::Duration::days(1));

        let result = collector
            .collect(None, &[], &time_range)
            .await;

        assert!(matches!(result, Err(CollectorError::InvalidTimeRange(_))));
    }

    #[tokio::test]
    async fn test_count_available() {
        let intent = create_test_intent(Uuid::nil(), Uuid::new_v4());
        let collector = InMemoryForensicDataCollector::new().with_intents(vec![intent]);
        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());

        let counts = collector
            .count_available(None, &[Uuid::new_v4()], &time_range)
            .await
            .unwrap();

        assert_eq!(counts.intent_count, 0); // Not matching
    }
}
