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

use crate::SqlxApprovalRequestRepository;

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

    /// Cancel all pending approval requests for an intent (Phase 2b bounded slice)
    /// Called when a new intent version is created to invalidate stale approval requests.
    /// Returns the number of cancelled requests.
    async fn cancel_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError>;

    /// Mark a pending approval request as expired (Phase 2b bounded slice).
    ///
    /// Transitions Pending → Expired. Only succeeds if the request is in Pending status.
    /// This is a manual expiry action — no background worker or automatic expiry in Phase 2b.
    ///
    /// Returns the updated approval request with Expired status.
    async fn mark_expired(
        &self,
        id: Uuid,
        expired_by: &str,
        reason: &str,
    ) -> Result<ApprovalRequest, IntentRebaseError>;

    /// List all approval requests for an intent (any status) scoped to a tenant.
    ///
    /// Phase 2b bounded invalidation slice: Used to find Approved approvals that need
    /// to be cancelled when trigger_reapproval creates a replacement pending request.
    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError>;

    /// Cancel all Approved approval requests for an intent (Phase 2b bounded invalidation slice).
    ///
    /// Called by trigger_reapproval after creating a new pending approval request.
    /// Only cancels approvals that are in Approved status — pending, rejected, expired,
    /// or already cancelled requests are not affected.
    ///
    /// Returns the number of cancelled approval requests.
    async fn cancel_approved_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError>;

    /// Cancel specific Approved approval requests by their IDs (Slice 1 bounded slice).
    ///
    /// Used by classifier-driven targeted cancellation in rebase_apply.
    /// Only cancels approvals that are BOTH in the provided IDs list AND in Approved status.
    /// Other statuses (pending, rejected, expired, cancelled) are not affected.
    ///
    /// Returns the number of cancelled approval requests.
    async fn cancel_approved_by_ids(
        &self,
        approval_ids: &[Uuid],
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError>;

    /// Returns a reference to self if this is a SQL-backed repository.
    ///
    /// Used by RLS-aware handlers to downcast from `dyn ApprovalRequestRepository`
    /// to `SqlxApprovalRequestRepository` for transaction-based operations.
    /// Returns `None` for in-memory repositories.
    fn as_sqlx_approval_repo(&self) -> Option<&SqlxApprovalRequestRepository>;
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
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

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
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

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

    async fn cancel_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        let mut requests = self.requests.write().await;
        let now = Utc::now();
        let mut count = 0;

        for request in requests.values_mut() {
            if request.intent_id == intent_id
                && request.tenant_id == tenant_id
                && request.status == ApprovalRequestStatus::Pending
            {
                request.status = ApprovalRequestStatus::Cancelled;
                request.updated_at = now;
                request.resolved_at = Some(now);
                request.resolved_by = Some(cancelled_by.to_string());
                request.resolution_notes = Some(reason.to_string());
                count += 1;
            }
        }

        Ok(count)
    }

    async fn mark_expired(
        &self,
        id: Uuid,
        expired_by: &str,
        reason: &str,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let mut requests = self.requests.write().await;

        // Check if request exists first
        if !requests.contains_key(&id) {
            return Err(IntentRebaseError::ApprovalRequestNotFound(id));
        }

        let request = requests.get_mut(&id).unwrap(); // safe: we just checked

        // Only pending requests can be expired
        if request.status != ApprovalRequestStatus::Pending {
            return Err(IntentRebaseError::ApprovalRequestNotPending(
                id,
                format!("{:?}", request.status),
            ));
        }

        let now = Utc::now();
        request.status = ApprovalRequestStatus::Expired;
        request.updated_at = now;
        request.resolved_at = Some(now);
        request.resolved_by = Some(expired_by.to_string());
        request.resolution_notes = Some(reason.to_string());

        Ok(request.clone())
    }

    async fn list_by_intent(
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
            .filter(|r| r.tenant_id == tenant_id)
            .collect();

        // Sort by created_at descending (newest first)
        result.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        Ok(result)
    }

    async fn cancel_approved_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        let mut requests = self.requests.write().await;
        let now = Utc::now();
        let mut count = 0;

        for request in requests.values_mut() {
            if request.intent_id == intent_id
                && request.tenant_id == tenant_id
                && request.status == ApprovalRequestStatus::Approved
            {
                request.status = ApprovalRequestStatus::Cancelled;
                request.updated_at = now;
                request.resolved_at = Some(now);
                request.resolved_by = Some(cancelled_by.to_string());
                request.resolution_notes = Some(reason.to_string());
                count += 1;
            }
        }

        Ok(count)
    }

    async fn cancel_approved_by_ids(
        &self,
        approval_ids: &[Uuid],
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        let mut requests = self.requests.write().await;
        let now = Utc::now();
        let mut count = 0;
        let id_set: std::collections::HashSet<_> = approval_ids.iter().collect();

        for request in requests.values_mut() {
            if id_set.contains(&request.id)
                && request.tenant_id == tenant_id
                && request.status == ApprovalRequestStatus::Approved
            {
                request.status = ApprovalRequestStatus::Cancelled;
                request.updated_at = now;
                request.resolved_at = Some(now);
                request.resolved_by = Some(cancelled_by.to_string());
                request.resolution_notes = Some(reason.to_string());
                count += 1;
            }
        }

        Ok(count)
    }

    fn as_sqlx_approval_repo(&self) -> Option<&SqlxApprovalRequestRepository> {
        None
    }
}

// =============================================================================
// Helper functions for approval request status enum conversion
// =============================================================================

pub fn approval_request_status_to_string(status: &ApprovalRequestStatus) -> &'static str {
    match status {
        ApprovalRequestStatus::Pending => "pending",
        ApprovalRequestStatus::Approved => "approved",
        ApprovalRequestStatus::Rejected => "rejected",
        ApprovalRequestStatus::Expired => "expired",
        ApprovalRequestStatus::Cancelled => "cancelled",
    }
}

/// Decode a status string from the database into an ApprovalRequestStatus enum.
///
/// Falls back to `Pending` for unknown strings. This is intentional: unknown status values
/// are treated as requiring further review rather than being incorrectly classified as
/// a terminal state (approved/rejected). Unknown status should be investigated as a data
/// integrity issue.
pub fn approval_request_status_from_string(s: &str) -> ApprovalRequestStatus {
    match s {
        "approved" => ApprovalRequestStatus::Approved,
        "rejected" => ApprovalRequestStatus::Rejected,
        "expired" => ApprovalRequestStatus::Expired,
        "cancelled" => ApprovalRequestStatus::Cancelled,
        _ => ApprovalRequestStatus::Pending,
    }
}
