# Phase 2 — Runtime-Integrated Rebase Checklist

**Exit Gate:** Phase 2 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 1 exit gate passed.

**Trạng thái:** `NOT STARTED`  
**Phase:** Phase 2  
**Target Duration:** 6–10 tuần

---

## 1. Runtime Adapter v1 (Temporal)

```
[ ] RuntimeAdapter trait defined and implemented
    Evidence:
    - PR merged: <link>
    - Code: runtime-adapter/src/trait.rs
    - Tests: runtime-adapter/tests/trait_tests.rs

[ ] TemporalAdapter: get_checkpoints(intent_id) implemented
    Evidence:
    - PR merged: <link>
    - Code: runtime-adapter/src/temporal_adapter.rs
    - Tests: checkpoint mapping tests pass

[ ] TemporalAdapter: send_rebase_signal(workflow_id, directive) implemented
    Evidence:
    - PR merged: <link>
    - Integration test: signal sent and received

[ ] TemporalAdapter: map_intent_version_to_checkpoint(intent_version) implemented
    Evidence:
    - PR merged: <link>
    - Tests: version-to-checkpoint mapping tests pass

[ ] TemporalAdapter: replay_from_checkpoint(workflow_id, checkpoint) implemented
    Evidence:
    - PR merged: <link>
    - Tests: replay test passes

[ ] Fallback adapter (mock/no-op) for non-Temporal runtimes
    Evidence:
    - Code: runtime-adapter/src/mock_adapter.rs
    - Tests: mock tests pass
```

---

## 2. Checkpoint Mapping

```
[ ] Checkpoint data model (checkpoint_id, intent_version, timestamp, workflow_state)
    Evidence:
    - PR merged: <link>
    - Code: intent-service/checkpoint.rs
    - Schema: 007_checkpoints.sql

[ ] Checkpoint storage (PostgreSQL)
    Evidence:
    - Migration: 007_checkpoints.sql
    - Tests: checkpoint CRUD tests pass

[ ] Checkpoint-to-intent-version alignment logic
    Evidence:
    - PR merged: <link>
    - Code: rebase-engine/checkpoint_mapping.rs
    - Tests: alignment tests pass

[ ] Checkpoint lifecycle (create on intent update, expire old checkpoints)
    Evidence:
    - Code: checkpoint service with TTL
    - Tests: lifecycle tests pass

[ ] Checkpoint query API for runtime adapter
    Evidence:
    - Internal API: get_checkpoints(intent_id)
    - Tests: query tests pass
```

---

## 3. Apply Rebase — Low/Medium Risk

```
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

**Next Phase:** [Phase 3 — Compensation + Production Hardening](./05-phase-3-hardening.md)