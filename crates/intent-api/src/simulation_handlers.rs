//! Rebase Simulation handlers (N4-4 bounded simulation slice)
//!
//! Phase 3 Batch 1: Contains GET /intents/{intent_id}/rebase-simulation and
//! POST /compensation-simulation/run handlers for read-only compensation simulation.

use axum::{
    extract::{Path, State},
    Json,
};
use intent_rebase_types::IntentRebaseError;

use crate::{
    types::{CompensationSimulationRequest, RebaseSimulationQuery},
    ApiErrorResponse, AppState,
};

#[cfg(feature = "jwt-auth")]
use crate::auth;

// ============================================================================
// N4-4: Rebase Simulation Endpoint (Phase 3 Batch 1 bounded simulation slice)
// ============================================================================

/// GET /intents/{intent_id}/rebase-simulation - Run compensation simulation for a rebase
///
/// **N4-4 scope:** Read-only mock simulation using CompensationSimulator.
/// Fetches side effects for the intent, constructs a RebaseContext, and runs
/// simulation to produce a SimulationReport with predicted outcomes.
///
/// **Mode behavior:**
/// - `deterministic` (default): Valid strategy+feasibility combos always succeed
/// - `stochastic`: Outcomes are probabilistic based on effect class success rates
///
/// **This endpoint is READ-ONLY** - it only simulates compensation outcomes
/// using mock executors. It does not execute real compensation actions.
pub async fn rebase_simulation(
    State(state): State<AppState>,
    Path(intent_id): Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<RebaseSimulationQuery>,
) -> Result<Json<compensation_service::SimulationReport>, ApiErrorResponse> {
    // Step 1: Get intent head to verify intent exists and obtain workflow_id
    let intent_head = state
        .service
        .get_intent_head(intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 1b: Validate version bounds — both versions must be >= 1
    if query.from_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("from_version ({}) must be >= 1", query.from_version),
        )));
    }
    if query.to_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("to_version ({}) must be >= 1", query.to_version),
        )));
    }

    // Step 1c: Validate version ordering — from_version must be less than to_version
    if query.from_version >= query.to_version {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!(
                "from_version ({}) must be less than to_version ({})",
                query.from_version, query.to_version
            ),
        )));
    }

    // Step 2: Fetch side effects for this intent and tenant
    let side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(intent_id, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 3: Construct RebaseContext using intent head's workflow_id
    let rebase_context = compensation_service::RebaseContext::new(
        intent_id,
        query.from_version,
        query.to_version,
        intent_head.intent.workflow_id,
    );

    // Step 4: Create simulator config based on mode query param
    let sim_config = match query.mode.as_deref() {
        Some("stochastic") => {
            if let Some(seed) = query.seed {
                compensation_service::SimulationConfig::stochastic_seed(seed)
            } else {
                // Stochastic mode without seed uses system entropy
                compensation_service::SimulationConfig::stochastic_seed(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                )
            }
        }
        Some("deterministic") | None => {
            // Default to deterministic mode
            compensation_service::SimulationConfig::deterministic()
        }
        Some(invalid_mode) => {
            // Invalid mode defaults to deterministic (safe fallback)
            tracing::warn!(
                "Invalid simulation mode '{}', defaulting to deterministic",
                invalid_mode
            );
            compensation_service::SimulationConfig::deterministic()
        }
    };

    // Step 5: Create simulator and run simulation
    let simulator = compensation_service::CompensationSimulator::with_config(sim_config);
    let report = simulator
        .simulate_side_effects(&side_effects, &rebase_context, query.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(report))
}

/// POST /compensation-simulation/run - Run compensation simulation for a rebase
///
/// **N4-4 scope:** Read-only mock simulation using CompensationSimulator.
/// Fetches side effects for the intent, constructs a RebaseContext, and runs
/// simulation to produce a SimulationReport with predicted outcomes.
///
/// This is the POST variant of the GET /intents/{intent_id}/rebase-simulation endpoint,
/// accepting request body instead of query parameters.
///
/// **Mode behavior:**
/// - `deterministic` (default): Valid strategy+feasibility combos always succeed
/// - `stochastic`: Outcomes are probabilistic based on effect class success rates
///
/// **This endpoint is READ-ONLY** - it only simulates compensation outcomes
/// using mock executors. It does not execute real compensation actions.
#[cfg(feature = "jwt-auth")]
pub async fn compensation_simulation_run(
    State(state): State<AppState>,
    auth::OptionalRlsTenantClaims(optional_rls_claims): auth::OptionalRlsTenantClaims,
    Json(request): Json<CompensationSimulationRequest>,
) -> Result<Json<compensation_service::SimulationReport>, ApiErrorResponse> {
    // Phase 5.1: JWT tenant guard - fail closed on mismatch, fail open when JWT absent
    if let Some(rls_claims) = optional_rls_claims {
        if request.tenant_id != rls_claims.tenant_id {
            let msg = format!(
                "Tenant mismatch: JWT tenant_id ({}) does not match request tenant_id ({})",
                rls_claims.tenant_id, request.tenant_id
            );
            tracing::warn!("compensation_simulation_run: tenant mismatch rejection");
            return Err(ApiErrorResponse(IntentRebaseError::Unauthorized(msg)));
        }
    }

    // Step 1: Get intent head to verify intent exists and obtain workflow_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 1b: Validate version bounds — both versions must be >= 1
    if request.from_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("from_version ({}) must be >= 1", request.from_version),
        )));
    }
    if request.to_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("to_version ({}) must be >= 1", request.to_version),
        )));
    }

    // Step 1c: Validate version ordering — from_version must be less than to_version
    if request.from_version >= request.to_version {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!(
                "from_version ({}) must be less than to_version ({})",
                request.from_version, request.to_version
            ),
        )));
    }

    // Step 2: Fetch side effects for this intent and tenant
    let all_side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(request.intent_id, request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2b: Filter by side_effect_ids if provided
    let side_effects = if let Some(ref ids) = request.side_effect_ids {
        all_side_effects
            .into_iter()
            .filter(|se| ids.contains(&se.id))
            .collect()
    } else {
        all_side_effects
    };

    // Step 3: Construct RebaseContext using intent head's workflow_id
    let rebase_context = compensation_service::RebaseContext::new(
        request.intent_id,
        request.from_version,
        request.to_version,
        intent_head.intent.workflow_id,
    );

    // Step 4: Create simulator config based on mode query param
    let sim_config = match request.mode.as_deref() {
        Some("stochastic") => {
            if let Some(seed) = request.seed {
                compensation_service::SimulationConfig::stochastic_seed(seed)
            } else {
                // Stochastic mode without seed uses system entropy
                compensation_service::SimulationConfig::stochastic_seed(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                )
            }
        }
        Some("deterministic") | None => {
            // Default to deterministic mode
            compensation_service::SimulationConfig::deterministic()
        }
        Some(invalid_mode) => {
            // Invalid mode defaults to deterministic (safe fallback)
            tracing::warn!(
                "Invalid simulation mode '{}', defaulting to deterministic",
                invalid_mode
            );
            compensation_service::SimulationConfig::deterministic()
        }
    };

    // Step 5: Create simulator and run simulation
    let simulator = compensation_service::CompensationSimulator::with_config(sim_config);
    let report = simulator
        .simulate_side_effects(&side_effects, &rebase_context, request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(report))
}

/// **Mode behavior:**
/// - `deterministic` (default): Valid strategy+feasibility combos always succeed
/// - `stochastic`: Outcomes are probabilistic based on effect class success rates
#[cfg(not(feature = "jwt-auth"))]
pub async fn compensation_simulation_run(
    State(state): State<AppState>,
    Json(request): Json<CompensationSimulationRequest>,
) -> Result<Json<compensation_service::SimulationReport>, ApiErrorResponse> {
    // Step 1: Get intent head to verify intent exists and obtain workflow_id
    let intent_head = state
        .service
        .get_intent_head(request.intent_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 1b: Validate version bounds — both versions must be >= 1
    if request.from_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("from_version ({}) must be >= 1", request.from_version),
        )));
    }
    if request.to_version < 1 {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!("to_version ({}) must be >= 1", request.to_version),
        )));
    }

    // Step 1c: Validate version ordering — from_version must be less than to_version
    if request.from_version >= request.to_version {
        return Err(ApiErrorResponse(IntentRebaseError::InvalidIntentVersion(
            format!(
                "from_version ({}) must be less than to_version ({})",
                request.from_version, request.to_version
            ),
        )));
    }

    // Step 2: Fetch side effects for this intent and tenant
    let all_side_effects = state
        .side_effect_service
        .list_side_effects_by_intent(request.intent_id, request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    // Step 2b: Filter by side_effect_ids if provided
    let side_effects = if let Some(ref ids) = request.side_effect_ids {
        all_side_effects
            .into_iter()
            .filter(|se| ids.contains(&se.id))
            .collect()
    } else {
        all_side_effects
    };

    // Step 3: Construct RebaseContext using intent head's workflow_id
    let rebase_context = compensation_service::RebaseContext::new(
        request.intent_id,
        request.from_version,
        request.to_version,
        intent_head.intent.workflow_id,
    );

    // Step 4: Create simulator config based on mode query param
    let sim_config = match request.mode.as_deref() {
        Some("stochastic") => {
            if let Some(seed) = request.seed {
                compensation_service::SimulationConfig::stochastic_seed(seed)
            } else {
                // Stochastic mode without seed uses system entropy
                compensation_service::SimulationConfig::stochastic_seed(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                )
            }
        }
        Some("deterministic") | None => {
            // Default to deterministic mode
            compensation_service::SimulationConfig::deterministic()
        }
        Some(invalid_mode) => {
            // Invalid mode defaults to deterministic (safe fallback)
            tracing::warn!(
                "Invalid simulation mode '{}', defaulting to deterministic",
                invalid_mode
            );
            compensation_service::SimulationConfig::deterministic()
        }
    };

    // Step 5: Create simulator and run simulation
    let simulator = compensation_service::CompensationSimulator::with_config(sim_config);
    let report = simulator
        .simulate_side_effects(&side_effects, &rebase_context, request.tenant_id)
        .await
        .map_err(ApiErrorResponse)?;

    Ok(Json(report))
}
