use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Webhook Subscription Types (Slice 4b — bounded local-dev subscription CRUD)
// =============================================================================

/// Request body for creating a webhook subscription.
///
/// Bounded local-dev: no secret manager, no production readiness claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebhookSubscriptionRequest {
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub subscription_id: Uuid,
    pub webhook_url: String,
    #[serde(default)]
    pub downstream_system_id: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<i32>,
    #[serde(default)]
    pub event_types: Option<Vec<String>>,
}

/// Request body for updating a webhook subscription.
///
/// All fields are optional; only provided fields are updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWebhookSubscriptionRequest {
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub downstream_system_id: Option<Option<String>>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<i32>,
    #[serde(default)]
    pub event_types: Option<Vec<String>>,
}

/// Response for a single webhook subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscriptionResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
    pub subscription_id: Uuid,
    pub webhook_url: String,
    pub downstream_system_id: Option<String>,
    pub status: String,
    pub max_attempts: i32,
    pub event_types: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<crate::webhook_subscription_repo::WebhookSubscriptionRecord>
    for WebhookSubscriptionResponse
{
    fn from(r: crate::webhook_subscription_repo::WebhookSubscriptionRecord) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            intent_id: r.intent_id,
            subscription_id: r.subscription_id,
            webhook_url: r.webhook_url,
            downstream_system_id: r.downstream_system_id,
            status: r.status,
            max_attempts: r.max_attempts,
            event_types: r.event_types,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Response for listing webhook subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWebhookSubscriptionsResponse {
    pub subscriptions: Vec<WebhookSubscriptionResponse>,
    pub total: usize,
}

/// Query parameters for listing webhook subscriptions.
#[derive(Debug, Deserialize)]
pub struct ListWebhookSubscriptionsQuery {
    pub tenant_id: Uuid,
    pub intent_id: Uuid,
}

// =============================================================================
// Webhook Outbox DLQ Types (Slice 5b — bounded local-dev failed-status DLQ)
// =============================================================================

/// Query parameters for listing webhook outbox DLQ (failed) records.
#[derive(Debug, Deserialize)]
pub struct ListWebhookOutboxDlqQuery {
    pub tenant_id: Uuid,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Response for listing webhook outbox DLQ records.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListWebhookOutboxDlqResponse {
    pub records: Vec<crate::webhook_outbox_repo::WebhookOutboxRecord>,
    pub total: usize,
}

/// Query parameters for replaying a webhook outbox DLQ record.
#[derive(Debug, Deserialize)]
pub struct ReplayWebhookOutboxDlqQuery {
    pub tenant_id: Uuid,
    /// Optional actor identity for replay audit metadata (Phase 1.2).
    /// If not provided, replay metadata is still updated with replay_count
    /// and replayed_at, but replayed_by remains null.
    pub replayed_by: Option<String>,
}

/// Response for replaying a webhook outbox DLQ record.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplayWebhookOutboxDlqResponse {
    pub record: crate::webhook_outbox_repo::WebhookOutboxRecord,
}

/// Query parameters for listing replayed webhook outbox records.
///
/// Phase 1.3: bounded local-dev replay audit query.
#[derive(Debug, Deserialize)]
pub struct ListWebhookOutboxReplayedQuery {
    pub tenant_id: Uuid,
    #[serde(default)]
    pub limit: Option<i64>,
    /// Optional RFC 3339 timestamp cutoff — only returns records replayed at or after this time.
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

/// Response for listing replayed webhook outbox records.
#[derive(Debug, Serialize, Deserialize)]
pub struct ListWebhookOutboxReplayedResponse {
    pub records: Vec<crate::webhook_outbox_repo::WebhookOutboxRecord>,
    pub total: usize,
}

/// Request body for bulk replaying failed webhook outbox records.
///
/// Phase 2.2: bounded local-dev bulk replay — hard cap enforced server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkReplayWebhookOutboxDlqRequest {
    pub tenant_id: Uuid,
    /// Optional maximum records to replay in this call. Hard cap of 100 is enforced
    /// regardless of the value provided.
    #[serde(default)]
    pub max_records: Option<i64>,
    /// Optional actor identity for replay audit metadata.
    #[serde(default)]
    pub replayed_by: Option<String>,
}

/// Response for bulk replaying failed webhook outbox records.
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkReplayWebhookOutboxDlqResponse {
    /// Number of records successfully replayed (Failed → Pending).
    pub replayed: usize,
    /// Number of records skipped because they were no longer in Failed status
    /// when the replay was attempted (race condition or already replayed).
    pub skipped: usize,
    /// Number of records that encountered an unexpected error during replay.
    pub errors: usize,
    /// The records that were successfully replayed.
    pub records: Vec<crate::webhook_outbox_repo::WebhookOutboxRecord>,
}

/// Query parameters for webhook outbox DLQ stats.
///
/// Phase 2.3: bounded local-dev stats query.
#[derive(Debug, Deserialize)]
pub struct WebhookOutboxDlqStatsQuery {
    pub tenant_id: Uuid,
}

/// Response for webhook outbox DLQ stats.
#[derive(Debug, Serialize, Deserialize)]
pub struct WebhookOutboxDlqStatsResponse {
    pub stats: crate::webhook_outbox_repo::WebhookOutboxDlqStats,
}
