//! Intent mutation handlers.
//!
//! Phase 2 bounded slice: Contains create_intent and create_version handlers
//! for POST /intents and POST /intents/{intent_id}/versions endpoints.
//! Both JWT and non-JWT variants are preserved via cfg gates.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use intent_rebase_types::{
    CreateIntentRequest, CreateIntentResponse, CreateVersionRequest, CreateVersionResponse,
    IntentRebaseError,
};
use uuid::Uuid;

#[cfg(feature = "jwt-auth")]
use crate::auth;
use crate::{ApiErrorResponse, AppState};

// ============================================================================
// Metrics Helper
// ============================================================================

/// Record intent version creation outcome
fn record_intent_version_created(status: &'static str) {
    metrics::counter!("intent_api_intent_version_created_total", "status" => status).increment(1);
}

// ============================================================================
// Input Validation
// ============================================================================

/// Validates required fields in CreateIntentRequest.
/// Returns Err with specific validation error if any field is invalid.
pub fn validate_create_intent_request(
    request: &CreateIntentRequest,
) -> Result<(), IntentRebaseError> {
    // Validate workflow_id is not nil/zero UUID
    if request.workflow_id == Uuid::nil() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "workflow_id cannot be nil".into(),
        ));
    }

    // Validate payload.objective.summary is not empty
    if request.payload.objective.summary.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.summary cannot be empty".into(),
        ));
    }

    // Validate payload.objective.success_statement is not empty
    if request
        .payload
        .objective
        .success_statement
        .trim()
        .is_empty()
    {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.success_statement cannot be empty".into(),
        ));
    }

    // Validate payload.objective.domain is not empty
    if request.payload.objective.domain.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.domain cannot be empty".into(),
        ));
    }

    Ok(())
}

/// Validates required fields in CreateVersionRequest.
/// Returns Err with specific validation error if any field is invalid.
pub fn validate_create_version_request(
    request: &CreateVersionRequest,
) -> Result<(), IntentRebaseError> {
    // Validate payload.objective.summary is not empty
    if request.payload.objective.summary.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.summary cannot be empty".into(),
        ));
    }

    // Validate payload.objective.success_statement is not empty
    if request
        .payload
        .objective
        .success_statement
        .trim()
        .is_empty()
    {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.success_statement cannot be empty".into(),
        ));
    }

    // Validate payload.objective.domain is not empty
    if request.payload.objective.domain.trim().is_empty() {
        return Err(IntentRebaseError::InvalidIngestRequest(
            "payload.objective.domain cannot be empty".into(),
        ));
    }

    Ok(())
}

/// Parse an optional i32 header value.
/// Returns Ok(None) if header is absent, Ok(Some(value)) if present and valid.
/// Returns Err(InvalidHeader) if header is present but malformed.
pub fn parse_optional_header(
    headers: &HeaderMap,
    name: &str,
) -> Result<Option<i32>, IntentRebaseError> {
    match headers.get(name) {
        None => Ok(None),
        Some(v) => {
            let s = v.to_str().map_err(|_| {
                IntentRebaseError::InvalidHeader(format!("{} header is not valid UTF-8", name))
            })?;
            s.parse::<i32>().map(Some).map_err(|_| {
                IntentRebaseError::InvalidHeader(format!(
                    "{} header must be an integer, got: {}",
                    name, s
                ))
            })
        }
    }
}

// ============================================================================
// Create Intent Handler
// ============================================================================

/// POST /intents - Create a new intent
///
/// Phase 3 P3-S5 bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
///
/// When jwt-auth feature is disabled, this handler uses the non-RLS path only.
#[cfg(feature = "jwt-auth")]
pub async fn create_intent(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), ApiErrorResponse> {
    // Phase 1: Input validation
    if let Err(e) = validate_create_intent_request(&request) {
        record_intent_version_created("error");
        return Err(ApiErrorResponse(e));
    }

    // Check if RLS path is available (pool exists AND JWT claims present)
    if let (Some(_rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // Determine tenant_id: use JWT tenant_id as authoritative
        // If request specifies tenant_id, validate it matches JWT
        let tenant_id = if let Some(request_tenant_id) = request.tenant_id {
            if request_tenant_id != rls_claims.tenant_id {
                let msg = format!(
                    "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                    rls_claims.tenant_id, request_tenant_id
                );
                tracing::warn!("create_intent: tenant mismatch rejection");
                record_intent_version_created("error");
                return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
            }
            rls_claims.tenant_id
        } else {
            // No tenant_id in request, use JWT tenant_id
            rls_claims.tenant_id
        };

        // Use RLS-aware method
        match state
            .service
            .create_intent_with_rls(request, tenant_id)
            .await
        {
            Ok(r) => {
                record_intent_version_created("success");
                tracing::debug!(
                    "create_intent: RLS path success for tenant_id={}",
                    tenant_id
                );
                Ok((StatusCode::CREATED, Json(r)))
            }
            Err(e) => {
                record_intent_version_created("error");
                Err(ApiErrorResponse(e))
            }
        }
    } else {
        // Non-RLS path (no JWT claims or rls_pool is None)
        match state.service.create_intent(request).await {
            Ok(r) => {
                record_intent_version_created("success");
                Ok((StatusCode::CREATED, Json(r)))
            }
            Err(e) => {
                record_intent_version_created("error");
                Err(ApiErrorResponse(e))
            }
        }
    }
}

/// POST /intents - Create a new intent (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_intent(
    State(state): State<AppState>,
    Json(request): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<CreateIntentResponse>), ApiErrorResponse> {
    // Phase 1: Input validation
    if let Err(e) = validate_create_intent_request(&request) {
        record_intent_version_created("error");
        return Err(ApiErrorResponse(e));
    }

    match state.service.create_intent(request).await {
        Ok(r) => {
            record_intent_version_created("success");
            Ok((StatusCode::CREATED, Json(r)))
        }
        Err(e) => {
            record_intent_version_created("error");
            Err(ApiErrorResponse(e))
        }
    }
}

// ============================================================================
// Create Version Handler
// ============================================================================

/// POST /intents/{intent_id}/versions - Create a new version
///
/// Optional OCC headers:
/// - `X-Expected-Version`: the version number the client expects to be current
/// - `X-Expected-Row-Version`: the row_version the client last observed
///   If provided, enables optimistic concurrency control. Returns 409 on conflict.
///   If headers are malformed (non-integer), returns 400 Bad Request.
///
/// Phase 3 P3-S5 bounded slice: When `state.rls_pool` is Some AND valid JWT claims
/// are present, this handler uses RLS-aware transaction wrapping for tenant isolation.
/// Falls back to non-RLS path when no JWT claims are present (backward compatible).
#[cfg(feature = "jwt-auth")]
pub async fn create_version(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Path(intent_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<CreateVersionResponse>), ApiErrorResponse> {
    let expected_version = match parse_optional_header(&headers, "x-expected-version") {
        Ok(v) => v,
        Err(e) => {
            record_intent_version_created("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let expected_row_version = match parse_optional_header(&headers, "x-expected-row-version") {
        Ok(v) => v,
        Err(e) => {
            record_intent_version_created("error");
            return Err(ApiErrorResponse(e));
        }
    };

    // Check if RLS path is available (pool exists AND JWT claims present)
    if let (Some(_rls_pool), Some(rls_claims)) = (&state.rls_pool, &optional_rls_claims) {
        // First fetch the intent to get its tenant_id for validation
        let intent_head = match state.service.get_intent_head(intent_id).await {
            Ok(head) => head,
            Err(e) => {
                record_intent_version_created("error");
                return Err(ApiErrorResponse(e));
            }
        };

        // Tenant mismatch rejection: JWT tenant must match the intent's tenant
        if intent_head.intent.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match intent tenant_id ({})",
                rls_claims.tenant_id, intent_head.intent.tenant_id
            );
            tracing::warn!("create_version: tenant mismatch rejection");
            record_intent_version_created("error");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }

        // Use RLS-aware method
        match state
            .service
            .create_version_with_rls(
                intent_id,
                request,
                expected_version,
                expected_row_version,
                rls_claims.tenant_id,
            )
            .await
        {
            Ok(r) => {
                record_intent_version_created("success");
                tracing::debug!(
                    "create_version: RLS path success for tenant_id={}",
                    rls_claims.tenant_id
                );
                Ok((StatusCode::CREATED, Json(r)))
            }
            Err(e) => {
                record_intent_version_created("error");
                Err(ApiErrorResponse(e))
            }
        }
    } else {
        // Non-RLS path (no JWT claims or rls_pool is None)
        match state
            .service
            .create_version(intent_id, request, expected_version, expected_row_version)
            .await
        {
            Ok(r) => {
                record_intent_version_created("success");
                Ok((StatusCode::CREATED, Json(r)))
            }
            Err(e) => {
                record_intent_version_created("error");
                Err(ApiErrorResponse(e))
            }
        }
    }
}

/// POST /intents/{intent_id}/versions - Create a new version (non-JWT fallback)
#[cfg(not(feature = "jwt-auth"))]
pub async fn create_version(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<CreateVersionResponse>), ApiErrorResponse> {
    let expected_version = match parse_optional_header(&headers, "x-expected-version") {
        Ok(v) => v,
        Err(e) => {
            record_intent_version_created("error");
            return Err(ApiErrorResponse(e));
        }
    };
    let expected_row_version = match parse_optional_header(&headers, "x-expected-row-version") {
        Ok(v) => v,
        Err(e) => {
            record_intent_version_created("error");
            return Err(ApiErrorResponse(e));
        }
    };

    match state
        .service
        .create_version(intent_id, request, expected_version, expected_row_version)
        .await
    {
        Ok(r) => {
            record_intent_version_created("success");
            Ok((StatusCode::CREATED, Json(r)))
        }
        Err(e) => {
            record_intent_version_created("error");
            Err(ApiErrorResponse(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_parse_optional_header_absent() {
        let headers = HeaderMap::new();
        let result = parse_optional_header(&headers, "x-expected-version").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_optional_header_valid_integer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-expected-version", HeaderValue::from_static("5"));
        let result = parse_optional_header(&headers, "x-expected-version").unwrap();
        assert_eq!(result, Some(5));
    }

    #[test]
    fn test_parse_optional_header_malformed_non_integer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-expected-version",
            HeaderValue::from_static("not-a-number"),
        );
        let result = parse_optional_header(&headers, "x-expected-version");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidHeader(_)));
        let msg = err.to_string();
        assert!(msg.contains("x-expected-version"));
        assert!(msg.contains("not-a-number"));
    }

    #[test]
    fn test_parse_optional_header_malformed_negative_integer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-expected-row-version", HeaderValue::from_static("-1"));
        let result = parse_optional_header(&headers, "x-expected-row-version");
        // -1 is a valid i32, so it should parse successfully
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(-1));
    }

    // === Input Validation Tests ===

    #[test]
    fn test_validate_create_intent_request_valid() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, SourceRef, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![SourceRef {
                ref_type: "spec".to_string(),
                id: "spec://test".to_string(),
            }],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_create_intent_request_nil_workflow_id() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::nil(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("workflow_id"));
    }

    #[test]
    fn test_validate_create_intent_request_empty_summary() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("summary"));
    }

    #[test]
    fn test_validate_create_intent_request_whitespace_summary() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, IntentAuthority, IntentConstraints, IntentMetadataV1,
            IntentObjective, IntentPayload, IntentPreferences, IntentReferences, IntentScope,
            RiskTier, Urgency,
        };

        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "   ".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
            tags: vec![],
        };

        let result = validate_create_intent_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("summary"));
    }

    #[test]
    fn test_validate_create_version_request_valid() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "Updating".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        let result = validate_create_version_request(&request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_create_version_request_empty_domain() {
        use intent_rebase_types::{
            AcceptanceCriteria, ActorRef, ChangeChannel, IntentAuthority, IntentConstraints,
            IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
            IntentScope, RiskTier, Urgency,
        };

        let request = CreateVersionRequest {
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test intent".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec![],
                    out_of_scope: vec![],
                },
                constraints: IntentConstraints {
                    functional: vec![],
                    non_functional: vec![],
                    policy: vec![],
                    budget: vec![],
                    time: vec![],
                },
                acceptance_criteria: AcceptanceCriteria {
                    required: vec![],
                    optional: vec![],
                },
                authority: IntentAuthority {
                    allowed_actions: vec![],
                    forbidden_actions: vec![],
                    approval_requirements: vec![],
                },
                preferences: IntentPreferences { tradeoffs: vec![] },
                references: IntentReferences {
                    specs: vec![],
                    tickets: vec![],
                    repos: vec![],
                    policies: vec![],
                },
                assumptions: intent_rebase_types::IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Medium,
                    urgency: Urgency::Medium,
                    confidence: 0.9,
                },
            },
            change_reason: "Updating".to_string(),
            change_channel: ChangeChannel::UserEdit,
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test-user".to_string(),
            },
        };

        let result = validate_create_version_request(&request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, IntentRebaseError::InvalidIngestRequest(_)));
        assert!(err.to_string().contains("domain"));
    }
}
