//! Artifact ingest handlers.
//!
//! Phase 3 Batch 1: Contains POST handler for artifact ingestion with optional
//! side effect capture.

use axum::{extract::State, Json};
use intent_rebase_types::IntentRebaseError;
use uuid::Uuid;

use crate::{types::ArtifactIngestResponse, ApiErrorResponse, AppState};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// Artifact Ingest Validators
// ============================================================================

/// Validate an artifact ingest request.
///
/// Returns `Ok(())` if valid, or an `Err(IntentRebaseError)` with details.
pub fn validate_artifact_ingest_request(
    request: &crate::types::ArtifactIngestRequest,
) -> Result<(), IntentRebaseError> {
    // Validate tenant_id is not nil/zero UUID
    if request.tenant_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "tenant_id cannot be nil".into(),
        ));
    }

    // Validate workflow_id is not nil/zero UUID
    if request.workflow_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "workflow_id cannot be nil".into(),
        ));
    }

    // Validate label is not empty
    if request.label.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "label cannot be empty".into(),
        ));
    }

    // Validate external_ref.ref_id is not nil UUID
    if request.external_ref.ref_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "external_ref.ref_id cannot be nil".into(),
        ));
    }

    // Phase 3 Batch 1: Validate side_effect_context if provided
    if let Some(ref context) = request.side_effect_context {
        // source_intent_id cannot be nil
        if context.source_intent_id == Uuid::nil() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.source_intent_id cannot be nil".into(),
            ));
        }

        // source_intent_version must be > 0
        if context.source_intent_version <= 0 {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "side_effect_context.source_intent_version must be > 0, got {}",
                context.source_intent_version
            )));
        }

        // effect_type cannot be empty or whitespace-only
        if context.effect_type.trim().is_empty() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.effect_type cannot be empty".into(),
            ));
        }

        // target cannot be empty or whitespace-only
        if context.target.trim().is_empty() {
            return Err(IntentRebaseError::InvalidIngestRequest(
                "side_effect_context.target cannot be empty".into(),
            ));
        }

        // idempotency_key, if provided, cannot be empty or whitespace-only
        if let Some(ref key) = context.idempotency_key {
            if key.trim().is_empty() {
                return Err(IntentRebaseError::InvalidIngestRequest(
                    "side_effect_context.idempotency_key cannot be empty".into(),
                ));
            }
        }
    }

    Ok(())
}

// ============================================================================
// Artifact Ingest Handlers (Phase 3 Batch 1 bounded slice)
// ============================================================================

/// POST /v1/graph/artifacts - Ingest an artifact with optional side effect capture
///
/// Phase 3 Batch 1 bounded slice: Ingests an artifact into the graph and optionally
/// records the side effect in the compensation ledger (capture-on-write groundwork).
///
/// This is the primary path for artifact-producing operations to record side effects.
#[cfg(feature = "jwt-auth")]
pub async fn ingest_artifact(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<crate::types::ArtifactIngestRequest>,
) -> Result<(axum::http::StatusCode, Json<ArtifactIngestResponse>), ApiErrorResponse> {
    // Phase 1: Input validation - validate request before processing
    validate_artifact_ingest_request(&request).map_err(ApiErrorResponse)?;

    // Extract side effect context before consuming request for side effect recording
    // after successful graph ingest. This preserves the context for the compensation
    // ledger write even though graph_service.ingest_artifact consumes the request.
    let side_effect_context = request.side_effect_context.clone();

    // Phase 3 P1-S5i: Use RLS-aware transaction wrapping when pool and JWT claims available
    let ingest_result = if let (Some(rls_pool), Some(rls_claims)) =
        (&state.rls_pool, &optional_rls_claims)
    {
        // Phase 5.1: JWT tenant guard - fail closed on mismatch
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("ingest_artifact: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Begin RLS-aware transaction
        let tx_result = rls_pool.begin_with_tenant(rls_claims.tenant_id).await;
        let mut tx = match tx_result {
            Ok(tx) => tx,
            Err(e) => {
                return Err(ApiErrorResponse(IntentRebaseError::Internal(format!(
                    "failed to begin RLS transaction: {}",
                    e
                ))));
            }
        };

        // Get the SQL repo and ingest artifact within the transaction
        if let Some(sql_repo) = state.graph_service.repo().as_sqlx_repo() {
            // Build the artifact request (consuming the request)
            let graph_request = intent_rebase_types::ArtifactIngestRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                external_ref: request.external_ref,
                label: request.label,
                depends_on_intent_versions: request.depends_on_intent_versions,
                properties: request.properties,
                side_effect_context: None, // Side effects recorded inside tx (ADR-08 Option A)
            };

            let result = sql_repo
                .ingest_artifact_with_tx(&mut tx, graph_request)
                .await;
            let ingest_result = match result {
                Ok(r) => r,
                Err(e) => {
                    return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                        "RLS artifact ingest failed: {}",
                        e
                    ))));
                }
            };

            // ADR-08 Option A: transactional side-effect recording inside RLS tx (fail-closed)
            if let Some(ref context) = side_effect_context {
                if let Some(sql_side_effect_repo) = state.side_effect_service.repo().as_sqlx_repo()
                {
                    let effect_class = match context.effect_class {
                        Some(intent_rebase_types::SideEffectClass::S0PureRead) => {
                            compensation_service::SideEffectClass::S0PureRead
                        }
                        Some(intent_rebase_types::SideEffectClass::S1InternalReversible) => {
                            compensation_service::SideEffectClass::S1InternalReversible
                        }
                        Some(intent_rebase_types::SideEffectClass::S2ExternalReversible) | None => {
                            compensation_service::SideEffectClass::S2ExternalReversible
                        }
                        Some(
                            intent_rebase_types::SideEffectClass::S3ExternalPartiallyReversible,
                        ) => compensation_service::SideEffectClass::S3ExternalPartiallyReversible,
                        Some(intent_rebase_types::SideEffectClass::S4Irreversible) => {
                            compensation_service::SideEffectClass::S4Irreversible
                        }
                    };

                    let recorded = if let Some(ref idempotency_key) = context.idempotency_key {
                        sql_side_effect_repo
                            .get_or_create_idempotent_with_tx(
                                &mut tx,
                                compensation_service::SideEffect::with_idempotency_key(
                                    request.tenant_id,
                                    context.source_intent_id,
                                    context.source_intent_version,
                                    effect_class,
                                    &context.effect_type,
                                    &context.target,
                                    idempotency_key,
                                ),
                            )
                            .await
                    } else {
                        sql_side_effect_repo
                            .create_with_tx(
                                &mut tx,
                                compensation_service::SideEffect::new(
                                    request.tenant_id,
                                    context.source_intent_id,
                                    context.source_intent_version,
                                    effect_class,
                                    &context.effect_type,
                                    &context.target,
                                ),
                            )
                            .await
                    };

                    match recorded {
                        Ok(effect) => {
                            tracing::debug!(
                                    "Recorded side effect {} for artifact {} (intent_id={}, version={}) inside RLS tx",
                                    effect.id,
                                    ingest_result.node.id,
                                    context.source_intent_id,
                                    context.source_intent_version
                                );
                            // Commit the transaction
                            if let Err(e) = tx.commit().await {
                                return Err(ApiErrorResponse(IntentRebaseError::StorageError(
                                    format!("failed to commit RLS transaction: {}", e),
                                )));
                            }
                            return Ok((
                                axum::http::StatusCode::CREATED,
                                Json(ArtifactIngestResponse {
                                    node: ingest_result.node,
                                    edges: ingest_result.edges,
                                    side_effect_recorded: true,
                                    side_effect_id: Some(effect.id),
                                }),
                            ));
                        }
                        Err(e) => {
                            // ADR-08 Option A: fail-closed — side-effect write failure aborts artifact ingest
                            tracing::warn!(
                                    "RLS side-effect recording failed for artifact {}: {:?} (artifact ingest rolled back)",
                                    ingest_result.node.id,
                                    e
                                );
                            return Err(ApiErrorResponse(IntentRebaseError::StorageError(
                                    format!(
                                        "RLS side-effect recording failed (artifact ingest rolled back): {}",
                                        e
                                    ),
                                )));
                        }
                    }
                }
            }

            // Commit the transaction (no side-effect context, or no SQL side-effect repo)
            if let Err(e) = tx.commit().await {
                return Err(ApiErrorResponse(IntentRebaseError::StorageError(format!(
                    "failed to commit RLS transaction: {}",
                    e
                ))));
            }

            tracing::debug!(
                "ingest_artifact: RLS path success for tenant_id={}",
                rls_claims.tenant_id
            );

            ingest_result
        } else {
            // Fallback to non-RLS path if repo doesn't support SQL
            tracing::warn!(
                "ingest_artifact: rls_pool set but repo doesn't support SQL, falling back"
            );

            // Delegate artifact ingest to graph_service - this handles prevalidation of
            // IntentVersion nodes, artifact node creation, and DependsOn edge wiring.
            // This avoids duplicating the core artifact ingest logic in intent-api.
            let graph_request = intent_rebase_types::ArtifactIngestRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                external_ref: request.external_ref,
                label: request.label,
                depends_on_intent_versions: request.depends_on_intent_versions,
                properties: request.properties,
                side_effect_context: None, // Consumed above for post-ingest recording
            };

            state
                .graph_service
                .ingest_artifact(graph_request)
                .await
                .map_err(ApiErrorResponse)?
        }
    } else {
        // Non-RLS path (no JWT claims or rls_pool is None)

        // Delegate artifact ingest to graph_service - this handles prevalidation of
        // IntentVersion nodes, artifact node creation, and DependsOn edge wiring.
        // This avoids duplicating the core artifact ingest logic in intent-api.
        let graph_request = intent_rebase_types::ArtifactIngestRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            external_ref: request.external_ref,
            label: request.label,
            depends_on_intent_versions: request.depends_on_intent_versions,
            properties: request.properties,
            side_effect_context: None, // Consumed above for post-ingest recording
        };

        state
            .graph_service
            .ingest_artifact(graph_request)
            .await
            .map_err(ApiErrorResponse)?
    };

    // Phase 3 Batch 1 (groundwork): Optionally record side effect if context provided.
    // For the RLS SQL path, side effects are already recorded inside the tx above (ADR-08 Option A).
    // This block only runs for non-RLS paths or when the SQL side-effect repo is unavailable.
    let mut side_effect_recorded = false;
    let mut side_effect_id = None;

    if let Some(ref context) = side_effect_context {
        let effect_class = match context.effect_class {
            Some(intent_rebase_types::SideEffectClass::S0PureRead) => {
                compensation_service::SideEffectClass::S0PureRead
            }
            Some(intent_rebase_types::SideEffectClass::S1InternalReversible) => {
                compensation_service::SideEffectClass::S1InternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S2ExternalReversible) | None => {
                compensation_service::SideEffectClass::S2ExternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S3ExternalPartiallyReversible) => {
                compensation_service::SideEffectClass::S3ExternalPartiallyReversible
            }
            Some(intent_rebase_types::SideEffectClass::S4Irreversible) => {
                compensation_service::SideEffectClass::S4Irreversible
            }
        };

        let recorded = if let Some(ref idempotency_key) = context.idempotency_key {
            state
                .side_effect_service
                .record_side_effect_with_idempotency(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                    idempotency_key,
                )
                .await
        } else {
            state
                .side_effect_service
                .record_side_effect(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                )
                .await
        };

        match recorded {
            Ok(effect) => {
                side_effect_recorded = true;
                side_effect_id = Some(effect.id);
                tracing::debug!(
                    "Recorded side effect {} for artifact {} (intent_id={}, version={})",
                    effect.id,
                    ingest_result.node.id,
                    context.source_intent_id,
                    context.source_intent_version
                );
            }
            Err(e) => {
                // Log but don't fail the artifact ingest if side effect recording fails
                tracing::warn!(
                    "Failed to record side effect for artifact {}: {:?}",
                    ingest_result.node.id,
                    e
                );
            }
        }
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ArtifactIngestResponse {
            node: ingest_result.node,
            edges: ingest_result.edges,
            side_effect_recorded,
            side_effect_id,
        }),
    ))
}

/// POST /v1/graph/artifacts - Ingest an artifact with optional side effect capture (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn ingest_artifact(
    State(state): State<AppState>,
    Json(request): Json<crate::types::ArtifactIngestRequest>,
) -> Result<(axum::http::StatusCode, Json<ArtifactIngestResponse>), ApiErrorResponse> {
    // Phase 1: Input validation - validate request before processing
    validate_artifact_ingest_request(&request).map_err(ApiErrorResponse)?;

    // Extract side effect context before consuming request for side effect recording
    // after successful graph ingest. This preserves the context for the compensation
    // ledger write even though graph_service.ingest_artifact consumes the request.
    let side_effect_context = request.side_effect_context.clone();

    // Delegate artifact ingest to graph_service - this handles prevalidation of
    // IntentVersion nodes, artifact node creation, and DependsOn edge wiring.
    // This avoids duplicating the core artifact ingest logic in intent-api.
    let graph_request = intent_rebase_types::ArtifactIngestRequest {
        tenant_id: request.tenant_id,
        workflow_id: request.workflow_id,
        external_ref: request.external_ref,
        label: request.label,
        depends_on_intent_versions: request.depends_on_intent_versions,
        properties: request.properties,
        side_effect_context: None, // Consumed above for post-ingest recording
    };

    let ingest_result = state
        .graph_service
        .ingest_artifact(graph_request)
        .await
        .map_err(ApiErrorResponse)?;

    // Phase 3 Batch 1 (groundwork): Optionally record side effect if context provided
    let mut side_effect_recorded = false;
    let mut side_effect_id = None;

    if let Some(ref context) = side_effect_context {
        let effect_class = match context.effect_class {
            Some(intent_rebase_types::SideEffectClass::S0PureRead) => {
                compensation_service::SideEffectClass::S0PureRead
            }
            Some(intent_rebase_types::SideEffectClass::S1InternalReversible) => {
                compensation_service::SideEffectClass::S1InternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S2ExternalReversible) | None => {
                compensation_service::SideEffectClass::S2ExternalReversible
            }
            Some(intent_rebase_types::SideEffectClass::S3ExternalPartiallyReversible) => {
                compensation_service::SideEffectClass::S3ExternalPartiallyReversible
            }
            Some(intent_rebase_types::SideEffectClass::S4Irreversible) => {
                compensation_service::SideEffectClass::S4Irreversible
            }
        };

        let recorded = if let Some(ref idempotency_key) = context.idempotency_key {
            state
                .side_effect_service
                .record_side_effect_with_idempotency(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                    idempotency_key,
                )
                .await
        } else {
            state
                .side_effect_service
                .record_side_effect(
                    request.tenant_id,
                    context.source_intent_id,
                    context.source_intent_version,
                    effect_class,
                    &context.effect_type,
                    &context.target,
                )
                .await
        };

        match recorded {
            Ok(effect) => {
                side_effect_recorded = true;
                side_effect_id = Some(effect.id);
                tracing::debug!(
                    "Recorded side effect {} for artifact {} (intent_id={}, version={})",
                    effect.id,
                    ingest_result.node.id,
                    context.source_intent_id,
                    context.source_intent_version
                );
            }
            Err(e) => {
                // Log but don't fail the artifact ingest if side effect recording fails
                tracing::warn!(
                    "Failed to record side effect for artifact {}: {:?}",
                    ingest_result.node.id,
                    e
                );
            }
        }
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ArtifactIngestResponse {
            node: ingest_result.node,
            edges: ingest_result.edges,
            side_effect_recorded,
            side_effect_id,
        }),
    ))
}

// ============================================================================
// Tests (Phase 3 Batch 1 bounded test decomposition slice)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ArtifactIngestRequest;
    use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

    // =========================================================================
    // Artifact Ingest Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_artifact_ingest_request_valid() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_tenant_id() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::nil(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("tenant_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_workflow_id() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::nil(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("workflow_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_empty_label() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("label"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_whitespace_label() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "   ".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("label"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_nil_external_ref_id() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::nil(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("external_ref.ref_id"));
    }

    // =========================================================================
    // Side Effect Context Validation Tests (Phase 3 Batch 1)
    // =========================================================================

    #[test]
    fn test_validate_artifact_ingest_request_valid_side_effect_context() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_nil_source_intent_id() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::nil(), // Invalid: nil UUID
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.source_intent_id"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_zero_version() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 0, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("source_intent_version"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_negative_version() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: -1, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("source_intent_version"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_effect_type() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "".to_string(), // Invalid: empty
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.effect_type"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_effect_type() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "   ".to_string(), // Invalid: whitespace-only
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.effect_type"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_target() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "".to_string(), // Invalid: empty
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.target"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_target() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "   ".to_string(), // Invalid: whitespace-only
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("side_effect_context.target"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_empty_idempotency_key() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("".to_string()), // Invalid: empty
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.idempotency_key"));
    }

    #[test]
    fn test_validate_artifact_ingest_request_side_effect_context_whitespace_idempotency_key() {
        let request = ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Valid Label".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("   ".to_string()), // Invalid: whitespace-only
            }),
        };

        let result = validate_artifact_ingest_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err
            .to_string()
            .contains("side_effect_context.idempotency_key"));
    }

    // =========================================================================
    // Artifact Ingest Handler Tests (Phase 3 Batch 1 groundwork)
    // =========================================================================

    #[cfg(feature = "jwt-auth")]
    use crate::test_helpers::create_test_service;
    #[cfg(feature = "jwt-auth")]
    use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
    #[cfg(feature = "jwt-auth")]
    use graph_service::{GraphService, InMemoryGraphRepository};
    #[cfg(feature = "jwt-auth")]
    use std::sync::Arc;

    /// Returns (state, graph_repo) so tests can create nodes directly via the graph_repo.
    #[cfg(feature = "jwt-auth")]
    fn create_test_service_with_graph_repo(
    ) -> (crate::AppState, Arc<dyn graph_service::GraphRepository>) {
        let state = create_test_service();
        let graph_repo = state.graph_service.repo().clone();
        (state, graph_repo)
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_success() {
        use graph_service::GraphRepository;
        use intent_rebase_types::{ExternalRef, ExternalRefType, NodeType};

        // Create a graph repo with an IntentVersion node that the artifact can depend on
        let graph_repo = Arc::new(InMemoryGraphRepository::new());
        let graph_svc = Arc::new(GraphService::new(graph_repo.clone()));

        // Use the same tenant_id and workflow_id for both the IntentVersion and the artifact
        let tenant_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node in the graph first
        let intent_version_ref_id = Uuid::new_v4();
        let _iv_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: intent_version_ref_id,
                }),
                label: "IntentVersion v1".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Build state with the graph service that has the IntentVersion node
        let mut state = create_test_service();
        state.graph_service = graph_svc.clone();

        // Create artifact request with the IntentVersion dependency and matching tenant/workflow IDs
        let request = crate::types::ArtifactIngestRequest {
            tenant_id,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![_iv_node.id],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await
        .expect("Artifact ingest should succeed");

        assert_eq!(result.0, StatusCode::CREATED);
        assert_eq!(result.1.node.label, "Test Artifact");
        assert!(!result.1.side_effect_recorded);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_nil_tenant_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::nil(), // Invalid: nil UUID
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_nil_workflow_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::nil(), // Invalid: nil UUID
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_empty_label_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "".to_string(), // Invalid: empty label
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_whitespace_label_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "   ".to_string(), // Invalid: whitespace-only label
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_nil_external_ref_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::nil(), // Invalid: nil UUID
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: None,
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_invalid_source_intent_id_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::nil(), // Invalid: nil UUID
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_invalid_version_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 0, // Invalid: must be > 0
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_effect_type_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "".to_string(), // Invalid: empty
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_target_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "".to_string(), // Invalid: empty
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_empty_idempotency_key_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("".to_string()), // Invalid: empty
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_context_whitespace_idempotency_key_rejected() {
        use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};

        let state = create_test_service();

        let request = crate::types::ArtifactIngestRequest {
            tenant_id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Test Artifact".to_string(),
            depends_on_intent_versions: vec![],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: Uuid::new_v4(),
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "https://artifact.example.com/123".to_string(),
                effect_class: None,
                idempotency_key: Some("   ".to_string()), // Invalid: whitespace-only
            }),
        };

        let result = super::ingest_artifact(
            State(state),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(request),
        )
        .await;
        let err_response = result.unwrap_err();
        let response = err_response.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // Side Effect Tenant Isolation Tests (Phase 3 Batch 1)
    // =========================================================================

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_tenant_isolation_cross_tenant_query() {
        // Test that side effects recorded by tenant A's artifact ingest
        // are NOT visible when tenant B queries by intent
        use intent_rebase_types::{
            ExternalRef, ExternalRefType, NodeType, SideEffectCaptureContext,
        };

        let (state, graph_repo) = create_test_service_with_graph_repo();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create an IntentVersion node in the graph that tenant A's artifact can depend on
        let intent_version_ref_id = Uuid::new_v4();
        let iv_node = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id: tenant_a,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: intent_version_ref_id,
                }),
                label: "IntentVersion v1".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Tenant A ingests an artifact with side effect capture
        let artifact_request_a = crate::types::ArtifactIngestRequest {
            tenant_id: tenant_a,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Tenant A Artifact".to_string(),
            depends_on_intent_versions: vec![iv_node.id],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: intent_version_ref_id,
                source_intent_version: 1,
                effect_type: "artifact_created".to_string(),
                target: "tenant-a-artifact-123".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        let result_a = super::ingest_artifact(
            State(state.clone()),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(artifact_request_a),
        )
        .await
        .expect("Tenant A artifact ingest should succeed");
        // ingest_artifact returns (StatusCode, Json<ArtifactIngestResponse>)
        assert!(result_a.1.side_effect_recorded);
        let side_effect_id_a = result_a
            .1
            .side_effect_id
            .expect("Should have side effect ID");

        // Tenant B attempts to query side effects for the same intent
        // (Tenant B has no side effects - they should see empty)
        let side_effects_b = state
            .side_effect_service
            .list_side_effects_by_intent(intent_version_ref_id, tenant_b)
            .await
            .expect("Query should succeed");

        // Tenant B should see NO side effects (tenant isolation)
        assert!(
            side_effects_b.is_empty(),
            "Tenant B should not see Tenant A's side effects"
        );

        // Tenant A should still see their own side effect
        let side_effects_a = state
            .side_effect_service
            .list_side_effects_by_intent(intent_version_ref_id, tenant_a)
            .await
            .expect("Query should succeed");
        assert_eq!(side_effects_a.len(), 1);
        assert_eq!(side_effects_a[0].id, side_effect_id_a);
        assert_eq!(side_effects_a[0].effect_type, "artifact_created");
    }

    #[cfg(feature = "jwt-auth")]
    #[tokio::test]
    async fn test_ingest_artifact_side_effect_tenant_isolation_separate_intents() {
        // Test that side effects for different tenants are isolated even with same intent ID
        use intent_rebase_types::{
            ExternalRef, ExternalRefType, NodeType, SideEffectCaptureContext,
        };

        let (state, graph_repo) = create_test_service_with_graph_repo();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();

        // Create IntentVersion nodes for each tenant
        let intent_ref_a = Uuid::new_v4();
        let intent_ref_b = Uuid::new_v4();

        let iv_node_a = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id: tenant_a,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: intent_ref_a,
                }),
                label: "Tenant A IntentVersion".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        let iv_node_b = graph_repo
            .create_node(intent_rebase_types::CreateGraphNodeRequest {
                tenant_id: tenant_b,
                workflow_id,
                node_type: NodeType::IntentVersion,
                external_ref: Some(ExternalRef {
                    ref_type: ExternalRefType::IntentVersion,
                    ref_id: intent_ref_b,
                }),
                label: "Tenant B IntentVersion".to_string(),
                properties: None,
            })
            .await
            .unwrap();

        // Tenant A ingests artifact
        let artifact_request_a = crate::types::ArtifactIngestRequest {
            tenant_id: tenant_a,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Tenant A Artifact".to_string(),
            depends_on_intent_versions: vec![iv_node_a.id],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: intent_ref_a,
                source_intent_version: 1,
                effect_type: "tenant_a_effect".to_string(),
                target: "target-a".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        // Tenant B ingests artifact
        let artifact_request_b = crate::types::ArtifactIngestRequest {
            tenant_id: tenant_b,
            workflow_id,
            external_ref: ExternalRef {
                ref_type: ExternalRefType::Artifact,
                ref_id: Uuid::new_v4(),
            },
            label: "Tenant B Artifact".to_string(),
            depends_on_intent_versions: vec![iv_node_b.id],
            properties: None,
            side_effect_context: Some(SideEffectCaptureContext {
                source_intent_id: intent_ref_b,
                source_intent_version: 1,
                effect_type: "tenant_b_effect".to_string(),
                target: "target-b".to_string(),
                effect_class: None,
                idempotency_key: None,
            }),
        };

        // Both ingests should succeed
        let result_a = super::ingest_artifact(
            State(state.clone()),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(artifact_request_a),
        )
        .await
        .expect("Tenant A artifact ingest should succeed");
        let result_b = super::ingest_artifact(
            State(state.clone()),
            crate::auth::OptionalRlsTenantClaims(None),
            Json(artifact_request_b),
        )
        .await
        .expect("Tenant B artifact ingest should succeed");

        // ingest_artifact returns (StatusCode, Json<ArtifactIngestResponse>)
        assert!(result_a.1.side_effect_recorded);
        assert!(result_b.1.side_effect_recorded);

        // Each tenant should see only their own side effect
        let side_effects_a = state
            .side_effect_service
            .list_side_effects_by_intent(intent_ref_a, tenant_a)
            .await
            .expect("Query should succeed");
        let side_effects_b = state
            .side_effect_service
            .list_side_effects_by_intent(intent_ref_b, tenant_b)
            .await
            .expect("Query should succeed");

        assert_eq!(side_effects_a.len(), 1);
        assert_eq!(side_effects_a[0].effect_type, "tenant_a_effect");
        assert_eq!(side_effects_b.len(), 1);
        assert_eq!(side_effects_b[0].effect_type, "tenant_b_effect");

        // Cross-query should return empty
        let side_effects_a_from_b = state
            .side_effect_service
            .list_side_effects_by_intent(intent_ref_a, tenant_b)
            .await
            .expect("Query should succeed");
        let side_effects_b_from_a = state
            .side_effect_service
            .list_side_effects_by_intent(intent_ref_b, tenant_a)
            .await
            .expect("Query should succeed");

        assert!(
            side_effects_a_from_b.is_empty(),
            "Tenant B should not see Tenant A's side effects for intent_ref_a"
        );
        assert!(
            side_effects_b_from_a.is_empty(),
            "Tenant A should not see Tenant B's side effects for intent_ref_b"
        );
    }
}
