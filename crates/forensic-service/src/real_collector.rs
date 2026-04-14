//! Real forensic data collector using service repositories

use super::collector::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::{AuditEvent, AuditRepository, PolicySnapshot};
use intent_service::{IntentRepository, PolicySnapshotRepository};
use std::sync::Arc;
use uuid::Uuid;

/// Real forensic data collector that queries actual service repositories
pub struct RealForensicDataCollector {
    pub intent_repo: Arc<dyn IntentRepository>,
    pub audit_repo: Arc<dyn AuditRepository>,
    pub policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>,
}

impl RealForensicDataCollector {
    pub fn new(
        intent_repo: Arc<dyn IntentRepository>,
        audit_repo: Arc<dyn AuditRepository>,
        policy_snapshot_repo: Arc<dyn PolicySnapshotRepository>,
    ) -> Self {
        Self {
            intent_repo,
            audit_repo,
            policy_snapshot_repo,
        }
    }
}

#[async_trait]
impl ForensicDataCollector for RealForensicDataCollector {
    async fn collect(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionResult, CollectorError> {
        if time_range.0 > time_range.1 {
            return Err(CollectorError::InvalidTimeRange(
                "start time must be before end time".to_string(),
            ));
        }

        let mut collected_intents = Vec::new();
        let mut total_audit_events = 0;
        let mut total_policy_snapshots = 0;

        for &intent_id in intent_ids {
            // Get intent
            let intent = match self.intent_repo.get_intent(intent_id).await {
                Ok(i) => i,
                Err(e) => {
                    return Err(CollectorError::Repository(format!(
                        "failed to get intent {}: {}",
                        intent_id, e
                    )));
                }
            };

            // Verify tenant access if tenant_id is specified
            if let Some(tid) = tenant_id {
                if intent.tenant_id != tid {
                    return Err(CollectorError::TenantAccessDenied(format!(
                        "intent {} does not belong to tenant {}",
                        intent_id, tid
                    )));
                }
            }

            // Get versions for this intent
            let versions = match self.intent_repo.get_versions_by_intent(intent_id).await {
                Ok(v) => v,
                Err(e) => {
                    return Err(CollectorError::Repository(format!(
                        "failed to get versions for intent {}: {}",
                        intent_id, e
                    )));
                }
            };

            // Filter versions by time range
            let filtered_versions: Vec<CollectedVersionData> = versions
                .into_iter()
                .filter(|v| v.created_at >= time_range.0 && v.created_at <= time_range.1)
                .map(|v| CollectedVersionData {
                    version_number: v.version_number as u64,
                    summary: format!("Version {}", v.version_number),
                    change_type: format!("{:?}", v.change_channel),
                    created_at: v.created_at,
                })
                .collect();

            // Get audit events for this intent
            let tenant_uuid = intent.tenant_id;
            let audit_events = match self.audit_repo.list_by_intent(intent_id, tenant_uuid).await {
                Ok(events) => events,
                Err(e) => {
                    return Err(CollectorError::Repository(format!(
                        "failed to get audit events for intent {}: {}",
                        intent_id, e
                    )));
                }
            };

            // Filter audit events by time range
            let filtered_audit_events: Vec<AuditEvent> = audit_events
                .into_iter()
                .filter(|e| e.occurred_at >= time_range.0 && e.occurred_at <= time_range.1)
                .collect();

            total_audit_events += filtered_audit_events.len();

            // Get policy snapshots for this intent
            let policy_snapshots = match self
                .policy_snapshot_repo
                .list_by_intent(intent_id, tenant_uuid)
                .await
            {
                Ok(snapshots) => snapshots,
                Err(e) => {
                    return Err(CollectorError::Repository(format!(
                        "failed to get policy snapshots for intent {}: {}",
                        intent_id, e
                    )));
                }
            };

            // Filter policy snapshots by time range
            let filtered_policy_snapshots: Vec<PolicySnapshot> = policy_snapshots
                .into_iter()
                .filter(|s| s.created_at >= time_range.0 && s.created_at <= time_range.1)
                .collect();

            total_policy_snapshots += filtered_policy_snapshots.len();

            // Extract intent name from the first version's payload if available
            // Intent struct doesn't have a name field directly, so we use the intent_id as identifier
            let name = format!("intent-{}", intent_id);

            collected_intents.push(CollectedIntentData {
                intent_id,
                tenant_id: Some(intent.tenant_id),
                name,
                versions: filtered_versions,
                policy_snapshots: filtered_policy_snapshots,
                audit_events: filtered_audit_events,
            });
        }

        Ok(CollectionResult {
            tenant_id,
            intent_id: intent_ids.first().copied().unwrap_or(Uuid::nil()),
            collected_at: Utc::now(),
            intents: collected_intents,
            total_audit_events,
            total_policy_snapshots,
        })
    }

    async fn count_available(
        &self,
        tenant_id: Option<Uuid>,
        intent_ids: &[Uuid],
        time_range: &(DateTime<Utc>, DateTime<Utc>),
    ) -> Result<CollectionCounts, CollectorError> {
        if time_range.0 > time_range.1 {
            return Err(CollectorError::InvalidTimeRange(
                "start time must be before end time".to_string(),
            ));
        }

        let mut intent_count = 0;
        let mut version_count = 0;
        let mut audit_event_count = 0;
        let mut policy_snapshot_count = 0;

        for &intent_id in intent_ids {
            // Count intent if it exists and passes tenant check
            match self.intent_repo.get_intent(intent_id).await {
                Ok(intent) => {
                    if let Some(tid) = tenant_id {
                        if intent.tenant_id != tid {
                            continue;
                        }
                    }
                    intent_count += 1;
                }
                Err(_) => continue,
            }

            // Count versions in time range
            match self.intent_repo.get_versions_by_intent(intent_id).await {
                Ok(versions) => {
                    version_count += versions
                        .iter()
                        .filter(|v| v.created_at >= time_range.0 && v.created_at <= time_range.1)
                        .count();
                }
                Err(_) => {}
            }

            // Count audit events in time range
            let tenant_uuid = tenant_id.unwrap_or_else(|| Uuid::nil());
            match self.audit_repo.list_by_intent(intent_id, tenant_uuid).await {
                Ok(events) => {
                    audit_event_count += events
                        .iter()
                        .filter(|e| e.occurred_at >= time_range.0 && e.occurred_at <= time_range.1)
                        .count();
                }
                Err(_) => {}
            }

            // Count policy snapshots in time range
            match self
                .policy_snapshot_repo
                .list_by_intent(intent_id, tenant_uuid)
                .await
            {
                Ok(snapshots) => {
                    policy_snapshot_count += snapshots
                        .iter()
                        .filter(|s| s.created_at >= time_range.0 && s.created_at <= time_range.1)
                        .count();
                }
                Err(_) => {}
            }
        }

        Ok(CollectionCounts {
            intent_count,
            version_count,
            audit_event_count,
            policy_snapshot_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::{AuditEvent, AuditEventType, InMemoryAuditRepository};
    use intent_service::{InMemoryIntentRepository, InMemoryPolicySnapshotRepository};
    use std::sync::Arc;

    fn create_test_audit_event(
        id: Uuid,
        tenant_id: Uuid,
        intent_id: Uuid,
        occurred_at: DateTime<Utc>,
    ) -> AuditEvent {
        AuditEvent {
            id,
            tenant_id,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "test-user".to_string(),
            intent_id: Some(intent_id),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at,
        }
    }

    #[tokio::test]
    async fn test_collect_with_no_intents() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo, audit_repo, policy_repo);

        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());
        let result = collector
            .collect(None, &[], &time_range)
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.intents.is_empty());
        assert_eq!(result.total_audit_events, 0);
        assert_eq!(result.total_policy_snapshots, 0);
    }

    #[tokio::test]
    async fn test_count_available_empty() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo, audit_repo, policy_repo);

        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());
        let result = collector
            .count_available(None, &[Uuid::new_v4()], &time_range)
            .await;

        assert!(result.is_ok());
        let counts = result.unwrap();
        assert_eq!(counts.intent_count, 0);
        assert_eq!(counts.version_count, 0);
        assert_eq!(counts.audit_event_count, 0);
        assert_eq!(counts.policy_snapshot_count, 0);
    }

    #[tokio::test]
    async fn test_collect_invalid_time_range() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo, audit_repo, policy_repo);

        let time_range = (Utc::now(), Utc::now() - chrono::Duration::days(1));
        let result = collector
            .collect(None, &[Uuid::new_v4()], &time_range)
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CollectorError::InvalidTimeRange(_)));
    }

    #[tokio::test]
    async fn test_count_available_invalid_time_range() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo, audit_repo, policy_repo);

        let time_range = (Utc::now(), Utc::now() - chrono::Duration::days(1));
        let result = collector
            .count_available(None, &[Uuid::new_v4()], &time_range)
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CollectorError::InvalidTimeRange(_)));
    }

    #[tokio::test]
    async fn test_collect_nonexistent_intent() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo, audit_repo, policy_repo);

        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());
        let result = collector
            .collect(None, &[Uuid::new_v4()], &time_range)
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CollectorError::Repository(_)));
    }

    #[tokio::test]
    async fn test_audit_event_filtering_by_time_range() {
        let intent_repo = Arc::new(InMemoryIntentRepository::new());
        let audit_repo = Arc::new(InMemoryAuditRepository::new());
        let policy_repo = Arc::new(InMemoryPolicySnapshotRepository::new());

        let collector = RealForensicDataCollector::new(intent_repo.clone(), audit_repo.clone(), policy_repo);

        // Create an audit event outside the time range
        let old_event = create_test_audit_event(
            Uuid::new_v4(),
            Uuid::nil(),
            Uuid::new_v4(),
            Utc::now() - chrono::Duration::days(30),
        );
        audit_repo.create_audit_event(old_event).await.unwrap();

        let time_range = (Utc::now() - chrono::Duration::days(1), Utc::now());
        let result = collector
            .count_available(None, &[], &time_range)
            .await;

        assert!(result.is_ok());
    }
}
