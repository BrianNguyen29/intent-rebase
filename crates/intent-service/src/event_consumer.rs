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
/// - Uses configured SnapshotStorage (InMemorySnapshotStorage by default, S3SnapshotStorage when configured)
pub struct SnapshotCreatorConsumer {
    /// Policy snapshot repository for persisting snapshots
    policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>,
    /// Snapshot storage for blob persistence (S3 or InMemory)
    snapshot_storage: Arc<dyn crate::s3_snapshot_storage::SnapshotStorage>,
}

impl SnapshotCreatorConsumer {
    /// Create a new SnapshotCreatorConsumer with the given repository and default InMemory storage.
    pub fn new(policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>) -> Self {
        Self::with_storage(
            policy_snapshot_repo,
            Arc::new(crate::s3_snapshot_storage::InMemorySnapshotStorage::new()),
        )
    }

    /// Create a new SnapshotCreatorConsumer with the given repository and custom storage.
    pub fn with_storage(
        policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>,
        snapshot_storage: Arc<dyn crate::s3_snapshot_storage::SnapshotStorage>,
    ) -> Self {
        Self {
            policy_snapshot_repo,
            snapshot_storage,
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
            .cloned()
            .unwrap_or_default()
    }

    /// Extract required approvers from event payload.
    /// Falls back to empty array when not available.
    fn extract_required_approvers(event: &PublishedEvent) -> Vec<serde_json::Value> {
        event
            .payload
            .get("required_approvers")
            .and_then(|v| v.as_array())
            .cloned()
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

        // Create the policy snapshot with placeholder URI
        let mut snapshot = PolicySnapshot::new(
            tenant_id,
            intent_id,
            intent_version,
            rule_pack_version,
            scope_definition,
        );

        // Phase 3 bounded slice: Upload blob to configured storage (S3 or InMemory fallback)
        // Serialize snapshot to JSON for blob storage
        let blob_bytes = match serde_json::to_vec(&snapshot) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!(
                    "SnapshotCreatorConsumer: failed to serialize snapshot: {}",
                    e
                );
                return ConsumeResult::Failed {
                    reason: format!("snapshot serialization failed: {}", e),
                };
            }
        };

        // Store blob and get actual URI (s3://... or memory://...)
        match self.snapshot_storage.put(&snapshot, &blob_bytes).await {
            Ok(uri) => {
                tracing::debug!(
                    "SnapshotCreatorConsumer: stored blob at '{}' for intent {} v{}",
                    uri,
                    intent_id,
                    intent_version
                );
                // Update snapshot_uri with actual storage URI (replaces placeholder)
                snapshot.snapshot_uri = uri;
            }
            Err(e) => {
                tracing::warn!(
                    "SnapshotCreatorConsumer: failed to store blob, using placeholder URI: {:?}",
                    e
                );
                // Continue with placeholder URI - this is intentional fail-open behavior
            }
        }

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
