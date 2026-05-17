//! Webhook subscription CRUD handlers (Slice 4b — bounded local-dev subscription CRUD)
//!
//! Provides HTTP handlers for per-intent webhook subscription management:
//! - POST /webhooks/subscriptions
//! - GET /webhooks/subscriptions?intent_id=...
//! - GET /webhooks/subscriptions/:id
//! - PATCH /webhooks/subscriptions/:id
//! - DELETE /webhooks/subscriptions/:id (soft-delete)
//!
//! Bounded scope: local-dev only; no production readiness claims.
//! See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 4b)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::types::{
    CreateWebhookSubscriptionRequest, ListWebhookSubscriptionsQuery,
    ListWebhookSubscriptionsResponse, UpdateWebhookSubscriptionRequest,
    WebhookSubscriptionResponse,
};
use crate::webhook_subscription_repo::WebhookSubscriptionRecord;
use crate::{ApiErrorResponse, AppState};

// =============================================================================
// POST /webhooks/subscriptions
// =============================================================================

/// Create a new webhook subscription.
///
/// Returns 201 Created with the created subscription, or 400/500 on error.
pub async fn create_subscription(
    State(state): State<AppState>,
    Json(body): Json<CreateWebhookSubscriptionRequest>,
) -> Result<(StatusCode, Json<WebhookSubscriptionResponse>), ApiErrorResponse> {
    let repo = match &state.webhook_subscription_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(IntentRebaseError::Internal(
                "Webhook subscription repository is not configured".to_string(),
            )));
        }
    };

    let record = WebhookSubscriptionRecord::new(
        body.tenant_id,
        body.intent_id,
        body.subscription_id,
        body.webhook_url,
        body.downstream_system_id,
        body.event_types
            .unwrap_or_else(|| vec!["intent_changed".to_string()]),
    );

    let record = if let Some(ma) = body.max_attempts {
        record.with_max_attempts(ma)
    } else {
        record
    };

    match repo.create(record).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created.into()))),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// GET /webhooks/subscriptions?intent_id=...
// =============================================================================

/// List webhook subscriptions for an intent.
///
/// Returns 200 OK with a list of subscriptions scoped to the intent.
pub async fn list_subscriptions(
    State(state): State<AppState>,
    Query(query): Query<ListWebhookSubscriptionsQuery>,
) -> Result<Json<ListWebhookSubscriptionsResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_subscription_repo {
        Some(r) => r,
        None => {
            return Ok(Json(ListWebhookSubscriptionsResponse {
                subscriptions: vec![],
                total: 0,
            }));
        }
    };

    match repo.list_by_intent(query.tenant_id, query.intent_id).await {
        Ok(list) => {
            let total = list.len();
            let subscriptions = list
                .into_iter()
                .map(WebhookSubscriptionResponse::from)
                .collect();
            Ok(Json(ListWebhookSubscriptionsResponse {
                subscriptions,
                total,
            }))
        }
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(IntentRebaseError::Internal(e.to_string()))),
    }
}

// =============================================================================
// GET /webhooks/subscriptions/:id
// =============================================================================

/// Get a single webhook subscription by ID.
///
/// Requires `tenant_id` as a query parameter for tenant scoping.
pub async fn get_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<crate::types::GetPolicySnapshotQuery>,
) -> Result<Json<WebhookSubscriptionResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_subscription_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(
                IntentRebaseError::WebhookSubscriptionNotFound(id),
            ));
        }
    };

    match repo.get(id, query.tenant_id).await {
        Ok(record) => Ok(Json(record.into())),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(e)),
    }
}

// =============================================================================
// PATCH /webhooks/subscriptions/:id
// =============================================================================

/// Update a webhook subscription.
///
/// Only allowed fields are updated: `webhook_url`, `downstream_system_id`,
/// `status`, `max_attempts`, `event_types`.
pub async fn update_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<crate::types::GetPolicySnapshotQuery>,
    Json(body): Json<UpdateWebhookSubscriptionRequest>,
) -> Result<Json<WebhookSubscriptionResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_subscription_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(
                IntentRebaseError::WebhookSubscriptionNotFound(id),
            ));
        }
    };

    // Validate status if provided
    if let Some(ref status) = body.status {
        let allowed = ["active", "paused", "disabled", "deleted"];
        if !allowed.contains(&status.as_str()) {
            return Err(ApiErrorResponse(IntentRebaseError::InvalidIngestRequest(
                format!("Invalid status '{}'. Allowed values: {:?}", status, allowed),
            )));
        }
    }

    // Validate max_attempts if provided
    if let Some(ma) = body.max_attempts {
        if !(1..=100).contains(&ma) {
            return Err(ApiErrorResponse(IntentRebaseError::InvalidIngestRequest(
                "max_attempts must be between 1 and 100".to_string(),
            )));
        }
    }

    match repo
        .update(
            id,
            query.tenant_id,
            body.webhook_url,
            body.downstream_system_id,
            body.status,
            body.max_attempts,
            body.event_types,
        )
        .await
    {
        Ok(record) => Ok(Json(record.into())),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(e)),
    }
}

// =============================================================================
// DELETE /webhooks/subscriptions/:id
// =============================================================================

/// Soft-delete a webhook subscription.
///
/// Sets status to `deleted` instead of removing the record.
pub async fn delete_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<crate::types::GetPolicySnapshotQuery>,
) -> Result<Json<WebhookSubscriptionResponse>, ApiErrorResponse> {
    let repo = match &state.webhook_subscription_repo {
        Some(r) => r,
        None => {
            return Err(ApiErrorResponse(
                IntentRebaseError::WebhookSubscriptionNotFound(id),
            ));
        }
    };

    match repo.soft_delete(id, query.tenant_id).await {
        Ok(record) => Ok(Json(record.into())),
        Err(IntentRebaseError::StorageError(msg)) => {
            Err(ApiErrorResponse(IntentRebaseError::StorageError(msg)))
        }
        Err(e) => Err(ApiErrorResponse(e)),
    }
}
