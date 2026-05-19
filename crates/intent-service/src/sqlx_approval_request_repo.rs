//! SQLx-backed approval request repository
//!
//! Provides `SqlxApprovalRequestRepository` for PostgreSQL persistence.

use async_trait::async_trait;
use chrono::Utc;
use intent_rebase_types::IntentRebaseError;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

use crate::approval_request_repo::{
    approval_request_status_from_string, approval_request_status_to_string, ApprovalRequest,
    ApprovalRequestRepository, ApprovalRequestStatus,
};

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

    /// Create a new approval request within an external RLS-aware transaction.
    ///
    /// This method inserts a new approval request using the provided transaction.
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// and committing/rolling back after this call.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `request` - The approval request to insert
    ///
    /// # Errors
    ///
    /// Returns error if the INSERT fails.
    pub async fn create_approval_request_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .execute(&mut **tx)
        .await
        .map_err(|e| IntentRebaseError::StorageError(format!("insert approval request: {}", e)))?;

        Ok(request.clone())
    }

    /// Cancel all Approved approval requests for an intent within an external RLS-aware transaction.
    ///
    /// This method performs a bulk UPDATE using the provided transaction.
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// and committing/rolling back after this call.
    ///
    /// Only cancels approvals that are in Approved status — pending, rejected, expired,
    /// or already cancelled requests are not affected.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `intent_id` - The intent ID
    /// * `tenant_id` - The tenant ID
    /// * `cancelled_by` - Actor ID who cancelled the approvals
    /// * `reason` - Reason for cancellation
    ///
    /// Returns the number of cancelled approval requests.
    pub async fn cancel_approved_by_intent_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "cancel approved approval requests by intent: {}",
                e
            ))
        })?;

        Ok(result.rows_affected() as usize)
    }

    /// Cancel specific Approved approval requests by their IDs within an external RLS-aware transaction.
    ///
    /// This method performs a bulk UPDATE using the provided transaction.
    /// The caller is responsible for beginning the transaction via `RlsAwarePool::begin_with_tenant`
    /// and committing/rolling back after this call.
    ///
    /// Only cancels approvals that are BOTH in the provided IDs list AND in Approved status.
    /// Other statuses (pending, rejected, expired, cancelled) are not affected.
    ///
    /// # Arguments
    ///
    /// * `tx` - A mutable reference to a `sqlx::Transaction` that already has
    ///   RLS tenant context set via `SET LOCAL app.current_tenant_id`
    /// * `approval_ids` - The approval request IDs to cancel
    /// * `tenant_id` - The tenant ID
    /// * `cancelled_by` - Actor ID who cancelled the approvals
    /// * `reason` - Reason for cancellation
    ///
    /// Returns the number of cancelled approval requests.
    pub async fn cancel_approved_by_ids_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        approval_ids: &[Uuid],
        tenant_id: Uuid,
        cancelled_by: &str,
        reason: &str,
    ) -> Result<usize, IntentRebaseError> {
        if approval_ids.is_empty() {
            return Ok(0);
        }

        let now = Utc::now();

        let num_ids = approval_ids.len();
        let id_placeholders: Vec<String> = (1..=num_ids).map(|i| format!("${}", i + 3)).collect();
        let in_clause = format!("({})", id_placeholders.join(", "));
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

        let mut q = sqlx::query(&query);
        q = q.bind(now).bind(cancelled_by).bind(reason);
        for id in approval_ids {
            q = q.bind(id);
        }
        q = q.bind(tenant_id);

        let result = q.execute(&mut **tx).await.map_err(|e| {
            IntentRebaseError::StorageError(format!(
                "cancel specific approved approval requests by ids: {}",
                e
            ))
        })?;

        Ok(result.rows_affected() as usize)
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
