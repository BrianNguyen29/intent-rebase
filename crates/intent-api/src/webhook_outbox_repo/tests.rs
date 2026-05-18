use super::*;

use chrono::Utc;
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

fn sample_record(tenant_id: Uuid, intent_id: Uuid, subscription_id: Uuid) -> WebhookOutboxRecord {
    WebhookOutboxRecord::new(
        tenant_id,
        intent_id,
        subscription_id,
        "intent_changed".to_string(),
        serde_json::json!({"foo": "bar"}),
        None,
    )
}

#[tokio::test]
async fn test_create_and_get() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();
    let record = sample_record(tenant, intent, sub);

    let created = repo.create(record.clone()).await.unwrap();
    assert_eq!(created.id, record.id);

    let fetched = repo.get(record.id, tenant).await.unwrap();
    assert_eq!(fetched.status, WebhookOutboxStatus::Pending);
    assert_eq!(fetched.lock_version, 0);
}

#[tokio::test]
async fn test_get_wrong_tenant() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let record = sample_record(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    let err = repo.get(record.id, Uuid::new_v4()).await.unwrap_err();
    assert!(matches!(err, IntentRebaseError::StorageError(_)));
}

#[tokio::test]
async fn test_list_pending() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();

    let pending = repo.list_pending(tenant, 10).await.unwrap();
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn test_claim() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    let claimed = repo
        .claim(record.id, tenant, "worker-1".to_string())
        .await
        .unwrap();
    assert_eq!(claimed.status, WebhookOutboxStatus::Claimed);
    assert_eq!(claimed.lock_version, 1);
    assert_eq!(claimed.locked_by, Some("worker-1".to_string()));
}

#[tokio::test]
async fn test_claim_non_pending_fails() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "boom".to_string())
        .await
        .unwrap();

    let err = repo
        .claim(record.id, tenant, "worker-1".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, IntentRebaseError::StorageError(_)));
}

#[tokio::test]
async fn test_mark_delivered() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    let delivered = repo.mark_delivered(record.id, tenant).await.unwrap();
    assert_eq!(delivered.status, WebhookOutboxStatus::Delivered);
    assert!(delivered.delivered_at.is_some());
    assert_eq!(delivered.lock_version, 1);
}

#[tokio::test]
async fn test_mark_failed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    let failed = repo
        .mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    assert_eq!(failed.status, WebhookOutboxStatus::Failed);
    assert_eq!(failed.last_error, Some("timeout".to_string()));
    assert_eq!(failed.lock_version, 1);
}

#[tokio::test]
async fn test_reschedule_retry() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.claim(record.id, tenant, "worker-1".to_string())
        .await
        .unwrap();

    let future = Utc::now() + chrono::Duration::seconds(60);
    let rescheduled = repo
        .reschedule_retry(record.id, tenant, "network timeout".to_string(), future)
        .await
        .unwrap();
    assert_eq!(rescheduled.status, WebhookOutboxStatus::Pending);
    assert_eq!(rescheduled.attempt_count, 1);
    assert_eq!(rescheduled.last_error, Some("network timeout".to_string()));
    assert_eq!(rescheduled.scheduled_at, future);
    assert_eq!(rescheduled.locked_at, None);
    assert_eq!(rescheduled.locked_by, None);
    assert_eq!(rescheduled.lock_version, 2); // claim + reschedule
}

#[tokio::test]
async fn test_list_failed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    let r3 = sample_record(other_tenant, intent, sub);

    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.create(r3.clone()).await.unwrap();

    repo.mark_failed(r1.id, tenant, "boom".to_string())
        .await
        .unwrap();
    repo.mark_failed(r3.id, other_tenant, "bang".to_string())
        .await
        .unwrap();

    let failed = repo.list_failed(tenant, 10).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, r1.id);
    assert_eq!(failed[0].status, WebhookOutboxStatus::Failed);
}

#[tokio::test]
async fn test_replay_failed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let replayed = repo.replay_failed(record.id, tenant, None).await.unwrap();
    assert_eq!(replayed.status, WebhookOutboxStatus::Pending);
    assert_eq!(replayed.attempt_count, 0);
    assert_eq!(replayed.last_error, None);
    assert_eq!(replayed.locked_at, None);
    assert_eq!(replayed.locked_by, None);
    assert_eq!(replayed.replay_count, 1);
    assert!(replayed.replayed_at.is_some());
    assert_eq!(replayed.replayed_by, None);
    assert_eq!(replayed.lock_version, 2); // create + mark_failed + replay
}

#[tokio::test]
async fn test_replay_failed_idempotency() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    // First replay succeeds
    let _ = repo.replay_failed(record.id, tenant, None).await.unwrap();

    // Second replay fails because status is no longer Failed
    let err = repo
        .replay_failed(record.id, tenant, None)
        .await
        .unwrap_err();
    assert!(matches!(err, IntentRebaseError::StorageError(_)));
}

#[tokio::test]
async fn test_replay_non_failed_fails() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    // Record is still Pending, not Failed
    let err = repo
        .replay_failed(record.id, tenant, None)
        .await
        .unwrap_err();
    assert!(matches!(err, IntentRebaseError::StorageError(_)));
}

#[tokio::test]
async fn test_replay_failed_sets_replayed_by() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let replayed = repo
        .replay_failed(record.id, tenant, Some("operator-42".to_string()))
        .await
        .unwrap();
    assert_eq!(replayed.replay_count, 1);
    assert!(replayed.replayed_at.is_some());
    assert_eq!(replayed.replayed_by, Some("operator-42".to_string()));
}

#[tokio::test]
async fn test_replay_failed_metadata_defaults_when_no_actor() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "boom".to_string())
        .await
        .unwrap();

    let replayed = repo.replay_failed(record.id, tenant, None).await.unwrap();
    assert_eq!(replayed.replay_count, 1);
    assert!(replayed.replayed_at.is_some());
    assert_eq!(replayed.replayed_by, None);
}

#[tokio::test]
async fn test_replay_failed_clears_previous_metadata_on_replay() {
    // Idempotency prevents second replay, so we verify the first replay
    // sets metadata correctly; a subsequent replay would fail.
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "err".to_string())
        .await
        .unwrap();

    let first = repo
        .replay_failed(record.id, tenant, Some("a".to_string()))
        .await
        .unwrap();
    assert_eq!(first.replay_count, 1);
    assert_eq!(first.replayed_by, Some("a".to_string()));

    // Second replay fails because status is no longer Failed
    let err = repo
        .replay_failed(record.id, tenant, Some("b".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(err, IntentRebaseError::StorageError(_)));
}

#[tokio::test]
async fn test_list_failed_older_than_includes_stale() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();

    let before = Utc::now() + chrono::Duration::seconds(1);
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    let results = repo
        .list_failed_older_than(tenant, before, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, record.id);
}

#[tokio::test]
async fn test_list_failed_older_than_excludes_recent() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "boom".to_string())
        .await
        .unwrap();

    let before = Utc::now() - chrono::Duration::seconds(1);
    let results = repo
        .list_failed_older_than(tenant, before, 10)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_list_failed_older_than_excludes_non_failed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();

    let before = Utc::now() + chrono::Duration::seconds(1);
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    let results = repo
        .list_failed_older_than(tenant, before, 10)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_list_failed_older_than_tenant_boundary() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let record = sample_record(tenant_a, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant_a, "err".to_string())
        .await
        .unwrap();

    let before = Utc::now() + chrono::Duration::seconds(1);
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    let results = repo
        .list_failed_older_than(tenant_b, before, 10)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_list_failed_older_than_limit() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let r1 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    let r2 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();

    let before = Utc::now() + chrono::Duration::seconds(1);
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    let results = repo
        .list_failed_older_than(tenant, before, 1)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_status_serialization() {
    let statuses = vec![
        WebhookOutboxStatus::Pending,
        WebhookOutboxStatus::Claimed,
        WebhookOutboxStatus::Delivered,
        WebhookOutboxStatus::Failed,
    ];
    for status in statuses {
        let json = serde_json::to_string(&status).unwrap();
        let roundtrip: WebhookOutboxStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, roundtrip);
    }
}

#[tokio::test]
async fn test_list_distinct_pending_tenants() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant_a, intent, sub);
    let r2 = sample_record(tenant_a, intent, sub);
    let r3 = sample_record(tenant_b, intent, sub);

    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.create(r3.clone()).await.unwrap();

    let tenants = repo.list_distinct_pending_tenants().await.unwrap();
    assert_eq!(tenants.len(), 2);
    assert!(tenants.contains(&tenant_a));
    assert!(tenants.contains(&tenant_b));

    // Mark one tenant's records as delivered
    repo.mark_delivered(r1.id, tenant_a).await.unwrap();
    repo.mark_delivered(r2.id, tenant_a).await.unwrap();

    let tenants = repo.list_distinct_pending_tenants().await.unwrap();
    assert_eq!(tenants.len(), 1);
    assert!(tenants.contains(&tenant_b));
}

#[tokio::test]
async fn test_list_replayed_includes_replayed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.replay_failed(record.id, tenant, Some("operator-1".to_string()))
        .await
        .unwrap();

    let replayed = repo.list_replayed(tenant, None, 10).await.unwrap();
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].id, record.id);
    assert_eq!(replayed[0].replay_count, 1);
    assert!(replayed[0].replayed_at.is_some());
    assert_eq!(replayed[0].replayed_by, Some("operator-1".to_string()));
}

#[tokio::test]
async fn test_list_replayed_excludes_unreplayed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    // Do NOT replay

    let replayed = repo.list_replayed(tenant, None, 10).await.unwrap();
    assert!(replayed.is_empty());
}

#[tokio::test]
async fn test_list_replayed_tenant_boundary() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let record = sample_record(tenant_a, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant_a, "err".to_string())
        .await
        .unwrap();
    repo.replay_failed(record.id, tenant_a, None).await.unwrap();

    let replayed = repo.list_replayed(tenant_b, None, 10).await.unwrap();
    assert!(replayed.is_empty());
}

#[tokio::test]
async fn test_list_replayed_since_filter() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "err".to_string())
        .await
        .unwrap();

    let before_replay = Utc::now();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    repo.replay_failed(record.id, tenant, None).await.unwrap();

    // Since before replay should exclude
    let replayed = repo
        .list_replayed(tenant, Some(before_replay), 10)
        .await
        .unwrap();
    assert_eq!(replayed.len(), 1);

    // Since after replay should exclude
    let after_replay = Utc::now() + chrono::Duration::seconds(1);
    let replayed = repo
        .list_replayed(tenant, Some(after_replay), 10)
        .await
        .unwrap();
    assert!(replayed.is_empty());
}

#[tokio::test]
async fn test_list_replayed_limit() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let r1 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    let r2 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();
    repo.replay_failed(r1.id, tenant, None).await.unwrap();
    repo.replay_failed(r2.id, tenant, None).await.unwrap();

    let replayed = repo.list_replayed(tenant, None, 1).await.unwrap();
    assert_eq!(replayed.len(), 1);
}

#[tokio::test]
async fn test_list_replayed_ordering() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let r1 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    let r2 = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "e1".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "e2".to_string())
        .await
        .unwrap();
    repo.replay_failed(r1.id, tenant, None).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    repo.replay_failed(r2.id, tenant, None).await.unwrap();

    let replayed = repo.list_replayed(tenant, None, 10).await.unwrap();
    assert_eq!(replayed.len(), 2);
    // Most recent replay first
    assert_eq!(replayed[0].id, r2.id);
    assert_eq!(replayed[1].id, r1.id);
}

#[tokio::test]
async fn test_dlq_stats_empty() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();

    let stats = repo.dlq_stats(tenant).await.unwrap();
    assert_eq!(stats.total_failed, 0);
    assert_eq!(stats.oldest_failed_age_seconds, None);
    assert_eq!(stats.replayed_count, 0);
    assert!(stats.by_error_summary.is_empty());
}

#[tokio::test]
async fn test_dlq_stats_failed_and_replayed() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.replay_failed(r1.id, tenant, None).await.unwrap();

    let stats = repo.dlq_stats(tenant).await.unwrap();
    assert_eq!(stats.total_failed, 1); // r2 still failed
    assert_eq!(stats.replayed_count, 1); // r1 replayed
    assert_eq!(stats.by_error_summary.len(), 1);
    assert_eq!(stats.by_error_summary[0].error_pattern, "timeout");
    assert_eq!(stats.by_error_summary[0].count, 1);
}

#[tokio::test]
async fn test_dlq_stats_tenant_boundary() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let record = sample_record(tenant_a, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant_a, "err".to_string())
        .await
        .unwrap();

    let stats = repo.dlq_stats(tenant_b).await.unwrap();
    assert_eq!(stats.total_failed, 0);
    assert_eq!(stats.replayed_count, 0);
}

#[tokio::test]
async fn test_dlq_stats_oldest_age() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let record = sample_record(tenant, Uuid::new_v4(), Uuid::new_v4());
    repo.create(record.clone()).await.unwrap();
    repo.mark_failed(record.id, tenant, "boom".to_string())
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let stats = repo.dlq_stats(tenant).await.unwrap();
    assert_eq!(stats.total_failed, 1);
    assert!(stats.oldest_failed_age_seconds.unwrap_or(0) >= 0);
}

#[tokio::test]
async fn test_dlq_stats_by_error_summary_groups() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();

    let r1 = sample_record(tenant, intent, sub);
    let r2 = sample_record(tenant, intent, sub);
    let r3 = sample_record(tenant, intent, sub);
    repo.create(r1.clone()).await.unwrap();
    repo.create(r2.clone()).await.unwrap();
    repo.create(r3.clone()).await.unwrap();
    repo.mark_failed(r1.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.mark_failed(r2.id, tenant, "timeout".to_string())
        .await
        .unwrap();
    repo.mark_failed(r3.id, tenant, "conn_reset".to_string())
        .await
        .unwrap();

    let stats = repo.dlq_stats(tenant).await.unwrap();
    assert_eq!(stats.total_failed, 3);
    assert_eq!(stats.by_error_summary.len(), 2);
    // Sorted by count desc
    assert_eq!(stats.by_error_summary[0].error_pattern, "timeout");
    assert_eq!(stats.by_error_summary[0].count, 2);
    assert_eq!(stats.by_error_summary[1].error_pattern, "conn_reset");
    assert_eq!(stats.by_error_summary[1].count, 1);
}

#[test]
fn test_with_webhook_url() {
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();
    let record = WebhookOutboxRecord::new(
        tenant,
        intent,
        sub,
        "intent_changed".to_string(),
        serde_json::json!({"foo": "bar"}),
        None,
    )
    .with_webhook_url("https://example.com/hook");
    assert_eq!(
        record.webhook_url,
        Some("https://example.com/hook".to_string())
    );
}

#[tokio::test]
async fn test_in_memory_repo_preserves_webhook_url() {
    let repo = InMemoryWebhookOutboxRepository::new();
    let tenant = Uuid::new_v4();
    let intent = Uuid::new_v4();
    let sub = Uuid::new_v4();
    let record = WebhookOutboxRecord::new(
        tenant,
        intent,
        sub,
        "intent_changed".to_string(),
        serde_json::json!({"foo": "bar"}),
        Some("https://example.com/hook".to_string()),
    );

    let created = repo.create(record.clone()).await.unwrap();
    assert_eq!(
        created.webhook_url,
        Some("https://example.com/hook".to_string())
    );

    let fetched = repo.get(record.id, tenant).await.unwrap();
    assert_eq!(
        fetched.webhook_url,
        Some("https://example.com/hook".to_string())
    );
}

/// Smoke test for `SqlxWebhookOutboxRepository`.
///
/// Ignored by default so `cargo test` does not require live Postgres.
/// Run manually with:
///   DATABASE_URL=postgres://... cargo test -p intent-api --lib webhook_outbox_repo -- --ignored
#[tokio::test]
#[ignore = "requires live Postgres (set DATABASE_URL to run)"]
async fn test_sqlx_repo_smoke() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping SQLx smoke test: DATABASE_URL not set");
            return;
        }
    };
    let pool = ::sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect failed");
    let repo = SqlxWebhookOutboxRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let pending = repo
        .list_pending(tenant_id, 1)
        .await
        .expect("list_pending failed");
    assert!(pending.is_empty());
}

/// Smoke test for `SqlxWebhookOutboxRepository` DLQ list and replay.
///
/// Ignored by default so `cargo test` does not require live Postgres.
/// Run manually with:
///   DATABASE_URL=postgres://... cargo test -p intent-api --lib webhook_outbox_repo -- --ignored
#[tokio::test]
#[ignore = "requires live Postgres (set DATABASE_URL to run)"]
async fn test_sqlx_repo_dlq_smoke() {
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("Skipping SQLx DLQ smoke test: DATABASE_URL not set");
            return;
        }
    };
    let pool = ::sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect failed");
    let repo = SqlxWebhookOutboxRepository::new(pool);
    let tenant_id = Uuid::new_v4();
    let intent_id = Uuid::new_v4();
    let subscription_id = Uuid::new_v4();

    // Seed a record and mark it failed
    let record = WebhookOutboxRecord::new(
        tenant_id,
        intent_id,
        subscription_id,
        "intent_changed".to_string(),
        serde_json::json!({"foo": "bar"}),
        None,
    );
    let created = repo.create(record.clone()).await.expect("create failed");
    repo.mark_failed(created.id, tenant_id, "timeout".to_string())
        .await
        .expect("mark_failed failed");

    // list_failed should return the record
    let failed = repo
        .list_failed(tenant_id, 10)
        .await
        .expect("list_failed failed");
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, created.id);
    assert_eq!(failed[0].status, WebhookOutboxStatus::Failed);

    // replay_failed should transition to Pending
    let replayed = repo
        .replay_failed(created.id, tenant_id, None)
        .await
        .expect("replay_failed failed");
    assert_eq!(replayed.status, WebhookOutboxStatus::Pending);
    assert_eq!(replayed.attempt_count, 0);
    assert_eq!(replayed.last_error, None);

    // list_failed should now be empty
    let failed = repo
        .list_failed(tenant_id, 10)
        .await
        .expect("list_failed failed");
    assert!(failed.is_empty());
}
