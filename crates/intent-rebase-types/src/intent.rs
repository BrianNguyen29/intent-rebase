//! Intent domain types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An intent document managed by IRE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub version: i32,
    pub spec: IntentSpec,
    pub metadata: IntentMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The mutable specification of an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    pub target: String,
    pub constraints: Vec<IntentConstraint>,
    pub rules_pack_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConstraint {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

/// Metadata about an intent (non-functional attributes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentMetadata {
    pub created_by: String,
    pub tags: Vec<String>,
    pub status: IntentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntentStatus {
    Draft,
    Active,
    Archived,
}
