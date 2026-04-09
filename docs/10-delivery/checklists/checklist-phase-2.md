# Phase 2 — Runtime-Integrated Rebase Checklist

**Exit Gate:** Phase 2 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 1 exit gate passed.

**Trạng thái:** `PHASE 2a COMPLETE / PHASE 2b BATCHED DELIVERY IN PROGRESS` — Internal groundwork delivered (checkpoint alignment, bounded apply orchestration, mock-backed runtime wiring). Phase 2b now also includes real TemporalAdapter connection/query/signal/mapping/cooperative replay, bounded external rebase-apply, bounded audit hooks, pending approval queue/read APIs, status-only approve/reject, and canonical public `risk_tier` exposure. Broader runtime-integrated scope remains incomplete and prerequisite-gated.
**Phase:** Phase 2 (split: 2a internal groundwork ✓ | 2b external/integrated pending)  
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

[x] TemporalAdapter: get_checkpoints() implemented - PHASE 2b BATCH 1 DELIVERED
    Evidence:
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::get_checkpoints)
    - Temporal-backed: lists workflow executions via temporalio-client visibility query
    - Feature-gated: `runtime-adapter/temporal`

[x] TemporalAdapter: send_rebase_signal(workflow_id, directive) implemented - PHASE 2b BATCH 1 DELIVERED
    Evidence:
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::send_rebase_signal)
    - Temporal-backed: untyped workflow signal with workflow_id sourced from signal metadata
    - Tests: signal metadata extraction + signal payload serialization tests pass

[x] TemporalAdapter: is_adapter_ready() implemented - PHASE 2b BATCH 1 DELIVERED
    Evidence:
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::is_adapter_ready)
    - Temporal-backed: gRPC health check via temporalio-client health service

[x] TemporalAdapter: map_intent_to_checkpoint(intent: IntentRef) implemented - PHASE 2b BATCH 2 DELIVERED
    Evidence:
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::map_intent_to_checkpoint)
    - Strategy: describe()-based workflow mapping with bounded validation on running workflow state
    - Tests: replay signal/status helper tests pass under runtime-adapter temporal feature

[x] TemporalAdapter: replay_from_checkpoint(workflow_id, checkpoint) implemented - PHASE 2b BATCH 2 DELIVERED
    Evidence:
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::replay_from_checkpoint)
    - Semantics: cooperative replay via Temporal signal carrying checkpoint metadata; native reset remains deferred
    - Tests: replay signal metadata tests pass under runtime-adapter temporal feature

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

[x] Checkpoint expiration (expire old checkpoints) - PARTIAL
    Evidence:
    - CheckpointService::run_expiration implemented (crates/intent-service/src/lib.rs, lines 769-771)
    - Tests: checkpoint_service_tests::test_run_expiration passes

[ ] Checkpoint creation on intent update (automatic)
    Evidence:
    - Not implemented - deferred to Phase 3 event-driven architecture
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

[x] RebaseApplySummary audit summary via audit_summary() method - INTERNAL GROUNDWORK DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (RebaseApplyResult::audit_summary method,
      RebaseApplySummary struct)
    - Derived summary aggregates: outcome, runtime_status, checkpoint_outcome, checkpoint_id,
      graph_updates_applied, graph_updates_failed, notification_required, rationale
    - No new persistent fields added to RebaseApplyResult (method-derived, preserves shape)
    - Tests: test_audit_summary_class_a_noop, test_audit_summary_class_d_blocked,
      test_audit_summary_proceed_success, test_audit_summary_no_checkpoint,
      test_audit_summary_degraded, test_audit_summary_skipped_not_ready,
      test_audit_summary_with_graph_updates

[x] Rebase apply endpoint: POST /intents/{id}/rebase-apply - BOUNDED EXTERNAL SLICE DELIVERED
    Evidence:
    - Code: crates/intent-api/src/lib.rs (rebase_apply handler + route registration)
    - OpenAPI: docs/04-api/openapi.yaml (/intents/{intent_id}/rebase-apply)
    - Behavior: Class A/B/C wired to existing bounded apply path; Class D/E return 202/manual review required

[x] Rebase apply audit trail for external apply - PHASE 2b BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/intent-rebase-types/src/audit_repo.rs (AuditRepository trait, InMemoryAuditRepository, SqlxAuditRepository)
    - Code: crates/intent-rebase-types/src/audit.rs (RebaseApplyAuditPayload, RebaseApplyBlockedAuditPayload, AuditEventType::RebaseApplyBlocked)
    - Audit events: RebaseApplied (all outcomes), RebaseApplyBlocked (D/E blocked only)
    - Best-effort actor attribution: fallback external-api/unknown
    - SQL-backed SqlxAuditRepository wired to production via build_router_with_sql_audit_and_approval bootstrap helper
    - Enum conversion: audit_event_type_to_string/from_string for PostgreSQL enum serialization
    - Tests: 5 audit_repo tests pass, 2 blocked audit tests pass, sqlx_audit_tests pass (enum conversions)

[x] approval_requests schema + bounded repository contract for blocked D/E external apply - PHASE 2b BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/intent-service/src/approval_request_repo.rs (ApprovalRequestRepository trait, InMemoryApprovalRequestRepository, SqlxApprovalRequestRepository, ApprovalRequest)
    - Code: infrastructure/migrations/008_create_approval_requests.sql
    - Schema: approval_requests table with pending/approved/rejected/expired/cancelled fields designed for future workflow expansion
    - SQL-backed SqlxApprovalRequestRepository wired to production via build_router_with_sql_audit_and_approval bootstrap helper
    - Enum conversion: approval_request_status_to_string/from_string for PostgreSQL enum serialization
    - Tests: create/get/list/update approval_request_repo tests pass, sqlx_approval_request_tests pass (enum conversions)

[x] Approval queue read/query API + status-only approve/reject with audit events - PHASE 2b BATCH 2 DELIVERED
    Evidence:
    - Code: crates/intent-api/src/lib.rs (list_pending_approval_requests, approve_approval_request, reject_approval_request handlers)
    - Code: crates/intent-service/src/approval_request_repo.rs (update_approval_request_status method added)
    - Code: crates/intent-rebase-types/src/audit.rs (ApprovalGrantedAuditPayload, ApprovalRevokedAuditPayload)
    - Code: crates/intent-rebase-types/src/audit_repo.rs (record_approval_granted, record_approval_revoked helper methods)
    - OpenAPI: docs/04-api/openapi.yaml (GET /approval-requests/pending, POST /approval-requests/{id}/approve, POST /approval-requests/{id}/reject)
    - Status-only approve/reject: only updates status and emits audit event; does NOT resume or re-trigger apply
    - Best-effort actor attribution: fallback external-api/approver or external-api/rejector
    - Error semantics: not-found → 404, non-pending transition → 409
    - Tests: approval_request_repo update tests pass

[x] Public risk classification exposed as canonical risk_tier (low/medium/high/critical) - PHASE 2b BATCH 2 DELIVERED
    Evidence:
    - Code: crates/rebase-engine/src/risk.rs (Severity::to_risk_tier)
    - Code: crates/rebase-engine/src/planner.rs (RebasePlan.risk_tier derived from DiffRiskAnalysis.severity)
    - Code: crates/intent-api/src/lib.rs (RebasePreviewResponse.risk_tier, RebaseApplyResponse.risk_tier)
    - OpenAPI: docs/04-api/openapi.yaml (risk_tier documented as primary public risk field)
    - decision_class and risk_level remain supporting fields

[x] Apply rebase policy mapped directly from risk_tier for LOW risk: automatic, no approval required - PHASE 2b DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (RiskTierGuard evaluates risk_tier as controlling policy)
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (LOW → AutoProceeded, no notification)
    - Tests: test_risk_tier_guard_low_auto_proceed_no_notification passes
    - Policy: LOW risk_tier → automatic, no approval required

[x] Apply rebase policy mapped directly from risk_tier for MEDIUM risk: automatic with notification - PHASE 2b DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (RiskTierGuard evaluates risk_tier as controlling policy)
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (MEDIUM → AutoProceededWithNotification)
    - Tests: test_risk_tier_guard_medium_auto_proceed_with_notification passes
    - Policy: MEDIUM risk_tier → automatic with notification

[x] Apply rebase policy mapped directly from risk_tier for HIGH/CRITICAL: blocked, requires manual approval - PHASE 2b DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (RiskTierGuard evaluates risk_tier as controlling policy)
    - Code: crates/rebase-orchestrator/src/apply_pipeline.rs (HIGH/CRITICAL → BlockedManualReview)
    - Tests: test_risk_tier_guard_high_blocked, test_risk_tier_guard_critical_blocked pass
    - Policy: HIGH/CRITICAL risk_tier → blocked, requires manual approval

[x] Rebase apply audit trail (who applied, when, what changed) - PHASE 2b BOUNDED SLICE DELIVERED
    Evidence:
    - Audit events: RebaseApplied + RebaseApplyBlocked capture actor, versions, decision class, rationale, runtime outcome, checkpoint alignment, and graph update summary
    - Code: crates/intent-api/src/lib.rs (external apply emits audit events)
    - Doc: ../../14-governance/01-audit-event-spec.md (bounded implementation status noted)
```

---

## 4. Approvals Revalidation

Current bounded approval queue/read/status-only workflow is delivered in Section 3. This section tracks the broader revalidation and policy-snapshot lifecycle that is still open.

```
[x] Approval invalidation on intent change - PHASE 2b BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/intent-service/src/approval_request_repo.rs (ApprovalRequestRepository::cancel_pending_by_intent)
    - Code: crates/intent-service/src/lib.rs (IntentService::create_version wires cancellation, with_approval_and_audit constructor)
    - Code: crates/intent-rebase-types/src/audit.rs (ApprovalCancelledAuditPayload, AuditEventType::ApprovalCancelled)
    - Code: crates/intent-rebase-types/src/audit_repo.rs (AuditRepository::record_approval_cancelled helper)
    - Tenant-safe: cancellation filters by intent_id AND tenant_id
    - Audit event: ApprovalCancelled emitted on cancellation (bounded taxonomy extension)
    - Tests: 6 new tests for cancel_pending_by_intent pass
    - Note: Minimal coherent extension to existing repository (not a new service)

[x] Approval scope canonicalization (deterministic JSON hashing groundwork) - BOUNDED SLICE DELIVERED
    Note: Canonical JSON serialization for scope_hash computation is implemented. Full approval scope computation, dependency-graph traversal, and revalidation remain future.
    Evidence:
    - Code: crates/intent-rebase-types/src/policy_snapshot.rs (compute_scope_hash, canonicalize_scope_definition, canonicalize_array_sorted, canonicalize_json_value)
    - Tests: 7 canonicalization tests pass (deterministic hashing for key ordering, array ordering, different content)
    - Doc: ../../13-adrs/07-approval-scope-canonicalization.md (updated)

[x] Policy snapshot read-only REST API surface (Phase 2b bounded slice - schema, types, repo, canonical hashing, and GET endpoints; S3 upload/write/revalidation out of scope)
    Note: Four read-only GET endpoints are implemented: GET /policy-snapshots/{id}, GET /policy-snapshots/intent/{intent_id}/latest, GET /policy-snapshots/intent/{intent_id}/versions/{version}, GET /policy-snapshots/intent/{intent_id}. S3 blob storage, write endpoints, and revalidation API remain future.
    Evidence:
    - Code: crates/intent-api/src/lib.rs:1252-1388 (handlers), 1604-1617 (routes)
    - Code: intent-service/policy_snapshot_repo.rs
    - Types: intent-rebase-types/src/policy_snapshot.rs
    - Schema: infrastructure/migrations/009_create_policy_snapshot.sql
    - Note: scope_hash uses canonical JSON serialization for deterministic hashing

[ ] Re-approval workflow: queue and notify approvers
    Evidence:
    - PR merged: <link>
    - Code: approval-service/workflow.rs
    - Integration: approval queue in console

[ ] Approval revalidation API: GET /api/v1/approvals/{id}/revalidate
    Evidence:
    - OpenAPI spec updated
    - Tests: revalidation tests pass

[ ] Full approval status lifecycle tracking (including expired/revalidated flows)
    Evidence:
    - Current bounded implementation supports pending queue creation plus status-only approved/rejected transitions
    - Expired/revalidated lifecycle, policy-linked status transitions, and revalidation APIs remain open
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

[x] Graph nodes updated when intent changes - BOUNDED SLICE DELIVERED (Phase 2b)
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs
    - GraphUpdater wired into apply_rebase path (update_graph_state called in Proceed path, lines 500-507)
    - update_graph_state method iterates affected items and calls graph_updater.update_node_state_if_affected
    - RebaseApplyResult.graph_updates populated with GraphUpdateResult for each mutation
    - RebaseApplySummary.graph_updates_applied/graph_updates_failed derived from graph_updates
    - OpenAPI: RebaseApplyResponse includes graph_updates_applied and graph_updates_failed fields
    - Tests: test_audit_summary_with_graph_updates passes (proves graph updates occur during apply with affected_items)

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
[x] Replay API: POST /intents/{id}/replay — PHASE 2b BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/intent-api/src/lib.rs (replay_intent handler + route registration)
    - Code: crates/rebase-orchestrator/src/lib.rs (RebaseOrchestrator::replay method)
    - OpenAPI: docs/04-api/openapi.yaml (/intents/{intent_id}/replay)
    - Semantics: bounded cooperative signal-based replay using existing runtime/checkpoint seams
    - Checkpoint selection: specific checkpoint_id OR most recent active checkpoint (bounded strategy)
    - Note: This is cooperative signal-based replay, NOT native Temporal reset

[x] Replay from specific checkpoint supported — BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/rebase-orchestrator/src/lib.rs (replay() method uses checkpoint_id param)
    - Code: crates/runtime-adapter/src/lib.rs (RuntimeAdapter trait, replay_from_checkpoint)
    - Code: crates/runtime-adapter/src/temporal_adapter.rs (TemporalAdapter::replay_from_checkpoint)
    - Tests: MockAdapter replay tests pass (runtime-adapter tests)
    - Note: Uses existing replay_from_checkpoint seam; not a new implementation

[x] Replay audit trail — BOUNDED SLICE DELIVERED
    Evidence:
    - Code: crates/intent-rebase-types/src/audit.rs (ReplayAuditPayload)
    - Code: crates/intent-rebase-types/src/audit.rs (AuditEventType::ReplayInitiated)
    - Code: crates/intent-rebase-types/src/audit_repo.rs (record_replay_initiated)
    - Audit event: ReplayInitiated emitted on bounded replay initiation
    - Note: Bounded to initiation event; full replay compatibility audit trail remains open

[ ] Replay with new intent version (intent version override)
    Evidence:
    - Code: rebase-service/replay_override.rs
    - Tests: replay override tests pass

[ ] Full replay compatibility (event streaming, replay status tracking)
    Evidence:
    - OpenAPI spec updated
    - Code: event-service/replay_status.rs
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
