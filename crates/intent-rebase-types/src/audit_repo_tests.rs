#[cfg(test)]
mod tests {
    use crate::*;
    use chrono::Utc;
    use std::sync::Arc;
    use uuid::Uuid;

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
            .record_rebase_applied(
                tenant_id,
                "test-user",
                intent_id,
                payload,
                trace_ctx.clone(),
            )
            .await;

        assert!(result.is_ok());

        // Verify event was stored
        let events = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(matches!(event.event_type, AuditEventType::RebaseApplied));
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
            .record_rebase_apply_blocked(
                tenant_id,
                "test-user",
                intent_id,
                payload,
                TraceContext::default(),
            )
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

    #[tokio::test]
    async fn test_get_audit_event_cross_tenant_blocked() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates an audit event
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id: tenant_a,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "tenant-a-user".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({}),
            trace_id: None,
            span_id: None,
            occurred_at: Utc::now(),
        };
        let event_id = event.id;
        repo.create_audit_event(event).await.unwrap();

        // Tenant A can get their own event
        let result = repo.get_audit_event(event_id, tenant_a).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tenant_id, tenant_a);

        // Tenant B cannot get Tenant A's event (should get NotFound)
        let result = repo.get_audit_event(event_id, tenant_b).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ArtifactNotFound(found_id) if found_id == event_id
        ));
    }

    #[tokio::test]
    async fn test_list_audit_events_cross_tenant_isolation() {
        let repo = Arc::new(InMemoryAuditRepository::new());

        let tenant_a = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let tenant_b = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

        // Tenant A creates 3 audit events
        for _ in 0..3 {
            let event = AuditEvent {
                id: Uuid::new_v4(),
                tenant_id: tenant_a,
                event_type: AuditEventType::RebaseApplied,
                actor_id: "tenant-a-user".to_string(),
                intent_id: Some(Uuid::new_v4()),
                artifact_id: None,
                payload: serde_json::json!({}),
                trace_id: None,
                span_id: None,
                occurred_at: Utc::now(),
            };
            repo.create_audit_event(event).await.unwrap();
        }

        // Tenant B creates 2 audit events
        for _ in 0..2 {
            let event = AuditEvent {
                id: Uuid::new_v4(),
                tenant_id: tenant_b,
                event_type: AuditEventType::ApprovalGranted,
                actor_id: "tenant-b-user".to_string(),
                intent_id: Some(Uuid::new_v4()),
                artifact_id: None,
                payload: serde_json::json!({}),
                trace_id: None,
                span_id: None,
                occurred_at: Utc::now(),
            };
            repo.create_audit_event(event).await.unwrap();
        }

        // List for tenant A should return 3 events
        let events_a = repo.list_by_tenant(tenant_a, 100).await.unwrap();
        assert_eq!(events_a.len(), 3);
        assert!(events_a.iter().all(|e| e.tenant_id == tenant_a));

        // List for tenant B should return 2 events
        let events_b = repo.list_by_tenant(tenant_b, 100).await.unwrap();
        assert_eq!(events_b.len(), 2);
        assert!(events_b.iter().all(|e| e.tenant_id == tenant_b));
    }
}

// =============================================================================
// SqlxAuditRepository unit tests (helper function tests)
// These test the enum conversion logic without requiring a database connection.
// =============================================================================

#[cfg(test)]
mod sqlx_audit_tests {
    use crate::*;

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

// =============================================================================
// SQLx Audit Repository smoke tests
// These require a live Postgres database with migrations applied.
// Preconditions:
//   - Docker Postgres running (e.g. via docker compose -f infrastructure/local/docker-compose.yml up -d)
//   - Migrations applied
//   - DATABASE_URL environment variable set
// Run manually with:
//   DATABASE_URL=postgres://... cargo test -p intent-rebase-types --lib sqlx_smoke -- --ignored
// =============================================================================

#[cfg(test)]
mod sqlx_smoke_tests {
    use crate::*;
    use uuid::Uuid;

    /// Minimal smoke test for SqlxAuditRepository create+get round-trip.
    ///
    /// Uses AuditEventType::RebaseApplied because the DB enum currently lacks newer variants.
    #[tokio::test]
    #[ignore = "requires live Postgres (set DATABASE_URL to run)"]
    async fn test_sqlx_audit_repo_smoke() {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("Skipping SQLx smoke test: DATABASE_URL not set");
                return;
            }
        };

        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect failed");
        let repo = SqlxAuditRepository::new(pool);

        let tenant_id = Uuid::new_v4();
        let event = AuditEvent {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: AuditEventType::RebaseApplied,
            actor_id: "smoke-test".to_string(),
            intent_id: Some(Uuid::new_v4()),
            artifact_id: None,
            payload: serde_json::json!({"smoke": true}),
            trace_id: None,
            span_id: None,
            occurred_at: chrono::Utc::now(),
        };

        repo.create_audit_event(event.clone())
            .await
            .expect("create_audit_event failed");

        let fetched = repo
            .get_audit_event(event.id, tenant_id)
            .await
            .expect("get_audit_event failed");

        assert_eq!(fetched.id, event.id);
        assert_eq!(fetched.tenant_id, event.tenant_id);
        assert!(matches!(fetched.event_type, AuditEventType::RebaseApplied));
    }
}
