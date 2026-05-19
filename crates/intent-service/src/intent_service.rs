use intent_rebase_types::{
    get_current_trace_context, AffectedItem, AffectedItemsPreview, ApprovalCancelledAuditPayload,
    AuditRepository, CreateIntentRequest, CreateIntentResponse, CreateVersionRequest,
    CreateVersionResponse, IntentHeadResponse, IntentRebaseError, IntentVersion,
    ListVersionsResponse, NodeType,
};
use rebase_engine::{compute_diff_with_risk_sync, DiffRiskAnalysis, IntentVersionDiff, RebasePlan};
use std::sync::Arc;
use uuid::Uuid;

use crate::{ApprovalRequestRepository, IntentRepository};

/// IntentService handles intent lifecycle operations
pub struct IntentService {
    repo: Arc<dyn IntentRepository>,
    /// Optional graph service for impact classification
    graph_service: Option<Arc<graph_service::GraphService>>,
    /// Optional approval request repository for cancelling pending approvals on version change
    approval_repo: Option<Arc<dyn ApprovalRequestRepository>>,
    /// Optional audit repository for recording cancellation events
    audit_repo: Option<Arc<dyn AuditRepository>>,
    /// System actor ID used for system-initiated cancellations
    system_actor_id: String,
    /// Phase 3 P3-S5: Optional RLS-aware pool for tenant-scoped transactions.
    /// When Some, RLS-aware methods are available for JWT-authenticated requests.
    rls_pool: Option<graph_service::RlsAwarePool>,
}

impl IntentService {
    pub fn new(repo: Arc<dyn IntentRepository>) -> Self {
        Self {
            repo,
            graph_service: None,
            approval_repo: None,
            audit_repo: None,
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with optional graph service for graph-integrated features
    pub fn with_graph_service(
        repo: Arc<dyn IntentRepository>,
        graph_service: Arc<graph_service::GraphService>,
    ) -> Self {
        Self {
            repo,
            graph_service: Some(graph_service),
            approval_repo: None,
            audit_repo: None,
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with approval and audit repositories for Phase 2b bounded slice.
    /// When approval_repo is provided, creating a new intent version will automatically cancel
    /// any pending approval requests for that intent.
    pub fn with_approval_and_audit(
        repo: Arc<dyn IntentRepository>,
        approval_repo: Arc<dyn ApprovalRequestRepository>,
        audit_repo: Arc<dyn AuditRepository>,
    ) -> Self {
        Self {
            repo,
            graph_service: None,
            approval_repo: Some(approval_repo),
            audit_repo: Some(audit_repo),
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Create a new IntentService with all optional services for Phase 2b bounded slice.
    pub fn with_all_services(
        repo: Arc<dyn IntentRepository>,
        graph_service: Arc<graph_service::GraphService>,
        approval_repo: Arc<dyn ApprovalRequestRepository>,
        audit_repo: Arc<dyn AuditRepository>,
    ) -> Self {
        Self {
            repo,
            graph_service: Some(graph_service),
            approval_repo: Some(approval_repo),
            audit_repo: Some(audit_repo),
            system_actor_id: "intent-service/system".to_string(),
            rls_pool: None,
        }
    }

    /// Set the RLS-aware pool for tenant-scoped transactions.
    ///
    /// Phase 3 P3-S5: Enables RLS-aware methods for JWT-authenticated requests.
    /// This should be called after constructing the service when using SQL-backed
    /// repositories with RLS enabled.
    pub fn with_rls_pool(mut self, pool: graph_service::RlsAwarePool) -> Self {
        self.rls_pool = Some(pool);
        self
    }

    /// Create a new intent with initial version (transactional)
    #[tracing::instrument(skip(self))]
    pub async fn create_intent(
        &self,
        request: CreateIntentRequest,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        self.repo.create_intent_tx(request).await
    }

    /// Create a new version of an existing intent with optimistic concurrency control
    ///
    /// If `expected_version` and `expected_row_version` are provided (non-zero), performs OCC check:
    /// - Returns `ConcurrencyConflict` if the intent's current version or row_version doesn't match
    ///   This allows clients to detect concurrent modifications and retry.
    ///
    /// Phase 2b bounded slice: When approval_repo is configured, this method will automatically
    /// cancel any pending approval requests for the intent when a new version is created.
    #[tracing::instrument(skip(self))]
    pub async fn create_version(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: Option<i32>,
        expected_row_version: Option<i32>,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let exp_ver = expected_version.unwrap_or(intent.current_version);
        let exp_row_ver = expected_row_version.unwrap_or(row_version);

        // Capture old version number before creating new version for cancellation
        let old_version = intent.current_version;

        let result = self
            .repo
            .create_version_with_occ(intent_id, request, exp_ver, exp_row_ver)
            .await?;

        // Phase 2b bounded slice: Cancel pending approval requests if approval_repo is configured
        if let Some(approval_repo) = &self.approval_repo {
            let tenant_id = intent.tenant_id;
            let cancellation_reason = format!(
                "Intent version changed from v{} to v{}",
                old_version, result.version_number
            );

            // Cancel all pending approval requests for this intent
            let cancelled_count = approval_repo
                .cancel_pending_by_intent(
                    intent_id,
                    tenant_id,
                    &self.system_actor_id,
                    &cancellation_reason,
                )
                .await
                .unwrap_or(0);

            // Emit audit event if audit_repo is configured and we cancelled any requests
            if cancelled_count > 0 {
                if let Some(audit_repo) = &self.audit_repo {
                    let audit_payload = ApprovalCancelledAuditPayload {
                        intent_id,
                        cancelled_version_from: old_version,
                        cancelled_version_to: result.version_number,
                        decision_class: "D/E".to_string(), // High risk decisions require approval
                        cancelled_by: self.system_actor_id.clone(),
                        cancellation_reason,
                        cancelled_count,
                    };
                    let _ = audit_repo
                        .record_approval_cancelled(
                            tenant_id,
                            &self.system_actor_id,
                            intent_id,
                            audit_payload,
                            get_current_trace_context(),
                        )
                        .await;
                }
            }
        }

        Ok(result)
    }

    // =============================================================================
    // RLS-aware methods (Phase 3 P3-S5 bounded slice)
    // =============================================================================

    /// Returns true if RLS pool is configured.
    pub fn has_rls_pool(&self) -> bool {
        self.rls_pool.is_some()
    }

    /// Create a new intent with initial version using RLS-aware transaction.
    ///
    /// Phase 3 P3-S5: This method wraps intent creation in an RLS-set transaction
    /// when `rls_pool` is configured. The tenant_id is extracted from the JWT claims
    /// and validated before beginning the transaction.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `rls_pool` is not configured (caller should fall back to non-RLS)
    /// - tenant_id is nil (RLS validation failure)
    /// - Transaction fails to begin or commit
    /// - Intent creation fails
    #[tracing::instrument(skip(self, request))]
    pub async fn create_intent_with_rls(
        &self,
        request: CreateIntentRequest,
        tenant_id: Uuid,
    ) -> Result<CreateIntentResponse, IntentRebaseError> {
        let rls_pool = self
            .rls_pool
            .as_ref()
            .ok_or_else(|| IntentRebaseError::Internal("RLS pool not configured".to_string()))?;

        // Validate tenant_id for RLS use
        intent_rebase_types::rls::validate_tenant_id_for_rls(tenant_id).map_err(|e| {
            IntentRebaseError::Internal(format!("invalid tenant_id for RLS: {}", e))
        })?;

        let mut tx = rls_pool.begin_with_tenant(tenant_id).await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin RLS transaction: {}", e))
        })?;

        // Get the SQL repository and create intent within the transaction
        let sql_repo = self.repo.as_sqlx_repo().ok_or_else(|| {
            IntentRebaseError::Internal("RLS requires SQL-backed repository".to_string())
        })?;

        let result = sql_repo
            .create_intent_with_tx(&mut tx, request, tenant_id)
            .await;

        match result {
            Ok(response) => {
                tx.commit().await.map_err(|e| {
                    IntentRebaseError::StorageError(format!(
                        "failed to commit RLS transaction: {}",
                        e
                    ))
                })?;
                Ok(response)
            }
            Err(e) => {
                // Transaction will be rolled back on drop, but we log the error
                tracing::error!(error = %e, "RLS intent creation failed, rolling back transaction");
                Err(e)
            }
        }
    }

    /// Create a new version of an existing intent using RLS-aware transaction.
    ///
    /// Phase 3 P3-S5: This method wraps version creation in an RLS-set transaction
    /// when `rls_pool` is configured. The tenant_id is extracted from the JWT claims
    /// and validated before beginning the transaction.
    ///
    /// If `expected_version` and `expected_row_version` are provided (non-zero), performs OCC check.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `rls_pool` is not configured (caller should fall back to non-RLS)
    /// - tenant_id is nil (RLS validation failure)
    /// - Transaction fails to begin or commit
    /// - Intent not found or version creation fails
    #[tracing::instrument(skip(self, request))]
    pub async fn create_version_with_rls(
        &self,
        intent_id: Uuid,
        request: CreateVersionRequest,
        expected_version: Option<i32>,
        expected_row_version: Option<i32>,
        tenant_id: Uuid,
    ) -> Result<CreateVersionResponse, IntentRebaseError> {
        let rls_pool = self
            .rls_pool
            .as_ref()
            .ok_or_else(|| IntentRebaseError::Internal("RLS pool not configured".to_string()))?;

        // Validate tenant_id for RLS use
        intent_rebase_types::rls::validate_tenant_id_for_rls(tenant_id).map_err(|e| {
            IntentRebaseError::Internal(format!("invalid tenant_id for RLS: {}", e))
        })?;

        let mut tx = rls_pool.begin_with_tenant(tenant_id).await.map_err(|e| {
            IntentRebaseError::StorageError(format!("failed to begin RLS transaction: {}", e))
        })?;

        // Get the SQL repository
        let sql_repo = self.repo.as_sqlx_repo().ok_or_else(|| {
            IntentRebaseError::Internal("RLS requires SQL-backed repository".to_string())
        })?;

        // First get the intent to check current version
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let exp_ver = expected_version.unwrap_or(intent.current_version);
        let exp_row_ver = expected_row_version.unwrap_or(row_version);

        // Capture old version number for potential approval cancellation
        let old_version = intent.current_version;

        let result = sql_repo
            .create_version_with_tx(&mut tx, intent_id, request, exp_ver, exp_row_ver)
            .await;

        match result {
            Ok(version_result) => {
                // Phase 2b: Cancel pending approvals if configured
                if let Some(approval_repo) = &self.approval_repo {
                    let cancellation_reason = format!(
                        "Intent version changed from v{} to v{}",
                        old_version, version_result.version_number
                    );

                    let cancelled_count = approval_repo
                        .cancel_pending_by_intent(
                            intent_id,
                            tenant_id,
                            &self.system_actor_id,
                            &cancellation_reason,
                        )
                        .await
                        .unwrap_or(0);

                    if cancelled_count > 0 {
                        if let Some(audit_repo) = &self.audit_repo {
                            let audit_payload = ApprovalCancelledAuditPayload {
                                intent_id,
                                cancelled_version_from: old_version,
                                cancelled_version_to: version_result.version_number,
                                decision_class: "D/E".to_string(),
                                cancelled_by: self.system_actor_id.clone(),
                                cancellation_reason,
                                cancelled_count,
                            };
                            let _ = audit_repo
                                .record_approval_cancelled(
                                    tenant_id,
                                    &self.system_actor_id,
                                    intent_id,
                                    audit_payload,
                                    get_current_trace_context(),
                                )
                                .await;
                        }
                    }
                }

                tx.commit().await.map_err(|e| {
                    IntentRebaseError::StorageError(format!(
                        "failed to commit RLS transaction: {}",
                        e
                    ))
                })?;
                Ok(version_result)
            }
            Err(e) => {
                tracing::error!(error = %e, "RLS version creation failed, rolling back transaction");
                Err(e)
            }
        }
    }

    /// Get the current (head) version of an intent
    #[tracing::instrument(skip(self))]
    pub async fn get_intent_head(
        &self,
        intent_id: Uuid,
    ) -> Result<IntentHeadResponse, IntentRebaseError> {
        let (intent, row_version) = self.repo.get_intent_for_update(intent_id).await?;
        let version = self
            .repo
            .get_version_by_intent_and_number(intent_id, intent.current_version)
            .await?;

        Ok(IntentHeadResponse {
            intent,
            version,
            row_version,
        })
    }

    /// Get a specific version of an intent by version number
    pub async fn get_version(
        &self,
        intent_id: Uuid,
        version_number: i32,
    ) -> Result<IntentVersion, IntentRebaseError> {
        self.repo
            .get_version_by_intent_and_number(intent_id, version_number)
            .await
    }

    /// List all versions of an intent (descending order per API spec)
    pub async fn list_versions(
        &self,
        intent_id: Uuid,
    ) -> Result<ListVersionsResponse, IntentRebaseError> {
        // Verify intent exists
        let _intent = self.repo.get_intent(intent_id).await?;
        let mut versions = self.repo.get_versions_by_intent(intent_id).await?;

        // Sort descending by version number (newest first)
        versions.sort_by_key(|b| std::cmp::Reverse(b.version_number));

        Ok(ListVersionsResponse {
            intent_id,
            total: versions.len(),
            versions,
        })
    }

    /// Compute diff between two versions of an intent
    ///
    /// Validates that both versions exist, belong to the same intent,
    /// and have valid ordering (from_version < to_version).
    /// Returns (from_version, to_version, diff, risk) tuple.
    #[tracing::instrument(skip(self))]
    pub async fn compute_diff(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<
        (
            IntentVersion,
            IntentVersion,
            IntentVersionDiff,
            DiffRiskAnalysis,
        ),
        IntentRebaseError,
    > {
        // Validate version ordering before fetching
        if from_version >= to_version {
            return Err(IntentRebaseError::InvalidIntentVersion(format!(
                "from_version ({}) must be less than to_version ({})",
                from_version, to_version
            )));
        }

        // Fetch both versions
        let from = self
            .repo
            .get_version_by_intent_and_number(intent_id, from_version)
            .await?;
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Compute diff with risk analysis using the synchronous function
        // The sync function is safe here since it only does in-memory computation
        let (diff, risk) = compute_diff_with_risk_sync(&from, &to)?;

        Ok((from, to, diff, risk))
    }

    /// Compute rebase preview between two versions of an intent
    ///
    /// Validates that both versions exist, belong to the same intent,
    /// and have valid ordering (from_version < to_version).
    /// Returns a rebase plan with decision class, rationale, and section decisions.
    ///
    /// This is a preview-only endpoint that does NOT include:
    /// - affected_items (requires graph integration - Phase 2)
    /// - deferred fields (Phase 2)
    #[tracing::instrument(skip(self))]
    pub async fn compute_rebase_preview(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<RebasePlan, IntentRebaseError> {
        // Validate version ordering before fetching
        if from_version >= to_version {
            return Err(IntentRebaseError::InvalidIntentVersion(format!(
                "from_version ({}) must be less than to_version ({})",
                from_version, to_version
            )));
        }

        // Fetch both versions
        let from = self
            .repo
            .get_version_by_intent_and_number(intent_id, from_version)
            .await?;
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Compute diff with risk analysis
        let (diff, risk) = compute_diff_with_risk_sync(&from, &to)?;

        // Generate rebase plan using the planner (Phase 1 baseline)
        // Note: This only uses diff+risk - no graph integration in Phase 1
        let plan = RebasePlan::from_diff_and_risk(&diff, &risk);

        Ok(plan)
    }

    /// Compute rebase preview with graph-integrated affected items.
    ///
    /// This method extends `compute_rebase_preview` by enriching the response
    /// with graph-based impact classification when graph service is available.
    ///
    /// If graph service is unavailable or the IntentVersion node is not found in the graph,
    /// the affected_items will have status=Unavailable but the endpoint will NOT fail.
    /// This ensures the rebase preview remains reliable even when graph coverage is incomplete.
    ///
    /// The affected_items are classified starting from the `to_version` IntentVersion node.
    #[tracing::instrument(skip(self))]
    pub async fn compute_rebase_preview_with_graph(
        &self,
        intent_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<RebasePlan, IntentRebaseError> {
        // First, compute the base rebase plan
        let plan = self
            .compute_rebase_preview(intent_id, from_version, to_version)
            .await?;

        // If no graph service is available, return the plan with unavailable status
        let graph_service = match &self.graph_service {
            Some(gs) => gs,
            None => return Ok(plan),
        };

        // Get the to_version to find its graph node
        let to = self
            .repo
            .get_version_by_intent_and_number(intent_id, to_version)
            .await?;

        // Try to classify affected items from the to_version
        let classification_result = graph_service
            .classify_affected_items_from_intent_version(to.id, Some(3))
            .await;

        match classification_result {
            Ok(Some(result)) => {
                // Graph classification succeeded - build affected items from result
                let (artifacts, approvals, side_effects) =
                    Self::classify_nodes_by_type(&result.classified_nodes);

                let affected_items =
                    AffectedItemsPreview::from_classification(artifacts, approvals, side_effects);

                // Create new plan with enriched affected_items
                let enriched_plan = RebasePlan {
                    decision_class: plan.decision_class,
                    rationale: plan.rationale,
                    section_decisions: plan.section_decisions,
                    affected_items,
                    deferred: plan.deferred,
                    manual_review_recommended: plan.manual_review_recommended,
                    risk_tier: plan.risk_tier,
                    risk_level: plan.risk_level,
                };

                Ok(enriched_plan)
            }
            Ok(None) | Err(_) => {
                // Graph node not found or classification failed - return with unavailable status
                // Note: We intentionally do NOT fail the endpoint here
                Ok(plan)
            }
        }
    }

    /// Helper to classify graph nodes by type from a classification result
    fn classify_nodes_by_type(
        classified_nodes: &[intent_rebase_types::ClassifiedNode],
    ) -> (Vec<AffectedItem>, Vec<AffectedItem>, Vec<AffectedItem>) {
        let mut artifacts = Vec::new();
        let mut approvals = Vec::new();
        let mut side_effects = Vec::new();

        for classified in classified_nodes {
            let item = AffectedItem {
                node_id: classified.node.id,
                label: classified.node.label.clone(),
                impact: classified.impact.clone(),
                reason: classified.reason.clone(),
                external_ref: classified.node.external_ref.clone(),
            };

            match classified.node.node_type {
                NodeType::Artifact => artifacts.push(item),
                NodeType::Approval => approvals.push(item),
                NodeType::SideEffect => side_effects.push(item),
                _ => {} // Skip other node types for affected items preview
            }
        }

        (artifacts, approvals, side_effects)
    }
}
