//! Approval request repository for Phase 2b bounded external apply slice
//!
//! Provides storage for approval_requests table records created when external
//! POST /intents/{intent_id}/rebase-apply hits blocked D/E path.
//! Only pending status is in scope for Phase 2b.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use intent_rebase_types::IntentRebaseError;
use std::collections::HashMap;
#[allow(unused_imports)]
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Status of an approval request
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
}

/// An approval request record (created when external rebase-apply hits D/E blocked path)
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub intent_id: Uuid,
    pub intent_version_from: i32,
    pub intent_version_to: i32,
    pub workflow_id: Uuid,
    pub tenant_id: Uuid,
    pub requestor_id: String,
    pub requestor_type: String,
    pub decision_class: String,
    pub reason: String,
    pub metadata: serde_json::Value,
    pub status: ApprovalRequestStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub resolution_notes: Option<String>,
}

impl ApprovalRequest {
    /// Create a new pending approval request for blocked D/E external apply
    #[allow(clippy::too_many_arguments)]
    pub fn new_pending(
        intent_id: Uuid,
        intent_version_from: i32,
        intent_version_to: i32,
        workflow_id: Uuid,
        tenant_id: Uuid,
        requestor_id: &str,
        requestor_type: &str,
        decision_class: &str,
        reason: &str,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            intent_id,
            intent_version_from,
            intent_version_to,
            workflow_id,
            tenant_id,
            requestor_id: requestor_id.to_string(),
            requestor_type: requestor_type.to_string(),
            decision_class: decision_class.to_string(),
            reason: reason.to_string(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            status: ApprovalRequestStatus::Pending,
            created_at: now,
            updated_at: now,
            expires_at: None,
            resolved_at: None,
            resolved_by: None,
            resolution_notes: None,
        }
    }
}

/// Repository trait for approval request storage
#[async_trait]
pub trait ApprovalRequestRepository: Send + Sync {
    /// Create a new approval request (only pending status in Phase 2b scope)
    async fn create_approval_request(
        &self,
        request: ApprovalRequest,
    ) -> Result<ApprovalRequest, IntentRebaseError>;

    /// Get an approval request by ID
    async fn get_approval_request(&self, id: Uuid) -> Result<ApprovalRequest, IntentRebaseError>;

    /// List pending approval requests by intent
    async fn list_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError>;

    /// List pending approval requests by tenant
    async fn list_pending_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError>;

    /// Update the status of an approval request (Phase 2b: approve/reject)
    /// Returns the updated approval request.
    async fn update_approval_request_status(
        &self,
        id: Uuid,
        status: ApprovalRequestStatus,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<ApprovalRequest, IntentRebaseError>;
}

/// In-memory approval request repository for Phase 2b bounded slice testing
pub struct InMemoryApprovalRequestRepository {
    requests: RwLock<HashMap<Uuid, ApprovalRequest>>,
    by_intent: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    by_tenant: RwLock<HashMap<Uuid, Vec<Uuid>>>,
}

impl InMemoryApprovalRequestRepository {
    pub fn new() -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
            by_intent: RwLock::new(HashMap::new()),
            by_tenant: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryApprovalRequestRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApprovalRequestRepository for InMemoryApprovalRequestRepository {
    async fn create_approval_request(
        &self,
        request: ApprovalRequest,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let mut requests = self.requests.write().await;
        let mut by_intent = self.by_intent.write().await;
        let mut by_tenant = self.by_tenant.write().await;

        // Store request
        requests.insert(request.id, request.clone());

        // Index by intent
        by_intent
            .entry(request.intent_id)
            .or_insert_with(Vec::new)
            .push(request.id);

        // Index by tenant
        by_tenant
            .entry(request.tenant_id)
            .or_insert_with(Vec::new)
            .push(request.id);

        Ok(request)
    }

    async fn get_approval_request(&self, id: Uuid) -> Result<ApprovalRequest, IntentRebaseError> {
        let requests = self.requests.read().await;
        requests
            .get(&id)
            .cloned()
            .ok_or(IntentRebaseError::ApprovalRequestNotFound(id))
    }

    async fn list_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError> {
        let requests = self.requests.read().await;
        let by_intent = self.by_intent.read().await;

        let ids = by_intent.get(&intent_id).cloned().unwrap_or_default();

        let mut result: Vec<ApprovalRequest> = ids
            .iter()
            .filter_map(|id| requests.get(id).cloned())
            .filter(|r| r.tenant_id == tenant_id && r.status == ApprovalRequestStatus::Pending)
            .collect();

        // Sort by created_at descending (newest first)
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(result)
    }

    async fn list_pending_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError> {
        let requests = self.requests.read().await;
        let by_tenant = self.by_tenant.read().await;

        let ids = by_tenant.get(&tenant_id).cloned().unwrap_or_default();

        let mut result: Vec<ApprovalRequest> = ids
            .iter()
            .filter_map(|id| requests.get(id).cloned())
            .filter(|r| r.tenant_id == tenant_id && r.status == ApprovalRequestStatus::Pending)
            .collect();

        // Sort by created_at descending (newest first)
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(result)
    }

    async fn update_approval_request_status(
        &self,
        id: Uuid,
        status: ApprovalRequestStatus,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let mut requests = self.requests.write().await;

        let request = requests.get_mut(&id).ok_or_else(|| {
            IntentRebaseError::Internal(format!("approval request not found: {}", id))
        })?;

        // Only pending requests can be approved/rejected
        if request.status != ApprovalRequestStatus::Pending {
            return Err(IntentRebaseError::ApprovalRequestNotPending(
                id,
                format!("{:?}", request.status),
            ));
        }

        let now = Utc::now();
        request.status = status;
        request.updated_at = now;
        request.resolved_at = Some(now);
        request.resolved_by = Some(resolved_by.to_string());
        request.resolution_notes = resolution_notes.map(|s| s.to_string());

        Ok(request.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
