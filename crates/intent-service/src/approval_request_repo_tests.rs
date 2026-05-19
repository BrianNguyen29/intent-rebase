#[cfg(test)]
mod tests {
    use crate::approval_request_repo::*;
    use intent_rebase_types::IntentRebaseError;
    use std::sync::Arc;
    use uuid::Uuid;

    fn create_test_request() -> ApprovalRequest {
        ApprovalRequest::new_pending(
            Uuid::new_v4(),
            1,
            2,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "external-api/unknown",
            "external-api",
            "D",
            "High severity change requires manual review",
        )
    }

    #[tokio::test]
    async fn test_create_approval_request() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;

        let result = repo.create_approval_request(request).await;
        assert!(result.is_ok());

        // Verify stored
        let stored = repo.get_approval_request(id).await.unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.status, ApprovalRequestStatus::Pending);
    }

    #[tokio::test]
    async fn test_list_pending_by_intent() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create multiple pending requests for same intent
        for _ in 0..3 {
            let request = ApprovalRequest::new_pending(
                intent_id,
                1,
                2,
                workflow_id,
                tenant_id,
                "external-api/unknown",
                "external-api",
                "D",
                "Blocked",
            );
            repo.create_approval_request(request).await.unwrap();
        }

        let pending = repo
            .list_pending_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn test_list_pending_by_tenant() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();

        // Create requests for tenant 1
        for _ in 0..2 {
            let request = ApprovalRequest::new_pending(
                Uuid::new_v4(),
                1,
                2,
                Uuid::new_v4(),
                tenant_1,
                "external-api/unknown",
                "external-api",
                "E",
                "Critical",
            );
            repo.create_approval_request(request).await.unwrap();
        }

        // Create request for tenant 2
        let request = ApprovalRequest::new_pending(
            Uuid::new_v4(),
            1,
            2,
            Uuid::new_v4(),
            tenant_2,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(request).await.unwrap();

        let pending_1 = repo.list_pending_by_tenant(tenant_1).await.unwrap();
        assert_eq!(pending_1.len(), 2);

        let pending_2 = repo.list_pending_by_tenant(tenant_2).await.unwrap();
        assert_eq!(pending_2.len(), 1);
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let id = Uuid::new_v4();
        let result = repo.get_approval_request(id).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotFound(found_id) if found_id == id
        ));
    }

    #[tokio::test]
    async fn test_update_status_not_pending_approved() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        // First approve it
        repo.update_approval_request_status(id, ApprovalRequestStatus::Approved, "test", None)
            .await
            .unwrap();

        // Now try to approve again - should fail with 409
        let result = repo
            .update_approval_request_status(id, ApprovalRequestStatus::Approved, "test", None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(found_id, status) if found_id == id && status == "Approved"
        ));
    }

    #[tokio::test]
    async fn test_update_status_not_pending_rejected() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        // First reject it
        repo.update_approval_request_status(id, ApprovalRequestStatus::Rejected, "test", None)
            .await
            .unwrap();

        // Now try to approve - should fail with 409
        let result = repo
            .update_approval_request_status(id, ApprovalRequestStatus::Approved, "test", None)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(found_id, status) if found_id == id && status == "Rejected"
        ));
    }

    #[tokio::test]
    async fn test_cancel_pending_by_intent_cancels_pending_requests() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create multiple pending requests for same intent
        for i in 0..3 {
            let request = ApprovalRequest::new_pending(
                intent_id,
                i + 1,
                i + 2,
                workflow_id,
                tenant_id,
                "external-api/unknown",
                "external-api",
                "D",
                "Blocked",
            );
            repo.create_approval_request(request).await.unwrap();
        }

        // Verify 3 pending requests exist
        let pending_before = repo
            .list_pending_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(pending_before.len(), 3);

        // Cancel all pending requests
        let count = repo
            .cancel_pending_by_intent(intent_id, tenant_id, "system", "New version created")
            .await
            .unwrap();
        assert_eq!(count, 3);

        // Verify no pending requests remain
        let pending_after = repo
            .list_pending_by_intent(intent_id, tenant_id)
            .await
            .unwrap();
        assert_eq!(pending_after.len(), 0);
    }

    #[tokio::test]
    async fn test_cancel_pending_by_intent_respects_tenant_isolation() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create pending request for tenant 1
        let request1 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_1,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(request1).await.unwrap();

        // Create pending request for tenant 2
        let request2 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_2,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(request2).await.unwrap();

        // Cancel for tenant 1 only
        let count = repo
            .cancel_pending_by_intent(intent_id, tenant_1, "system", "New version created")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Tenant 1 should have no pending, tenant 2 should still have 1
        let pending_1 = repo
            .list_pending_by_intent(intent_id, tenant_1)
            .await
            .unwrap();
        assert_eq!(pending_1.len(), 0);

        let pending_2 = repo
            .list_pending_by_intent(intent_id, tenant_2)
            .await
            .unwrap();
        assert_eq!(pending_2.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_pending_by_intent_returns_zero_when_none_pending() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // No requests exist - cancellation should return 0
        let count = repo
            .cancel_pending_by_intent(intent_id, tenant_id, "system", "New version created")
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_cancel_pending_by_intent_only_cancels_pending_status() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create a pending request
        let pending_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let pending_id = pending_request.id;
        repo.create_approval_request(pending_request).await.unwrap();

        // Create and then approve another request
        let approved_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let approved_id = approved_request.id;
        repo.create_approval_request(approved_request)
            .await
            .unwrap();
        repo.update_approval_request_status(
            approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Cancel pending - should only cancel the pending one
        let count = repo
            .cancel_pending_by_intent(intent_id, tenant_id, "system", "New version created")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Pending request should now be cancelled
        let pending = repo.get_approval_request(pending_id).await.unwrap();
        assert_eq!(pending.status, ApprovalRequestStatus::Cancelled);

        // Approved request should still be approved (not affected)
        let approved = repo.get_approval_request(approved_id).await.unwrap();
        assert_eq!(approved.status, ApprovalRequestStatus::Approved);
    }

    #[tokio::test]
    async fn test_cancel_pending_by_intent_sets_resolution_fields() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let request_id = request.id;
        repo.create_approval_request(request).await.unwrap();

        // Cancel with specific reason
        repo.cancel_pending_by_intent(
            intent_id,
            tenant_id,
            "system",
            "Intent version changed to v3",
        )
        .await
        .unwrap();

        // Verify resolution fields are set
        let cancelled = repo.get_approval_request(request_id).await.unwrap();
        assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);
        assert_eq!(cancelled.resolved_by, Some("system".to_string()));
        assert_eq!(
            cancelled.resolution_notes,
            Some("Intent version changed to v3".to_string())
        );
        assert!(cancelled.resolved_at.is_some());
    }

    #[tokio::test]
    async fn test_mark_expired_pending_request() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        // Expire it
        let expired = repo
            .mark_expired(id, "system", "Approval time limit exceeded")
            .await
            .unwrap();

        assert_eq!(expired.status, ApprovalRequestStatus::Expired);
        assert_eq!(expired.resolved_by, Some("system".to_string()));
        assert_eq!(
            expired.resolution_notes,
            Some("Approval time limit exceeded".to_string())
        );
        assert!(expired.resolved_at.is_some());
    }

    #[tokio::test]
    async fn test_mark_expired_non_pending_request_fails() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        // First approve it
        repo.update_approval_request_status(id, ApprovalRequestStatus::Approved, "test", None)
            .await
            .unwrap();

        // Now try to expire it - should fail
        let result = repo.mark_expired(id, "system", "Too late").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(found_id, status)
            if found_id == id && status == "Approved"
        ));
    }

    #[tokio::test]
    async fn test_mark_expired_not_found() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let id = Uuid::new_v4();

        let result = repo.mark_expired(id, "system", "Never existed").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotFound(found_id) if found_id == id
        ));
    }

    #[tokio::test]
    async fn test_mark_expired_approved_request_fails() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        repo.update_approval_request_status(id, ApprovalRequestStatus::Approved, "test", None)
            .await
            .unwrap();

        let result = repo.mark_expired(id, "system", "Already approved").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(..)
        ));
    }

    #[tokio::test]
    async fn test_mark_expired_rejected_request_fails() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        repo.update_approval_request_status(id, ApprovalRequestStatus::Rejected, "test", None)
            .await
            .unwrap();

        let result = repo.mark_expired(id, "system", "Already rejected").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(..)
        ));
    }

    #[tokio::test]
    async fn test_mark_expired_cancelled_request_fails() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());
        let request = create_test_request();
        let id = request.id;
        repo.create_approval_request(request).await.unwrap();

        repo.update_approval_request_status(id, ApprovalRequestStatus::Cancelled, "test", None)
            .await
            .unwrap();

        let result = repo.mark_expired(id, "system", "Already cancelled").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IntentRebaseError::ApprovalRequestNotPending(..)
        ));
    }

    #[tokio::test]
    async fn test_list_by_intent_returns_all_statuses() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create requests with different statuses
        // Create pending request
        let pending_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(pending_request).await.unwrap();

        // Create and approve another request
        let approved_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let approved_id = approved_request.id;
        repo.create_approval_request(approved_request)
            .await
            .unwrap();
        repo.update_approval_request_status(
            approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Create and reject another request
        let rejected_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let rejected_id = rejected_request.id;
        repo.create_approval_request(rejected_request)
            .await
            .unwrap();
        repo.update_approval_request_status(
            rejected_id,
            ApprovalRequestStatus::Rejected,
            "approver",
            None,
        )
        .await
        .unwrap();

        // List all approvals - should return all 3 regardless of status
        let all = repo.list_by_intent(intent_id, tenant_id).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_list_by_intent_respects_tenant_isolation() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create request for tenant 1
        let request1 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_1,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(request1).await.unwrap();

        // Create request for tenant 2
        let request2 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_2,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(request2).await.unwrap();

        // Tenant 1 should only see their request
        let tenant_1_approvals = repo.list_by_intent(intent_id, tenant_1).await.unwrap();
        assert_eq!(tenant_1_approvals.len(), 1);

        // Tenant 2 should only see their request
        let tenant_2_approvals = repo.list_by_intent(intent_id, tenant_2).await.unwrap();
        assert_eq!(tenant_2_approvals.len(), 1);
    }

    #[tokio::test]
    async fn test_cancel_approved_by_intent_cancels_only_approved() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create pending request
        let pending_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let pending_id = pending_request.id;
        repo.create_approval_request(pending_request).await.unwrap();

        // Create and approve another request
        let approved_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let approved_id = approved_request.id;
        repo.create_approval_request(approved_request)
            .await
            .unwrap();
        repo.update_approval_request_status(
            approved_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Create and reject another request
        let rejected_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let rejected_id = rejected_request.id;
        repo.create_approval_request(rejected_request)
            .await
            .unwrap();
        repo.update_approval_request_status(
            rejected_id,
            ApprovalRequestStatus::Rejected,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Cancel approved approvals
        let count = repo
            .cancel_approved_by_intent(intent_id, tenant_id, "system", "Scope changed")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Pending should still be pending
        let pending = repo.get_approval_request(pending_id).await.unwrap();
        assert_eq!(pending.status, ApprovalRequestStatus::Pending);

        // Approved should now be cancelled
        let approved = repo.get_approval_request(approved_id).await.unwrap();
        assert_eq!(approved.status, ApprovalRequestStatus::Cancelled);

        // Rejected should still be rejected
        let rejected = repo.get_approval_request(rejected_id).await.unwrap();
        assert_eq!(rejected.status, ApprovalRequestStatus::Rejected);
    }

    #[tokio::test]
    async fn test_cancel_approved_by_intent_returns_zero_when_none_approved() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create only a pending request
        let pending_request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        repo.create_approval_request(pending_request).await.unwrap();

        // Cancel approved - should return 0
        let count = repo
            .cancel_approved_by_intent(intent_id, tenant_id, "system", "Scope changed")
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_cancel_approved_by_intent_sets_resolution_fields() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        let request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let request_id = request.id;
        repo.create_approval_request(request).await.unwrap();

        repo.update_approval_request_status(
            request_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Cancel with specific reason
        repo.cancel_approved_by_intent(
            intent_id,
            tenant_id,
            "external-api/trigger-reapproval",
            "Superseded by new approval request due to scope change",
        )
        .await
        .unwrap();

        // Verify resolution fields are set
        let cancelled = repo.get_approval_request(request_id).await.unwrap();
        assert_eq!(cancelled.status, ApprovalRequestStatus::Cancelled);
        assert_eq!(
            cancelled.resolved_by,
            Some("external-api/trigger-reapproval".to_string())
        );
        assert_eq!(
            cancelled.resolution_notes,
            Some("Superseded by new approval request due to scope change".to_string())
        );
        assert!(cancelled.resolved_at.is_some());
    }

    #[tokio::test]
    async fn test_cancel_approved_by_intent_respects_tenant_isolation() {
        let repo = Arc::new(InMemoryApprovalRequestRepository::new());

        let intent_id = Uuid::new_v4();
        let tenant_1 = Uuid::new_v4();
        let tenant_2 = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create and approve request for tenant 1
        let request1 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_1,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let request1_id = request1.id;
        repo.create_approval_request(request1).await.unwrap();
        repo.update_approval_request_status(
            request1_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Create and approve request for tenant 2
        let request2 = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_2,
            "external-api/unknown",
            "external-api",
            "D",
            "Blocked",
        );
        let request2_id = request2.id;
        repo.create_approval_request(request2).await.unwrap();
        repo.update_approval_request_status(
            request2_id,
            ApprovalRequestStatus::Approved,
            "approver",
            None,
        )
        .await
        .unwrap();

        // Cancel for tenant 1 only
        let count = repo
            .cancel_approved_by_intent(intent_id, tenant_1, "system", "Scope changed")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Tenant 1's request should be cancelled
        let cancelled1 = repo.get_approval_request(request1_id).await.unwrap();
        assert_eq!(cancelled1.status, ApprovalRequestStatus::Cancelled);

        // Tenant 2's request should still be approved
        let still_approved = repo.get_approval_request(request2_id).await.unwrap();
        assert_eq!(still_approved.status, ApprovalRequestStatus::Approved);
    }
}

// =============================================================================
// SqlxApprovalRequestRepository unit tests (helper function tests)
// These test the enum conversion logic without requiring a database connection.
// =============================================================================

#[cfg(test)]
mod sqlx_approval_request_tests {
    use crate::approval_request_repo::*;

    #[test]
    fn test_approval_request_status_to_string() {
        assert_eq!(
            approval_request_status_to_string(&ApprovalRequestStatus::Pending),
            "pending"
        );
        assert_eq!(
            approval_request_status_to_string(&ApprovalRequestStatus::Approved),
            "approved"
        );
        assert_eq!(
            approval_request_status_to_string(&ApprovalRequestStatus::Rejected),
            "rejected"
        );
        assert_eq!(
            approval_request_status_to_string(&ApprovalRequestStatus::Expired),
            "expired"
        );
        assert_eq!(
            approval_request_status_to_string(&ApprovalRequestStatus::Cancelled),
            "cancelled"
        );
    }

    #[test]
    fn test_approval_request_status_from_string() {
        assert_eq!(
            approval_request_status_from_string("pending"),
            ApprovalRequestStatus::Pending
        );
        assert_eq!(
            approval_request_status_from_string("approved"),
            ApprovalRequestStatus::Approved
        );
        assert_eq!(
            approval_request_status_from_string("rejected"),
            ApprovalRequestStatus::Rejected
        );
        assert_eq!(
            approval_request_status_from_string("expired"),
            ApprovalRequestStatus::Expired
        );
        assert_eq!(
            approval_request_status_from_string("cancelled"),
            ApprovalRequestStatus::Cancelled
        );
        // Unknown values default to Pending
        assert_eq!(
            approval_request_status_from_string("unknown"),
            ApprovalRequestStatus::Pending
        );
    }
}
