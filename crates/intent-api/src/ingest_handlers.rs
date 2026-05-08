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
    let ingest_result =
        if let (Some(rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
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
                    side_effect_context: None, // Side effects recorded post-commit
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

                // Commit the transaction
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
}
