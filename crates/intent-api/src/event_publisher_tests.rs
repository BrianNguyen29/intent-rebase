use crate::publish_audit_event;
use crate::router::build_router;
use crate::RebaseOrchestrator;
use graph_service::{GraphService, InMemoryGraphRepository};
use intent_service::{InMemoryCheckpointRepository, InMemoryIntentRepository, IntentService};
use runtime_adapter::MockAdapter;
use std::sync::Arc;
use uuid::Uuid;

use crate::test_helpers::create_test_service_with_publisher;

// =========================================================================
// Phase 2b: Event Publishing Tests (bounded event-streaming slice)
// =========================================================================

#[tokio::test]
async fn test_event_publisher_none_skips_publishing() {
    // Test that when event_publisher is None, publish_audit_event is a no-op
    let publisher: Option<Arc<dyn intent_rebase_types::EventPublisher>> = None;
    let tenant_id = Uuid::new_v4();

    // Should not panic or error - just silently skip
    publish_audit_event(
        &publisher,
        tenant_id,
        "RebaseApplied",
        &serde_json::json!({ "test": true }),
    )
    .await;
}

#[tokio::test]
async fn test_event_publisher_inmemory_stores_events() {
    // Test that InMemoryEventPublisher stores events correctly
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let state = create_test_service_with_publisher(publisher.clone());

    // Verify publisher is ready
    assert!(state.event_publisher.as_ref().unwrap().is_ready());
}

#[tokio::test]
async fn test_publish_audit_event_helper_success() {
    // Test publish_audit_event helper with InMemoryEventPublisher
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let tenant_id = Uuid::new_v4();
    let payload = serde_json::json!({
    "from_version": 1,
    "to_version": 2,
    "outcome": "auto_proceeded"
    });

    let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
        Some(publisher.clone());
    publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;

    // Verify event was published
    let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
    let events = publisher.get_events_for_subject(&subject_str).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].schema_version, "v1");
    assert_eq!(events[0].payload, payload);
}

#[tokio::test]
async fn test_publish_audit_event_helper_multiple_events() {
    // Test that multiple events are published with monotonic sequences
    let publisher = Arc::new(intent_rebase_types::InMemoryEventPublisher::new());
    let tenant_id = Uuid::new_v4();

    let publisher_for_call: Option<Arc<dyn intent_rebase_types::EventPublisher>> =
        Some(publisher.clone());

    // Publish 3 events
    for i in 1..=3 {
        let payload = serde_json::json!({ "index": i });
        publish_audit_event(&publisher_for_call, tenant_id, "RebaseApplied", &payload).await;
    }

    // Verify sequence is monotonic
    let subject_str = format!("audit.events.v1.{}.RebaseApplied", tenant_id);
    let events = publisher.get_events_for_subject(&subject_str).await;
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[2].sequence, 3);
}

#[tokio::test]
async fn test_noop_event_publisher_skips() {
    // Test that NoOpEventPublisher skips all events (always returns Skipped)
    use intent_rebase_types::{EventPublisher, TraceContext};
    let publisher = Arc::new(intent_rebase_types::NoOpEventPublisher::new());
    let tenant_id = Uuid::new_v4();
    let payload = serde_json::json!({ "test": true });
    let subject = intent_rebase_types::EventSubject::from_audit_event(tenant_id, "RebaseApplied");

    // NoOpEventPublisher should skip (return Skipped)
    let result = publisher
        .publish(&subject, &payload, TraceContext::default())
        .await;
    match result {
        intent_rebase_types::PublishResult::Skipped { reason } => {
            assert!(reason.contains("disabled"));
        }
        _ => panic!("Expected Skipped result from NoOpEventPublisher"),
    }
}

#[tokio::test]
async fn test_build_router_accepts_event_publisher() {
    // Test that build_router accepts event_publisher parameter
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo = Arc::new(intent_rebase_types::InMemoryAuditRepository::new())
        as Arc<dyn intent_rebase_types::AuditRepository>;
    let approval_repo = Arc::new(intent_service::InMemoryApprovalRequestRepository::new())
        as Arc<dyn intent_service::ApprovalRequestRepository>;
    let policy_snapshot_repo = Arc::new(intent_service::InMemoryPolicySnapshotRepository::new())
        as Arc<dyn intent_service::PolicySnapshotRepository>;
    let event_publisher = Some(Arc::new(intent_rebase_types::InMemoryEventPublisher::new())
        as Arc<dyn intent_rebase_types::EventPublisher>);
    let side_effect_svc = Arc::new(compensation_service::SideEffectService::new(Arc::new(
        compensation_service::InMemorySideEffectRepository::new(),
    )));
    let compensation_action_svc = Arc::new(compensation_service::CompensationActionService::new(
        Arc::new(compensation_service::InMemoryCompensationActionRepository::new()),
    ));
    let orchestration_run_repo =
        Arc::new(compensation_service::InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(compensation_service::OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));

    let _router: axum::Router = build_router(
        service,
        graph_svc,
        side_effect_svc,
        compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_repo,
        approval_repo,
        policy_snapshot_repo,
        event_publisher,
        Arc::new(forensic_service::InMemoryForensicVerificationService::new())
            as Arc<dyn forensic_service::ForensicVerificationService>,
        Arc::new(forensic_service::InMemoryForensicArchiveGenerator::new())
            as Arc<dyn forensic_service::ForensicArchiveGenerator>,
        Arc::new(forensic_service::ForensicBundleService::new(
            Arc::new(forensic_service::InMemoryBundleRepository::new()),
            Arc::new(forensic_service::InMemoryBundleStorage::new("test-bucket")),
            Arc::new(forensic_service::InMemoryForensicDataCollector::new()),
        )),
        None,
        None,
    );
    // Router builds successfully - this verifies the signature change works
}
