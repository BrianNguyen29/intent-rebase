use crate::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

/// Mock collector that returns configurable data for testing.
struct MockCollector {
    intents: Vec<CollectedIntentData>,
}

impl MockCollector {
    fn new() -> Self {
        Self { intents: vec![] }
    }

    fn with_intents(mut self, intents: Vec<CollectedIntentData>) -> Self {
        self.intents = intents;
        self
    }
}

#[async_trait]
impl ForensicDataCollector for MockCollector {
    async fn collect(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionResult, CollectorError> {
        if time_range.0 > time_range.1 {
            return Err(CollectorError::InvalidTimeRange(
                "start must be before end".to_string(),
            ));
        }

        let filtered: Vec<CollectedIntentData> = self
            .intents
            .iter()
            .filter(|i| intent_ids.contains(&i.intent_id))
            .cloned()
            .collect();

        Ok(CollectionResult {
            tenant_id,
            intent_id: intent_ids.first().copied().unwrap_or(Uuid::nil()),
            collected_at: Utc::now(),
            intents: filtered,
            total_audit_events: 0,
            total_policy_snapshots: 0,
        })
    }

    async fn count_available(
        &self,
        _tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        _time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionCounts, CollectorError> {
        Ok(CollectionCounts {
            intent_count: intent_ids.len(),
            version_count: intent_ids.len(),
            audit_event_count: 0,
            policy_snapshot_count: 0,
        })
    }
}

fn create_test_intent(tenant_id: Uuid, intent_id: Uuid) -> CollectedIntentData {
    CollectedIntentData {
        intent_id,
        tenant_id: Some(tenant_id),
        name: format!("intent-{}", intent_id),
        versions: vec![CollectedVersionData {
            version_number: 1,
            summary: "Test version".to_string(),
            change_type: "create".to_string(),
            created_at: Utc::now(),
        }],
        policy_snapshots: vec![],
        audit_events: vec![],
    }
}

#[tokio::test]
async fn test_create_bundle_success() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let intent = create_test_intent(tenant_id, intent_id);

    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new().with_intents(vec![intent]));

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![intent_id],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "test-user".to_string(),
    };

    let result = service.create_bundle(request).await.unwrap();

    assert_eq!(result.bundle.tenant_id, tenant_id);
    assert_eq!(result.bundle.status, BundleStatus::Ready);
    assert_eq!(result.bundle.contents.intent_versions, 1);
    assert!(result.bundle_size_bytes > 0);
    assert!(result.storage_location.contains("test-bucket"));
}

#[tokio::test]
async fn test_create_bundle_invalid_time_range() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new());

    let service = ForensicBundleService::new(repo, storage, collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![],
        time_range: BundleTimeRange {
            start: Utc::now(),
            end: Utc::now() - chrono::Duration::days(1),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "test-user".to_string(),
    };

    let result = service.create_bundle(request).await;
    assert!(matches!(
        result,
        Err(ForensicBundleServiceError::InvalidTimeRange(_))
    ));
}

#[tokio::test]
async fn test_create_bundle_empty_intents() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new()); // Empty intents

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![Uuid::new_v4()], // Non-existent intent
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::ComplianceAudit,
        created_by: "test-user".to_string(),
    };

    // Should succeed with empty bundle (valid - no data found for time range)
    let result = service.create_bundle(request).await.unwrap();
    assert_eq!(result.bundle.contents.intent_versions, 0);
    assert_eq!(result.bundle.status, BundleStatus::Ready);
}

#[tokio::test]
async fn test_tenant_isolation_bundles() {
    let tenant1 = Uuid::new_v4();
    let tenant2 = Uuid::new_v4();
    let intent1 = Uuid::new_v4();
    let intent2 = Uuid::new_v4();

    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));

    let collector1 =
        Arc::new(MockCollector::new().with_intents(vec![create_test_intent(tenant1, intent1)]));
    let collector2 =
        Arc::new(MockCollector::new().with_intents(vec![create_test_intent(tenant2, intent2)]));

    let service1 = ForensicBundleService::new(repo.clone(), storage.clone(), collector1);
    let service2 = ForensicBundleService::new(repo.clone(), storage.clone(), collector2);

    let time_range = BundleTimeRange {
        start: Utc::now() - chrono::Duration::days(1),
        end: Utc::now(),
    };

    // Create bundle for tenant 1
    let req1 = CreateForensicBundleRequest {
        tenant_id: tenant1,
        intent_ids: vec![intent1],
        time_range: time_range.clone(),
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "user1".to_string(),
    };
    service1.create_bundle(req1).await.unwrap();

    // Create bundle for tenant 2
    let req2 = CreateForensicBundleRequest {
        tenant_id: tenant2,
        intent_ids: vec![intent2],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "user2".to_string(),
    };
    service2.create_bundle(req2).await.unwrap();

    // Tenant 1 should only see their bundle
    let tenant1_bundles = repo.list_by_tenant(tenant1, None).await.unwrap();
    assert_eq!(tenant1_bundles.len(), 1);
    assert_eq!(tenant1_bundles[0].tenant_id, tenant1);

    // Tenant 2 should only see their bundle
    let tenant2_bundles = repo.list_by_tenant(tenant2, None).await.unwrap();
    assert_eq!(tenant2_bundles.len(), 1);
    assert_eq!(tenant2_bundles[0].tenant_id, tenant2);
}

#[tokio::test]
async fn test_get_bundle() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(
        MockCollector::new().with_intents(vec![create_test_intent(tenant_id, Uuid::new_v4())]),
    );

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::Legal,
        created_by: "test-user".to_string(),
    };

    let created = service.create_bundle(request).await.unwrap();
    let retrieved = service.get_bundle(created.bundle.bundle_id).await.unwrap();

    assert_eq!(retrieved.bundle_id, created.bundle.bundle_id);
    assert_eq!(retrieved.tenant_id, tenant_id);
}

#[tokio::test]
async fn test_get_bundle_not_found() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new());

    let service = ForensicBundleService::new(repo, storage, collector);

    let result = service.get_bundle(Uuid::new_v4()).await;
    assert!(matches!(
        result,
        Err(ForensicBundleServiceError::NotFound(_))
    ));
}

#[tokio::test]
async fn test_download_bundle_bytes() {
    let tenant_id = Uuid::new_v4();
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(
        MockCollector::new().with_intents(vec![create_test_intent(tenant_id, Uuid::new_v4())]),
    );

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "test-user".to_string(),
    };

    let created = service.create_bundle(request).await.unwrap();
    let bytes = service
        .download_bundle_bytes(created.bundle.bundle_id)
        .await
        .unwrap();

    // Verify it's valid JSON
    let parsed: ForensicBundle = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed.bundle_id, created.bundle.bundle_id);
}

#[tokio::test]
async fn test_verify_bundle_replay_success() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let intent = create_test_intent(tenant_id, intent_id);

    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new().with_intents(vec![intent]));

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![intent_id],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "test-user".to_string(),
    };

    let created = service.create_bundle(request).await.unwrap();
    let bundle_id = created.bundle.bundle_id;

    // Build content sections from the same mock data
    let content_sections = crate::bundle_hasher::ContentSectionsForVerification {
        intent_versions: crate::bundle_hasher::IntentVersionsForHash {
            versions: vec![crate::bundle_hasher::IntentVersionEntry {
                intent_id,
                version: 1,
                content_hash: format!("{:032x}", 1),
            }],
        },
        artifacts: crate::bundle_hasher::ArtifactsForHash { artifacts: vec![] },
        approvals: crate::bundle_hasher::ApprovalsForHash { approvals: vec![] },
        audit_events: crate::bundle_hasher::AuditEventsForHash { events: vec![] },
        policy_snapshots: crate::bundle_hasher::PolicySnapshotsForHash { snapshots: vec![] },
    };

    let result = service
        .verify_bundle_replay(bundle_id, content_sections)
        .await;
    assert!(
        result.is_ok(),
        "Replay verification should succeed: {:?}",
        result
    );

    let response = result.unwrap();
    assert!(response.report.overall_verified);
}

#[tokio::test]
async fn test_verify_bundle_replay_tampered_content() {
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let intent = create_test_intent(tenant_id, intent_id);

    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new().with_intents(vec![intent]));

    let service = ForensicBundleService::new(repo.clone(), storage.clone(), collector);

    let request = CreateForensicBundleRequest {
        tenant_id,
        intent_ids: vec![intent_id],
        time_range: BundleTimeRange {
            start: Utc::now() - chrono::Duration::days(1),
            end: Utc::now(),
        },
        purpose: BundlePurpose::IncidentInvestigation,
        created_by: "test-user".to_string(),
    };

    let created = service.create_bundle(request).await.unwrap();
    let bundle_id = created.bundle.bundle_id;

    // Build tampered content sections
    let content_sections = crate::bundle_hasher::ContentSectionsForVerification {
        intent_versions: crate::bundle_hasher::IntentVersionsForHash {
            versions: vec![crate::bundle_hasher::IntentVersionEntry {
                intent_id,
                version: 1,
                content_hash: "tampered_content_hash_000000000000".to_string(),
            }],
        },
        artifacts: crate::bundle_hasher::ArtifactsForHash { artifacts: vec![] },
        approvals: crate::bundle_hasher::ApprovalsForHash { approvals: vec![] },
        audit_events: crate::bundle_hasher::AuditEventsForHash { events: vec![] },
        policy_snapshots: crate::bundle_hasher::PolicySnapshotsForHash { snapshots: vec![] },
    };

    let result = service
        .verify_bundle_replay(bundle_id, content_sections)
        .await;
    assert!(
        result.is_ok(),
        "Replay verification should complete: {:?}",
        result
    );

    let response = result.unwrap();
    assert!(!response.report.overall_verified);
    assert!(response.report.sections_failed > 0);
}

#[tokio::test]
async fn test_verify_bundle_replay_not_found() {
    let repo = Arc::new(InMemoryBundleRepository::new());
    let storage = Arc::new(InMemoryBundleStorage::new("test-bucket"));
    let collector = Arc::new(MockCollector::new());

    let service = ForensicBundleService::new(repo, storage, collector);

    let content_sections = crate::bundle_hasher::ContentSectionsForVerification::default();

    let result = service
        .verify_bundle_replay(Uuid::new_v4(), content_sections)
        .await;
    assert!(matches!(
        result,
        Err(ForensicBundleServiceError::NotFound(_))
    ));
}
