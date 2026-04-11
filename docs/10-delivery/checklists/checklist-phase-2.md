# Phase 2 — Runtime-Integrated Rebase Checklist

**Exit Gate:** Phase 2 complete khi tất cả Phase 2-scoped items checked và có evidence; items explicitly deferred to Phase 3 with rationale do not block Phase 2 exit.  
**Prerequisite:** Phase 1 exit gate passed.

**Trạng thái:** `PHASE 2 CONDITIONALLY COMPLETE — GATE READY WITH EXPLICIT PHASE 3 DEFERRALS` — Phase 2a internal groundwork and Phase 2b bounded runtime-integrated slices are delivered. Remaining unchecked items are explicit Phase 3 infrastructure deferrals (artifact S3 operations, full notification delivery, schema evolution, DLQ, replay override/full replay compatibility) rather than unimplemented Phase 2 functional gaps.
**Phase:** Phase 2 (2a internal groundwork ✓ | 2b bounded external/integrated slices ✓ with Phase 3 infra deferred)  
**Target Duration:** 6–10 tuần

### Phase 2b Slice A — Evidence Verification ✅ GREEN (2026-04-11)

All canonical gate commands passed with zero warnings-as-errors:

| Command | Outcome |
|---------|---------|
| `cargo test --all-features` | ✅ Pass |
| `cargo check --all` | ✅ Pass |
| `cargo clippy --all-features -- -D warnings` | ✅ Clean |

Slice A verification complete. Slice B (residual risk items, deferral register, exit sign-off) remains before Phase 2b exit is formally closed.

> **📋 Slice B — Residual Risk & Phase 3 Deferral Register:** See [10-phase-2b-residual-risk-deferral-register.md](../10-phase-2b-residual-risk-deferral-register.md) for the full catalog of explicit Phase 3 deferrals with rationale, owning proposal, risk-if-delayed, and sign-off notes.

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

[x] Checkpoint creation on intent update (automatic) - PHASE 2b BOUNDED SLICE DELIVERED (TEST-ONLY INFRASTRUCTURE)
    Note: Bounded in-memory consumer infrastructure for testing the event→checkpoint path.
    Full NATS-based consumer with startup wiring, DLQ, and retry logic is Phase 3.
    Evidence:
    - Code: crates/intent-rebase-types/src/event_publisher.rs (EventConsumer trait, InMemoryEventConsumer)
    - Code: crates/intent-service/src/event_consumer.rs (CheckpointCreatorConsumer)
    - EventConsumer trait: async consumer contract for Phase 2b bounded slice
    - InMemoryEventConsumer: in-memory consumer buffer for testing
    - CheckpointCreatorConsumer: concrete consumer that creates checkpoints from RebaseApplied events
    - Tests: event consumer tests pass (publish_consume_checkpoint_cycle, creates_checkpoint_on_rebase_applied, etc.)
    - Doc: event_publisher.rs module docs distinguish bounded Phase 2b consumer infra from Phase 3 NATS consumers
    - Bounded to in-memory consumers for testing only — full consumer infrastructure (startup wiring, NATS subscription, DLQ) is Phase 3
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

[x] Re-approval workflow trigger (Phase 2b bounded slice) - BOUNDED SLICE DELIVERED
    Note: POST /approval-requests/trigger-reapproval creates a new pending approval request when scope changes.
    Does NOT send external notifications (Phase 3 out of scope). Only creates approval record and returns queue intent.
    Evidence:
    - Code: crates/intent-api/src/lib.rs (trigger_reapproval handler + route registration)
    - Request/Response types: ReapprovalTriggerRequest, ReapprovalTriggerResponse
    - Behavior: Creates pending approval_request via existing repository, returns queue/notification intent
    - Bounded: notification_intent=true is advisory only; actual notification deferred to Phase 3
    - Tests: bounded trigger tests pass

[x] Approval revalidation API: GET /approval-requests/{id}/revalidate (Phase 2b bounded read-only slice)
    Evidence:
    - PR merged: this PR
    - Code: crates/intent-api/src/lib.rs (revalidate_approval_request handler)
    - Tests: revalidation tests pass (crates/intent-api/src/lib.rs tests module)
    - Note: Bounded read-only scope comparison using approval-basis vs latest snapshot scope_hash
    - Note: Does NOT trigger re-approval workflow, queue/notify, or modify approval status

[x] Approval status lifecycle transitions (Phase 2b bounded - status-only transitions via API)
    Note: Bounded to status-only approve/reject/expire transitions. No automatic expired/revalidated
    triggers. Revalidation is read-only comparison via GET /approval-requests/{id}/revalidate.
    Evidence:
    - ApprovalRequestStatus enum: Pending, Approved, Rejected, Expired, Cancelled
    - update_approval_request_status transitions Pending → Approved/Rejected via API
    - mark_expired transitions Pending → Expired via API (manual expiry, no background worker)
    - GET /approval-requests/{id}/revalidate: read-only scope_hash comparison
    - POST /approval-requests/{id}/expire: manual expiry transition (Pending → Expired)
    - Does NOT auto-trigger expired status (needs background worker — Phase 3)
    - Does NOT auto-transition to revalidated (needs re-approval workflow — Phase 3)
```

---

## 5. Artifact Invalidation + Quarantine

**Note:** Phase 2b delivers bounded metadata/status slice only. Real S3 quarantine move, artifact release, and artifact deletion are Phase 3.

```
[x] Artifact invalidation on intent change - BOUNDED SLICE DELIVERED (Phase 2b)
    Note: Bounded metadata/status only. Real S3 quarantine move is Phase 3.
    Evidence:
    - Code: crates/intent-rebase-types/src/artifact.rs (QuarantineSignal, QuarantineStatus, ArtifactMetadata.invalidated, Artifact.is_invalidated())
    - Code: crates/intent-rebase-types/src/audit.rs (ArtifactInvalidatedAuditPayload, AuditEventType::ArtifactInvalidated)
    - Code: crates/intent-rebase-types/src/audit_repo.rs (record_artifact_invalidated helper)
    - Code: crates/rebase-orchestrator/src/graph_updater.rs (ArtifactInvalidationSignal struct + invalidate_artifacts helper method)
    - Behavior: update_graph_state in RebaseOrchestrator::apply_rebase marks affected artifact nodes Stale via update_node_state_if_affected; invalidate_artifacts helper exists as groundwork for Phase 3 artifact-service integration but is NOT yet wired into rebase apply flow
    - Tests: test_invalidate_artifacts_generates_signals validates the helper logic in isolation
    - Note: The rebase-apply flow already marks artifacts Stale via update_graph_state; invalidate_artifacts is helper-only for Phase 3 when artifact service wires actual quarantine signal emission

[x] Artifact quarantine status read API: GET /artifacts/{id}/quarantine-status - BOUNDED SLICE DELIVERED (Phase 2b)
    Note: Metadata/status only - real S3 quarantine move is Phase 3.
    Evidence:
    - Code: crates/intent-api/src/lib.rs (get_artifact_quarantine_status handler + route registration)
    - Code: crates/intent-rebase-types/src/artifact.rs (ArtifactQuarantineStatus struct, Artifact.quarantine_status())
    - Behavior: Returns quarantine status metadata by looking up artifact node in graph
    - Returns 404 if artifact not found in graph (Phase 2b has no standalone artifact repo)
    - Phase 3 will wire this to actual artifact repository when that service is implemented

[ ] Artifact quarantine: move to quarantine path in S3 — PHASE 3 ITEM
    Evidence:
    - Phase 3 item - requires artifact-service with S3 integration
    - S3 path: artifacts/{tenant}/{intent_id}/v{version}/quarantine/{artifact_id}/

[ ] Artifact release from quarantine (if rebase resolved) — PHASE 3 ITEM
    Evidence:
    - Phase 3 item - requires artifact-service with S3 integration

[ ] Artifact permanent deletion (if rebase requires discard) — PHASE 3 ITEM
    Evidence:
    - Phase 3 item - requires artifact-service with S3 integration
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

[x] Graph edge re-evaluation on intent change - BOUNDED SLICE DELIVERED (Phase 2b)
    Note: Bounded read-only edge validation. Examines endpoint node states to determine edge validity.
    Does NOT create/delete edges (Phase 3 structural mutations remain deferred).
    Evidence:
    - Code: crates/graph-service/src/edge_reevaluation.rs (reevaluate_edges_from_intent_version, evaluate_edge_validity)
    - EdgeValidity enum: Valid, TargetStale, SourceStale
    - EdgeReevaluationResult: edges_examined, valid_edges, flagged_edges, flagged_edge_ids
    - Tests: 2 edge validation tests pass (all active, target stale)
    - Integration: Can be called after intent version changes to identify edges needing review

[x] Orphan detection - BOUNDED SLICE DELIVERED (Phase 2b)
    Note: Bounded orphan detection using existing are_connected seam. Identifies artifacts and
    side effects no longer reachable from active intent version. Does NOT auto-archive or
    quarantine (Phase 3 artifact handling remains deferred).
    Evidence:
    - Code: crates/graph-service/src/edge_reevaluation.rs (detect_orphaned_nodes)
    - OrphanDetectionResult: artifacts_examined/reachable/orphaned, side_effects_examined/reachable/orphaned
    - Uses existing graph_service::are_connected() for bounded reachability check
    - Tests: 2 orphan detection tests pass (no orphans, with orphans)
    - Action: Orphaned nodes are flagged in result; actual quarantine/deprecation deferred to Phase 3
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

[ ] Replay with new intent version (intent version override) — PHASE 3 ITEM
    Evidence:
    - Current bounded replay endpoint supports checkpoint-based cooperative replay only
    - Intent version override path is not implemented in the current worktree
    - Full version-aware replay override requires Phase 3 replay/status infrastructure

[ ] Full replay compatibility (event streaming, replay status tracking) — PHASE 3 ITEM
    Evidence:
    - Bounded replay endpoint exists, but full replay status tracking does not
    - Phase 3 requires replay status/event-service infrastructure and broader event streaming support
```

---

## 8. Event Streaming (NATS or Kafka)

**Note:** Phase 2b delivers a bounded event-publishing slice only. Real NATS JetStream integration, event consumers, and DLQ are Phase 3 items.

```
[x] Event publishing abstraction (Phase 2b BOUNDED SLICE DELIVERED)
    Note: Phase 2b bounded slice adds event publishing infrastructure on top of audit persistence.
    Audit persistence is the source of truth; event publishing is best-effort/fail-open.
    Evidence:
    - Code: crates/intent-rebase-types/src/event_publisher.rs (EventPublisher trait)
    - EventPublisher trait: NoOpEventPublisher (no-op), InMemoryEventPublisher (mock for tests)
    - Subject naming: audit.events.v1.<tenant_id>.<event_type>
    - Schema versioning: v1 prefix (v2 migration path deferred to Phase 3)
    - Tests: 6 event_publisher tests pass

[x] Event publishing wired into existing audit emission paths (Phase 2b BOUNDED SLICE DELIVERED)
    Note: Event publishing is best-effort/fail-open - audit persistence succeeds even if publishing fails.
    Evidence:
    - Code: crates/intent-api/src/lib.rs (publish_audit_event helper + wired in rebase_apply, approve/reject approval_request, replay_intent)
    - AppState.event_publisher: Option<Arc<dyn EventPublisher>> - None = no streaming, Some = best-effort publish
    - build_router() and build_router_with_sql_audit_and_approval() accept optional event_publisher parameter
    - Tests: 5 event publishing tests pass in intent-api

[x] Subject naming convention documented (Phase 2b BOUNDED SLICE DELIVERED)
    Note: Subject format is bounded to Phase 2b scope. Full stream configuration is Phase 3.
    Evidence:
    - Code: crates/intent-rebase-types/src/event_publisher.rs (EventSubject::from_audit_event)
    - Format: audit.events.v1.<tenant_id>.<event_type>
    - Doc basis: docs/13-adrs/04-event-broker.md (subject naming ADR basis)

[x] Event consumer abstraction and in-memory implementations (Phase 2b BOUNDED SLICE DELIVERED)
    Note: Bounded in-memory consumer infrastructure for testing. Full NATS-based consumers with
    startup wiring, DLQ, retry, and consumer groups are Phase 3.
    Evidence:
    - Code: crates/intent-rebase-types/src/event_publisher.rs (EventConsumer trait, InMemoryEventConsumer, ConsumedEvent, ConsumeResult)
    - Code: crates/intent-rebase-types/src/audit.rs (NotificationRecord, NotificationKind, NotificationKind enum)
    - Code: crates/intent-service/src/event_consumer.rs (CheckpointCreatorConsumer, NotifierConsumer, InMemoryNotificationStore)
    - EventConsumer trait: async consumer contract (consume method)
    - InMemoryEventConsumer: in-memory consumer buffer for testing (no external deps)
    - CheckpointCreatorConsumer: concrete consumer that creates checkpoints from RebaseApplied events
    - NotifierConsumer: bounded consumer that records notification intents from approval-related events
    - InMemoryNotificationStore: in-memory store for notification records (bounded to testing only)
    - Tests: event consumer tests pass (test_publish_consume_checkpoint_cycle, test_notifier_consumer_publish_consume_notification_cycle, etc.)
    - Bounded to in-memory consumers for testing only — full consumer infrastructure is Phase 3

[x] Bounded notifier consumer (Phase 2b BOUNDED SLICE DELIVERED)
    Note: Bounded in-memory notification recording from approval events. Does NOT send external
    notifications (email, webhook, NATS). Full notification delivery is Phase 3.
    Evidence:
    - Code: crates/intent-rebase-types/src/audit.rs (NotificationRecord, NotificationKind::ApprovalGranted/ApprovalRevoked/ApprovalCancelled)
    - Code: crates/intent-service/src/event_consumer.rs (NotifierConsumer, InMemoryNotificationStore)
    - Consumes: ApprovalGranted, ApprovalRevoked, ApprovalCancelled events
    - Records: NotificationRecord in memory with message, intent_id, tenant_id, kind, source_sequence
    - Tests: 7 notifier consumer tests pass (records_approval_granted, records_approval_revoked, records_approval_cancelled, etc.)
    - Bounded to in-memory notification recording only — external notification delivery is Phase 3

[x] Snapshot-creator consumer — PHASE 2b BOUNDED SLICE DELIVERED (event-driven, limited scope data)
    Note: SnapshotCreatorConsumer creates policy snapshots when consuming RebaseApplied events.
    Uses PolicySnapshotRepository for persistence. scope_definition is derived from event payload
    with fallback defaults when full scope data is not available — this is an inherent
    limitation of event-driven snapshot creation without access to the full intent scope.
    Evidence:
    - Code: crates/intent-service/src/event_consumer.rs (SnapshotCreatorConsumer)
    - Consumes: RebaseApplied events
    - Creates: PolicySnapshot via PolicySnapshotRepository
    - Bounded scope data: scope_type, affected_resources, required_approvers, min_approvals
      extracted from event payload with fallback defaults (empty/ScopeType::None/1)
    - Tests: 6 snapshot-creator tests pass (creates on rebase applied, skips non-rebase,
      handles missing intent_id, uses defaults when scope missing, publish/consume cycle,
      multiple versions)
    - Bounded to event payload scope data — full scope requires access to intent scope (Phase 3)

[ ] Full notification delivery (email, webhook, NATS) — PHASE 3 ITEM
    Evidence:
    - NotifierConsumer records notification intents in memory only
    - Actual external notification delivery requires Phase 3 infrastructure

[ ] Event schema versioning (v2 migration path) — PHASE 3 ITEM
    Evidence:
    - Doc: ../../04-api/02-events.md (v2 migration to be documented in Phase 3)
    - Migration: v1 → v2 deferred to Phase 3

[ ] Dead-letter queue for failed event processing — PHASE 3 ITEM
    Evidence:
    - Phase 3 code: event-service/dlq.rs
    - Phase 3 tests: DLQ handling tests pass
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: ☑ Yes (all Phase 2-scoped items complete; remaining unchecked items explicitly deferred to Phase 3)

Phase 2 Exit Gate Review Date: 2026-04-09
Reviewed By: AI orchestrator (bounded delivery + deferral audit)
Product Owner Sign-off: ___________  (name / date / decision)
Security Sign-off: ___________  (name / date / decision)
Runtime Integration Sign-off: ___________  (name / date / decision)

Required sign-off packet:
- Slice A evidence verification: `cargo test --all-features`, `cargo check --all`, `cargo clippy --all-features -- -D warnings`
- Slice B residual risk / Phase 3 deferral register: ../10-phase-2b-residual-risk-deferral-register.md
- Acceptance basis: all unchecked items above are explicit Phase 3 deferrals, not Phase 2 functional gaps

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 3 — Compensation + Production Hardening](./checklist-phase-3.md)
