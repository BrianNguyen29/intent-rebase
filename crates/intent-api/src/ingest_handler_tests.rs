use super::*;
use crate::ingest_handlers::validate_artifact_ingest_request;
use crate::test_helpers::{create_test_service, create_test_service_with_graph_repo};
use crate::types::ArtifactIngestRequest;
use intent_rebase_types::{ExternalRef, ExternalRefType, SideEffectCaptureContext};
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
#[cfg(feature = "jwt-auth")]
use graph_service::{GraphService, InMemoryGraphRepository};
#[cfg(feature = "jwt-auth")]
use std::sync::Arc;

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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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

    let result = crate::ingest_handlers::ingest_artifact(
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
    use intent_rebase_types::{ExternalRef, ExternalRefType, NodeType, SideEffectCaptureContext};

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

    let result_a = crate::ingest_handlers::ingest_artifact(
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
    use intent_rebase_types::{ExternalRef, ExternalRefType, NodeType, SideEffectCaptureContext};

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
    let result_a = crate::ingest_handlers::ingest_artifact(
        State(state.clone()),
        crate::auth::OptionalRlsTenantClaims(None),
        Json(artifact_request_a),
    )
    .await
    .expect("Tenant A artifact ingest should succeed");
    let result_b = crate::ingest_handlers::ingest_artifact(
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
