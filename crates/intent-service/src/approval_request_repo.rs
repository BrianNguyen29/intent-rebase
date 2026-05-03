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
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

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
// SQLx-backed Approval Request Repository
// =============================================================================

use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;

/// SQL-backed repository for approval request persistence using PostgreSQL.
/// Follows the same patterns as SqlxCheckpointRepository.
pub struct SqlxApprovalRequestRepository {
    pool: PgPool,
}

impl SqlxApprovalRequestRepository {
    /// Create a new SqlxApprovalRequestRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Convert a database row to an ApprovalRequest domain object
    fn row_to_request(&self, row: PgRow) -> Result<ApprovalRequest, IntentRebaseError> {
        let status_str: String = row.get("status");
        let metadata_json: serde_json::Value = row.get("metadata");

        Ok(ApprovalRequest {
            id: row.get("id"),
            intent_id: row.get("intent_id"),
            intent_version_from: row.get("intent_version_from"),
            intent_version_to: row.get("intent_version_to"),
            workflow_id: row.get("workflow_id"),
            tenant_id: row.get("tenant_id"),
            requestor_id: row.get("requestor_id"),
            requestor_type: row.get("requestor_type"),
            decision_class: row.get("decision_class"),
            reason: row.get("reason"),
            metadata: metadata_json,
            status: approval_request_status_from_string(&status_str),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            expires_at: row.get("expires_at"),
            resolved_at: row.get("resolved_at"),
            resolved_by: row.get("resolved_by"),
            resolution_notes: row.get("resolution_notes"),
        })
    }

    /// Insert a new approval request into the database
    async fn insert_request(
        &self,
        request: &ApprovalRequest,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let metadata_json = serde_json::to_value(&request.metadata).map_err(|e| {
            IntentRebaseError::SerializationError(format!("approval request metadata: {}", e))
        })?;
        let status_str = approval_request_status_to_string(&request.status);

        sqlx::query(
            r#"
            INSERT INTO approval_requests (
                id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            "#,
        )
        .bind(request.id)
        .bind(request.intent_id)
        .bind(request.intent_version_from)
        .bind(request.intent_version_to)
        .bind(request.workflow_id)
        .bind(request.tenant_id)
        .bind(&request.requestor_id)
        .bind(&request.requestor_type)
        .bind(&request.decision_class)
        .bind(&request.reason)
        .bind(metadata_json)
        .bind(status_str)
        .bind(request.created_at)
        .bind(request.updated_at)
        .bind(request.expires_at)
        .bind(request.resolved_at)
        .bind(&request.resolved_by)
        .bind(&request.resolution_notes)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert approval request: {}", e)))?;

        Ok(request.clone())
    }

    /// Update the status of an approval request in the database using atomic conditional UPDATE.
    /// The WHERE clause includes status = 'pending' to prevent TOCTOU races between concurrent
    /// approve/reject operations on the same approval request.
    async fn update_status_in_db(
        &self,
        id: Uuid,
        status: ApprovalRequestStatus,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let status_str = approval_request_status_to_string(&status);
        let now = Utc::now();

        // Atomic conditional UPDATE: only succeeds if current status is 'pending'
        // This eliminates the TOCTOU race between checking status and updating it
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = $1, updated_at = $2, resolved_at = $2, resolved_by = $3, resolution_notes = $4
            WHERE id = $5 AND status = 'pending'
            "#,
        )
        .bind(status_str)
        .bind(now)
        .bind(resolved_by)
        .bind(resolution_notes)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update approval request status: {}", e))
        })?;

        // If no rows were affected, the request was not in pending status
        // (either already processed, not found, or concurrent operation won the race)
        if result.rows_affected() == 0 {
            return Err(IntentRebaseError::ApprovalRequestNotPending(
                id,
                "atomic update failed - request not in pending status".to_string(),
            ));
        }

        self.get_approval_request(id).await
    }

    /// Get an approval request by ID using an external transaction.
    ///
    /// This method is used by RLS-wrapped operations where the caller manages
    /// the transaction lifecycle.
    pub async fn get_approval_request_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: Uuid,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            FROM approval_requests
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch approval request: {}", e)))?;

        match row {
            Some(r) => self.row_to_request(r),
            None => Err(IntentRebaseError::ApprovalRequestNotFound(id)),
        }
    }

    /// Update the status of an approval request within an external RLS-aware transaction.
    ///
    /// This method performs an atomic conditional UPDATE using the provided transaction.
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// and committing/rolling back after this call.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `id` - The approval request ID
    /// * `status` - The new status (Approved or Rejected)
    /// * `resolved_by` - Actor ID who resolved the request
    /// * `resolution_notes` - Optional notes about the resolution
    ///
    /// # Errors
    ///
    /// Returns error if the UPDATE affects 0 rows (not pending or not found).
    pub async fn update_status_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: Uuid,
        status: ApprovalRequestStatus,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let status_str = approval_request_status_to_string(&status);
        let now = Utc::now();

        // Atomic conditional UPDATE: only succeeds if current status is 'pending'
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = $1, updated_at = $2, resolved_at = $2, resolved_by = $3, resolution_notes = $4
            WHERE id = $5 AND status = 'pending'
            "#,
        )
        .bind(status_str)
        .bind(now)
        .bind(resolved_by)
        .bind(resolution_notes)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("update approval request status: {}", e))
        })?;

        // If no rows were affected, the request was not in pending status
        if result.rows_affected() == 0 {
            return Err(IntentRebaseError::ApprovalRequestNotPending(
                id,
                "atomic update failed - request not in pending status".to_string(),
            ));
        }

        // Fetch and return the updated request using the transaction
        self.get_approval_request_with_tx(tx, id).await
    }

    /// Mark an approval request as expired within an external RLS-aware transaction.
    ///
    /// This method performs an atomic conditional UPDATE using the provided transaction.
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// and committing/rolling back after this call.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `id` - The approval request ID
    /// * `expired_by` - Actor ID who expired the request
    /// * `reason` - Reason for expiry
    ///
    /// # Errors
    ///
    /// Returns error if the UPDATE affects 0 rows (not pending or not found).
    pub async fn mark_expired_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: Uuid,
        expired_by: &str,
        reason: &str,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let now = Utc::now();

        // Atomic conditional UPDATE: only expires if current status is 'pending'
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = 'expired',
                updated_at = $1,
                resolved_at = $1,
                resolved_by = $2,
                resolution_notes = $3
            WHERE id = $4 AND status = 'pending'
            "#,
        )
        .bind(now)
        .bind(expired_by)
        .bind(reason)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("expire approval request: {}", e)))?;

        // If no rows were affected, the request was not in pending status
        if result.rows_affected() == 0 {
            // Determine the actual status for the error message
            let current = self.get_approval_request_with_tx(tx, id).await;
            match current {
                Ok(req) => {
                    return Err(IntentRebaseError::ApprovalRequestNotPending(
                        id,
                        format!("{:?}", req.status),
                    ));
                }
                Err(_) => {
                    return Err(IntentRebaseError::ApprovalRequestNotFound(id));
                }
            }
        }

        self.get_approval_request_with_tx(tx, id).await
    }
}

#[async_trait]
impl ApprovalRequestRepository for SqlxApprovalRequestRepository {
    async fn create_approval_request(
        &self,
        request: ApprovalRequest,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        self.insert_request(&request).await
    }

    async fn get_approval_request(&self, id: Uuid) -> Result<ApprovalRequest, IntentRebaseError> {
        let row = sqlx::query(
            r#"
            SELECT id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            FROM approval_requests
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("fetch approval request: {}", e)))?;

        match row {
            Some(r) => self.row_to_request(r),
            None => Err(IntentRebaseError::ApprovalRequestNotFound(id)),
        }
    }

    async fn list_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            FROM approval_requests
            WHERE intent_id = $1 AND tenant_id = $2 AND status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list pending approval requests by intent: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_request(r)).collect()
    }

    async fn list_pending_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            FROM approval_requests
            WHERE tenant_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "list pending approval requests by tenant: {}",
                e
            ))
        })?;

        rows.into_iter().map(|r| self.row_to_request(r)).collect()
    }

    async fn update_approval_request_status(
        &self,
        id: Uuid,
        status: ApprovalRequestStatus,
        resolved_by: &str,
        resolution_notes: Option<&str>,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        // Atomic conditional UPDATE handles the pending-status check internally
        // to prevent TOCTOU races between concurrent approve/reject operations.
        // Returns error if no rows affected (not pending or not found).
        self.update_status_in_db(id, status, resolved_by, resolution_notes)
            .await
    }

    async fn cancel_pending_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        let now = Utc::now();

        // Bulk update: cancel all pending approval requests for this intent+tenant
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = 'cancelled',
                updated_at = $1,
                resolved_at = $1,
                resolved_by = $2,
                resolution_notes = $3
            WHERE intent_id = $4 AND tenant_id = $5 AND status = 'pending'
            "#,
        )
        .bind(now)
        .bind(cancelled_by)
        .bind(reason)
        .bind(intent_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "cancel pending approval requests by intent: {}",
                e
            ))
        })?;

        Ok(result.rows_affected() as usize)
    }

    async fn mark_expired(
        &self,
        id: Uuid,
        expired_by: &str,
        reason: &str,
    ) -> Result<ApprovalRequest, IntentRebaseError> {
        let now = Utc::now();

        // Atomic conditional UPDATE: only expires if current status is 'pending'
        // This eliminates the TOCTOU race between checking status and updating it
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = 'expired',
                updated_at = $1,
                resolved_at = $1,
                resolved_by = $2,
                resolution_notes = $3
            WHERE id = $4 AND status = 'pending'
            "#,
        )
        .bind(now)
        .bind(expired_by)
        .bind(reason)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("expire approval request: {}", e)))?;

        // If no rows were affected, the request was not in pending status
        if result.rows_affected() == 0 {
            // Determine the actual status for the error message
            let current = self.get_approval_request(id).await;
            match current {
                Ok(req) => {
                    return Err(IntentRebaseError::ApprovalRequestNotPending(
                        id,
                        format!("{:?}", req.status),
                    ));
                }
                Err(_) => {
                    return Err(IntentRebaseError::ApprovalRequestNotFound(id));
                }
            }
        }

        self.get_approval_request(id).await
    }

    async fn list_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>, IntentRebaseError> {
        let rows = sqlx::query(
            r#"
            SELECT id, intent_id, intent_version_from, intent_version_to,
                workflow_id, tenant_id, requestor_id, requestor_type,
                decision_class, reason, metadata, status,
                created_at, updated_at, expires_at,
                resolved_at, resolved_by, resolution_notes
            FROM approval_requests
            WHERE intent_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(intent_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!("list approval requests by intent: {}", e))
        })?;

        rows.into_iter().map(|r| self.row_to_request(r)).collect()
    }

    async fn cancel_approved_by_intent(
        &self,
        intent_id: Uuid,
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        let now = Utc::now();

        // Bulk update: cancel all approved approval requests for this intent+tenant
        let result = sqlx::query(
            r#"
            UPDATE approval_requests
            SET status = 'cancelled',
                updated_at = $1,
                resolved_at = $1,
                resolved_by = $2,
                resolution_notes = $3
            WHERE intent_id = $4 AND tenant_id = $5 AND status = 'approved'
            "#,
        )
        .bind(now)
        .bind(cancelled_by)
        .bind(reason)
        .bind(intent_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "cancel approved approval requests by intent: {}",
                e
            ))
        })?;

        Ok(result.rows_affected() as usize)
    }

    async fn cancel_approved_by_ids(
        &self,
        approval_ids: &[Uuid],
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        if approval_ids.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();

        // Build the query with a IN clause for the IDs using array positioning
        // sqlx uses 1-indexed positional parameters
        // Order: $1=now, $2=cancelled_by, $3=reason, then IDs, then tenant_id
        let num_ids = approval_ids.len();
        let id_placeholders: Vec<String> = (1..=num_ids)
            .map(|i| format!("${}", i + 3)) // IDs start at $4
            .collect();
        let in_clause = format!("({})", id_placeholders.join(", "));
        // tenant_id comes after all IDs: $ (3 + num_ids + 1)
        let tenant_idx = 3 + num_ids + 1;

        let query = format!(
            r#"
            UPDATE approval_requests
            SET status = 'cancelled',
                updated_at = $1,
                resolved_at = $1,
                resolved_by = $2,
                resolution_notes = $3
            WHERE id IN {} AND tenant_id = ${} AND status = 'approved'
            "#,
            in_clause, tenant_idx
        );

        // Build the query with sqlx - order: now, cancelled_by, reason, ids..., tenant_id
        let mut q = sqlx::query(&query);
        q = q.bind(now).bind(cancelled_by).bind(reason);
        for id in approval_ids {
            q = q.bind(id);
        }
        q = q.bind(tenant_id);

        let result = q.execute(&self.pool).await.map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "cancel specific approved approval requests by ids: {}",
                e
            ))
        })?;

        Ok(result.rows_affected() as usize)
    }

    /// Returns a reference to self if this is a SQL-backed repository.
    ///
    /// Used by RLS-aware handlers to downcast from `dyn ApprovalRequestRepository`
    /// to `SqlxApprovalRequestRepository` for transaction-based operations.
    fn as_sqlx_approval_repo(&self) -> Option<&SqlxApprovalRequestRepository> {
        Some(self)
    }
}

// =============================================================================
// Helper functions for approval request status enum conversion
// =============================================================================

fn approval_request_status_to_string(status: &ApprovalRequestStatus) -> &'static str {
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
fn approval_request_status_from_string(s: &str) -> ApprovalRequestStatus {
    match s {
        "approved" => ApprovalRequestStatus::Approved,
        "rejected" => ApprovalRequestStatus::Rejected,
        "expired" => ApprovalRequestStatus::Expired,
        "cancelled" => ApprovalRequestStatus::Cancelled,
        _ => ApprovalRequestStatus::Pending,
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
    use super::*;

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
