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
