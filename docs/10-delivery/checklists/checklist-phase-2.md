# Phase 2 — Runtime-Integrated Rebase Checklist

**Exit Gate:** Phase 2 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 1 exit gate passed.

**Trạng thái:** `IN PROGRESS` — internal groundwork delivered for checkpoint alignment, bounded apply orchestration, and mock-backed runtime wiring; external/runtime-integrated Phase 2 scope remains incomplete.  
**Phase:** Phase 2  
**Target Duration:** 6–10 tuần

---

## 1. Runtime Adapter v1 (Temporal)

```
[x] RuntimeAdapter trait defined and implemented
    Evidence:
    - Code: crates/runtime-adapter/src/lib.rs (RuntimeAdapter trait with 5 methods)
      - get_checkpoints(), send_rebase_signal(), map_intent_to_checkpoint(),
        replay_from_checkpoint(), is_adapter_ready()

[x] RuntimeAdapter trait injection into RebaseOrchestrator - INTERNAL WIRING DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs
    - Field: runtime_adapter: Arc<dyn RuntimeAdapter> in RebaseOrchestrator struct
    - Constructor: RebaseOrchestrator::new() takes Arc<dyn RuntimeAdapter>
    - Test constructor: RebaseOrchestrator::with_mock_adapter() for testing
    - Tests: 5 new tests covering success, signal failure, replay failure, and readiness checks

[x] Internal execution loop: send_rebase_signal wired into proceed path - INTERNAL WIRING DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs
    - Method: send_runtime_rebase_signal()
    - Integration point: proceed path after checkpoint alignment and graph updates
    - Graceful degradation: runtime failures don't block apply outcome
    - Tests: test_runtime_execution_success, test_runtime_signal_failure_graceful_continuation

[x] Internal execution loop: replay_from_checkpoint wired for aligned checkpoints - INTERNAL WIRING DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs
    - Method: send_runtime_rebase_signal() calls replay_from_checkpoint
    - Only attempted when aligned.checkpoint_id is Some
    - Graceful degradation: replay failure doesn't block apply outcome
    - Tests: test_runtime_replay_failure_graceful_continuation

[x] Runtime readiness check: is_runtime_ready() method - INTERNAL WIRING DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (is_runtime_ready)
    - Tests: test_runtime_ready_check, test_runtime_not_ready_check

[ ] TemporalAdapter: get_checkpoints() implemented
    Evidence:
    - PR merged: <link>
    - Code: runtime-adapter/src/temporal_adapter.rs
    - Tests: checkpoint mapping tests pass

[ ] TemporalAdapter: send_rebase_signal(workflow_id, directive) implemented
    Evidence:
    - PR merged: <link>
    - Integration test: signal sent and received

[ ] TemporalAdapter: map_intent_to_checkpoint(intent: IntentRef) implemented
    Evidence:
    - PR merged: <link>
    - Code: runtime-adapter/src/temporal_adapter.rs
    - Tests: intent-to-checkpoint mapping tests pass

[ ] TemporalAdapter: replay_from_checkpoint(workflow_id, checkpoint) implemented
    Evidence:
    - PR merged: <link>
    - Tests: replay test passes

[x] Fallback adapter (mock/no-op) for non-Temporal runtimes
    Evidence:
    - Code: crates/runtime-adapter/src/lib.rs (MockAdapter, 13 passing tests)
```

---

## 2. Checkpoint Mapping

```
[x] Checkpoint data model (checkpoint_id, intent_version, timestamp, workflow_state)
    Evidence:
    - Code: crates/intent-rebase-types/src/checkpoint.rs (Checkpoint domain type)
    - Schema: infrastructure/migrations/007_create_checkpoints.sql

[x] Checkpoint storage (PostgreSQL) - PARTIAL
    Evidence:
    - Migration: infrastructure/migrations/007_create_checkpoints.sql
    - Code: crates/intent-service/src/checkpoint_repo.rs (SqlxCheckpointRepository)
    - Tests: checkpoint enum conversion tests pass (4 tests in sqlx_checkpoint_tests)

[x] Checkpoint service/query layer - PARTIAL GROUNDWORK
    Evidence:
    - Code: crates/intent-service/src/lib.rs (CheckpointService with 12 methods)
    - Internal API: create_checkpoint, get_checkpoint, list_by_workflow, list_by_intent,
      get_latest_checkpoint, get_checkpoint_for_version, activate/supersede/invalidate_checkpoint,
      run_expiration, list_checkpoints_by_type
    - Tests: 15 checkpoint service tests pass

[x] Tenant resolution seam - PARTIAL GROUNDWORK
    Evidence:
    - Code: crates/intent-service/src/sqlx_repository.rs (TenantResolver trait, DefaultTenantResolver, StaticTenantResolver)
    - SqlxIntentRepository updated to use tenant_resolver instead of Uuid::new_v4() placeholder
    - Exported for internal use: pub use sqlx_repository::TenantResolver

[x] Checkpoint-to-intent-version alignment logic - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/checkpoint_aligner.rs (CheckpointAligner, AlignedCheckpoint)
    - Internal-only: no HTTP endpoint, aligns planner checkpoint candidates to real checkpoint records
    - Outcomes: Aligned, ClosestMatch, NoCheckpointRequired, NoCheckpointFound, MultipleCandidates
    - Tests: 4 alignment tests pass (test_align_class_a_no_checkpoint_needed, test_align_no_checkpoint_found,
      test_align_with_checkpoints, test_alignment_report)

[ ] Checkpoint lifecycle (create on intent update, expire old checkpoints)
    Evidence:
    - CheckpointService::run_expiration implemented
    - Tests: lifecycle tests pass
```

---

## 3. Apply Rebase — Low/Medium Risk

```
[x] Internal low/medium apply pipeline - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (ApplyPipeline, ApplyGuard trait)
    - Class A: No-op, return immediately
    - Class B/C: Auto-proceed with optional notification
    - Class D/E: Blocked, requires manual review
    - Guards: LowMediumGuard (default), HighCriticalGuard (strict mode), StandardGuard
    - Tests: 12 apply pipeline tests pass (guard evaluations, factory methods, custom guard)

[x] RebaseOrchestrator entry point for internal apply - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (RebaseOrchestrator struct)
    - Coordinates: checkpoint alignment, graph state updates, apply pipeline
    - Methods: align_checkpoint, update_graph_state, apply_rebase, plan_and_apply
    - Tests: orchestrator tests cover Class A no-op, Class D/E blocked, Class B proceed,
      no-checkpoint proceed, runtime-not-ready skipped execution, graph state update,
      plan_and_apply, and runtime execution degradation/success paths

[x] RebaseApplyResult exposes runtime_execution_result field - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (RebaseApplyResult.runtime_execution_result,
      RuntimeExecutionResult, RuntimeExecutionStatus)
    - Structured runtime outcome: status, signal_sent, replay_completed, replay_attempted, status_message
    - Status variants: NotApplicable, SkippedNotReady, Degraded, Succeeded, SucceededNoReplay
    - replay_attempted distinguishes "no checkpoint available" from "replay failed"
    - Populated in no-op/blocked/proceed paths
    - Tests: runtime execution success/failure tests pass (test_runtime_execution_success,
      test_runtime_signal_failure_graceful_continuation, test_runtime_replay_failure_graceful_continuation)
    - No-checkpoint proceed path test: test_orchestrator_class_b_proceeds_no_checkpoint

[x] Runtime readiness gating skips execution when adapter is not ready - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (send_runtime_rebase_signal gates on adapter readiness)
    - Structured outcome: RuntimeExecutionStatus::SkippedNotReady distinguishes skipped execution
      from signal/replay failure
    - Rationale cleanup: apply rationale remains decision-focused while runtime detail stays in
      runtime_execution_result.status_message
    - Tests: test_skipped_not_ready_when_adapter_not_ready

[ ] Rebase apply endpoint: POST /api/v1/intents/{id}/rebase-apply
    Evidence:
    - OpenAPI spec updated
    - Code: rebase-service/apply.rs

[ ] Risk classification: low/medium/high/critical
    Evidence:
    - PR merged: <link>
    - Code: rebase-engine/risk_classifier.rs
    - Tests: classification tests pass

[ ] Apply rebase for LOW risk: automatic, no approval required
    Evidence:
    - Code: apply pipeline auto-proceed for low
    - Tests: auto-apply tests pass
    - Integration test: rebase applied without manual approval

[ ] Apply rebase for MEDIUM risk: automatic with notification
    Evidence:
    - Code: apply pipeline proceeds + webhook notification sent
    - Tests: medium-risk apply tests pass
    - Webhook: rebase.applied event sent

[ ] Apply rebase for HIGH/CRITICAL: blocked, requires manual approval
    Evidence:
    - Code: apply pipeline blocks + approval workflow triggered
    - Tests: blocked apply returns 202 Accepted (pending approval)
    - UI: approval queue visible in console

[ ] Rebase apply audit trail (who applied, when, what changed)
    Evidence:
    - Audit event: rebase.applied with full detail
    - Doc: ../../14-governance/01-audit-event-spec.md (updated)
```

---

## 4. Approvals Revalidation

```
[ ] Approval scope canonicalization implemented
    Evidence:
    - PR merged: <link>
    - Doc: ../../13-adrs/07-approval-scope-canonicalization.md
    - Code: approval-service/canonicalization.rs

[ ] Policy snapshot creation (S3-backed immutable record)
    Evidence:
    - PR merged: <link>
    - Code: approval-service/snapshot.rs
    - Schema: approval_scope, policy_snapshot tables

[ ] Approval invalidation on intent change
    Evidence:
    - PR merged: <link>
    - Code: approval-service/invalidation.rs
    - Tests: invalidation tests pass

[ ] Re-approval workflow: queue and notify approvers
    Evidence:
    - PR merged: <link>
    - Code: approval-service/workflow.rs
    - Integration: approval queue in console

[ ] Approval revalidation API: GET /api/v1/approvals/{id}/revalidate
    Evidence:
    - OpenAPI spec updated
    - Tests: revalidation tests pass

[ ] Approval status tracking (pending, approved, rejected, expired)
    Evidence:
    - Code: approval-service/status.rs
    - Tests: status transition tests pass
```

---

## 5. Artifact Invalidation + Quarantine

```
[ ] Artifact invalidation on intent change
    Evidence:
    - PR merged: <link>
    - Code: artifact-service/invalidation.rs
    - Tests: invalidation tests pass

[ ] Artifact quarantine: move to quarantine path in S3
    Evidence:
    - PR merged: <link>
    - Code: artifact-service/quarantine.rs
    - S3 path: artifacts/{tenant}/{intent_id}/v{version}/quarantine/{artifact_id}/

[ ] Artifact quarantine status API: GET /api/v1/artifacts/{id}/quarantine-status
    Evidence:
    - OpenAPI spec updated
    - Tests: quarantine status tests pass

[ ] Artifact release from quarantine (if rebase resolved)
    Evidence:
    - Code: artifact-service/release.rs
    - Tests: release tests pass

[ ] Artifact permanent deletion (if rebase requires discard)
    Evidence:
    - Code: artifact-service/delete.rs
    - Approval required: security-reviewer role
    - Audit: artifact.deleted event with reason
```

---

## 6. Graph Update on Rebase

```
[x] Graph state update orchestration - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/graph_updater.rs (GraphUpdater)
    - State-only mutations: Active→Stale/Invalid, Stale→Active/Archived/Invalid
    - No structural mutations: no create/delete nodes or edges (deferred to Phase 3)
    - Methods: update_node_state_if_affected, mark_artifacts_stale, mark_approvals_stale,
      revalidate_nodes, archive_nodes, get_state_summary
    - Tests: 4 graph updater tests pass (valid/invalid transitions, terminal state, not found)

[ ] Graph nodes updated when intent changes
    Evidence:
    - PR merged: <link>
    - Code: graph-service/rebase_update.rs
    - Tests: graph update tests pass

[ ] Graph edges re-evaluated on intent change
    Evidence:
    - Code: graph-service/edge_reevaluation.rs
    - Tests: edge reeval tests pass

[ ] Orphan detection (nodes no longer reachable from active intent)
    Evidence:
    - Code: graph-service/orphan_detection.rs
    - Tests: orphan detection tests pass
    - Action: quarantine orphaned artifacts
```

---

## 7. Replay Compatibility

```
[ ] Replay API: POST /api/v1/intents/{id}/replay
    Evidence:
    - OpenAPI spec updated
    - Code: rebase-service/replay.rs

[ ] Replay from specific checkpoint supported
    Evidence:
    - PR merged: <link>
    - Code: runtime-adapter/replay.rs
    - Tests: replay from checkpoint tests pass

[ ] Replay with new intent version (intent version override)
    Evidence:
    - Code: rebase-service/replay_override.rs
    - Tests: replay override tests pass

[ ] Replay audit trail (replay initiated by, replay reason)
    Evidence:
    - Audit event: intent.replayed with full metadata
```

---

## 8. Event Streaming (NATS or Kafka)

```
[ ] Event publishing for all rebase-related events
    Evidence:
    - PR merged: <link>
    - Code: event-service/publish.rs
    - Subjects: rebase.signal.>, artifact.>, approval.>

[ ] Event consumers for async processing (checkpoint creation, snapshot)
    Evidence:
    - PR merged: <link>
    - Code: event-service/consumers.rs
    - Consumers: checkpoint-creator, snapshot-creator, notifier

[ ] Event schema versioning (v1, v2 migration path)
    Evidence:
    - Doc: ../../04-api/02-events.md (updated)
    - Migration: v1 → v2 documented

[ ] Dead-letter queue for failed event processing
    Evidence:
    - Code: event-service/dlq.rs
    - Tests: DLQ handling tests pass
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Phase 2 Exit Gate Review Date: ___________
Reviewed By: ___________
Product Owner Sign-off: ___________
Security Sign-off: ___________
Runtime Integration Sign-off: ___________

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 3 — Compensation + Production Hardening](./checklist-phase-3.md)
