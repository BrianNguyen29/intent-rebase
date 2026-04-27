//! Compensation simulation domain models, mock executors, and orchestrator.
//!
//! This module provides:
//! - Simulation models: SimulationConfig, SimulationMode, ResidualRisk,
//!   ResidualRiskLevel, SimulationRecommendation, SimulationOutcome, SimulationReport
//! - Mock executors: MockRollbackExecutor, MockCounterActionExecutor,
//!   MockFollowupNoticeExecutor, MockEscalationExecutor
//! - CompensationSimulator orchestrator for running simulations
//!
//! **N4-1 scope:** Simulation domain models (no real execution in simulation mode).
//! **N4-2 scope:** Mock executors with deterministic/stochastic config that
//!   implement the CompensationExecutor trait but do not mutate external systems.
//! **N4-3 scope:** CompensationSimulator orchestrator that plans and simulates
//!   compensation actions.
//!
//! Mock executors enforce compatible strategy+feasibility gating semantics
//! rather than accepting every action silently.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, ExecutionResult, RebaseContext, StrategyType,
};
use crate::compensation_executor::CompensationExecutor;
use crate::compensation_planner::BoundedCompensationPlanner;
use crate::side_effect::{SideEffect, SideEffectClass};
use intent_rebase_types::IntentRebaseError;

// =============================================================================
// Shared Helpers
// =============================================================================

/// Linear congruential generator constant for deterministic randomness.
const LCG_A: u64 = 6364136223846793005;
const LCG_C: u64 = 1;

/// Generate a random value in [0.0, 1.0) for stochastic mode.
///
/// Uses a simple LCG for deterministic randomness when seed is provided,
/// otherwise uses system time as entropy source.
fn random_value(seed: Option<u64>) -> f64 {
    match seed {
        Some(seed) => {
            let mut state = seed.wrapping_mul(LCG_A).wrapping_add(LCG_C);
            state = state.rotate_left(1);
            (state % 1000000) as f64 / 1000000.0
        }
        None => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0) as u64;
            let mut state = nanos.wrapping_mul(LCG_A).wrapping_add(LCG_C);
            state = state.rotate_left(1);
            (state % 1000000) as f64 / 1000000.0
        }
    }
}

/// Convert CompensationFeasibility to corresponding SideEffectClass.
///
/// This is a lossless mapping since the feasibility levels were derived
/// from effect classes during compensation planning.
pub fn feasibility_to_effect_class(feasibility: CompensationFeasibility) -> SideEffectClass {
    match feasibility {
        CompensationFeasibility::Automatic => SideEffectClass::S1InternalReversible,
        CompensationFeasibility::SemiAutomatic => SideEffectClass::S2ExternalReversible,
        CompensationFeasibility::ManualOnly => SideEffectClass::S3ExternalPartiallyReversible,
        CompensationFeasibility::NotPossible => SideEffectClass::S4Irreversible,
    }
}

// =============================================================================
// Simulation Configuration & Mode
// =============================================================================

/// Simulation mode determines whether outcomes are deterministic or probabilistic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    /// Deterministic mode: outcomes are fixed based on strategy/feasibility validity.
    /// Valid combos always succeed; invalid combos always fail.
    Deterministic,
    /// Stochastic mode: outcomes are probabilistic based on effect class
    /// success probabilities.
    Stochastic,
}

/// Configuration for compensation simulation.
///
/// **N4-1 scope:** Bounded config with mode and optional RNG seed for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    /// Simulation mode (deterministic vs stochastic).
    pub mode: SimulationMode,
    /// Optional RNG seed for stochastic mode reproducibility.
    /// If None, a random seed is used.
    pub seed: Option<u64>,
    /// Default success probabilities by side effect class (used in stochastic mode).
    /// If not provided, defaults are used.
    pub probabilities: Option<SimulationProbabilities>,
}

/// Default success probabilities by side effect class.
///
/// Used in stochastic mode when probabilities are not explicitly configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationProbabilities {
    /// Success probability for S1 (InternalReversible) effects.
    /// Default: 0.95
    pub s1_internal_reversible: f64,
    /// Success probability for S2 (ExternalReversible) effects.
    /// Default: 0.70
    pub s2_external_reversible: f64,
    /// Success probability for S3 (ExternalPartiallyReversible) effects.
    /// Default: 0.50
    pub s3_external_partially_reversible: f64,
    /// Success probability for S4 (Irreversible) effects.
    /// Default: 0.10
    pub s4_irreversible: f64,
}

impl Default for SimulationProbabilities {
    fn default() -> Self {
        Self {
            s1_internal_reversible: 0.95,
            s2_external_reversible: 0.70,
            s3_external_partially_reversible: 0.50,
            s4_irreversible: 0.10,
        }
    }
}

impl SimulationProbabilities {
    /// Get success probability for a given side effect class.
    pub fn probability_for(&self, effect_class: SideEffectClass) -> f64 {
        match effect_class {
            SideEffectClass::S0PureRead => 1.0, // Read has no effect to compensate
            SideEffectClass::S1InternalReversible => self.s1_internal_reversible,
            SideEffectClass::S2ExternalReversible => self.s2_external_reversible,
            SideEffectClass::S3ExternalPartiallyReversible => self.s3_external_partially_reversible,
            SideEffectClass::S4Irreversible => self.s4_irreversible,
        }
    }

    /// Get success probability for a given feasibility level.
    ///
    /// Convenience method that first converts feasibility to effect class,
    /// then looks up the probability.
    pub fn probability_for_feasibility(&self, feasibility: CompensationFeasibility) -> f64 {
        let effect_class = feasibility_to_effect_class(feasibility);
        self.probability_for(effect_class)
    }
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            mode: SimulationMode::Deterministic,
            seed: None,
            probabilities: None,
        }
    }
}

impl SimulationConfig {
    /// Create a new deterministic simulation config.
    pub fn deterministic() -> Self {
        Self::default()
    }

    /// Create a new stochastic simulation config with a seed.
    pub fn stochastic_seed(seed: u64) -> Self {
        Self {
            mode: SimulationMode::Stochastic,
            seed: Some(seed),
            probabilities: None,
        }
    }

    /// Create a new stochastic simulation config with custom probabilities (and no seed).
    pub fn stochastic_with_probabilities(probabilities: SimulationProbabilities) -> Self {
        Self {
            mode: SimulationMode::Stochastic,
            seed: None,
            probabilities: Some(probabilities),
        }
    }

    /// Create a new stochastic simulation config with both seed and custom probabilities.
    pub fn stochastic(seed: u64, probabilities: SimulationProbabilities) -> Self {
        Self {
            mode: SimulationMode::Stochastic,
            seed: Some(seed),
            probabilities: Some(probabilities),
        }
    }

    /// Get the effective probabilities (custom or default).
    pub fn probabilities(&self) -> SimulationProbabilities {
        self.probabilities.clone().unwrap_or_default()
    }
}

// =============================================================================
// Residual Risk Model
// =============================================================================

/// Residual risk level after compensation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualRiskLevel {
    /// Risk is negligible or fully mitigated.
    Low,
    /// Risk is moderate and should be monitored.
    Medium,
    /// Risk is high and requires attention.
    High,
}

impl ResidualRiskLevel {
    /// Get risk level from probability of success.
    pub fn from_success_probability(probability: f64) -> Self {
        if probability >= 0.90 {
            ResidualRiskLevel::Low
        } else if probability >= 0.50 {
            ResidualRiskLevel::Medium
        } else {
            ResidualRiskLevel::High
        }
    }
}

/// Residual risk after compensation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidualRisk {
    /// Risk level.
    pub level: ResidualRiskLevel,
    /// Human-readable description of the residual risk.
    pub description: String,
    /// Probability of successful compensation (0.0 to 1.0).
    pub success_probability: f64,
}

impl ResidualRisk {
    /// Create a residual risk from a probability.
    pub fn from_probability(probability: f64, effect_class: SideEffectClass) -> Self {
        let level = ResidualRiskLevel::from_success_probability(probability);
        let description = format!(
            "Residual risk for {:?} compensation: {:.1}% success probability",
            effect_class,
            probability * 100.0
        );
        Self {
            level,
            description,
            success_probability: probability,
        }
    }

    /// Create a residual risk from a probability, deriving effect class from feasibility.
    pub fn from_probability_and_feasibility(
        probability: f64,
        feasibility: CompensationFeasibility,
    ) -> Self {
        let effect_class = feasibility_to_effect_class(feasibility);
        Self::from_probability(probability, effect_class)
    }
}

// =============================================================================
// Simulation Recommendation
// =============================================================================

/// Recommendation for compensation action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationRecommendation {
    /// Proceed with compensation automatically.
    ProceedAuto,
    /// Proceed with compensation but require manual approval.
    ProceedManual,
    /// Do not compensate; the risk is too high.
    DoNotCompensate,
    /// Escalate to human review.
    Escalate,
}

impl SimulationRecommendation {
    /// Get recommendation based on success probability and feasibility.
    pub fn from_probability_and_feasibility(
        probability: f64,
        feasibility: CompensationFeasibility,
    ) -> Self {
        // High confidence + automatic feasibility = auto proceed
        if probability >= 0.90 && feasibility == CompensationFeasibility::Automatic {
            SimulationRecommendation::ProceedAuto
        }
        // Medium confidence or semi-automatic = manual approval
        else if probability >= 0.50 {
            SimulationRecommendation::ProceedManual
        }
        // Low confidence + not possible = escalate
        else if feasibility == CompensationFeasibility::NotPossible || probability < 0.10 {
            SimulationRecommendation::Escalate
        }
        // Otherwise do not compensate
        else {
            SimulationRecommendation::DoNotCompensate
        }
    }
}

// =============================================================================
// Simulation Outcome & Report
// =============================================================================

/// Outcome of a simulated compensation action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationOutcome {
    /// The action that was simulated.
    pub action_id: Uuid,
    /// Whether the simulation predicted success.
    pub predicted_success: bool,
    /// Success probability used in simulation.
    pub success_probability: f64,
    /// Error code if simulation predicted failure.
    pub error_code: Option<String>,
    /// Error detail if simulation predicted failure.
    pub error_detail: Option<String>,
    /// Residual risk assessment.
    pub residual_risk: ResidualRisk,
    /// Recommendation for this action.
    pub recommendation: SimulationRecommendation,
    /// Simulation mode used.
    pub mode: SimulationMode,
    /// Timestamp of simulation.
    pub simulated_at: DateTime<Utc>,
}

impl SimulationOutcome {
    /// Create a successful simulation outcome.
    pub fn success(action: &CompensationAction, probability: f64, mode: SimulationMode) -> Self {
        let effect_class = feasibility_to_effect_class(action.feasibility);
        let residual_risk = ResidualRisk::from_probability(probability, effect_class);
        let recommendation = SimulationRecommendation::from_probability_and_feasibility(
            probability,
            action.feasibility,
        );

        Self {
            action_id: action.id,
            predicted_success: true,
            success_probability: probability,
            error_code: None,
            error_detail: None,
            residual_risk,
            recommendation,
            mode,
            simulated_at: Utc::now(),
        }
    }

    /// Create a failed simulation outcome.
    pub fn failure(
        action: &CompensationAction,
        error_code: &str,
        error_detail: Option<String>,
        mode: SimulationMode,
    ) -> Self {
        let effect_class = feasibility_to_effect_class(action.feasibility);
        let residual_risk = ResidualRisk::from_probability(0.0, effect_class);
        let recommendation =
            SimulationRecommendation::from_probability_and_feasibility(0.0, action.feasibility);

        Self {
            action_id: action.id,
            predicted_success: false,
            success_probability: 0.0,
            error_code: Some(error_code.to_string()),
            error_detail,
            residual_risk,
            recommendation,
            mode,
            simulated_at: Utc::now(),
        }
    }

    /// Create a successful outcome with deterministic success probability of 1.0.
    pub fn deterministic_success(action: &CompensationAction) -> Self {
        Self::success(action, 1.0, SimulationMode::Deterministic)
    }

    /// Create a failed outcome for an invalid strategy/feasibility combo.
    pub fn invalid_combo(
        action: &CompensationAction,
        error_code: &str,
        error_detail: &str,
    ) -> Self {
        Self::failure(
            action,
            error_code,
            Some(error_detail.to_string()),
            SimulationMode::Deterministic,
        )
    }
}

/// Aggregated report from a simulation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationReport {
    /// Total actions simulated.
    pub total_actions: usize,
    /// Actions predicted to succeed.
    pub successful_count: usize,
    /// Actions predicted to fail.
    pub failed_count: usize,
    /// Individual outcomes.
    pub outcomes: Vec<SimulationOutcome>,
    /// Aggregate residual risk level.
    pub overall_risk: ResidualRiskLevel,
    /// Simulation configuration used.
    pub config: SimulationConfig,
    /// Timestamp when simulation completed.
    pub completed_at: DateTime<Utc>,
}

impl SimulationReport {
    /// Create a new simulation report from outcomes.
    pub fn new(outcomes: Vec<SimulationOutcome>, config: SimulationConfig) -> Self {
        let total_actions = outcomes.len();
        let successful_count = outcomes.iter().filter(|o| o.predicted_success).count();
        let failed_count = total_actions - successful_count;

        // Overall risk is the worst risk among all outcomes
        let overall_risk = outcomes
            .iter()
            .map(|o| o.residual_risk.level)
            .max()
            .unwrap_or(ResidualRiskLevel::Low);

        Self {
            total_actions,
            successful_count,
            failed_count,
            outcomes,
            overall_risk,
            config,
            completed_at: Utc::now(),
        }
    }
}

// =============================================================================
// Mock Executors
// =============================================================================

/// Mock Rollback Executor for simulation.
///
/// **N4-2 scope:** Implements CompensationExecutor but does not mutate external
/// systems. Supports deterministic and stochastic modes with configurable
/// success probabilities.
///
/// In deterministic mode:
/// - Valid Rollback + Automatic combos succeed
/// - All other combos fail with appropriate error code
///
/// In stochastic mode:
/// - Success is determined by probability based on effect class
/// - Uses SimulationProbabilities for success rates
#[derive(Clone)]
pub struct MockRollbackExecutor {
    config: SimulationConfig,
}

impl MockRollbackExecutor {
    /// Create a new MockRollbackExecutor with the given config.
    pub fn new(config: SimulationConfig) -> Self {
        Self { config }
    }

    /// Create with default deterministic config.
    pub fn deterministic() -> Self {
        Self::new(SimulationConfig::deterministic())
    }

    /// Create with stochastic config and seed.
    pub fn stochastic(seed: u64) -> Self {
        Self::new(SimulationConfig::stochastic_seed(seed))
    }

    /// Execute simulation logic.
    async fn simulate(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        // Strategy gate: only Rollback is supported
        if action.strategy_type != StrategyType::Rollback {
            return Ok(ExecutionResult::failure(
                "Unsupported strategy type for MockRollbackExecutor",
                "MOCK_UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "MockRollbackExecutor only supports Rollback, got {:?}",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only Automatic is supported
        if action.feasibility != CompensationFeasibility::Automatic {
            return Ok(ExecutionResult::failure(
                "Unsupported feasibility for MockRollbackExecutor",
                "MOCK_UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "MockRollbackExecutor only supports Automatic feasibility, got {:?}",
                    action.feasibility
                )),
            ));
        }

        match self.config.mode {
            SimulationMode::Deterministic => Ok(ExecutionResult::success(
                "MockRollbackExecutor: deterministic success",
            )),
            SimulationMode::Stochastic => {
                let probabilities = self.config.probabilities();
                let success_prob = probabilities.s1_internal_reversible;
                let rand_val = random_value(self.config.seed);

                if rand_val < success_prob {
                    Ok(ExecutionResult::success(
                        "MockRollbackExecutor: stochastic success",
                    ))
                } else {
                    Ok(ExecutionResult::failure(
                        "MockRollbackExecutor: stochastic failure",
                        "MOCK_STOCHASTIC_FAILURE",
                        Some(format!(
                            "Random value {} >= success probability {}",
                            rand_val, success_prob
                        )),
                    ))
                }
            }
        }
    }
}

impl CompensationExecutor for MockRollbackExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.simulate(action).await
    }
}

/// Mock CounterAction Executor for simulation.
///
/// **N4-2 scope:** Implements CompensationExecutor but does not mutate external
/// systems. Supports deterministic and stochastic modes.
#[derive(Clone)]
pub struct MockCounterActionExecutor {
    config: SimulationConfig,
}

impl MockCounterActionExecutor {
    /// Create a new MockCounterActionExecutor with the given config.
    pub fn new(config: SimulationConfig) -> Self {
        Self { config }
    }

    /// Create with default deterministic config.
    pub fn deterministic() -> Self {
        Self::new(SimulationConfig::deterministic())
    }

    /// Create with stochastic config and seed.
    pub fn stochastic(seed: u64) -> Self {
        Self::new(SimulationConfig::stochastic_seed(seed))
    }

    /// Execute simulation logic.
    async fn simulate(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        // Strategy gate: only CounterAction is supported
        if action.strategy_type != StrategyType::CounterAction {
            return Ok(ExecutionResult::failure(
                "Unsupported strategy type for MockCounterActionExecutor",
                "MOCK_UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "MockCounterActionExecutor only supports CounterAction, got {:?}",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only SemiAutomatic is supported
        if action.feasibility != CompensationFeasibility::SemiAutomatic {
            return Ok(ExecutionResult::failure(
                "Unsupported feasibility for MockCounterActionExecutor",
                "MOCK_UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "MockCounterActionExecutor only supports SemiAutomatic feasibility, got {:?}",
                    action.feasibility
                )),
            ));
        }

        match self.config.mode {
            SimulationMode::Deterministic => Ok(ExecutionResult::success(
                "MockCounterActionExecutor: deterministic success",
            )),
            SimulationMode::Stochastic => {
                let probabilities = self.config.probabilities();
                let success_prob = probabilities.s2_external_reversible;
                let rand_val = random_value(self.config.seed);

                if rand_val < success_prob {
                    Ok(ExecutionResult::success(
                        "MockCounterActionExecutor: stochastic success",
                    ))
                } else {
                    Ok(ExecutionResult::failure(
                        "MockCounterActionExecutor: stochastic failure",
                        "MOCK_STOCHASTIC_FAILURE",
                        Some(format!(
                            "Random value {} >= success probability {}",
                            rand_val, success_prob
                        )),
                    ))
                }
            }
        }
    }
}

impl CompensationExecutor for MockCounterActionExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.simulate(action).await
    }
}

/// Mock FollowupNotice Executor for simulation.
#[derive(Clone)]
pub struct MockFollowupNoticeExecutor {
    config: SimulationConfig,
}

impl MockFollowupNoticeExecutor {
    /// Create a new MockFollowupNoticeExecutor with the given config.
    pub fn new(config: SimulationConfig) -> Self {
        Self { config }
    }

    /// Create with default deterministic config.
    pub fn deterministic() -> Self {
        Self::new(SimulationConfig::deterministic())
    }

    /// Create with stochastic config and seed.
    pub fn stochastic(seed: u64) -> Self {
        Self::new(SimulationConfig::stochastic_seed(seed))
    }

    /// Execute simulation logic.
    async fn simulate(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        // Strategy gate: only FollowupNotice is supported
        if action.strategy_type != StrategyType::FollowupNotice {
            return Ok(ExecutionResult::failure(
                "Unsupported strategy type for MockFollowupNoticeExecutor",
                "MOCK_UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "MockFollowupNoticeExecutor only supports FollowupNotice, got {:?}",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only ManualOnly is supported
        if action.feasibility != CompensationFeasibility::ManualOnly {
            return Ok(ExecutionResult::failure(
                "Unsupported feasibility for MockFollowupNoticeExecutor",
                "MOCK_UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "MockFollowupNoticeExecutor only supports ManualOnly feasibility, got {:?}",
                    action.feasibility
                )),
            ));
        }

        match self.config.mode {
            SimulationMode::Deterministic => Ok(ExecutionResult::success(
                "MockFollowupNoticeExecutor: deterministic success",
            )),
            SimulationMode::Stochastic => {
                let probabilities = self.config.probabilities();
                let success_prob = probabilities.s3_external_partially_reversible;
                let rand_val = random_value(self.config.seed);

                if rand_val < success_prob {
                    Ok(ExecutionResult::success(
                        "MockFollowupNoticeExecutor: stochastic success",
                    ))
                } else {
                    Ok(ExecutionResult::failure(
                        "MockFollowupNoticeExecutor: stochastic failure",
                        "MOCK_STOCHASTIC_FAILURE",
                        Some(format!(
                            "Random value {} >= success probability {}",
                            rand_val, success_prob
                        )),
                    ))
                }
            }
        }
    }
}

impl CompensationExecutor for MockFollowupNoticeExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.simulate(action).await
    }
}

/// Mock Escalation Executor for simulation.
#[derive(Clone)]
pub struct MockEscalationExecutor {
    config: SimulationConfig,
}

impl MockEscalationExecutor {
    /// Create a new MockEscalationExecutor with the given config.
    pub fn new(config: SimulationConfig) -> Self {
        Self { config }
    }

    /// Create with default deterministic config.
    pub fn deterministic() -> Self {
        Self::new(SimulationConfig::deterministic())
    }

    /// Create with stochastic config and seed.
    pub fn stochastic(seed: u64) -> Self {
        Self::new(SimulationConfig::stochastic_seed(seed))
    }

    /// Execute simulation logic.
    async fn simulate(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        // Strategy gate: only Escalation is supported
        if action.strategy_type != StrategyType::Escalation {
            return Ok(ExecutionResult::failure(
                "Unsupported strategy type for MockEscalationExecutor",
                "MOCK_UNSUPPORTED_STRATEGY_TYPE",
                Some(format!(
                    "MockEscalationExecutor only supports Escalation, got {:?}",
                    action.strategy_type
                )),
            ));
        }

        // Feasibility gate: only NotPossible is supported
        if action.feasibility != CompensationFeasibility::NotPossible {
            return Ok(ExecutionResult::failure(
                "Unsupported feasibility for MockEscalationExecutor",
                "MOCK_UNSUPPORTED_FEASIBILITY",
                Some(format!(
                    "MockEscalationExecutor only supports NotPossible feasibility, got {:?}",
                    action.feasibility
                )),
            ));
        }

        match self.config.mode {
            SimulationMode::Deterministic => Ok(ExecutionResult::success(
                "MockEscalationExecutor: deterministic success",
            )),
            SimulationMode::Stochastic => {
                let probabilities = self.config.probabilities();
                let success_prob = probabilities.s4_irreversible;
                let rand_val = random_value(self.config.seed);

                if rand_val < success_prob {
                    Ok(ExecutionResult::success(
                        "MockEscalationExecutor: stochastic success",
                    ))
                } else {
                    Ok(ExecutionResult::failure(
                        "MockEscalationExecutor: stochastic failure",
                        "MOCK_STOCHASTIC_FAILURE",
                        Some(format!(
                            "Random value {} >= success probability {}",
                            rand_val, success_prob
                        )),
                    ))
                }
            }
        }
    }
}

impl CompensationExecutor for MockEscalationExecutor {
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        self.simulate(action).await
    }
}

// =============================================================================
// CompensationSimulator Orchestrator (N4-3)
// =============================================================================

/// Enum wrapping all mock executors for static dispatch.
///
/// Since CompensationExecutor has async methods (not dyn compatible),
/// we use an enum to select the appropriate executor at runtime.
#[derive(Clone)]
enum MockExecutor {
    Rollback(MockRollbackExecutor),
    CounterAction(MockCounterActionExecutor),
    FollowupNotice(MockFollowupNoticeExecutor),
    Escalation(MockEscalationExecutor),
}

impl MockExecutor {
    /// Execute an action through this executor.
    async fn execute(
        &self,
        action: &CompensationAction,
    ) -> Result<ExecutionResult, IntentRebaseError> {
        match self {
            MockExecutor::Rollback(executor) => executor.execute(action).await,
            MockExecutor::CounterAction(executor) => executor.execute(action).await,
            MockExecutor::FollowupNotice(executor) => executor.execute(action).await,
            MockExecutor::Escalation(executor) => executor.execute(action).await,
        }
    }
}

/// Orchestrator for compensation simulation.
///
/// Coordinates planning and execution of compensation simulations,
/// dispatching actions to appropriate mock executors based on strategy type.
///
/// **N4-3 scope:** Takes pre-planned actions or plans from side effects,
/// runs them through mock executors, and aggregates results into a SimulationReport.
/// Does NOT integrate with repositories - caller provides side effects directly.
pub struct CompensationSimulator {
    config: SimulationConfig,
    planner: BoundedCompensationPlanner,
    rollback_executor: MockRollbackExecutor,
    counter_action_executor: MockCounterActionExecutor,
    followup_notice_executor: MockFollowupNoticeExecutor,
    escalation_executor: MockEscalationExecutor,
}

impl CompensationSimulator {
    /// Create a new CompensationSimulator with the given config.
    pub fn new(config: SimulationConfig) -> Self {
        let planner = BoundedCompensationPlanner::new();
        let rollback_executor = MockRollbackExecutor::new(config.clone());
        let counter_action_executor = MockCounterActionExecutor::new(config.clone());
        let followup_notice_executor = MockFollowupNoticeExecutor::new(config.clone());
        let escalation_executor = MockEscalationExecutor::new(config.clone());

        Self {
            config,
            planner,
            rollback_executor,
            counter_action_executor,
            followup_notice_executor,
            escalation_executor,
        }
    }

    /// Create with default deterministic config.
    pub fn deterministic() -> Self {
        Self::new(SimulationConfig::deterministic())
    }

    /// Create with stochastic config and seed.
    pub fn stochastic(seed: u64) -> Self {
        Self::new(SimulationConfig::stochastic_seed(seed))
    }

    /// Create with custom config.
    pub fn with_config(config: SimulationConfig) -> Self {
        Self::new(config)
    }

    /// Get the executor for a given strategy type.
    fn executor_for_strategy(&self, strategy: StrategyType) -> MockExecutor {
        match strategy {
            StrategyType::Rollback => MockExecutor::Rollback(self.rollback_executor.clone()),
            StrategyType::CounterAction => {
                MockExecutor::CounterAction(self.counter_action_executor.clone())
            }
            StrategyType::FollowupNotice => {
                MockExecutor::FollowupNotice(self.followup_notice_executor.clone())
            }
            StrategyType::Escalation => MockExecutor::Escalation(self.escalation_executor.clone()),
            StrategyType::Quarantine => {
                // Quarantine not supported - will return failure outcome
                // Use Rollback as placeholder since it will fail immediately
                MockExecutor::Rollback(self.rollback_executor.clone())
            }
        }
    }

    /// Simulate compensation for a list of pre-planned actions.
    ///
    /// Returns a SimulationReport with aggregated outcomes.
    pub async fn simulate_actions(
        &self,
        actions: Vec<CompensationAction>,
    ) -> Result<SimulationReport, IntentRebaseError> {
        let mut outcomes = Vec::with_capacity(actions.len());

        for action in actions {
            let outcome = self.simulate_single_action(&action).await;
            outcomes.push(outcome);
        }

        Ok(SimulationReport::new(outcomes, self.config.clone()))
    }

    /// Simulate compensation for side effects.
    ///
    /// Uses the bounded planner to generate compensation actions from side effects,
    /// then runs them through appropriate mock executors.
    pub async fn simulate_side_effects(
        &self,
        side_effects: &[SideEffect],
        rebase_context: &RebaseContext,
        tenant_id: Uuid,
    ) -> Result<SimulationReport, IntentRebaseError> {
        // Plan compensation actions from side effects
        let actions = self
            .planner
            .plan_from_side_effects(rebase_context, side_effects, tenant_id);

        // Simulate each action
        self.simulate_actions(actions).await
    }

    /// Simulate a single action and return the outcome.
    async fn simulate_single_action(&self, action: &CompensationAction) -> SimulationOutcome {
        // Quarantine strategy is not supported - return failure immediately
        if action.strategy_type == StrategyType::Quarantine {
            return SimulationOutcome::invalid_combo(
                action,
                "MOCK_UNSUPPORTED_STRATEGY_TYPE",
                "Mock executors do not support Quarantine strategy",
            );
        }

        // Select executor based on strategy type
        let executor = self.executor_for_strategy(action.strategy_type);

        // Execute through the mock executor
        let result = executor.execute(action).await;

        match result {
            Ok(execution_result) => {
                if execution_result.success {
                    // Get success probability for this feasibility level
                    let probability = if self.config.mode == SimulationMode::Deterministic {
                        1.0
                    } else {
                        self.config
                            .probabilities()
                            .probability_for_feasibility(action.feasibility)
                    };
                    SimulationOutcome::success(action, probability, self.config.mode)
                } else {
                    let error_code = execution_result
                        .error_code
                        .unwrap_or_else(|| "UNKNOWN".to_string());
                    let error_detail = execution_result
                        .error_detail
                        .unwrap_or_else(|| "Unknown error".to_string());
                    SimulationOutcome::failure(
                        action,
                        &error_code,
                        Some(error_detail),
                        self.config.mode,
                    )
                }
            }
            Err(e) => SimulationOutcome::failure(
                action,
                "EXECUTION_ERROR",
                Some(format!("{:?}", e)),
                self.config.mode,
            ),
        }
    }
}

impl Default for CompensationSimulator {
    fn default() -> Self {
        Self::deterministic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compensation_action::{CompensationFeasibility, RebaseContext, StrategyType};
    use crate::compensation_executor::CompensationExecutor;
    use crate::side_effect::SideEffectClass;

    fn create_test_action(
        strategy_type: StrategyType,
        feasibility: CompensationFeasibility,
    ) -> CompensationAction {
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());
        CompensationAction::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            intent_id,
            rebase_context,
            feasibility,
            strategy_type,
            "Test compensation",
        )
    }

    fn create_test_side_effect(
        tenant_id: Uuid,
        intent_id: Uuid,
        effect_class: SideEffectClass,
    ) -> SideEffect {
        SideEffect::new(
            tenant_id,
            intent_id,
            1,
            effect_class,
            "test_effect",
            "test-target",
        )
    }

    // === Shared Helper Tests ===

    #[test]
    fn test_feasibility_to_effect_class() {
        assert_eq!(
            feasibility_to_effect_class(CompensationFeasibility::Automatic),
            SideEffectClass::S1InternalReversible
        );
        assert_eq!(
            feasibility_to_effect_class(CompensationFeasibility::SemiAutomatic),
            SideEffectClass::S2ExternalReversible
        );
        assert_eq!(
            feasibility_to_effect_class(CompensationFeasibility::ManualOnly),
            SideEffectClass::S3ExternalPartiallyReversible
        );
        assert_eq!(
            feasibility_to_effect_class(CompensationFeasibility::NotPossible),
            SideEffectClass::S4Irreversible
        );
    }

    #[test]
    fn test_random_value_deterministic() {
        let v1 = random_value(Some(42));
        let v2 = random_value(Some(42));
        assert_eq!(v1, v2, "Same seed should produce same value");
    }

    #[test]
    fn test_random_value_different_seeds() {
        let v1 = random_value(Some(42));
        let v2 = random_value(Some(123));
        assert_ne!(v1, v2, "Different seeds should produce different values");
    }

    // === Deterministic Mode Tests ===

    #[tokio::test]
    async fn test_mock_rollback_deterministic_success() {
        let executor = MockRollbackExecutor::deterministic();
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

        let result = executor.execute(&action).await.unwrap();

        assert!(
            result.success,
            "Expected success for valid Rollback+Automatic combo"
        );
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_mock_rollback_deterministic_fail_wrong_strategy() {
        let executor = MockRollbackExecutor::deterministic();
        let action = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::Automatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_rollback_deterministic_fail_wrong_feasibility() {
        let executor = MockRollbackExecutor::deterministic();
        let action = create_test_action(
            StrategyType::Rollback,
            CompensationFeasibility::SemiAutomatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_counter_action_deterministic_success() {
        let executor = MockCounterActionExecutor::deterministic();
        let action = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::SemiAutomatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(
            result.success,
            "Expected success for valid CounterAction+SemiAutomatic combo"
        );
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_mock_counter_action_deterministic_fail_wrong_strategy() {
        let executor = MockCounterActionExecutor::deterministic();
        let action = create_test_action(
            StrategyType::Rollback,
            CompensationFeasibility::SemiAutomatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_counter_action_deterministic_fail_wrong_feasibility() {
        let executor = MockCounterActionExecutor::deterministic();
        let action = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::Automatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_followup_notice_deterministic_success() {
        let executor = MockFollowupNoticeExecutor::deterministic();
        let action = create_test_action(
            StrategyType::FollowupNotice,
            CompensationFeasibility::ManualOnly,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(
            result.success,
            "Expected success for valid FollowupNotice+ManualOnly combo"
        );
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_mock_followup_notice_deterministic_fail_wrong_feasibility() {
        let executor = MockFollowupNoticeExecutor::deterministic();
        let action = create_test_action(
            StrategyType::FollowupNotice,
            CompensationFeasibility::Automatic,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_escalation_deterministic_success() {
        let executor = MockEscalationExecutor::deterministic();
        let action = create_test_action(
            StrategyType::Escalation,
            CompensationFeasibility::NotPossible,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(
            result.success,
            "Expected success for valid Escalation+NotPossible combo"
        );
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn test_mock_escalation_deterministic_fail_wrong_feasibility() {
        let executor = MockEscalationExecutor::deterministic();
        let action = create_test_action(
            StrategyType::Escalation,
            CompensationFeasibility::ManualOnly,
        );

        let result = executor.execute(&action).await.unwrap();

        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
        );
    }

    // === Stochastic Mode Tests ===

    #[tokio::test]
    async fn test_mock_rollback_stochastic_with_seed() {
        // Using a seed should produce deterministic results
        let executor = MockRollbackExecutor::stochastic(12345);
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);

        // Run multiple times with same seed - should get same result
        let result1 = executor.execute(&action).await.unwrap();
        let result2 = executor.execute(&action).await.unwrap();

        // Same seed should produce same outcome
        assert_eq!(result1.success, result2.success);
    }

    #[tokio::test]
    async fn test_mock_rollback_stochastic_invalid_strategy_fails() {
        let executor = MockRollbackExecutor::stochastic(12345);
        let action =
            create_test_action(StrategyType::Escalation, CompensationFeasibility::Automatic);

        let result = executor.execute(&action).await.unwrap();

        // Even stochastic mode should fail on invalid strategy
        assert!(!result.success);
        assert_eq!(
            result.error_code,
            Some("MOCK_UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_counter_action_stochastic_with_seed() {
        let executor = MockCounterActionExecutor::stochastic(54321);
        let action = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::SemiAutomatic,
        );

        let result1 = executor.execute(&action).await.unwrap();
        let result2 = executor.execute(&action).await.unwrap();

        assert_eq!(result1.success, result2.success);
    }

    #[tokio::test]
    async fn test_mock_followup_notice_stochastic_with_seed() {
        let executor = MockFollowupNoticeExecutor::stochastic(99999);
        let action = create_test_action(
            StrategyType::FollowupNotice,
            CompensationFeasibility::ManualOnly,
        );

        let result1 = executor.execute(&action).await.unwrap();
        let result2 = executor.execute(&action).await.unwrap();

        assert_eq!(result1.success, result2.success);
    }

    #[tokio::test]
    async fn test_mock_escalation_stochastic_with_seed() {
        let executor = MockEscalationExecutor::stochastic(11111);
        let action = create_test_action(
            StrategyType::Escalation,
            CompensationFeasibility::NotPossible,
        );

        let result1 = executor.execute(&action).await.unwrap();
        let result2 = executor.execute(&action).await.unwrap();

        assert_eq!(result1.success, result2.success);
    }

    // === Default Probability Tests ===

    #[test]
    fn test_default_probabilities() {
        let probs = SimulationProbabilities::default();

        assert_eq!(probs.s1_internal_reversible, 0.95);
        assert_eq!(probs.s2_external_reversible, 0.70);
        assert_eq!(probs.s3_external_partially_reversible, 0.50);
        assert_eq!(probs.s4_irreversible, 0.10);
    }

    #[test]
    fn test_probabilities_for_side_effect_class() {
        let probs = SimulationProbabilities::default();

        assert_eq!(probs.probability_for(SideEffectClass::S0PureRead), 1.0);
        assert_eq!(
            probs.probability_for(SideEffectClass::S1InternalReversible),
            0.95
        );
        assert_eq!(
            probs.probability_for(SideEffectClass::S2ExternalReversible),
            0.70
        );
        assert_eq!(
            probs.probability_for(SideEffectClass::S3ExternalPartiallyReversible),
            0.50
        );
        assert_eq!(probs.probability_for(SideEffectClass::S4Irreversible), 0.10);
    }

    #[test]
    fn test_probabilities_for_feasibility() {
        let probs = SimulationProbabilities::default();

        assert_eq!(
            probs.probability_for_feasibility(CompensationFeasibility::Automatic),
            0.95
        );
        assert_eq!(
            probs.probability_for_feasibility(CompensationFeasibility::SemiAutomatic),
            0.70
        );
        assert_eq!(
            probs.probability_for_feasibility(CompensationFeasibility::ManualOnly),
            0.50
        );
        assert_eq!(
            probs.probability_for_feasibility(CompensationFeasibility::NotPossible),
            0.10
        );
    }

    // === Residual Risk Tests ===

    #[test]
    fn test_residual_risk_level_from_probability() {
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.95),
            ResidualRiskLevel::Low
        );
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.90),
            ResidualRiskLevel::Low
        );
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.51),
            ResidualRiskLevel::Medium
        );
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.50),
            ResidualRiskLevel::Medium
        );
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.49),
            ResidualRiskLevel::High
        );
        assert_eq!(
            ResidualRiskLevel::from_success_probability(0.10),
            ResidualRiskLevel::High
        );
    }

    // === SimulationRecommendation Tests ===

    #[test]
    fn test_recommendation_from_probability_and_feasibility() {
        // High probability + Automatic = ProceedAuto
        assert_eq!(
            SimulationRecommendation::from_probability_and_feasibility(
                0.95,
                CompensationFeasibility::Automatic
            ),
            SimulationRecommendation::ProceedAuto
        );

        // Medium probability + SemiAutomatic = ProceedManual
        assert_eq!(
            SimulationRecommendation::from_probability_and_feasibility(
                0.70,
                CompensationFeasibility::SemiAutomatic
            ),
            SimulationRecommendation::ProceedManual
        );

        // Low probability + NotPossible = Escalate
        assert_eq!(
            SimulationRecommendation::from_probability_and_feasibility(
                0.10,
                CompensationFeasibility::NotPossible
            ),
            SimulationRecommendation::Escalate
        );

        // Very low probability = DoNotCompensate
        // Must be >= 0.10 and < 0.50 and not NotPossible
        assert_eq!(
            SimulationRecommendation::from_probability_and_feasibility(
                0.20,
                CompensationFeasibility::ManualOnly
            ),
            SimulationRecommendation::DoNotCompensate
        );
    }

    // === SimulationReport Tests ===

    #[test]
    fn test_simulation_report_creation() {
        let action1 =
            create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
        let action2 = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::SemiAutomatic,
        );

        let outcomes = vec![
            SimulationOutcome::success(&action1, 0.95, SimulationMode::Deterministic),
            SimulationOutcome::failure(
                &action2,
                "MOCK_STOCHASTIC_FAILURE",
                None,
                SimulationMode::Deterministic,
            ),
        ];

        let report = SimulationReport::new(outcomes, SimulationConfig::deterministic());

        assert_eq!(report.total_actions, 2);
        assert_eq!(report.successful_count, 1);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.overall_risk, ResidualRiskLevel::High);
    }

    // === Invalid Strategy/Feasibility Path Tests ===

    #[tokio::test]
    async fn test_mock_rollback_invalid_strategy_gate() {
        let executor = MockRollbackExecutor::deterministic();

        // Test all invalid strategy types
        for strategy in [
            StrategyType::CounterAction,
            StrategyType::FollowupNotice,
            StrategyType::Quarantine,
            StrategyType::Escalation,
        ] {
            let action = create_test_action(strategy, CompensationFeasibility::Automatic);
            let result = executor.execute(&action).await.unwrap();
            assert!(
                !result.success,
                "Expected failure for strategy {:?}",
                strategy
            );
            assert_eq!(
                result.error_code,
                Some("MOCK_UNSUPPORTED_STRATEGY_TYPE".to_string())
            );
        }
    }

    #[tokio::test]
    async fn test_mock_rollback_invalid_feasibility_gate() {
        let executor = MockRollbackExecutor::deterministic();

        // Test all invalid feasibility levels
        for feasibility in [
            CompensationFeasibility::SemiAutomatic,
            CompensationFeasibility::ManualOnly,
            CompensationFeasibility::NotPossible,
        ] {
            let action = create_test_action(StrategyType::Rollback, feasibility);
            let result = executor.execute(&action).await.unwrap();
            assert!(
                !result.success,
                "Expected failure for feasibility {:?}",
                feasibility
            );
            assert_eq!(
                result.error_code,
                Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
            );
        }
    }

    #[tokio::test]
    async fn test_mock_counter_action_invalid_feasibility_gate() {
        let executor = MockCounterActionExecutor::deterministic();

        for feasibility in [
            CompensationFeasibility::Automatic,
            CompensationFeasibility::ManualOnly,
            CompensationFeasibility::NotPossible,
        ] {
            let action = create_test_action(StrategyType::CounterAction, feasibility);
            let result = executor.execute(&action).await.unwrap();
            assert!(
                !result.success,
                "Expected failure for feasibility {:?}",
                feasibility
            );
            assert_eq!(
                result.error_code,
                Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
            );
        }
    }

    // === Custom Probability Stochastic Test ===

    #[tokio::test]
    async fn test_custom_probability_stochastic() {
        // Create a config with 100% success probability for S1
        let custom_probs = SimulationProbabilities {
            s1_internal_reversible: 1.0, // Always succeed
            s2_external_reversible: 0.0, // Always fail
            s3_external_partially_reversible: 0.0,
            s4_irreversible: 0.0,
        };
        let config = SimulationConfig::stochastic(42, custom_probs);
        let simulator = CompensationSimulator::with_config(config);

        // Simulate with S1 - should always succeed
        let s1_action =
            create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
        let report = simulator.simulate_actions(vec![s1_action]).await.unwrap();

        assert_eq!(report.total_actions, 1);
        assert_eq!(report.successful_count, 1);
        assert_eq!(report.failed_count, 0);
        assert!(report.outcomes[0].predicted_success);

        // Simulate with S2 - should always fail
        let s2_action = create_test_action(
            StrategyType::CounterAction,
            CompensationFeasibility::SemiAutomatic,
        );
        let report2 = simulator.simulate_actions(vec![s2_action]).await.unwrap();

        assert_eq!(report2.total_actions, 1);
        assert_eq!(report2.successful_count, 0);
        assert_eq!(report2.failed_count, 1);
        assert!(!report2.outcomes[0].predicted_success);
    }

    // === CompensationSimulator Tests ===

    #[tokio::test]
    async fn test_simulator_deterministic_mixed_actions() {
        let simulator = CompensationSimulator::deterministic();

        let actions = vec![
            create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic),
            create_test_action(
                StrategyType::CounterAction,
                CompensationFeasibility::SemiAutomatic,
            ),
            create_test_action(
                StrategyType::FollowupNotice,
                CompensationFeasibility::ManualOnly,
            ),
            create_test_action(
                StrategyType::Escalation,
                CompensationFeasibility::NotPossible,
            ),
        ];

        let report = simulator.simulate_actions(actions).await.unwrap();

        // All valid combos should succeed in deterministic mode
        assert_eq!(report.total_actions, 4);
        assert_eq!(report.successful_count, 4);
        assert_eq!(report.failed_count, 0);
        assert_eq!(report.overall_risk, ResidualRiskLevel::Low);
    }

    #[tokio::test]
    async fn test_simulator_deterministic_invalid_combo_fails() {
        let simulator = CompensationSimulator::deterministic();

        // Invalid combo: Rollback + SemiAutomatic
        let actions = vec![create_test_action(
            StrategyType::Rollback,
            CompensationFeasibility::SemiAutomatic,
        )];

        let report = simulator.simulate_actions(actions).await.unwrap();

        assert_eq!(report.total_actions, 1);
        assert_eq!(report.successful_count, 0);
        assert_eq!(report.failed_count, 1);
        assert!(!report.outcomes[0].predicted_success);
    }

    #[tokio::test]
    async fn test_simulator_from_side_effects() {
        let simulator = CompensationSimulator::deterministic();
        let tenant_id = Uuid::new_v4();
        let intent_id = Uuid::new_v4();
        let rebase_context = RebaseContext::new(intent_id, 1, 2, Uuid::new_v4());

        let side_effects = vec![
            create_test_side_effect(tenant_id, intent_id, SideEffectClass::S0PureRead), // S0: no action
            create_test_side_effect(tenant_id, intent_id, SideEffectClass::S1InternalReversible),
            create_test_side_effect(tenant_id, intent_id, SideEffectClass::S2ExternalReversible),
            create_test_side_effect(
                tenant_id,
                intent_id,
                SideEffectClass::S3ExternalPartiallyReversible,
            ),
            create_test_side_effect(tenant_id, intent_id, SideEffectClass::S4Irreversible),
        ];

        let report = simulator
            .simulate_side_effects(&side_effects, &rebase_context, tenant_id)
            .await
            .unwrap();

        // S0 produces no action, so 4 actions instead of 5
        assert_eq!(report.total_actions, 4);
        assert_eq!(report.successful_count, 4);
        assert_eq!(report.failed_count, 0);
    }

    #[tokio::test]
    async fn test_simulator_quarantine_strategy_fails() {
        let simulator = CompensationSimulator::deterministic();

        // Quarantine is not supported
        let actions = vec![create_test_action(
            StrategyType::Quarantine,
            CompensationFeasibility::ManualOnly,
        )];

        let report = simulator.simulate_actions(actions).await.unwrap();

        assert_eq!(report.total_actions, 1);
        assert_eq!(report.successful_count, 0);
        assert_eq!(report.failed_count, 1);
        assert_eq!(
            report.outcomes[0].error_code,
            Some("MOCK_UNSUPPORTED_STRATEGY_TYPE".to_string())
        );
    }

    // === Serde Compatibility Tests ===

    #[test]
    fn test_simulation_mode_serde() {
        let modes = vec![
            (SimulationMode::Deterministic, "deterministic"),
            (SimulationMode::Stochastic, "stochastic"),
        ];

        for (mode, expected_str) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert!(json.contains(expected_str));

            let deserialized: SimulationMode = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, mode);
        }
    }

    #[test]
    fn test_residual_risk_level_serde() {
        let levels = vec![
            (ResidualRiskLevel::Low, "low"),
            (ResidualRiskLevel::Medium, "medium"),
            (ResidualRiskLevel::High, "high"),
        ];

        for (level, expected_str) in levels {
            let json = serde_json::to_string(&level).unwrap();
            assert!(json.contains(expected_str));

            let deserialized: ResidualRiskLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, level);
        }
    }

    #[test]
    fn test_simulation_config_serde() {
        let config =
            SimulationConfig::stochastic_with_probabilities(SimulationProbabilities::default());
        let json = serde_json::to_string(&config).unwrap();

        let deserialized: SimulationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mode, SimulationMode::Stochastic);
    }

    #[test]
    fn test_simulation_outcome_serde() {
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
        let outcome = SimulationOutcome::success(&action, 0.95, SimulationMode::Deterministic);

        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: SimulationOutcome = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.action_id, outcome.action_id);
        assert_eq!(deserialized.predicted_success, outcome.predicted_success);
        assert_eq!(
            deserialized.success_probability,
            outcome.success_probability
        );
    }

    // === SimulationOutcome Tests ===

    #[test]
    fn test_simulation_outcome_deterministic_success() {
        let action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
        let outcome = SimulationOutcome::deterministic_success(&action);

        assert!(outcome.predicted_success);
        assert_eq!(outcome.success_probability, 1.0);
        assert_eq!(outcome.mode, SimulationMode::Deterministic);
        assert!(outcome.error_code.is_none());
    }

    #[test]
    fn test_simulation_outcome_invalid_combo() {
        let action = create_test_action(
            StrategyType::Rollback,
            CompensationFeasibility::SemiAutomatic,
        );
        let outcome = SimulationOutcome::invalid_combo(
            &action,
            "MOCK_UNSUPPORTED_FEASIBILITY",
            "Test error detail",
        );

        assert!(!outcome.predicted_success);
        assert_eq!(outcome.success_probability, 0.0);
        assert_eq!(
            outcome.error_code,
            Some("MOCK_UNSUPPORTED_FEASIBILITY".to_string())
        );
        assert_eq!(outcome.error_detail, Some("Test error detail".to_string()));
    }

    // === ResidualRisk From Feasibility Test ===

    #[test]
    fn test_residual_risk_from_feasibility() {
        let risk = ResidualRisk::from_probability_and_feasibility(
            0.95,
            CompensationFeasibility::Automatic,
        );

        assert_eq!(risk.level, ResidualRiskLevel::Low);
        assert_eq!(risk.success_probability, 0.95);
        assert!(risk.description.contains("S1"));
    }
}
