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
