use crate::*;
use chrono::Utc;
use intent_rebase_types::IntentRebaseError;
use std::sync::Arc;
use uuid::Uuid;

fn create_test_bundle(tenant_id: Uuid, purpose: BundlePurpose) -> ForensicBundle {
    ForensicBundle::new(
        tenant_id,
        crate::BundleTimeRange {
            start: Utc::now(),
            end: Utc::now(),
        },
        purpose,
        BundleContents::default(),
        "test-user",
        None,
    )
}

// =============================================================================
// Repository tests
// =============================================================================

#[tokio::test]
async fn test_create_bundle() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    let result = repo.create(bundle.clone()).await;
    assert!(result.is_ok());
    let created = result.unwrap();
    assert_eq!(created.bundle_id, bundle_id);
    assert_eq!(created.tenant_id, tenant_id);
    assert_eq!(created.status, BundleStatus::Pending);
}

#[tokio::test]
async fn test_create_bundle_duplicate_id() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);

    repo.create(bundle.clone()).await.unwrap();

    // Try to create with same bundle (same ID)
    let result = repo.create(bundle).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_bundle() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::ComplianceAudit);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();

    let result = repo.get(bundle_id).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().bundle_id, bundle_id);
}

#[tokio::test]
async fn test_get_bundle_not_found() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let result = repo.get(Uuid::new_v4()).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::ForensicBundleNotFound(_)
    ));
}

#[tokio::test]
async fn test_list_by_tenant() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    for _ in 0..3 {
        let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
        repo.create(bundle).await.unwrap();
    }

    let result = repo.list_by_tenant(tenant_id, None).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_by_tenant_with_limit() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    for _ in 0..5 {
        let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
        repo.create(bundle).await.unwrap();
    }

    let result = repo.list_by_tenant(tenant_id, Some(2)).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_by_tenant_and_status() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    let bundle1 = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    repo.create(bundle1).await.unwrap();

    let bundle2 = create_test_bundle(tenant_id, BundlePurpose::Legal);
    let bundle2_id = bundle2.bundle_id;
    repo.create(bundle2).await.unwrap();

    // Update one to Generating
    repo.update_status(bundle2_id, BundleStatus::Generating)
        .await
        .unwrap();

    let result = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Pending)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);

    let result = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Generating)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_by_tenant_and_purpose() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    let _bundle1 = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    repo.create(_bundle1).await.unwrap();

    let _bundle2 = create_test_bundle(tenant_id, BundlePurpose::ComplianceAudit);
    repo.create(_bundle2).await.unwrap();

    let _bundle3 = create_test_bundle(tenant_id, BundlePurpose::ComplianceAudit);
    repo.create(_bundle3).await.unwrap();

    let result = repo
        .list_by_tenant_and_purpose(tenant_id, BundlePurpose::ComplianceAudit)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}

#[tokio::test]
async fn test_update_status_pending_to_generating() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();

    let result = repo
        .update_status(bundle_id, BundleStatus::Generating)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, BundleStatus::Generating);
}

#[tokio::test]
async fn test_update_status_generating_to_ready() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();
    repo.update_status(bundle_id, BundleStatus::Generating)
        .await
        .unwrap();

    let result = repo.update_status(bundle_id, BundleStatus::Ready).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, BundleStatus::Ready);
}

#[tokio::test]
async fn test_update_status_generating_to_failed() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();
    repo.update_status(bundle_id, BundleStatus::Generating)
        .await
        .unwrap();

    let result = repo.update_status(bundle_id, BundleStatus::Failed).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, BundleStatus::Failed);
}

#[tokio::test]
async fn test_update_status_invalid_transition_ready_to_generating() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();
    repo.update_status(bundle_id, BundleStatus::Generating)
        .await
        .unwrap();
    repo.update_status(bundle_id, BundleStatus::Ready)
        .await
        .unwrap();

    // Try invalid transition: Ready -> Generating
    let result = repo
        .update_status(bundle_id, BundleStatus::Generating)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidForensicBundleStatusTransition { .. }
    ));
}

#[tokio::test]
async fn test_update_status_invalid_transition_failed_to_ready() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();
    repo.update_status(bundle_id, BundleStatus::Generating)
        .await
        .unwrap();
    repo.update_status(bundle_id, BundleStatus::Failed)
        .await
        .unwrap();

    // Try invalid transition: Failed -> Ready
    let result = repo.update_status(bundle_id, BundleStatus::Ready).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::InvalidForensicBundleStatusTransition { .. }
    ));
}

#[tokio::test]
async fn test_update_status_pending_to_failed() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();

    // Pending can go directly to Failed (e.g., invalid request params)
    let result = repo.update_status(bundle_id, BundleStatus::Failed).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().status, BundleStatus::Failed);
}

#[tokio::test]
async fn test_update_status_not_found() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let result = repo
        .update_status(Uuid::new_v4(), BundleStatus::Ready)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        IntentRebaseError::ForensicBundleNotFound(_)
    ));
}

#[tokio::test]
async fn test_update_contents() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();

    let new_contents = BundleContents {
        intent_versions: 10,
        artifacts: 25,
        approvals: 5,
        audit_events: 5000,
        policy_snapshots: 3,
    };

    let result = repo.update_contents(bundle_id, new_contents.clone()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().contents.intent_versions, 10);
}

#[tokio::test]
async fn test_list_terminal_bundles() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    // Create bundles in different states
    let bundle1 = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    repo.create(bundle1).await.unwrap();

    let bundle2 = create_test_bundle(tenant_id, BundlePurpose::Legal);
    let bundle2_id = bundle2.bundle_id;
    repo.create(bundle2).await.unwrap();
    repo.update_status(bundle2_id, BundleStatus::Generating)
        .await
        .unwrap();
    repo.update_status(bundle2_id, BundleStatus::Ready)
        .await
        .unwrap();

    let bundle3 = create_test_bundle(tenant_id, BundlePurpose::ComplianceAudit);
    let bundle3_id = bundle3.bundle_id;
    repo.create(bundle3).await.unwrap();
    repo.update_status(bundle3_id, BundleStatus::Generating)
        .await
        .unwrap();
    repo.update_status(bundle3_id, BundleStatus::Failed)
        .await
        .unwrap();

    // Only Ready and Failed should be terminal
    let result = repo.list_terminal_bundles(tenant_id, None).await;
    assert!(result.is_ok());
    let bundles = result.unwrap();
    assert_eq!(bundles.len(), 2);
    assert!(bundles.iter().all(|b| b.status.is_terminal()));
}

#[tokio::test]
async fn test_list_terminal_bundles_with_limit() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();

    // Create 3 terminal bundles
    for _ in 0..3 {
        let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
        let bundle_id = bundle.bundle_id;
        repo.create(bundle).await.unwrap();
        repo.update_status(bundle_id, BundleStatus::Generating)
            .await
            .unwrap();
        repo.update_status(bundle_id, BundleStatus::Ready)
            .await
            .unwrap();
    }

    let result = repo.list_terminal_bundles(tenant_id, Some(2)).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 2);
}

// =============================================================================
// BundleStatus tests
// =============================================================================

#[test]
fn test_bundle_status_is_terminal() {
    assert!(!BundleStatus::Pending.is_terminal());
    assert!(!BundleStatus::Generating.is_terminal());
    assert!(BundleStatus::Ready.is_terminal());
    assert!(BundleStatus::Failed.is_terminal());
}

#[test]
fn test_bundle_status_can_transition_pending() {
    // Pending -> Generating: valid
    assert!(BundleStatus::Pending.can_transition_to(BundleStatus::Generating));
    // Pending -> Failed: valid (e.g., invalid request)
    assert!(BundleStatus::Pending.can_transition_to(BundleStatus::Failed));
    // Pending -> Ready: invalid (must go through Generating)
    assert!(!BundleStatus::Pending.can_transition_to(BundleStatus::Ready));
    // Pending -> Pending: valid (no-op)
    assert!(BundleStatus::Pending.can_transition_to(BundleStatus::Pending));
}

#[test]
fn test_bundle_status_can_transition_generating() {
    // Generating -> Ready: valid
    assert!(BundleStatus::Generating.can_transition_to(BundleStatus::Ready));
    // Generating -> Failed: valid
    assert!(BundleStatus::Generating.can_transition_to(BundleStatus::Failed));
    // Generating -> Pending: invalid
    assert!(!BundleStatus::Generating.can_transition_to(BundleStatus::Pending));
    // Generating -> Generating: valid (no-op)
    assert!(BundleStatus::Generating.can_transition_to(BundleStatus::Generating));
}

#[test]
fn test_bundle_status_can_transition_terminal() {
    // Ready is terminal - no transitions allowed
    assert!(!BundleStatus::Ready.can_transition_to(BundleStatus::Pending));
    assert!(!BundleStatus::Ready.can_transition_to(BundleStatus::Generating));
    assert!(!BundleStatus::Ready.can_transition_to(BundleStatus::Failed));
    assert!(BundleStatus::Ready.can_transition_to(BundleStatus::Ready)); // no-op

    // Failed is terminal - no transitions allowed
    assert!(!BundleStatus::Failed.can_transition_to(BundleStatus::Pending));
    assert!(!BundleStatus::Failed.can_transition_to(BundleStatus::Generating));
    assert!(!BundleStatus::Failed.can_transition_to(BundleStatus::Ready));
    assert!(BundleStatus::Failed.can_transition_to(BundleStatus::Failed)); // no-op
}

// =============================================================================
// Cross-bundle status isolation tests
// =============================================================================

#[tokio::test]
async fn test_tenant_isolation_bundles() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant1 = Uuid::new_v4();
    let tenant2 = Uuid::new_v4();

    let bundle1 = create_test_bundle(tenant1, BundlePurpose::IncidentInvestigation);
    let bundle1_id = bundle1.bundle_id;
    repo.create(bundle1).await.unwrap();

    let bundle2 = create_test_bundle(tenant2, BundlePurpose::IncidentInvestigation);
    let bundle2_id = bundle2.bundle_id;
    repo.create(bundle2).await.unwrap();

    // Each tenant should only see their own bundles
    let result1 = repo.list_by_tenant(tenant1, None).await.unwrap();
    let result2 = repo.list_by_tenant(tenant2, None).await.unwrap();

    assert_eq!(result1.len(), 1);
    assert_eq!(result2.len(), 1);
    assert_eq!(result1[0].bundle_id, bundle1_id);
    assert_eq!(result2[0].bundle_id, bundle2_id);
}

#[tokio::test]
async fn test_status_index_maintained_across_transitions() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let tenant_id = Uuid::new_v4();
    let bundle = create_test_bundle(tenant_id, BundlePurpose::IncidentInvestigation);
    let bundle_id = bundle.bundle_id;

    repo.create(bundle).await.unwrap();

    // Initially in Pending
    let pending = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Pending)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    // Transition to Generating
    repo.update_status(bundle_id, BundleStatus::Generating)
        .await
        .unwrap();

    let pending = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Pending)
        .await
        .unwrap();
    let generating = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Generating)
        .await
        .unwrap();
    assert_eq!(pending.len(), 0);
    assert_eq!(generating.len(), 1);

    // Transition to Ready
    repo.update_status(bundle_id, BundleStatus::Ready)
        .await
        .unwrap();

    let pending = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Pending)
        .await
        .unwrap();
    let generating = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Generating)
        .await
        .unwrap();
    let ready = repo
        .list_by_tenant_and_status(tenant_id, BundleStatus::Ready)
        .await
        .unwrap();
    assert_eq!(pending.len(), 0);
    assert_eq!(generating.len(), 0);
    assert_eq!(ready.len(), 1);
}
