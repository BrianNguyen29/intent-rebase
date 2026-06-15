use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use intent_rebase_types::{
    CreateIntentRequest, DiffRequest, IntentHeadResponse, IntentVersion, ListVersionsResponse,
    ValidateIntentResponse, ValidationError,
};
use uuid::Uuid;
use validator::Validate;

use crate::{ApiErrorResponse, AppState, DiffResponse};

// ============================================================================
// Intent Validation Handler
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

// ============================================================================
// Intent Read-Only Query Handlers
// ============================================================================

/// GET /intents/{intent_id} - Get intent head (current version)
pub async fn get_intent_head(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
) -> Result<Json<IntentHeadResponse>, ApiErrorResponse> {
    state
        .service
        .get_intent_head(intent_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /intents/{intent_id}/versions - List all versions (descending order)
pub async fn list_versions(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
) -> Result<Json<ListVersionsResponse>, ApiErrorResponse> {
    state
        .service
        .list_versions(intent_id)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

/// GET /intents/{intent_id}/versions/{version_number} - Get specific version
pub async fn get_version(
    State(state): State<AppState>,
    Path((intent_id, version_number)): Path<(Uuid, i32)>,
) -> Result<Json<IntentVersion>, ApiErrorResponse> {
    state
        .service
        .get_version(intent_id, version_number)
        .await
        .map(Json)
        .map_err(ApiErrorResponse)
}

// ============================================================================
// Diff Computation Handler
// ============================================================================

/// Record diff compute duration
pub(crate) fn record_diff_compute_duration(duration_secs: f64) {
    metrics::histogram!("intent_api_diff_compute_duration_seconds").record(duration_secs);
}

/// POST /intents/{intent_id}/diff - Compute diff between two versions
pub async fn compute_diff(
    State(state): State<AppState>,
    Path(intent_id): Path<Uuid>,
    Json(request): Json<DiffRequest>,
) -> Result<Json<DiffResponse>, ApiErrorResponse> {
    let start = std::time::Instant::now();
    let result = state
        .service
        .compute_diff(intent_id, request.from_version, request.to_version)
        .await;

    let duration = start.elapsed().as_secs_f64();
    record_diff_compute_duration(duration);

    match result {
        Ok((from_version, to_version, diff, risk)) => Ok(Json(DiffResponse {
            intent_id,
            from_version,
            to_version,
            diff,
            risk,
        })),
        Err(e) => Err(ApiErrorResponse(e)),
    }
}

pub fn add_routes(router: Router<crate::AppState>) -> Router<crate::AppState> {
    router
        .route("/v1/intents/validate", post(validate_intent))
        .route(
            "/intents",
            post(crate::intent_mutation_handlers::create_intent),
        )
        .route("/intents/:intent_id", get(get_intent_head))
        .route(
            "/intents/:intent_id/versions",
            post(crate::intent_mutation_handlers::create_version),
        )
        .route("/intents/:intent_id/versions", get(list_versions))
        .route(
            "/intents/:intent_id/versions/:version_number",
            get(get_version),
        )
        .route("/intents/:intent_id/diff", post(compute_diff))
        .route(
            "/intents/:intent_id/rebase-preview",
            post(crate::rebase_preview_handlers::rebase_preview),
        )
        .route(
            "/intents/:intent_id/rebase-apply",
            post(crate::rebase_apply_handlers::rebase_apply),
        )
        // Replay endpoint (Phase 2b bounded replay slice)
        .route(
            "/intents/:intent_id/replay",
            post(crate::replay_handlers::replay_intent),
        )
        // Side effect query endpoint (Phase 3 Batch 1 groundwork)
        .route(
            "/intents/:intent_id/side-effects",
            get(crate::query_handlers::list_side_effects),
        )
        // N4-4: Rebase simulation endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/intents/:intent_id/rebase-simulation",
            get(crate::simulation_handlers::rebase_simulation),
        )
        // N4-4 POST: Compensation simulation run endpoint (Phase 3 Batch 1 bounded simulation slice)
        .route(
            "/compensation-simulation/run",
            post(crate::simulation_handlers::compensation_simulation_run),
        )
        // Orchestration dashboard endpoint (Phase 3 Batch 1 bounded read-only slice)
        .route(
            "/intents/:intent_id/orchestration-dashboard",
            get(crate::query_handlers::get_orchestration_dashboard),
        )
        // ImpactReport endpoint (Phase 2 bounded MVP — on-demand read-only projection)
        .route(
            "/intents/:intent_id/impact-report",
            get(crate::query_handlers::get_impact_report),
        )
}
