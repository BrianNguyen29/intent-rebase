# Current Project Status

## Executive Summary

**Current Phase:** Phase 3 — Compensation + Production Hardening, Batch 1 largely delivered.  
**Phase 2b status:** Slice A (evidence verification) green — all canonical gates pass (`cargo test --all-features`, `cargo check --all`, `cargo clippy --all-features -- -D warnings`). Slice B (residual risk items, deferral register, sign-off) complete. **Phase 2b is APPROVED — Brian Nguyen (sole signer, personal project) signed off as APPROVED for all three reviewer roles (Product Owner, Security, Runtime Integration) on 2026-04-28. Phase 2b exit gate is CLOSED. Phase 3 entry is AUTHORIZED.** See the [Phase 2b External Sign-Off Packet](./11-phase-2b-sign-off-packet.md) for the full decision capture and deferral register.
**Phase 3 Batch 1 delivered:** Side effect ledger, compensation-actions CRUD + APIs, batch orchestration, policy gate, orchestration dashboard, orchestration coordination view, dry-run planner, single-shot orchestration runtime (HTTP + CLI), and POST /compensation-simulation/run (commit fe2a1f6). Tenant context hardening delivered (commit de2d80d). Phase 2b exit is closed; bounded planner/executor/retry/rollback record delivered as part of Phase 3 Batch 1.
**Phase 3 Batch 2 status:** Bounded slices delivered (SLO definitions provisional, alerting rules, error budget panels, distributed tracing, benchmarks); external SRE sign-off gates remain.
**Phase 3 Batch 3b status:** Forensic verification with real entity counts (commit 7b05c5b), bounded generation (POST /forensic/bundle with in-memory storage; S3BundleStorage seam exists), bounded export (POST /forensic/export), and bounded download slices delivered; S3-backed retrieval/storage lifecycle and full runtime replay remain Phase 4 scope.
**Production readiness:** Not yet production-ready. Phase 3 Batch 1 delivers bounded API surfaces; SRE hardening (external gates), tenant isolation verification, forensic replay (Phase 4), and performance work are still open.

---

## Feature Completion vs Production Readiness

This project distinguishes between **non-production feature completion** and **production readiness**:

| Dimension | Non-Production Feature Completion | Production Readiness |
|-----------|----------------------------------|---------------------|
| **Scope** | Bounded slices delivered per phase | Full phase exit gate closed |
| **Evidence** | Code compiles, tests pass, docs updated | External sign-off (SRE, Security) |
| **Verification** | Internal canonical gates (cargo test, clippy) | Load testing, pen testing, compliance audit |
| **Status** | "Delivered" or "In Progress" | "Production Ready" |
| **Commits** | fe2a1f6 (POST sim), de2d80d (tenant), 7b05c5b (forensic counts) | Phase 3 exit gate pending |

**Current state:** Feature implementation is ongoing (commits fe2a1f6, de2d80d, 7b05c5b pushed to origin/main). Production readiness gates remain open — SRE sign-off, tenant isolation verification, pen testing, and Phase 3 exit gate are pending.

---

## Implemented Phases on Main

| Phase | Status | Key Delivered |
|-------|--------|----------------|
| Phase 0 — Foundations | ✓ Complete | Repo scaffold, ADRs, architecture baseline, local dev, CI |
| Phase 1 — Core Control Plane MVP | ✓ Complete | Intent schema + versioning (PR #21), Graph HTTP API (PR #22), Observability v1 (PR #23), Security v1 (PR #24) |
| Phase 2 — Runtime-Integrated Rebase | ✅ Complete | Phase 2a runtime adapter delivered; Phase 2b complete and signed off — exit gate CLOSED |
| Phase 3 — Compensation + Hardening | 🔄 Active | Batch 0 scaffold + planning ✅; Batch 1 largely delivered ⚠️; Batch 2 (observability/SRE) in progress; Batch 3 (P3: tenant isolation) in progress; Batch 4 (P5: performance, P6: security) in progress |

---

## Phase 3 Batch 1 — Delivered Surfaces

### Phase 4 Lifecycle First Slice (Bounded — Non-Production)

> **Status:** Implemented (2026-04-29) — bounded slice only, **NOT production-ready**

Phase 4 lifecycle first slice delivered as bounded non-production feature:

- **Delivered:** Single NATS consumer lifecycle (`CheckpointCreatorConsumer`) behind `INTENT_API_NATS_CONSUMER=true` runtime gate
- **Delivered:** Graceful shutdown via watch channel (SIGINT/SIGTERM stops poll loop without hanging)
- **Delivered:** Fail-open on NATS connection/lifecycle failure (warning logged, HTTP server continues)
- **NOT delivered:** DLQ worker (remains Phase 4+ future work)
- **NOT delivered:** Multi-consumer chain (remains future scope)
- **NOT delivered:** S3 runtime wiring (remains Phase 4 scope)
- **NOT delivered:** Production readiness (external sign-off not claimed)

**Env gate behavior:**
- `INTENT_API_NATS_CONSUMER=true` enables consumer lifecycle (requires `NATS_URL`)
- Default (unset/false): HTTP startup behavior unchanged
- If NATS unavailable: fail-open with warning unless strict mode

**Bounded scope claim:** This is a bounded Phase 4 first slice implementing a single CheckpointCreatorConsumer behind a compile/runtime gate. DLQ worker, multi-consumer chain, and S3 runtime wiring remain future work. Production deployment readiness is not claimed.

### Side Effect Ledger
- Model with `effect_id`, `intent_id`, `intent_version`, `effect_type`, `target`, `timestamp`, `tenant_id`
- Capture-on-write via `POST /v1/graph/artifacts` with optional `side_effect_context` (artifact-ingest only; other artifact-producing operations not yet covered)
- Query API: `GET /intents/{intent_id}/side-effects`
- Idempotency: tenant-scoped atomic record with duplicate protection

### Compensation Actions
- Model with `action_type`, `target`, `parameters`, `status`, `intent_id`, `trigger_context`, `execution_result_payload`
- Query API: `GET /intents/{intent_id}/compensation-actions` (read-only; no execution)
- Approve API: `POST /compensation-actions/{action_id}/approve` — Pending → Approved
- Waive API: `POST /compensation-actions/{action_id}/waive` — Pending → Waived
- Execute API: `POST /compensation-actions/{action_id}/execute` — executor gate: only Approved actions execute; routes to one of four bounded executors (RollbackExecutor, CounterActionExecutor, FollowupNoticeExecutor, EscalationExecutor)
- Reapprove API: `POST /compensation-actions/{action_id}/reapprove` — Failed → Pending (fail-closed; retryable errors + remaining budget only)

### Batch Orchestration
- DLQ query API: `GET /compensation-actions/dlq` — derived DLQ from Failed + (exhausted budget OR non-retryable)
- Batch candidates API: `GET /compensation-actions/batch-candidates` — four categories (pending approval, approved auto-executable, retryable failed, DLQ)
- Batch approve: `POST /compensation-actions/batch-approve`
- Batch reapprove: `POST /compensation-actions/batch-reapprove`
- Batch execute: `POST /compensation-actions/batch-execute`

### Orchestration Views + Dry-Run
- Dashboard API: `GET /intents/{intent_id}/orchestration-dashboard` — read-only summary
- Coordination status API: `GET /compensation-actions/orchestration-coordination` — read-only coordination view
- Dry-run planner: `POST /compensation-actions/orchestration-dry-run` — READ-ONLY; returns propose actions (approve/reapprove/execute/no_action) + reason

### Single-Shot Orchestration Runtime
- HTTP: `POST /compensation-actions/runs` (202 Accepted) + `GET /compensation-actions/runs/{run_id}`
- CLI: `intent-cli run` + `intent-cli get-run`
- Auto-decides approve/reapprove/execute/skip per action; persists run handle
- No queue polling, no distributed claiming/locking, no background scheduler

### Policy Gate
- `GET /compensation-actions/policy-gate` and `GET /intents/{intent_id}/compensation-policy-gate` — read-only gate evaluation

### Compensation Simulation (N4-4 — Bounded API Slice)
- `GET /intents/{intent_id}/rebase-simulation` — read-only mock simulation using CompensationSimulator
- `POST /compensation-simulation/run` — POST variant with request body format (commit fe2a1f6)
- Mode: `deterministic` (default) or `stochastic` with optional seed for reproducibility
- Returns SimulationReport with predicted compensation outcomes based on side effects
- **This endpoint is READ-ONLY** — does not execute real compensation actions
- N4-4 bounded slice delivered; full N4 (full compensation simulation with live executors) remains Phase 4 scope

---

## Key Open Areas

| Area | Status | Blocking |
|------|--------|----------|
| Approval invalidation (bounded) | ✅ Delivered | `trigger_reapproval` cancels Approved approvals; `rebase_apply` BlockedManualReview cancels Approved approvals; `Cancelled` status used as substitute for `Invalidated` |
| Phase 2b exit gate | ✅ Closed | Brian Nguyen (sole signer, personal project) approved all three roles — 2026-04-28 |
| Side effect rollback record (compensation applied, result) | ✅ Delivered | Bounded: schema + repository for compensation applied/result fields |
| Compensation planner (full — bounded delivered) | ✅ Delivered | Bounded planner: generates compensation plans from side effects using class-based strategy routing; S2 plans route to CounterAction+SemiAutomatic (per class routing); fail-closed on unsupported strategy classes |
| Compensation executor (four bounded executors — Rollback/CounterAction/FollowupNotice/Escalation) | ✅ Delivered | Bounded: RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible); fail-closed on non-matching combos; S2 planner/executor alignment resolved (S2ExternalReversible routes to CounterAction+SemiAutomatic) |
| Compensation audit trail | ✅ Delivered | Bounded: `compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed` events |
| SLO definitions + alerting + error budget | 🔄 In Progress | Batch 2 bounded slices delivered (SLO definitions provisional, alerting rules, error budget panels); external SRE sign-off remains open |
| Distributed tracing across Phase 2→3 | 🔄 In Progress | Batch 2 bounded in-process OTEL propagation delivered; cross-process propagation investigated and deferred (Temporal SDK limitation) |
| Performance benchmarks | 🔄 In Progress | Bounded slices delivered: rebase-engine sync bench, graph traversal bench, DB bench, HTTP server bench with in-memory repos; full production load testing remains open |
| Runbooks | 🔄 In Progress | RB6-RB10 delivered (rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn) |
| Tenant isolation verification tests | 🔄 In Progress | Bounded slices delivered: P3-S1 (tenant isolation tests), P3-S2 (quota enforcement), P3-S3 (rule pack isolation), P3-S4 (audit query isolation), P3-S5 (tenant service scaffold) |
| Forensic bundle (model, generation, API, replay) | 🔄 In Progress | Bounded slices delivered: verification API, export API, integrity hashing, replay surface; **bundle generation (POST /forensic/bundle) delivered with in-memory storage at runtime; S3BundleStorage seam exists but not wired; list bundles (GET /forensic/bundles) and download API surfaces delivered as bounded in-memory; S3-backed retrieval/storage lifecycle remains Phase 4 scope**; full replay remains open |
| NATS consumer lifecycle (Phase 4 first slice) | ✅ Bounded Delivered | Bounded multi-consumer registry (`ConsumerRegistry`) implemented; single consumer (`CheckpointCreatorConsumer`) wired behind `INTENT_API_NATS_CONSUMER=true` gate; graceful shutdown via shared watch channel; fail-open on NATS unavailability; **Snapshot/DLQ consumers NOT enabled (future Phase 4+ scope)** |
| Threat model v2, penetration testing | 🔄 In Progress | Threat model v2 documented; pen test scope defined (planning artifact only); pen test execution and external security review remain open |
| Load testing | 🔄 In Progress | Bounded HTTP load harness delivered (intent-api HTTP server with in-memory repos); full production load testing remains gated on P5 full completion |

---

## Canonical Verification Commands

```bash
# Run all tests
cargo test --all-features

# Run compensation-service tests
cargo test -p compensation-service --all-features

# Run intent-api tests
cargo test -p intent-api --all-features

# Run graph-service tests
cargo test -p graph-service --all-features

# Build verification (no emit)
cargo check --all

# Intent-cli build
cargo check -p intent-cli

# lint
cargo clippy --all-features -- -D warnings
```

---

## Prioritized Next Steps

| Priority | Area | Action | Owner |
|----------|------|--------|-------|
| **P0** | Phase 2b sign-off | Close Phase2b sign-off name/date documentation | Backend Lead |
| **P0/P1** | NATS consumer lifecycle | NATS consumer subscription lifecycle (first slice implemented — bounded ACK-all); gate remaining DLQ/multi-consumer work on DLQ design approval (G1-G5) | Backend Lead / SRE |
| **P1** | Forensic S3BundleStorage | Wire S3BundleStorage into forensic-service runtime OR keep docs demoted | Backend Lead |
| **P1** | S3 snapshot production | Complete S3 snapshot production lifecycle (Object Lock/chain-hash deferred to Phase 4) | Backend Lead |
| **P1** | Production readiness | Production load testing + SRE/Security external sign-offs | SRE / Security |
| **P2/Phase4** | Forensic replay | Full replay capability + Object Lock/chain-hash for snapshots | Backend Lead |

> **Note:** Phase 3 bounded commits are pushed to origin/main. Local canonical gates passed: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check --workspace`, `cargo test --workspace --all-features -j 1`, targeted nats/s3_snapshot/snapshot_creator/trace_context/approval_revalidation/forensic tests, and `git diff --check`. Latest observed GitHub Actions push runs report `startup_failure` before jobs are created — remote CI is not passing. Production readiness is not claimed.

---

## Related Docs

- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Batch 0 Execution](./06-phase-3-batch-0-execution.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
- [Phase 3 Completion Execution Plan](./15-phase-3-completion-execution-plan.md)
- [10 Completion Proposals Tracker](./09-completion-proposals-tracker.md)
