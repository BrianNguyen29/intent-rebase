//! Shared test helpers for intent-api handler tests.
//!
//! Phase 3 bounded slice: Contains canonical test helper functions used across
//! multiple handler test modules to reduce duplication.
//!
//! # Included Helpers
//!
//! - `create_test_service()`: Creates minimal AppState for handler tests
//! - `create_test_service_with_forensic_config()`: AppState with forensic archive config
//! - `create_test_rls_claims()`: Creates RlsTenantClaims for JWT auth testing
//! - `create_test_optional_rls_claims()`: Creates OptionalRlsTenantClaims wrapper
//! - `create_test_payload()`: Creates IntentPayload with default "Test intent" summary
//! - `create_test_payload_with_params()`: Creates IntentPayload with custom summary and in_scope

#[cfg(test)]
use std::time::Instant;

#[cfg(test)]
use uuid::Uuid;

#[cfg(all(test, feature = "jwt-auth"))]
use crate::auth::Claims;

#[cfg(all(test, feature = "jwt-auth"))]
use crate::auth::RlsTenantClaims;

#[cfg(all(test, feature = "jwt-auth"))]
use crate::auth::OptionalRlsTenantClaims;

#[cfg(test)]
use compensation_service::{
    CompensationActionService, InMemoryCompensationActionRepository,
    InMemoryOrchestrationRunRepository, InMemorySideEffectRepository, OrchestrationRuntime,
    SideEffectService,
};

#[cfg(test)]
use forensic_service::{
    ForensicBundleService, InMemoryBundleRepository, InMemoryBundleStorage,
    InMemoryForensicArchiveGenerator, InMemoryForensicDataCollector,
    InMemoryForensicVerificationService,
};

#[cfg(test)]
use graph_service::{GraphService, InMemoryGraphRepository};

#[cfg(test)]
use intent_rebase_types::{
    AcceptanceCriteria, InMemoryAuditRepository, IntentAssumptions, IntentAuthority,
    IntentConstraints, IntentMetadataV1, IntentObjective, IntentPayload, IntentPreferences,
    IntentReferences, IntentScope, RiskTier, Urgency,
};

#[cfg(test)]
use intent_service::{
    InMemoryApprovalRequestRepository, InMemoryCheckpointRepository, InMemoryIntentRepository,
    InMemoryPolicySnapshotRepository, IntentService,
};

#[cfg(test)]
use runtime_adapter::MockAdapter;

#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use intent_rebase_types::EventPublisher;

#[cfg(test)]
use crate::AppState;

#[cfg(test)]
use crate::RebaseOrchestrator;

/// Create minimal AppState for handler tests.
///
/// This is the canonical test service builder used across handler test modules.
/// It creates an in-memory service stack suitable for handler-level testing.
///
/// # Features
///
/// - In-memory intent repository
/// - In-memory graph repository
/// - In-memory checkpoint repository
/// - In-memory audit repository
/// - In-memory approval request repository
/// - In-memory policy snapshot repository
/// - In-memory side effect service
/// - In-memory compensation action service
/// - In-memory orchestration runtime
/// - In-memory forensic services
#[cfg(test)]
pub fn create_test_service() -> AppState {
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo =
        Arc::new(InMemoryAuditRepository::new()) as Arc<dyn intent_rebase_types::AuditRepository>;
    let approval_repo = Arc::new(InMemoryApprovalRequestRepository::new())
        as Arc<dyn intent_service::ApprovalRequestRepository>;
    let policy_snapshot_repo = Arc::new(InMemoryPolicySnapshotRepository::new())
        as Arc<dyn intent_service::PolicySnapshotRepository>;
    let side_effect_repo = Arc::new(InMemorySideEffectRepository::new());
    let side_effect_svc = Arc::new(SideEffectService::new(side_effect_repo));
    let compensation_action_repo = Arc::new(InMemoryCompensationActionRepository::new());
    let compensation_action_svc =
        Arc::new(CompensationActionService::new(compensation_action_repo));
    let orchestration_run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));
    let forensic_svc = Arc::new(InMemoryForensicVerificationService::new())
        as Arc<dyn forensic_service::ForensicVerificationService>;
    let forensic_archive_gen = Arc::new(InMemoryForensicArchiveGenerator::new());
    let forensic_bundle_svc = Arc::new(ForensicBundleService::new(
        Arc::new(InMemoryBundleRepository::new()),
        Arc::new(InMemoryBundleStorage::new("test-bucket")),
        Arc::new(InMemoryForensicDataCollector::new()),
    ));
    AppState {
        service,
        graph_service: graph_svc,
        side_effect_service: side_effect_svc,
        compensation_action_service: compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_service: audit_repo,
        approval_request_repo: approval_repo,
        policy_snapshot_repo,
        event_publisher: None,
        forensic_service: forensic_svc,
        forensic_archive_generator: forensic_archive_gen,
        forensic_bundle_service: forensic_bundle_svc,
        start_time: Instant::now(),
        rls_pool: None,
    }
}

/// Create AppState with a custom event publisher.
#[cfg(test)]
pub fn create_test_service_with_publisher(publisher: Arc<dyn EventPublisher>) -> AppState {
    let mut state = create_test_service();
    state.event_publisher = Some(publisher);
    state
}

/// Create AppState for lib.rs tests with configured forensic archive generator.
///
/// This variant preserves the forensic archive generator configuration used by lib.rs tests:
/// - intent_version_count = 5
/// - artifact_count = 10
/// - audit_event_count = 100
/// - policy_snapshot_count = 3
///
/// Used by lib.rs tests that verify artifact/policy snapshot counts.
#[cfg(test)]
pub fn create_test_service_with_forensic_config() -> AppState {
    let repo = Arc::new(InMemoryIntentRepository::new());
    let graph_repo = Arc::new(InMemoryGraphRepository::new());
    let checkpoint_repo = Arc::new(InMemoryCheckpointRepository::new());
    let graph_svc = Arc::new(GraphService::new(graph_repo));
    let service = Arc::new(IntentService::new(repo));
    let orchestrator = Arc::new(RebaseOrchestrator::new(
        checkpoint_repo,
        graph_svc.clone(),
        Arc::new(MockAdapter::ready()),
    ));
    let audit_repo =
        Arc::new(InMemoryAuditRepository::new()) as Arc<dyn intent_rebase_types::AuditRepository>;
    let approval_repo = Arc::new(InMemoryApprovalRequestRepository::new())
        as Arc<dyn intent_service::ApprovalRequestRepository>;
    let policy_snapshot_repo = Arc::new(InMemoryPolicySnapshotRepository::new())
        as Arc<dyn intent_service::PolicySnapshotRepository>;
    let side_effect_repo = Arc::new(InMemorySideEffectRepository::new());
    let side_effect_svc = Arc::new(SideEffectService::new(side_effect_repo));
    let compensation_action_repo = Arc::new(InMemoryCompensationActionRepository::new());
    let compensation_action_svc =
        Arc::new(CompensationActionService::new(compensation_action_repo));
    let orchestration_run_repo = Arc::new(InMemoryOrchestrationRunRepository::new());
    let orchestration_runtime = Arc::new(OrchestrationRuntime::new(
        compensation_action_svc.clone(),
        orchestration_run_repo,
    ));
    let forensic_svc = Arc::new(InMemoryForensicVerificationService::new())
        as Arc<dyn forensic_service::ForensicVerificationService>;
    let forensic_archive_gen = Arc::new(
        InMemoryForensicArchiveGenerator::new()
            .with_intent_version_count(5)
            .with_artifact_count(10)
            .with_audit_event_count(100)
            .with_policy_snapshot_count(3),
    );
    let forensic_bundle_svc = Arc::new(ForensicBundleService::new(
        Arc::new(InMemoryBundleRepository::new()),
        Arc::new(InMemoryBundleStorage::new("test-bucket")),
        Arc::new(InMemoryForensicDataCollector::new()),
    ));
    AppState {
        service,
        graph_service: graph_svc,
        side_effect_service: side_effect_svc,
        compensation_action_service: compensation_action_svc,
        orchestration_runtime,
        orchestrator,
        audit_service: audit_repo,
        approval_request_repo: approval_repo,
        policy_snapshot_repo,
        event_publisher: None,
        forensic_service: forensic_svc,
        forensic_archive_generator: forensic_archive_gen,
        forensic_bundle_service: forensic_bundle_svc,
        start_time: Instant::now(),
        rls_pool: None,
    }
}

/// Helper to create RlsTenantClaims for testing.
///
/// Creates test JWT claims with the specified tenant_id.
/// Uses `new_unchecked` which is only available in test context.
#[cfg(all(test, feature = "jwt-auth"))]
pub fn create_test_rls_claims(tenant_id: Uuid) -> RlsTenantClaims {
    let claims = Claims {
        sub: "test-user".to_string(),
        tenant_id: tenant_id.to_string(),
        roles: vec!["admin".to_string()],
        exp: 9999999999,
        iat: 0,
    };
    // new_unchecked is #[cfg(test)] so this only works in tests
    RlsTenantClaims::new_unchecked(tenant_id, claims)
}

/// Helper to create OptionalRlsTenantClaims for testing.
///
/// Wraps the result of `create_test_rls_claims` in an Option.
#[cfg(all(test, feature = "jwt-auth"))]
pub fn create_test_optional_rls_claims(tenant_id: Uuid) -> OptionalRlsTenantClaims {
    OptionalRlsTenantClaims(Some(create_test_rls_claims(tenant_id)))
}

/// Create IntentPayload with custom summary and in_scope items.
///
/// This parameterized variant allows tests to preserve their exact summary strings
/// while sharing the common payload structure.
#[cfg(test)]
pub fn create_test_payload_with_params(summary: &str, in_scope: &[&str]) -> IntentPayload {
    IntentPayload {
        objective: IntentObjective {
            summary: summary.to_string(),
            success_statement: "Success".to_string(),
            domain: "testing".to_string(),
        },
        scope: IntentScope {
            in_scope: in_scope.iter().map(|s| s.to_string()).collect(),
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
    }
}

/// Create IntentPayload with default "Test intent" summary and "item1" in_scope.
#[cfg(test)]
pub fn create_test_payload() -> IntentPayload {
    create_test_payload_with_params("Test intent", &["item1"])
}
