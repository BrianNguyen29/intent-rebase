//! Audit persistence for external rebase apply outcomes
//!
//! Phase 2b bounded slice: Audit persistence for external POST /intents/{intent_id}/rebase-apply outcomes.
//! This module provides storage-agnostic audit event persistence.
//! In-memory implementation for tests, SQL-backed for production.

use super::{
    AuditEvent, AuditEventType, IntentRebaseError, RebaseApplyAuditPayload,
    RebaseApplyBlockedAuditPayload,
};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Repository trait for audit event storage
#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Persist an audit event
    async fn create_audit_event(&self, event: AuditEvent) -> Result<(), IntentRebaseError>;

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
    async fn record_rebase_applied(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyAuditPayload,
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
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }

    /// Record a RebaseApplyBlocked audit event (helper method with default implementation)
    async fn record_rebase_apply_blocked(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyBlockedAuditPayload,
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
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        self.create_audit_event(event).await
    }
}

/// In-memory audit repository for testing and Phase 2b bounded slice
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

/// AuditService provides audit event creation helpers for Phase 2b bounded slice
pub struct AuditService {
    repo: Arc<dyn AuditRepository>,
}

impl AuditService {
    pub fn new(repo: Arc<dyn AuditRepository>) -> Self {
        Self { repo }
    }

    /// Record a RebaseApplied audit event
    pub async fn record_rebase_applied(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyAuditPayload,
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
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };

        self.repo.create_audit_event(event).await
    }

    /// Record a RebaseApplyBlocked audit event (when external apply hits D/E blocked path)
    pub async fn record_rebase_apply_blocked(
        &self,
        tenant_id: Uuid,
        actor_id: &str,
        intent_id: Uuid,
        payload: RebaseApplyBlockedAuditPayload,
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
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };

        self.repo.create_audit_event(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _service = AuditService::new(repo.clone());

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
        let service = AuditService::new(repo.clone());

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let payload = create_test_payload();

        let result = service
            .record_rebase_applied(tenant_id, "test-user", intent_id, payload)
            .await;

        assert!(result.is_ok());

        // Verify event was stored
        let events = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            AuditEventType::RebaseApplied
        ));
    }

    #[tokio::test]
    async fn test_record_rebase_apply_blocked() {
        let repo = Arc::new(InMemoryAuditRepository::new());
        let service = AuditService::new(repo.clone());

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

        let result = service
            .record_rebase_apply_blocked(tenant_id, "test-user", intent_id, payload)
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
