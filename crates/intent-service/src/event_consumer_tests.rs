use crate::event_consumer::*;
use crate::{
    CheckpointRepository, CheckpointService, InMemoryCheckpointRepository,
    InMemoryPolicySnapshotRepository, PolicySnapshotRepository,
};
use intent_rebase_types::{
    CheckpointStatus, CheckpointType, ConsumeResult, EventConsumer, EventPublisher, EventSubject,
    NotificationKind, NotificationRecord, PublishedEvent, ScopeType, TraceContext,
};
use std::sync::Arc;
use uuid::Uuid;

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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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

    publisher
        .publish(&subject, &payload, TraceContext::default())
        .await;

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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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

    publisher
        .publish(&subject, &payload, TraceContext::default())
        .await;

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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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
        trace_id: None,
        span_id: None,
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

    publisher
        .publish(&subject, &payload, TraceContext::default())
        .await;

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
            trace_id: None,
            span_id: None,
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
