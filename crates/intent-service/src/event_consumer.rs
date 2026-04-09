//! Event consumer implementations for Phase 2b bounded slice
//!
//! This module provides concrete event consumers that process published events.
//! The consumers are bounded to in-memory operation for Phase 2b - full NATS-based
//! consumers are Phase 3.
//!
//! ## Bounded Slice (Phase 2b)
//!
//! - `CheckpointCreatorConsumer`: Creates checkpoints when consuming RebaseApplied events
//!   Uses CheckpointService for actual checkpoint creation
//! - `SnapshotCreatorConsumer`: Creates policy snapshots when consuming RebaseApplied events
//!   Uses PolicySnapshotRepository for persistence
//!   **Bounded to event payload scope data**: scope_definition is derived from event payload
//!   with fallback defaults when full scope data is not available in the event.
//! - `NotifierConsumer`: Records notification intents when consuming approval-related events
//!   (ApprovalGranted, ApprovalRevoked, ApprovalCancelled)
//!   **Bounded to in-memory notification recording only (Phase 2b)**: No external
//!   email/webhook/NATS delivery. Full notification delivery is Phase 3.
//!
//! ## What is NOT implemented (Phase 3)
//!
//! - NATS-based consumers with real subscription management
//! - Dead-letter queue (DLQ) for failed event processing
//! - Full consumer startup wiring and lifecycle management
//! - Consumer groups and parallel processing
//! - External notification delivery (email, webhook, NATS)

use async_trait::async_trait;
use intent_rebase_types::{
    CheckpointType, ConsumeResult, EventSubject, NotificationKind, NotificationRecord,
    PolicySnapshot, PublishedEvent, ScopeDefinition, ScopeType,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// Re-export CheckpointService for consumer use
use crate::CheckpointService;

// Re-export PolicySnapshotRepository for SnapshotCreatorConsumer
use crate::PolicySnapshotRepository;

/// Phase 2b: Checkpoint-creator consumer that creates checkpoints from consumed events.
///
/// This consumer implements the `EventConsumer` trait and creates checkpoints
/// when it consumes `RebaseApplied` events. The checkpoint captures:
/// - Intent ID and version
/// - Workflow ID (from event metadata)
/// - Tenant ID
/// - Event outcome and decision class
///
/// **Bounded to Phase 2b**: This consumer:
/// - Works with in-memory events only (no NATS subscription)
/// - Does NOT implement retry logic (Phase 3 DLQ)
/// - Does NOT implement consumer startup wiring (Phase 3)
/// - Is designed for testing the event→checkpoint path
///
/// For production use with real NATS, see Phase 3 consumer infrastructure.
pub struct CheckpointCreatorConsumer {
    /// Checkpoint service for creating checkpoints
    checkpoint_service: Arc<CheckpointService>,
}

impl CheckpointCreatorConsumer {
    /// Create a new CheckpointCreatorConsumer with the given checkpoint service.
    pub fn new(checkpoint_service: Arc<CheckpointService>) -> Self {
        Self { checkpoint_service }
    }

    /// Extract intent_id from event payload.
    /// Returns None if intent_id is not found in payload.
    fn extract_intent_id(event: &PublishedEvent) -> Option<Uuid> {
        event
            .payload
            .get("intent_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .or_else(|| {
                event
                    .payload
                    .get("intentId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
            })
    }

    /// Extract tenant_id from event subject.
    /// The subject format is: audit.events.v1.<tenant_id>.<event_type>
    fn extract_tenant_id(subject: &EventSubject) -> Uuid {
        // Parse tenant_id from subject - it's the 4th component
        // Format: audit.events.v1.<tenant_id>.<event_type>
        let parts: Vec<&str> = subject.subject.split('.').collect();
        if parts.len() >= 4 {
            Uuid::parse_str(parts[3]).unwrap_or_else(|_| Uuid::nil())
        } else {
            Uuid::nil()
        }
    }

    /// Extract intent version from event payload (to_version).
    fn extract_intent_version(event: &PublishedEvent) -> i32 {
        event
            .payload
            .get("to_version")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0)
    }

    /// Extract from_version from event payload.
    fn extract_from_version(event: &PublishedEvent) -> i32 {
        event
            .payload
            .get("from_version")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0)
    }

    /// Extract workflow_id from event payload.
    fn extract_workflow_id(event: &PublishedEvent) -> Uuid {
        event
            .payload
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::nil)
    }

    /// Extract outcome from event payload.
    fn extract_outcome(event: &PublishedEvent) -> String {
        event
            .payload
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Map outcome string to CheckpointType.
    fn map_outcome_to_checkpoint_type(outcome: &str) -> CheckpointType {
        match outcome {
            "auto_proceeded" => CheckpointType::RebaseCompleted,
            "requires_approval" => CheckpointType::RebaseCompleted, // Fallback to RebaseCompleted for approval cases
            "approval_denied" => CheckpointType::RebaseCompleted,
            "rejected" => CheckpointType::RebaseCompleted,
            _ => CheckpointType::RebaseCompleted,
        }
    }
}

#[async_trait]
impl intent_rebase_types::EventConsumer for CheckpointCreatorConsumer {
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult {
        // Only process RebaseApplied events
        if !event.subject.contains("RebaseApplied") {
            tracing::debug!(
                "CheckpointCreatorConsumer: skipping non-RebaseApplied event '{}'",
                event.subject
            );
            return ConsumeResult::Consumed {
                subject: event.subject.clone(),
                sequence: event.sequence,
            };
        }

        // Extract required fields
        let intent_id = match Self::extract_intent_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!("CheckpointCreatorConsumer: missing intent_id in event payload");
                return ConsumeResult::Failed {
                    reason: "missing intent_id in event payload".to_string(),
                };
            }
        };

        let tenant_id = Self::extract_tenant_id(&EventSubject {
            subject: event.subject.clone(),
            schema_version: "v1",
            event_type: "RebaseApplied".to_string(),
            tenant_id: Uuid::nil(),
        });

        let intent_version = Self::extract_intent_version(event);
        let from_version = Self::extract_from_version(event);
        let outcome = Self::extract_outcome(event);
        let workflow_id = Self::extract_workflow_id(event);
        let checkpoint_type = Self::map_outcome_to_checkpoint_type(&outcome);

        // Build workflow state from event payload
        let workflow_state = serde_json::json!({
            "from_version": from_version,
            "to_version": intent_version,
            "outcome": outcome,
            "decision_class": event.payload.get("decision_class").and_then(|v| v.as_str()).unwrap_or("unknown"),
        });

        // Create checkpoint via CheckpointService
        match self
            .checkpoint_service
            .create_checkpoint_with_defaults(
                intent_id,
                intent_version,
                workflow_id,
                tenant_id,
                checkpoint_type,
                workflow_state,
            )
            .await
        {
            Ok(checkpoint) => {
                tracing::info!(
                    "CheckpointCreatorConsumer: created checkpoint {} for intent {} v{}",
                    checkpoint.checkpoint_id,
                    intent_id,
                    intent_version
                );
                ConsumeResult::Consumed {
                    subject: event.subject.clone(),
                    sequence: event.sequence,
                }
            }
            Err(e) => {
                tracing::error!(
                    "CheckpointCreatorConsumer: failed to create checkpoint: {:?}",
                    e
                );
                ConsumeResult::Failed {
                    reason: format!("checkpoint creation failed: {}", e),
                }
            }
        }
    }
}

/// Phase 2b: Snapshot-creator consumer that creates policy snapshots from consumed events.
///
/// This consumer implements the `EventConsumer` trait and creates policy snapshots
/// when it consumes `RebaseApplied` events. The snapshot captures:
///
/// - Intent ID and version
/// - Rule pack version (from event payload, with fallback)
/// - Scope definition (from event payload with fallback defaults)
///
/// **Bounded to Phase 2b with limited scope data**:
///
/// This consumer derives `scope_definition` from event payload fields:
/// - `affected_resources`: extracted from payload if present, else empty
/// - `required_approvers`: extracted from payload if present, else empty
/// - `min_approvals`: extracted from payload if present, else default 1
/// - `scope_type`: derived from payload if present, else `ScopeType::None`
///
/// When full scope data is not available in the event payload, the snapshot
/// uses default/empty scope values. This is an inherent limitation of the
/// event-driven approach for snapshot creation without access to the full intent scope.
///
/// **Bounded to Phase 2b**:
/// - Works with in-memory events only (no NATS subscription)
/// - Does NOT retry logic (Phase 3 DLQ)
/// - Does NOT implement consumer startup wiring (Phase 3)
/// - Uses `memory://` URI placeholder (S3 upload is Phase 3)
pub struct SnapshotCreatorConsumer {
    /// Policy snapshot repository for persisting snapshots
    policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>,
}

impl SnapshotCreatorConsumer {
    /// Create a new SnapshotCreatorConsumer with the given repository.
    pub fn new(policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>) -> Self {
        Self {
            policy_snapshot_repo,
        }
    }

    /// Extract intent_id from event payload.
    fn extract_intent_id(event: &PublishedEvent) -> Option<Uuid> {
        event
            .payload
            .get("intent_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .or_else(|| {
                event
                    .payload
                    .get("intentId")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
            })
    }

    /// Extract tenant_id from event subject.
    fn extract_tenant_id(subject: &EventSubject) -> Uuid {
        let parts: Vec<&str> = subject.subject.split('.').collect();
        if parts.len() >= 4 {
            Uuid::parse_str(parts[3]).unwrap_or_else(|_| Uuid::nil())
        } else {
            Uuid::nil()
        }
    }

    /// Extract intent version from event payload (to_version).
    fn extract_intent_version(event: &PublishedEvent) -> i32 {
        event
            .payload
            .get("to_version")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(0)
    }

    /// Extract rule pack version from event payload.
    fn extract_rule_pack_version(event: &PublishedEvent) -> String {
        event
            .payload
            .get("rule_pack_version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "v1.0.0".to_string())
    }

    /// Extract scope_type from event payload.
    /// Falls back to ScopeType::None when not available.
    fn extract_scope_type(event: &PublishedEvent) -> ScopeType {
        event
            .payload
            .get("scope_type")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "full" => ScopeType::Full,
                "partial" => ScopeType::Partial,
                _ => ScopeType::None,
            })
            .unwrap_or(ScopeType::None)
    }

    /// Extract affected resources from event payload.
    /// Falls back to empty array when not available.
    fn extract_affected_resources(event: &PublishedEvent) -> Vec<serde_json::Value> {
        event
            .payload
            .get("affected_resources")
            .and_then(|v| v.as_array())
            .map(|arr| arr.clone())
            .unwrap_or_default()
    }

    /// Extract required approvers from event payload.
    /// Falls back to empty array when not available.
    fn extract_required_approvers(event: &PublishedEvent) -> Vec<serde_json::Value> {
        event
            .payload
            .get("required_approvers")
            .and_then(|v| v.as_array())
            .map(|arr| arr.clone())
            .unwrap_or_default()
    }

    /// Extract min_approvals from event payload.
    /// Falls back to 1 when not available.
    fn extract_min_approvals(event: &PublishedEvent) -> i32 {
        event
            .payload
            .get("min_approvals")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(1)
    }

    /// Build a ScopeDefinition from event payload fields.
    ///
    /// **Bounded scope data extraction**: When event payload doesn't contain
    /// full scope data, uses default/empty values. This is an inherent limitation
    /// of event-driven snapshot creation without access to the full intent scope.
    fn build_scope_definition(event: &PublishedEvent) -> ScopeDefinition {
        ScopeDefinition {
            scope_type: Self::extract_scope_type(event),
            affected_resources: Self::extract_affected_resources(event),
            required_approvers: Self::extract_required_approvers(event),
            min_approvals: Self::extract_min_approvals(event),
        }
    }
}

#[async_trait]
impl intent_rebase_types::EventConsumer for SnapshotCreatorConsumer {
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult {
        // Only process RebaseApplied events
        if !event.subject.contains("RebaseApplied") {
            tracing::debug!(
                "SnapshotCreatorConsumer: skipping non-RebaseApplied event '{}'",
                event.subject
            );
            return ConsumeResult::Consumed {
                subject: event.subject.clone(),
                sequence: event.sequence,
            };
        }

        // Extract required fields
        let intent_id = match Self::extract_intent_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!("SnapshotCreatorConsumer: missing intent_id in event payload");
                return ConsumeResult::Failed {
                    reason: "missing intent_id in event payload".to_string(),
                };
            }
        };

        let tenant_id = Self::extract_tenant_id(&EventSubject {
            subject: event.subject.clone(),
            schema_version: "v1",
            event_type: "RebaseApplied".to_string(),
            tenant_id: Uuid::nil(),
        });

        let intent_version = Self::extract_intent_version(event);
        if intent_version == 0 {
            tracing::warn!(
                "SnapshotCreatorConsumer: missing or invalid to_version in event payload"
            );
            return ConsumeResult::Failed {
                reason: "missing to_version in event payload".to_string(),
            };
        }

        let rule_pack_version = Self::extract_rule_pack_version(event);
        let scope_definition = Self::build_scope_definition(event);

        // Create the policy snapshot
        let snapshot = PolicySnapshot::new(
            tenant_id,
            intent_id,
            intent_version,
            rule_pack_version,
            scope_definition,
        );

        match self.policy_snapshot_repo.create_snapshot(snapshot).await {
            Ok(created) => {
                tracing::info!(
                    "SnapshotCreatorConsumer: created policy snapshot {} for intent {} v{}",
                    created.id,
                    intent_id,
                    intent_version
                );
                ConsumeResult::Consumed {
                    subject: event.subject.clone(),
                    sequence: event.sequence,
                }
            }
            Err(e) => {
                tracing::error!(
                    "SnapshotCreatorConsumer: failed to create policy snapshot: {:?}",
                    e
                );
                ConsumeResult::Failed {
                    reason: format!("policy snapshot creation failed: {}", e),
                }
            }
        }
    }
}

// =============================================================================
// In-Memory Notification Store (for testing notifier consumer)
// =============================================================================

/// Phase 2b: In-memory notification store for testing the notifier consumer.
///
/// This store records notification intents from the `NotifierConsumer`.
/// It is bounded to in-memory operation for Phase 2b testing.
///
/// **This is bounded to testing only (Phase 2b)**:
/// - No external notification delivery
/// - No persistence
/// - Full notification delivery is Phase 3
#[derive(Debug)]
pub struct InMemoryNotificationStore {
    /// Stored notification records
    records: RwLock<Vec<NotificationRecord>>,
}

impl InMemoryNotificationStore {
    /// Create a new in-memory notification store
    pub fn new() -> Self {
        Self {
            records: RwLock::new(Vec::new()),
        }
    }

    /// Add a notification record
    pub async fn add(&self, record: NotificationRecord) {
        let mut records = self.records.write().await;
        records.push(record);
    }

    /// Get all notification records
    pub async fn get_all(&self) -> Vec<NotificationRecord> {
        let records = self.records.read().await;
        records.clone()
    }

    /// Get notification records by kind
    pub async fn get_by_kind(&self, kind: NotificationKind) -> Vec<NotificationRecord> {
        let records = self.records.read().await;
        records.iter().filter(|r| r.kind == kind).cloned().collect()
    }

    /// Get notification records for a specific intent
    pub async fn get_by_intent(&self, intent_id: Uuid) -> Vec<NotificationRecord> {
        let records = self.records.read().await;
        records
            .iter()
            .filter(|r| r.intent_id == intent_id)
            .cloned()
            .collect()
    }

    /// Get the count of notification records
    pub async fn count(&self) -> usize {
        let records = self.records.read().await;
        records.len()
    }

    /// Clear all notification records (for test isolation)
    pub async fn clear(&self) {
        let mut records = self.records.write().await;
        records.clear();
    }

    /// Check if any notification records exist
    pub async fn has_records(&self) -> bool {
        let records = self.records.read().await;
        !records.is_empty()
    }
}

impl Default for InMemoryNotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Notifier Consumer (Phase 2b bounded slice)
// =============================================================================

/// Phase 2b: Notifier consumer that records notification intents from consumed events.
///
/// This consumer implements the `EventConsumer` trait and records notification intents
/// when it consumes approval-related events:
///
/// - `ApprovalGranted`: Records notification that approval was granted
/// - `ApprovalRevoked`: Records notification that approval was revoked/rejected
/// - `ApprovalCancelled`: Records notification that approvals were cancelled
///
/// **Bounded to Phase 2b**:
/// - Works with in-memory events only (no NATS subscription)
/// - Records notification intents IN MEMORY only - does NOT send external notifications
/// - Does NOT implement retry logic (Phase 3 DLQ)
/// - Does NOT implement consumer startup wiring (Phase 3)
/// - Does NOT implement external notification delivery (Phase 3)
///
/// For production use with real NATS and external notification delivery, see Phase 3.
pub struct NotifierConsumer {
    /// Notification store for recording notification intents
    notification_store: Arc<InMemoryNotificationStore>,
}

impl NotifierConsumer {
    /// Create a new NotifierConsumer with the given notification store.
    pub fn new(notification_store: Arc<InMemoryNotificationStore>) -> Self {
        Self { notification_store }
    }

    /// Extract tenant_id from event subject.
    /// The subject format is: audit.events.v1.<tenant_id>.<event_type>
    fn extract_tenant_id(subject: &EventSubject) -> Uuid {
        // Parse tenant_id from subject - it's the 4th component
        // Format: audit.events.v1.<tenant_id>.<event_type>
        let parts: Vec<&str> = subject.subject.split('.').collect();
        if parts.len() >= 4 {
            Uuid::parse_str(parts[3]).unwrap_or_else(|_| Uuid::nil())
        } else {
            Uuid::nil()
        }
    }

    /// Extract approval_request_id from event payload.
    fn extract_approval_request_id(event: &PublishedEvent) -> Option<Uuid> {
        event
            .payload
            .get("approval_request_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Extract intent_id from event payload.
    fn extract_intent_id(event: &PublishedEvent) -> Option<Uuid> {
        event
            .payload
            .get("intent_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Extract decision_class from event payload.
    fn extract_decision_class(event: &PublishedEvent) -> String {
        event
            .payload
            .get("decision_class")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Handle ApprovalGranted event - record notification intent.
    fn handle_approval_granted(&self, tenant_id: Uuid, event: &PublishedEvent) -> ConsumeResult {
        let intent_id = match Self::extract_intent_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!("NotifierConsumer: missing intent_id in ApprovalGranted event");
                return ConsumeResult::Failed {
                    reason: "missing intent_id in event payload".to_string(),
                };
            }
        };

        let approval_request_id = match Self::extract_approval_request_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!(
                    "NotifierConsumer: missing approval_request_id in ApprovalGranted event"
                );
                return ConsumeResult::Failed {
                    reason: "missing approval_request_id in event payload".to_string(),
                };
            }
        };

        let decision_class = Self::extract_decision_class(event);

        let record = NotificationRecord::approval_granted(
            tenant_id,
            intent_id,
            approval_request_id,
            &decision_class,
            event.sequence,
        );

        // Record the notification intent
        let notification_store = self.notification_store.clone();
        let record_clone = record.clone();
        tokio::spawn(async move {
            notification_store.add(record_clone).await;
        });

        tracing::info!(
            "NotifierConsumer: recorded ApprovalGranted notification for intent {}",
            intent_id
        );

        ConsumeResult::Consumed {
            subject: event.subject.clone(),
            sequence: event.sequence,
        }
    }

    /// Handle ApprovalRevoked event - record notification intent.
    fn handle_approval_revoked(&self, tenant_id: Uuid, event: &PublishedEvent) -> ConsumeResult {
        let intent_id = match Self::extract_intent_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!("NotifierConsumer: missing intent_id in ApprovalRevoked event");
                return ConsumeResult::Failed {
                    reason: "missing intent_id in event payload".to_string(),
                };
            }
        };

        let approval_request_id = match Self::extract_approval_request_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!(
                    "NotifierConsumer: missing approval_request_id in ApprovalRevoked event"
                );
                return ConsumeResult::Failed {
                    reason: "missing approval_request_id in event payload".to_string(),
                };
            }
        };

        let decision_class = Self::extract_decision_class(event);

        let record = NotificationRecord::approval_revoked(
            tenant_id,
            intent_id,
            approval_request_id,
            &decision_class,
            event.sequence,
        );

        // Record the notification intent
        let notification_store = self.notification_store.clone();
        let record_clone = record.clone();
        tokio::spawn(async move {
            notification_store.add(record_clone).await;
        });

        tracing::info!(
            "NotifierConsumer: recorded ApprovalRevoked notification for intent {}",
            intent_id
        );

        ConsumeResult::Consumed {
            subject: event.subject.clone(),
            sequence: event.sequence,
        }
    }

    /// Handle ApprovalCancelled event - record notification intent.
    fn handle_approval_cancelled(&self, tenant_id: Uuid, event: &PublishedEvent) -> ConsumeResult {
        let intent_id = match Self::extract_intent_id(event) {
            Some(id) => id,
            None => {
                tracing::warn!("NotifierConsumer: missing intent_id in ApprovalCancelled event");
                return ConsumeResult::Failed {
                    reason: "missing intent_id in event payload".to_string(),
                };
            }
        };

        // Extract cancelled_count
        let cancelled_count = event
            .payload
            .get("cancelled_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        // Extract cancellation_reason
        let cancellation_reason = event
            .payload
            .get("cancellation_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let record = NotificationRecord::approval_cancelled(
            tenant_id,
            intent_id,
            cancelled_count,
            &cancellation_reason,
            event.sequence,
        );

        // Record the notification intent
        let notification_store = self.notification_store.clone();
        let record_clone = record.clone();
        tokio::spawn(async move {
            notification_store.add(record_clone).await;
        });

        tracing::info!(
            "NotifierConsumer: recorded ApprovalCancelled notification for intent {} ({} cancelled)",
            intent_id,
            cancelled_count
        );

        ConsumeResult::Consumed {
            subject: event.subject.clone(),
            sequence: event.sequence,
        }
    }
}

#[async_trait]
impl intent_rebase_types::EventConsumer for NotifierConsumer {
    async fn consume(&self, event: &PublishedEvent) -> ConsumeResult {
        let tenant_id = Self::extract_tenant_id(&EventSubject {
            subject: event.subject.clone(),
            schema_version: "v1",
            event_type: "".to_string(),
            tenant_id: Uuid::nil(),
        });

        // Route to appropriate handler based on event type
        if event.subject.contains("ApprovalGranted") {
            self.handle_approval_granted(tenant_id, event)
        } else if event.subject.contains("ApprovalRevoked") {
            self.handle_approval_revoked(tenant_id, event)
        } else if event.subject.contains("ApprovalCancelled") {
            self.handle_approval_cancelled(tenant_id, event)
        } else {
            // Skip events not relevant to notification
            tracing::debug!(
                "NotifierConsumer: skipping non-approval event '{}'",
                event.subject
            );
            ConsumeResult::Consumed {
                subject: event.subject.clone(),
                sequence: event.sequence,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckpointRepository, CheckpointService, InMemoryCheckpointRepository};
    use crate::{InMemoryPolicySnapshotRepository, PolicySnapshotRepository};
    use intent_rebase_types::{
        CheckpointStatus, EventConsumer, EventPublisher, EventSubject, ScopeType,
    };
    use std::sync::Arc;

    fn create_test_event(
        tenant_id: Uuid,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
        outcome: &str,
    ) -> PublishedEvent {
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let payload = serde_json::json!({
            "intent_id": intent_id.to_string(),
            "from_version": from_version,
            "to_version": to_version,
            "outcome": outcome,
            "decision_class": "B",
            "workflow_id": Uuid::new_v4().to_string(),
        });

        PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload,
            published_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_consumer_creates_checkpoint_on_rebase_applied() {
        // Setup
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
        let consumer = Arc::new(CheckpointCreatorConsumer::new(checkpoint_service));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let event = create_test_event(tenant_id, intent_id, 1, 2, "auto_proceeded");

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Verify checkpoint was created
        let checkpoints = checkpoint_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 1);

        let checkpoint = &checkpoints[0];
        assert_eq!(checkpoint.intent_id, intent_id);
        assert_eq!(checkpoint.intent_version, 2); // to_version
        assert_eq!(checkpoint.checkpoint_type, CheckpointType::RebaseCompleted);
    }

    #[tokio::test]
    async fn test_consumer_skips_non_rebase_events() {
        // Setup
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
        let consumer = Arc::new(CheckpointCreatorConsumer::new(checkpoint_service));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "approval_request_id": Uuid::new_v4().to_string(),
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume non-RebaseApplied event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // No checkpoint should be created
        let checkpoints = checkpoint_repo
            .list_by_intent(Uuid::new_v4(), tenant_id)
            .await
            .unwrap();
        assert!(checkpoints.is_empty());
    }

    #[tokio::test]
    async fn test_consumer_handles_missing_intent_id() {
        // Setup
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
        let consumer = Arc::new(CheckpointCreatorConsumer::new(checkpoint_service));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                // Missing intent_id
                "from_version": 1,
                "to_version": 2,
                "outcome": "auto_proceeded",
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume event with missing intent_id
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Failed { .. }));
    }

    #[tokio::test]
    async fn test_consumer_uses_correct_checkpoint_type_for_outcome() {
        // Setup
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
        let consumer = Arc::new(CheckpointCreatorConsumer::new(checkpoint_service));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Test auto_proceeded -> RebaseCompleted
        let event1 = create_test_event(tenant_id, intent_id, 1, 2, "auto_proceeded");
        consumer.consume(&event1).await;

        let checkpoints = checkpoint_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(
            checkpoints[0].checkpoint_type,
            CheckpointType::RebaseCompleted
        );
    }

    #[tokio::test]
    async fn test_publish_consume_checkpoint_cycle() {
        // Full cycle test: publish event -> consume with CheckpointCreatorConsumer -> verify checkpoint
        use intent_rebase_types::InMemoryEventPublisher;

        // Setup services
        let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
        let checkpoint_service = Arc::new(CheckpointService::new(checkpoint_repo.clone()));
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(CheckpointCreatorConsumer::new(checkpoint_service));

        // Create and publish a RebaseApplied event
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let payload = serde_json::json!({
            "intent_id": intent_id.to_string(),
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded",
            "decision_class": "B",
            "workflow_id": workflow_id.to_string(),
        });

        publisher.publish(&subject, &payload).await;

        // Verify event was published
        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 1);

        // Consume the event (triggers checkpoint creation)
        let consume_result = consumer.consume(&events[0]).await;
        assert!(matches!(consume_result, ConsumeResult::Consumed { .. }));

        // Verify checkpoint was created via CheckpointService
        let checkpoints = checkpoint_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 1);

        let checkpoint = &checkpoints[0];
        assert_eq!(checkpoint.intent_id, intent_id);
        assert_eq!(checkpoint.intent_version, 2);
        assert_eq!(checkpoint.workflow_id, workflow_id);
        assert_eq!(checkpoint.tenant_id, tenant_id);
        assert_eq!(checkpoint.status, CheckpointStatus::Pending);
        assert_eq!(checkpoint.checkpoint_type, CheckpointType::RebaseCompleted);

        // Verify workflow_state contains event data
        assert_eq!(
            checkpoint.workflow_state.get("from_version").unwrap(),
            &serde_json::json!(1)
        );
        assert_eq!(
            checkpoint.workflow_state.get("to_version").unwrap(),
            &serde_json::json!(2)
        );
        assert_eq!(
            checkpoint.workflow_state.get("outcome").unwrap(),
            &serde_json::json!("auto_proceeded")
        );
    }

    // =====================================================================
    // NotifierConsumer tests (Phase 2b bounded notifier slice)
    // =====================================================================

    #[tokio::test]
    async fn test_notifier_consumer_records_approval_granted() {
        // Setup
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "approval_request_id": approval_request_id.to_string(),
                "intent_id": intent_id.to_string(),
                "decision_class": "D",
                "resolved_by": "admin",
                "resolution_notes": "Approved after review",
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Give the spawned task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify notification was recorded
        let records = notification_store.get_all().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].intent_id, intent_id);
        assert_eq!(records[0].kind, NotificationKind::ApprovalGranted);
        assert!(records[0].message.contains("Approval granted"));
    }

    #[tokio::test]
    async fn test_notifier_consumer_records_approval_revoked() {
        // Setup
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalRevoked");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "approval_request_id": approval_request_id.to_string(),
                "intent_id": intent_id.to_string(),
                "decision_class": "E",
                "resolved_by": "admin",
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Give the spawned task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify notification was recorded
        let records = notification_store.get_all().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].intent_id, intent_id);
        assert_eq!(records[0].kind, NotificationKind::ApprovalRevoked);
        assert!(records[0].message.contains("Approval revoked"));
    }

    #[tokio::test]
    async fn test_notifier_consumer_records_approval_cancelled() {
        // Setup
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalCancelled");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "intent_id": intent_id.to_string(),
                "cancelled_version_from": 1,
                "cancelled_version_to": 2,
                "decision_class": "D/E",
                "cancelled_by": "intent-service/system",
                "cancellation_reason": "Intent version changed",
                "cancelled_count": 3,
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Give the spawned task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify notification was recorded
        let records = notification_store.get_all().await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].intent_id, intent_id);
        assert_eq!(records[0].kind, NotificationKind::ApprovalCancelled);
        assert!(records[0].message.contains("Approval cancelled"));
    }

    #[tokio::test]
    async fn test_notifier_consumer_skips_non_approval_events() {
        // Setup
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "intent_id": Uuid::new_v4().to_string(),
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume non-approval event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Verify no notification was recorded
        let count = notification_store.count().await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_notifier_consumer_handles_missing_intent_id() {
        // Setup
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                // Missing intent_id
                "approval_request_id": Uuid::new_v4().to_string(),
                "decision_class": "D",
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume event with missing intent_id
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Failed { .. }));
    }

    #[tokio::test]
    async fn test_notifier_consumer_publish_consume_notification_cycle() {
        // Full cycle test: publish event -> consume with NotifierConsumer -> verify notification recorded
        use intent_rebase_types::InMemoryEventPublisher;

        // Setup services
        let notification_store = Arc::new(InMemoryNotificationStore::new());
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(NotifierConsumer::new(notification_store.clone()));

        // Create and publish an ApprovalGranted event
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let approval_request_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");
        let payload = serde_json::json!({
            "approval_request_id": approval_request_id.to_string(),
            "intent_id": intent_id.to_string(),
            "intent_version_from": 1,
            "intent_version_to": 2,
            "decision_class": "D",
            "resolved_by": "admin",
        });

        publisher.publish(&subject, &payload).await;

        // Verify event was published
        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 1);

        // Consume the event (triggers notification recording)
        let consume_result = consumer.consume(&events[0]).await;
        assert!(matches!(consume_result, ConsumeResult::Consumed { .. }));

        // Give the spawned task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify notification was recorded
        let records = notification_store.get_all().await;
        assert_eq!(records.len(), 1);

        let record = &records[0];
        assert_eq!(record.intent_id, intent_id);
        assert_eq!(record.tenant_id, tenant_id);
        assert_eq!(record.kind, NotificationKind::ApprovalGranted);
        assert!(record.message.contains("D"));
        assert_eq!(record.source_sequence, 1);

        // Verify we can filter by kind
        let granted_records = notification_store
            .get_by_kind(NotificationKind::ApprovalGranted)
            .await;
        assert_eq!(granted_records.len(), 1);

        let revoked_records = notification_store
            .get_by_kind(NotificationKind::ApprovalRevoked)
            .await;
        assert_eq!(revoked_records.len(), 0);

        // Verify we can filter by intent
        let intent_records = notification_store.get_by_intent(intent_id).await;
        assert_eq!(intent_records.len(), 1);
    }

    #[tokio::test]
    async fn test_notification_store_clear_and_count() {
        let notification_store = Arc::new(InMemoryNotificationStore::new());

        // Initially empty
        assert!(!notification_store.has_records().await);
        assert_eq!(notification_store.count().await, 0);

        // Add a notification directly
        let record = NotificationRecord::approval_granted(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "D",
            1,
        );
        notification_store.add(record).await;

        assert!(notification_store.has_records().await);
        assert_eq!(notification_store.count().await, 1);

        // Clear
        notification_store.clear().await;
        assert!(!notification_store.has_records().await);
        assert_eq!(notification_store.count().await, 0);
    }

    // =====================================================================
    // SnapshotCreatorConsumer tests (Phase 2b bounded slice)
    // =====================================================================

    #[tokio::test]
    async fn test_snapshot_creator_creates_snapshot_on_rebase_applied() {
        // Setup
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let payload = serde_json::json!({
            "intent_id": intent_id.to_string(),
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded",
            "decision_class": "B",
            "rule_pack_version": "v2.1.0",
            "scope_type": "partial",
            "affected_resources": [
                {"type": "artifact", "id": "artifact-123"}
            ],
            "required_approvers": [
                {"type": "role", "id": "admin"}
            ],
            "min_approvals": 2,
        });

        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload,
            published_at: chrono::Utc::now(),
        };

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Verify snapshot was created
        let snapshots = policy_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let snapshot = &snapshots[0];
        assert_eq!(snapshot.intent_id, intent_id);
        assert_eq!(snapshot.intent_version, 2);
        assert_eq!(snapshot.rule_pack_version, "v2.1.0");
        assert_eq!(snapshot.scope_definition.scope_type, ScopeType::Partial);
        assert_eq!(snapshot.scope_definition.min_approvals, 2);
        assert!(!snapshot.scope_hash.is_empty());
        // URI should be memory:// placeholder
        assert!(snapshot.snapshot_uri.starts_with("memory://"));
    }

    #[tokio::test]
    async fn test_snapshot_creator_skips_non_rebase_events() {
        // Setup
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "ApprovalGranted");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                "intent_id": Uuid::new_v4().to_string(),
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume non-RebaseApplied event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // No snapshot should be created
        let snapshots = policy_repo
            .list_by_intent(Uuid::new_v4(), tenant_id)
            .await
            .unwrap();
        assert!(snapshots.is_empty());
    }

    #[tokio::test]
    async fn test_snapshot_creator_handles_missing_intent_id() {
        // Setup
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        let tenant_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload: serde_json::json!({
                // Missing intent_id
                "from_version": 1,
                "to_version": 2,
            }),
            published_at: chrono::Utc::now(),
        };

        // Consume event with missing intent_id
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Failed { .. }));
    }

    #[tokio::test]
    async fn test_snapshot_creator_uses_defaults_when_scope_data_missing() {
        // Setup
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        // Payload with no scope data
        let payload = serde_json::json!({
            "intent_id": intent_id.to_string(),
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded",
        });

        let event = PublishedEvent {
            subject: subject.subject,
            schema_version: "v1".to_string(),
            sequence: 1,
            payload,
            published_at: chrono::Utc::now(),
        };

        // Consume the event
        let result = consumer.consume(&event).await;
        assert!(matches!(result, ConsumeResult::Consumed { .. }));

        // Verify snapshot was created with default scope
        let snapshots = policy_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let snapshot = &snapshots[0];
        assert_eq!(snapshot.scope_definition.scope_type, ScopeType::None);
        assert!(snapshot.scope_definition.affected_resources.is_empty());
        assert!(snapshot.scope_definition.required_approvers.is_empty());
        assert_eq!(snapshot.scope_definition.min_approvals, 1); // default
        assert_eq!(snapshot.rule_pack_version, "v1.0.0"); // default
    }

    #[tokio::test]
    async fn test_snapshot_creator_publish_consume_snapshot_cycle() {
        use intent_rebase_types::InMemoryEventPublisher;

        // Setup services
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let publisher = Arc::new(InMemoryEventPublisher::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        // Create and publish a RebaseApplied event
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
        let payload = serde_json::json!({
            "intent_id": intent_id.to_string(),
            "from_version": 1,
            "to_version": 2,
            "outcome": "auto_proceeded",
            "decision_class": "B",
            "rule_pack_version": "v3.0.0",
            "scope_type": "full",
            "min_approvals": 1,
        });

        publisher.publish(&subject, &payload).await;

        // Verify event was published
        let events = publisher.get_events_for_subject(&subject.subject).await;
        assert_eq!(events.len(), 1);

        // Consume the event (triggers snapshot creation)
        let consume_result = consumer.consume(&events[0]).await;
        assert!(matches!(consume_result, ConsumeResult::Consumed { .. }));

        // Verify snapshot was created via repository
        let snapshots = policy_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 1);

        let snapshot = &snapshots[0];
        assert_eq!(snapshot.intent_id, intent_id);
        assert_eq!(snapshot.intent_version, 2);
        assert_eq!(snapshot.rule_pack_version, "v3.0.0");
        assert_eq!(snapshot.scope_definition.scope_type, ScopeType::Full);
        assert!(snapshot.scope_hash.len() == 64); // SHA256 hex
    }

    #[tokio::test]
    async fn test_snapshot_creator_multiple_versions() {
        // Setup
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());
        let consumer = Arc::new(SnapshotCreatorConsumer::new(policy_repo.clone()));

        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();

        // Create snapshots for versions 1, 2, 3
        for version in 1..=3 {
            let subject = EventSubject::from_audit_event(tenant_id, "RebaseApplied");
            let payload = serde_json::json!({
                "intent_id": intent_id.to_string(),
                "from_version": version - 1,
                "to_version": version,
                "outcome": "auto_proceeded",
            });

            let event = PublishedEvent {
                subject: subject.subject,
                schema_version: "v1".to_string(),
                sequence: version as u64,
                payload,
                published_at: chrono::Utc::now(),
            };

            consumer.consume(&event).await;
        }

        // Verify 3 snapshots were created
        let snapshots = policy_repo
            .list_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 3);

        // Versions should be 1, 2, 3
        let versions: Vec<i32> = snapshots.iter().map(|s| s.intent_version).collect();
        assert!(versions.contains(&1));
        assert!(versions.contains(&2));
        assert!(versions.contains(&3));
    }
}
