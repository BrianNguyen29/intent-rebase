use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// Simulation Types
// =============================================================================

/// Query parameters for rebase simulation endpoint.
///
/// **N4-4 scope:** Deterministic/stochastic mock simulation using CompensationSimulator.
/// This is READ-ONLY simulation - does not execute real compensation actions.
#[derive(Debug, Deserialize)]
pub struct RebaseSimulationQuery {
    /// Tenant ID to scope the query (required)
    pub tenant_id: Uuid,
    /// Source intent version before rebase (required)
    pub from_version: i32,
    /// Target intent version after rebase (required)
    pub to_version: i32,
    /// Simulation mode: "deterministic" (default) or "stochastic"
    #[serde(default)]
    pub mode: Option<String>,
    /// RNG seed for stochastic mode reproducibility (optional, only used when mode=stochastic)
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Request body for POST /compensation-simulation/run endpoint.
///
/// **N4-4 scope:** Bounded read-only compensation simulation using CompensationSimulator.
/// This is READ-ONLY simulation - does not execute real compensation actions or mutate state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationSimulationRequest {
    /// Intent ID to simulate compensation for (required)
    pub intent_id: Uuid,
    /// Tenant ID to scope the query (required)
    pub tenant_id: Uuid,
    /// Source intent version before rebase (required)
    pub from_version: i32,
    /// Target intent version after rebase (required)
    pub to_version: i32,
    /// Simulation mode: "deterministic" (default) or "stochastic"
    #[serde(default)]
    pub mode: Option<String>,
    /// RNG seed for stochastic mode reproducibility (optional, only used when mode=stochastic)
    #[serde(default)]
    pub seed: Option<u64>,
    /// Optional list of specific side effect IDs to simulate.
    /// If provided, only these side effects are included in the simulation.
    /// If not provided, all side effects for the intent are simulated.
    #[serde(default)]
    pub side_effect_ids: Option<Vec<Uuid>>,
}
