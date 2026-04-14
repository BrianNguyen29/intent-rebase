//! Audit persistence for external rebase apply outcomes
//!
//! Phase 2b bounded slice: Audit persistence for external POST /intents/{intent_id}/rebase-apply outcomes.
//! This module provides storage-agnostic audit event persistence.
//! In-memory implementation for tests, SQL-backed for production.

use super::{
    ApprovalCancelledAuditPayload, ApprovalExpiredAuditPayload, ApprovalGrantedAuditPayload,
    ApprovalRevokedAuditPayload, AuditEvent, AuditEventType,
    CompensationCompletedAuditPayload, CompensationFailedAuditPayload,
    CompensationPlannedAuditPayload, CompensationStartedAuditPayload, IntentRebaseError,
    RebaseApplyAuditPayload, RebaseApplyBlockedAuditPayload, ReplayAuditPayload, TraceContext,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for audit event storage
#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Persist an audit event
    async fn create_audit_event(&self, event: AuditEvent) -> Result<(), IntentRebaseError>;

    /// Get a single audit event by ID (tenant-scoped).
    /// Returns Err(ArtifactNotFound) if event doesn't exist or belongs to a different tenant.
    async fn get_audit_event(
        &self,
        event_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<AuditEvent, IntentRebaseError>;

    /// List audit events by intent (ordered by occurred_at descending)
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError>;

    /// List audit events by tenant (ordered by occurred_at descending)
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError>;

    /// Record a RebaseApplied audit event (helper method with default implementation)
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext` to carry
    /// trace_id/span_id from the active span into the audit event. Pass `TraceContext::default()`
    /// if no trace context is available.
    async fn record_rebase_applied(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_rebase_applied_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record a RebaseApplied audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_rebase_applied_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::RebaseApplied,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a RebaseApplyBlocked audit event (helper method with default implementation)
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_rebase_apply_blocked(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyBlockedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_rebase_apply_blocked_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record a RebaseApplyBlocked audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_rebase_apply_blocked_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyBlockedAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::RebaseApplyBlocked,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record an ApprovalGranted audit event (Phase 2b bounded slice)
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_approval_granted(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalGrantedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_approval_granted_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record an ApprovalGranted audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_approval_granted_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalGrantedAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ApprovalGranted,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record an ApprovalRevoked audit event (Phase 2b bounded slice)
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_approval_revoked(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalRevokedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_approval_revoked_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record an ApprovalRevoked audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_approval_revoked_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalRevokedAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ApprovalRevoked,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record an ApprovalCancelled audit event (Phase 2b bounded slice)
    /// This is emitted when pending approval requests are cancelled due to intent version change
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_approval_cancelled(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalCancelledAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ApprovalCancelled,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record an ApprovalExpired audit event (Phase 2b bounded expiry slice)
    /// This is emitted when a pending approval request is manually marked as expired.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_approval_expired(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalExpiredAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_approval_expired_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record an ApprovalExpired audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_approval_expired_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ApprovalExpiredAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ApprovalExpired,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a ReplayInitiated audit event (Phase 2b bounded replay slice)
    /// This is emitted when a replay operation is initiated via the public replay endpoint.
    /// Note: This is bounded cooperative signal-based replay, NOT native Temporal reset.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_replay_initiated(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ReplayAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        self.record_replay_initiated_with_trace(tenant_id, actor_id, intent_id, payload, trace_context.trace_id, trace_context.span_id)
            .await
    }

    /// Record a ReplayInitiated audit event with explicit trace context (P2-S3 bounded slice)
    async fn record_replay_initiated_with_trace(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: ReplayAuditPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ReplayInitiated,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_id,
            span_id: span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record an ArtifactInvalidated audit event (Phase 2b bounded slice)
    /// This is emitted when artifacts are invalidated due to intent change.
    /// Note: This is bounded metadata/status only. Real S3 quarantine move is Phase 3.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_artifact_invalidated(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        artifact_id: Uuid,
        payload: super::ArtifactInvalidatedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::ArtifactInvalidated,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: Some(artifact_id),
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a CompensationPlanned audit event (Phase 3 Batch 0 scaffold)
    ///
    /// Emitted when a compensation action is created/planned.
    /// Note: This uses the action's own ID as the compensation_plan_id since
    /// the CompensationActionService does not have a separate planning phase.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_compensation_planned(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: CompensationPlannedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::CompensationPlanned,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a CompensationStarted audit event (Phase 3 Batch 0 scaffold)
    ///
    /// Emitted when compensation execution begins.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_compensation_started(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: CompensationStartedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::CompensationStarted,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a CompensationCompleted audit event (Phase 3 Batch 0 scaffold)
    ///
    /// Emitted when compensation execution completes successfully.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_compensation_completed(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: CompensationCompletedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::CompensationCompleted,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a CompensationFailed audit event (Phase 3 Batch 0 scaffold)
    ///
    /// Emitted when compensation execution fails.
    ///
    /// Phase 3 bounded trace continuity slice: accepts optional `TraceContext`.
    async fn record_compensation_failed(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: CompensationFailedAuditPayload,
        trace_context: TraceContext,
    ) -> Result<(), IntentRebaseError> {
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::CompensationFailed,
            actor_id: actor_id.to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::to_value(payload).map_err(|e| {
                IntentRebaseError::SerializationError(format!("audit payload: {}", e))
            })?,
            trace_id: trace_context.trace_id,
            span_id: trace_context.span_id,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }
}
pub struct InMemoryAuditRepository {
    events: RwLock<HashMap<Uuid, AuditEvent>>,
    by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryAuditRepository {
    pub fn new() -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryAuditRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditRepository for InMemoryAuditRepository {
    async fn create_audit_event(&self, event: AuditEvent) -> Result<(), IntentRebaseError> {
        let mut events = self.events.write().await;
        let mut by_intent = self.by_intent.write().await;
        let mut by_tenant = self.by_tenant.write().await;

        // Store event
        events.insert(event.id, event.clone());

        // Index by intent if applicable
        if let Some(intent_id) = event.intent_id {
            by_intent
                .entry(intent_id)
                .or_insert_with(Vec::new)
                .push(event.id);
        }

        // Index by tenant
        by_tenant
            .entry(event.tenant_id)
            .or_insert_with(Vec::new)
            .push(event.id);

        Ok(())
    }

    async fn get_audit_event(
        &self,
        event_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<AuditEvent, IntentRebaseError> {
        let events = self.events.read().await;
        let event = events
            .get(&event_id)
            .cloned()
            .ok_or(IntentRebaseError::ArtifactNotFound(event_id))?;

        // Tenant isolation: ensure the event belongs to the requesting tenant
        if event.tenant_id != tenant_id {
            return Err(IntentRebaseError::ArtifactNotFound(event_id));
        }

        Ok(event)
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError> {
        let events = self.events.read().await;
        let by_intent = self.by_intent.read().await;

        let event_ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<AuditEvent> = event_ids
            .iter()
            .filter_map(|id| events.get(id).cloned())
            .filter(|e| e.tenant_id == tenant_id)
            .collect();

        // Sort by occurred_at descending (newest first)
        result.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));

        Ok(result)
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError> {
        let events = self.events.read().await;
        let by_tenant = self.by_tenant.read().await;

        let event_ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();

        let mut result: Vec<AuditEvent> = event_ids
            .iter()
            .filter_map(|id| events.get(id).cloned())
            .collect();

        // Sort by occurred_at descending (newest first)
        result.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));

        // Apply limit
        result.truncate(limit);

        Ok(result)
    }
}

// =============================================================================
// SQLx-backed Audit Repository
// =============================================================================

/// SQL-backed repository for audit event persistence using PostgreSQL.
/// Follows the same patterns as SqlxCheckpointRepository and SqlxIntentRepository.
pub struct SqlxAuditRepository {
    pool: PgPool,
}

impl SqlxAuditRepository {
    /// Create a new SqlxAuditRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to an AuditEvent domain object
    fn row_to_event(&self, row: PgRow) -> Result<AuditEvent, IntentRebaseError> {
        let event_type_str: String = row.get("event_type");
        let payload_json: serde_json::Value = row.get("payload");

        Ok(AuditEvent {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            event_type: audit_event_type_from_string(&event_type_str),
            actor_id: row.get("actor_id"),
            intent_id: row.get("intent_id"),
            artifact_id: row.get("artifact_id"),
            payload: payload_json,
            trace_id: row.get("trace_id"),
            span_id: row.get("span_id"),
            occurred_at: row.get("occurred_at"),
        })
    }

    /// Insert a new audit event into the database
    async fn insert_event(&self, event: &AuditEvent) -> Result<(), IntentRebaseError> {
        let payload_json = serde_json::to_value(&event.payload)
            .map_err(|e| IntentRebaseError::SerializationError(format!("audit payload: {}", e)))?;
        let event_type_str = audit_event_type_to_string(&event.event_type);

        sqlx::query(
            r#"
            INSERT INTO audit_events (
                id, tenant_id, event_type, actor_id, intent_id, artifact_id,
                payload, trace_id, span_id, occurred_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(event.id)
        .bind(event.tenant_id)
        .bind(event_type_str)
        .bind(&event.actor_id)
        .bind(event.intent_id)
        .bind(event.artifact_id)
        .bind(payload_json)
        .bind(&event.trace_id)
        .bind(&event.span_id)
        .bind(event.occurred_at)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert audit event: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl AuditRepository for SqlxAuditRepository {
    async fn create_audit_event(&self, event: AuditEvent) -> Result<(), IntentRebaseError> {
        self.insert_event(&event).await
    }

    async fn get_audit_event(
        &self,
        event_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<AuditEvent, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, event_type, actor_id, intent_id, artifact_id,
                payload, trace_id, span_id, occurred_at
            FROM audit_events
            WHERE id = $1 AND tenant_id = $2
            "#,
        )
        .bind(event_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("get audit event: {}", e)))?;

        match row {
            Some(r) => self.row_to_event(r),
            None => Err(IntentRebaseError::ArtifactNotFound(event_id)),
        }
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, event_type, actor_id, intent_id, artifact_id,
                payload, trace_id, span_id, occurred_at
            FROM audit_events
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY occurred_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list audit events by intent: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_event(r)).collect()
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, event_type, actor_id, intent_id, artifact_id,
                payload, trace_id, span_id, occurred_at
            FROM audit_events
            WHERE tenant_id = $1
            ORDER BY occurred_at DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list audit events by tenant: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_event(r)).collect()
    }
}

// =============================================================================
// Helper functions for audit event type enum conversion
// =============================================================================

fn audit_event_type_to_string(event_type: &AuditEventType) -> &'static str {
    match event_type {
        AuditEventType::IntentCreated => "IntentCreated",
        AuditEventType::IntentUpdated => "IntentUpdated",
        AuditEventType::IntentArchived => "IntentArchived",
        AuditEventType::RebaseDetected => "RebaseDetected",
        AuditEventType::RebasePreviewGenerated => "RebasePreviewGenerated",
        AuditEventType::RebaseApplied => "RebaseApplied",
        AuditEventType::RebaseApplyBlocked => "RebaseApplyBlocked",
        AuditEventType::ApprovalRequired => "ApprovalRequired",
        AuditEventType::ApprovalGranted => "ApprovalGranted",
        AuditEventType::ApprovalRevoked => "ApprovalRevoked",
        AuditEventType::ApprovalCancelled => "ApprovalCancelled",
        AuditEventType::ApprovalExpired => "ApprovalExpired",
        AuditEventType::ReplayInitiated => "ReplayInitiated",
        AuditEventType::ArtifactProduced => "ArtifactProduced",
        AuditEventType::ArtifactInvalidated => "ArtifactInvalidated",
        AuditEventType::CompensationPlanned => "CompensationPlanned",
        AuditEventType::CompensationStarted => "CompensationStarted",
        AuditEventType::CompensationCompleted => "CompensationCompleted",
        AuditEventType::CompensationFailed => "CompensationFailed",
        AuditEventType::ForensicBundleRequested => "ForensicBundleRequested",
        AuditEventType::ForensicBundleGenerated => "ForensicBundleGenerated",
    }
}

/// Decode an event type string from the database into an AuditEventType enum.
///
/// Falls back to `RebaseApplied` for unknown strings. This is a pragmatic fallback since
/// RebaseApplied is the most common event type and treating unknown events as applied
/// preserves audit continuity rather than failing. Unknown event types should be
/// investigated as a data integrity issue.
fn audit_event_type_from_string(s: &str) -> AuditEventType {
    match s {
        "IntentCreated" => AuditEventType::IntentCreated,
        "IntentUpdated" => AuditEventType::IntentUpdated,
        "IntentArchived" => AuditEventType::IntentArchived,
        "RebaseDetected" => AuditEventType::RebaseDetected,
        "RebasePreviewGenerated" => AuditEventType::RebasePreviewGenerated,
        "RebaseApplied" => AuditEventType::RebaseApplied,
        "RebaseApplyBlocked" => AuditEventType::RebaseApplyBlocked,
        "ApprovalRequired" => AuditEventType::ApprovalRequired,
        "ApprovalGranted" => AuditEventType::ApprovalGranted,
        "ApprovalRevoked" => AuditEventType::ApprovalRevoked,
        "ApprovalCancelled" => AuditEventType::ApprovalCancelled,
        "ApprovalExpired" => AuditEventType::ApprovalExpired,
        "ReplayInitiated" => AuditEventType::ReplayInitiated,
        "ArtifactProduced" => AuditEventType::ArtifactProduced,
        "ArtifactInvalidated" => AuditEventType::ArtifactInvalidated,
        "CompensationPlanned" => AuditEventType::CompensationPlanned,
        "CompensationStarted" => AuditEventType::CompensationStarted,
        "CompensationCompleted" => AuditEventType::CompensationCompleted,
        "CompensationFailed" => AuditEventType::CompensationFailed,
        "ForensicBundleRequested" => AuditEventType::ForensicBundleRequested,
        "ForensicBundleGenerated" => AuditEventType::ForensicBundleGenerated,
        _ => AuditEventType::RebaseApplied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_payload() -> RebaseApplyAuditPayload {
        RebaseApplyAuditPayload {
            from_version: 1,
            to_version: 2,
            decision_class: "B".to_string(),
            risk_level: 2,
            outcome: "auto_proceeded".to_string(),
            manual_review_required: false,
            rationale: "Test apply".to_string(),
            aligned_checkpoint_id: None,
            checkpoint_alignment_outcome: None,
            runtime_execution_status: "succeeded".to_string(),
            signal_sent: true,
            replay_attempted: false,
            replay_completed: false,
            graph_updates_applied: 0,
            graph_updates_failed: 0,
        }
    }

    #[tokio::test]
    async fn test_create_audit_event() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            event_type: AuditEventType::RebaseApplied,
            actor_id: "test".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };

        let result = repo.create_audit_event(event.clone()).await;
        assert!(result.is_ok());

        // Verify event was stored
        let events = repo.events.read().await;
        assert!(events.contains_key(&event.id));
    }

    #[tokio::test]
    async fn test_record_rebase_applied() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let payload = create_test_payload();
        let trace_ctx = TraceContext::new(
            Some("0af7651916cd43dd8448eb211c80319c".to_string()),
            Some("b7ad6b7169203331".to_string()),
        );

        let result = repo
            .record_rebase_applied(tenant_id, "test-user", intent_id, payload, trace_ctx.clone())
            .await;

        assert!(result.is_ok());

        // Verify event was stored
        let events = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(matches!(
            event.event_type,
            AuditEventType::RebaseApplied
        ));
        // Verify trace context was captured
        assert_eq!(event.trace_id, trace_ctx.trace_id);
        assert_eq!(event.span_id, trace_ctx.span_id);
    }

    #[tokio::test]
    async fn test_record_rebase_apply_blocked() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let payload = RebaseApplyBlockedAuditPayload {
            from_version: 1,
            to_version: 2,
            decision_class: "D".to_string(),
            risk_level: 4,
            rationale: "High risk".to_string(),
            requestor_id: "external-api/unknown".to_string(),
            requestor_type: "external-api".to_string(),
        };

        let result = repo
            .record_rebase_apply_blocked(tenant_id, "test-user", intent_id, payload, TraceContext::default())
            .await;

        assert!(result.is_ok());

        // Verify event was stored
        let events = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            AuditEventType::RebaseApplyBlocked
        ));
    }

    #[tokio::test]
    async fn test_list_by_tenant_with_limit() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let tenant_id = Uuid::new_v4();
        let other_tenant_id = Uuid::new_v4();

        // Create events for tenant_id
        for i in 0..5 {
            let event = AuditEvent {
                id: Uuid::new_v4(),
                tenant_id,
                event_type: AuditEventType::RebaseApplied,
                actor_id: "test".to_string(),
                intent_id: Some(Uuid::new_v4()),
                artifact_id: None,
                payload: serde_json::json!({ "index": i }),
                trace_id: None,
                span_id: None,
                occurred_at: Utc::now(),
            };
            repo.create_audit_event(event).await.unwrap();
        }

        // Create events for other_tenant_id
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: other_tenant_id,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "test".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        repo.create_audit_event(event).await.unwrap();

        // List with limit
        let events = repo.list_by_tenant(tenant_id, 3).await.unwrap();
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent_filters_tenant() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create event for tenant 1
        let event1 = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_1,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "test".to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        repo.create_audit_event(event1).await.unwrap();

        // Create event for tenant 2 with same intent_id
        let event2 = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_2,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "test".to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        repo.create_audit_event(event2).await.unwrap();

        // List for tenant 1 should only return tenant 1's event
        let events = repo.list_by_intent(intent_id, tenant_1).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_id, tenant_1);

        // List for tenant 2 should only return tenant 2's event
        let events = repo.list_by_intent(intent_id, tenant_2).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_id, tenant_2);
    }
}

// =============================================================================
// SqlxAuditRepository unit tests (helper function tests)
// These test the enum conversion logic without requiring a database connection.
// =============================================================================

#[cfg(test)]
mod sqlx_audit_tests {
    use super::*;

    #[test]
    fn test_audit_event_type_to_string() {
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::IntentCreated),
            "IntentCreated"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::IntentUpdated),
            "IntentUpdated"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::IntentArchived),
            "IntentArchived"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::RebaseDetected),
            "RebaseDetected"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::RebasePreviewGenerated),
            "RebasePreviewGenerated"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::RebaseApplied),
            "RebaseApplied"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::RebaseApplyBlocked),
            "RebaseApplyBlocked"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::ApprovalRequired),
            "ApprovalRequired"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::ApprovalGranted),
            "ApprovalGranted"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::ApprovalRevoked),
            "ApprovalRevoked"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::ArtifactProduced),
            "ArtifactProduced"
        );
        assert_eq!(
            audit_event_type_to_string(&AuditEventType::ArtifactInvalidated),
            "ArtifactInvalidated"
        );
    }

    #[test]
    fn test_audit_event_type_from_string() {
        assert_eq!(
            audit_event_type_from_string("IntentCreated"),
            AuditEventType::IntentCreated
        );
        assert_eq!(
            audit_event_type_from_string("IntentUpdated"),
            AuditEventType::IntentUpdated
        );
        assert_eq!(
            audit_event_type_from_string("IntentArchived"),
            AuditEventType::IntentArchived
        );
        assert_eq!(
            audit_event_type_from_string("RebaseDetected"),
            AuditEventType::RebaseDetected
        );
        assert_eq!(
            audit_event_type_from_string("RebasePreviewGenerated"),
            AuditEventType::RebasePreviewGenerated
        );
        assert_eq!(
            audit_event_type_from_string("RebaseApplied"),
            AuditEventType::RebaseApplied
        );
        assert_eq!(
            audit_event_type_from_string("RebaseApplyBlocked"),
            AuditEventType::RebaseApplyBlocked
        );
        assert_eq!(
            audit_event_type_from_string("ApprovalRequired"),
            AuditEventType::ApprovalRequired
        );
        assert_eq!(
            audit_event_type_from_string("ApprovalGranted"),
            AuditEventType::ApprovalGranted
        );
        assert_eq!(
            audit_event_type_from_string("ApprovalRevoked"),
            AuditEventType::ApprovalRevoked
        );
        assert_eq!(
            audit_event_type_from_string("ArtifactProduced"),
            AuditEventType::ArtifactProduced
        );
        assert_eq!(
            audit_event_type_from_string("ArtifactInvalidated"),
            AuditEventType::ArtifactInvalidated
        );
        assert_eq!(
            audit_event_type_from_string("ApprovalExpired"),
            AuditEventType::ApprovalExpired
        );
        // Unknown values default to RebaseApplied
        assert_eq!(
            audit_event_type_from_string("Unknown"),
            AuditEventType::RebaseApplied
        );
    }
}
