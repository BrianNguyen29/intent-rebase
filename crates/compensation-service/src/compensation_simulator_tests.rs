use crate::compensation_action::{
    CompensationAction, CompensationFeasibility, RebaseContext, StrategyType,
};
use crate::compensation_executor::CompensationExecutor;
use crate::compensation_simulator::*;
use crate::side_effect::{SideEffect, SideEffectClass};
use uuid::Uuid;

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
    let action = create_test_action(StrategyType::Escalation, CompensationFeasibility::Automatic);

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
    let action1 = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
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
    let s1_action = create_test_action(StrategyType::Rollback, CompensationFeasibility::Automatic);
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
    let risk =
        ResidualRisk::from_probability_and_feasibility(0.95, CompensationFeasibility::Automatic);

    assert_eq!(risk.level, ResidualRiskLevel::Low);
    assert_eq!(risk.success_probability, 0.95);
    assert!(risk.description.contains("S1"));
}
