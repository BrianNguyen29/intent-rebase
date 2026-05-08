//! Intent validation handlers.
//!
//! Phase 2 bounded slice: Contains POST /v1/intents/validate handler for
//! validating intent requests without persisting.

use axum::Json;
use intent_rebase_types::{CreateIntentRequest, ValidateIntentResponse, ValidationError};
use validator::Validate;

// ============================================================================
// Validation Handler (Phase 2 bounded slice)
// ============================================================================

/// Recursively collect nested validation errors from ValidationErrors
pub fn collect_nested_errors(
    errors: &validator::ValidationErrors,
    prefix: &str,
    out: &mut Vec<(String, validator::ValidationError)>,
) {
    for (field, kind) in errors.0.iter() {
        match kind {
            validator::ValidationErrorsKind::Field(field_errors) => {
                for e in field_errors {
                    let full_field = if prefix.is_empty() {
                        field.to_string()
                    } else {
                        format!("{prefix}.{field}")
                    };
                    out.push((
                        full_field,
                        validator::ValidationError {
                            code: e.code.clone(),
                            message: e.message.clone(),
                            params: e.params.clone(),
                        },
                    ));
                }
            }
            validator::ValidationErrorsKind::Struct(nested) => {
                let new_prefix = if prefix.is_empty() {
                    field.to_string()
                } else {
                    format!("{prefix}.{field}")
                };
                collect_nested_errors(nested, &new_prefix, out);
            }
            validator::ValidationErrorsKind::List(_) => {
                // Skip list errors for now (collections not used in Phase 1)
            }
        }
    }
}

/// POST /v1/intents/validate - Validate an intent request without persisting
pub async fn validate_intent(
    Json(request): Json<CreateIntentRequest>,
) -> Json<ValidateIntentResponse> {
    match request.validate() {
        Ok(()) => Json(ValidateIntentResponse {
            valid: true,
            errors: vec![],
        }),
        Err(errs) => {
            let mut raw_errors: Vec<(String, validator::ValidationError)> = Vec::new();
            collect_nested_errors(&errs, "", &mut raw_errors);
            let validation_errors: Vec<ValidationError> = raw_errors
                .into_iter()
                .map(|(field, e)| ValidationError {
                    field,
                    message: e.message.as_ref().unwrap_or(&e.code).to_string(),
                })
                .collect();

            Json(ValidateIntentResponse {
                valid: validation_errors.is_empty(),
                errors: validation_errors,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intent_rebase_types::{
        AcceptanceCriteria, ActorRef, IntentAssumptions, IntentAuthority, IntentConstraints,
        IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences, IntentReferences,
        IntentScope, RiskTier, SourceRef, Urgency,
    };
    use uuid::Uuid;

    #[tokio::test]
    async fn test_validate_intent_valid_request() {
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
                    success_statement: "Success statement".to_string(),
                    domain: "testing".to_string(),
                },
                scope: IntentScope {
                    in_scope: vec!["item1".to_string()],
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
                assumptions: IntentAssumptions { explicit: vec![] },
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
            tags: vec!["test".to_string()],
        };

        let result = validate_intent(Json(request)).await;
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_validate_intent_empty_summary() {
        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
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
                assumptions: IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 0.5,
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let result = validate_intent(Json(request)).await;
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
        assert!(
            field_names.iter().any(|f| f.contains("summary")),
            "Expected summary validation error"
        );
    }

    #[tokio::test]
    async fn test_validate_intent_confidence_out_of_range() {
        let request = CreateIntentRequest {
            tenant_id: None,
            workflow_id: Uuid::new_v4(),
            source_refs: vec![],
            payload: IntentPayload {
                objective: IntentObjective {
                    summary: "Test".to_string(),
                    success_statement: "Success".to_string(),
                    domain: "test".to_string(),
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
                assumptions: IntentAssumptions { explicit: vec![] },
                metadata: IntentMetadataV1 {
                    risk_tier: RiskTier::Low,
                    urgency: Urgency::Low,
                    confidence: 1.5, // Out of range (should be 0.0-1.0)
                },
            },
            created_by: ActorRef {
                actor_type: "user".to_string(),
                actor_id: "test".to_string(),
            },
            tags: vec![],
        };

        let result = validate_intent(Json(request)).await;
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        let field_names: Vec<&str> = result.errors.iter().map(|e| e.field.as_str()).collect();
        assert!(
            field_names.iter().any(|f| f.contains("confidence")),
            "Expected confidence validation error"
        );
    }
}
