//! Database query benchmarks for intent-service
//!
//! This module measures the latency of critical SQLx repository queries
//! using an in-memory fixture store to exercise SQL query paths.
//!
//! Note: This is a benchmark harness only — it verifies the benchmark
//! infrastructure builds and runs. Actual performance targets and production
//! load testing remain outstanding (gated on Phase 5 full completion).
//!
//! Bounded Scope (P5 groundwork):
//! - Intent queries: create_intent_tx, get_intent, get_versions_by_intent, create_version_with_occ
//! - Approval request queries: list_pending_by_intent, list_pending_by_tenant, update_approval_request_status
//! - Policy snapshot queries: list_by_intent, get_latest_by_intent, get_by_intent_version
//!
//! Out of Scope for this slice:
//! - Real PostgreSQL connection pool benchmarks (requires live DB)
//! - Connection pool sizing and tuning
//! - Production load testing (k6/Artillery)
//! - p50/p95/p99 SLA targets (gated on Phase 5 completion)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use intent_rebase_types::{
    AcceptanceCriteria, ActorRef, ChangeChannel, CreateIntentRequest, CreateVersionRequest,
    IntentAssumptions, IntentAuthority, IntentConstraints, IntentMetadataV1, IntentObjective,
    IntentPayload, IntentPreferences, IntentReferences, IntentScope, RiskTier, ScopeDefinition,
    ScopeType, SourceRef, Urgency,
};
use intent_service::{
    ApprovalRequest, ApprovalRequestRepository, ApprovalRequestStatus,
    InMemoryApprovalRequestRepository, InMemoryIntentRepository, InMemoryPolicySnapshotRepository,
    IntentRepository, PolicySnapshotRepository,
};
use std::sync::Arc;
use uuid::Uuid;

/// Helper: create a test intent payload
fn create_test_payload() -> IntentPayload {
    IntentPayload {
        objective: IntentObjective {
            summary: "Benchmark intent".to_string(),
            success_statement: "Benchmark success".to_string(),
            domain: "bench".to_string(),
        },
        scope: IntentScope {
            in_scope: vec!["item1".to_string(), "item2".to_string()],
            out_of_scope: vec!["excluded1".to_string()],
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
            confidence: 0.9,
        },
    }
}

/// Helper: create a test intent create request
fn create_test_request() -> CreateIntentRequest {
    CreateIntentRequest {
        tenant_id: None,
        workflow_id: Uuid::new_v4(),
        source_refs: vec![SourceRef {
            ref_type: "spec".to_string(),
            id: "spec://bench".to_string(),
        }],
        payload: create_test_payload(),
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "bench".to_string(),
        },
        tags: vec!["bench".to_string()],
    }
}

/// Helper: create a test intent create version request
fn create_version_request() -> CreateVersionRequest {
    CreateVersionRequest {
        created_by: ActorRef {
            actor_type: "user".to_string(),
            actor_id: "bench".to_string(),
        },
        change_reason: "benchmark version".to_string(),
        change_channel: ChangeChannel::UserEdit,
        payload: create_test_payload(),
    }
}

// =============================================================================
// Benchmark: Intent CRUD operations
// =============================================================================

fn bench_intent_create_tx(c: &mut Criterion) {
    // Runtime must be created OUTSIDE the benchmark loop — benchmark measures
    // repo.create_intent_tx, not runtime construction overhead.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryIntentRepository::new());

    c.bench_function("intent_create_tx", |b| {
        b.iter(|| {
            let request = create_test_request();
            let result = repo.create_intent_tx(black_box(request));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_intent_get(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryIntentRepository::new());

    // Seed an intent
    let intent_id = {
        let request = create_test_request();
        let result = rt.block_on(repo.create_intent_tx(request)).unwrap();
        result.intent_id
    };

    c.bench_function("intent_get", |b| {
        b.iter(|| {
            let result = repo.get_intent(black_box(intent_id));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_intent_create_version_occ(c: &mut Criterion) {
    // Runtime must be created OUTSIDE the benchmark loop.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryIntentRepository::new());

    c.bench_function("intent_create_version_occ", |b| {
        // Reseed intent inside iteration so OCC check always succeeds
        // (without reseed, 2nd+ iterations fail with ConcurrencyConflict
        // since current_version != expected_version, measuring error path).
        b.iter(|| {
            let intent_id = {
                let request = create_test_request();
                rt.block_on(repo.create_intent_tx(request))
                    .unwrap()
                    .intent_id
            };
            let request = create_version_request();
            // OCC: expect version 1, row_version 1 (seed creates v1)
            let result =
                repo.create_version_with_occ(black_box(intent_id), black_box(request), 1, 1);
            let _ = rt.block_on(result);
        });
    });
}

fn bench_get_versions_by_intent(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryIntentRepository::new());

    // Seed an intent with 5 versions
    let intent_id = {
        let request = create_test_request();
        let result = rt.block_on(repo.create_intent_tx(request)).unwrap();
        let id = result.intent_id;
        for ver in 1..5 {
            let request = create_version_request();
            let _ = rt.block_on(repo.create_version_with_occ(id, request, ver, ver));
        }
        id
    };

    c.bench_function("intent_get_versions_by_intent_5", |b| {
        b.iter(|| {
            let result = repo.get_versions_by_intent(black_box(intent_id));
            let _ = rt.block_on(result);
        });
    });
}

// =============================================================================
// Benchmark: Approval Request queries
// =============================================================================

fn bench_approval_request_list_pending_by_intent(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryApprovalRequestRepository::new());

    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Seed 10 pending approval requests
    for i in 0..10 {
        let request = ApprovalRequest::new_pending(
            intent_id,
            i,
            i + 1,
            workflow_id,
            tenant_id,
            "bench",
            "user",
            "D",
            "benchmark",
        );
        let _ = rt.block_on(repo.create_approval_request(request));
    }

    c.bench_function("approval_request_list_pending_by_intent_10", |b| {
        b.iter(|| {
            let result = repo.list_pending_by_intent(black_box(intent_id), black_box(tenant_id));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_approval_request_list_pending_by_tenant(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryApprovalRequestRepository::new());

    let tenant_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();

    // Seed 20 pending approval requests across 5 intents
    for _ in 0..20 {
        let intent_id = Uuid::new_v4();
        let request = ApprovalRequest::new_pending(
            intent_id,
            1,
            2,
            workflow_id,
            tenant_id,
            "bench",
            "user",
            "D",
            "benchmark",
        );
        let _ = rt.block_on(repo.create_approval_request(request));
    }

    c.bench_function("approval_request_list_pending_by_tenant_20", |b| {
        b.iter(|| {
            let result = repo.list_pending_by_tenant(black_box(tenant_id));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_approval_request_update_status(c: &mut Criterion) {
    // Runtime must be created OUTSIDE the benchmark loop.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryApprovalRequestRepository::new());

    c.bench_function("approval_request_update_status", |b| {
        // Reseed approval request inside iteration so each measurement
        // updates a Pending request to Approved (not Approved→Approved).
        b.iter(|| {
            let id = {
                let request = ApprovalRequest::new_pending(
                    Uuid::new_v4(),
                    1,
                    2,
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    "bench",
                    "user",
                    "D",
                    "benchmark",
                );
                let id = request.id;
                let _ = rt.block_on(repo.create_approval_request(request));
                id
            };
            let result = repo.update_approval_request_status(
                black_box(id),
                black_box(ApprovalRequestStatus::Approved),
                black_box("bench-approver"),
                black_box(None),
            );
            let _ = rt.block_on(result);
        });
    });
}

// =============================================================================
// Benchmark: Policy Snapshot queries
// =============================================================================

fn bench_policy_snapshot_list_by_intent(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Seed 5 policy snapshots for different versions
    for ver in 1..=5 {
        let snapshot = intent_rebase_types::PolicySnapshot::new(
            tenant_id,
            intent_id,
            ver,
            format!("v{}.0", ver),
            ScopeDefinition::default(),
        );
        let _ = rt.block_on(repo.create_snapshot(snapshot));
    }

    c.bench_function("policy_snapshot_list_by_intent_5", |b| {
        b.iter(|| {
            let result = repo.list_by_intent(black_box(intent_id), black_box(tenant_id));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_policy_snapshot_get_latest(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Seed 3 policy snapshots
    for ver in 1..=3 {
        let snapshot = intent_rebase_types::PolicySnapshot::new(
            tenant_id,
            intent_id,
            ver,
            format!("v{}.0", ver),
            ScopeDefinition {
                scope_type: ScopeType::Partial,
                affected_resources: vec![],
                required_approvers: vec![],
                min_approvals: 1,
            },
        );
        let _ = rt.block_on(repo.create_snapshot(snapshot));
    }

    c.bench_function("policy_snapshot_get_latest_by_intent_3", |b| {
        b.iter(|| {
            let result = repo.get_latest_by_intent(black_box(intent_id), black_box(tenant_id));
            let _ = rt.block_on(result);
        });
    });
}

fn bench_policy_snapshot_get_by_version(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = Arc::new(InMemoryPolicySnapshotRepository::new());

    let intent_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    // Seed 5 policy snapshots
    for ver in 1..=5 {
        let snapshot = intent_rebase_types::PolicySnapshot::new(
            tenant_id,
            intent_id,
            ver,
            format!("v{}.0", ver),
            ScopeDefinition::default(),
        );
        let _ = rt.block_on(repo.create_snapshot(snapshot));
    }

    c.bench_function("policy_snapshot_get_by_intent_version_5", |b| {
        b.iter(|| {
            let result = repo.get_by_intent_version(
                black_box(intent_id),
                black_box(3),
                black_box(tenant_id),
            );
            let _ = rt.block_on(result);
        });
    });
}

// =============================================================================
// Benchmark group
// =============================================================================

criterion_group!(
    benches,
    // Intent CRUD
    bench_intent_create_tx,
    bench_intent_get,
    bench_intent_create_version_occ,
    bench_get_versions_by_intent,
    // Approval requests
    bench_approval_request_list_pending_by_intent,
    bench_approval_request_list_pending_by_tenant,
    bench_approval_request_update_status,
    // Policy snapshots
    bench_policy_snapshot_list_by_intent,
    bench_policy_snapshot_get_latest,
    bench_policy_snapshot_get_by_version,
);
criterion_main!(benches);
