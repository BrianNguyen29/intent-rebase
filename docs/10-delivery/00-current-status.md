# Current Project Status

## Executive Summary

**Current Phase:** Phase 3 — Compensation + Production Hardening, Batch 1 largely delivered.  
**Phase 2b status:** Slice A (evidence verification) green — all canonical gates pass (`cargo test --all-features`, `cargo check --all`, `cargo clippy --all-features -- -D warnings`). Slice B (residual risk items, deferral register, sign-off) complete. **Phase 2b is APPROVED — Brian Nguyen (sole signer, personal project) signed off as APPROVED for all three reviewer roles (Product Owner, Security, Runtime Integration) on 2026-04-28. Phase 2b exit gate is CLOSED. Phase 3 entry is AUTHORIZED.** See the [Phase 2b External Sign-Off Packet](./11-phase-2b-sign-off-packet.md) for the full decision capture and deferral register.
**Phase 3 Batch 1 delivered:** Side effect ledger, compensation-actions CRUD + APIs, batch orchestration, policy gate, orchestration dashboard, orchestration coordination view, dry-run planner, single-shot orchestration runtime (HTTP + CLI), and POST /compensation-simulation/run (commit fe2a1f6). Tenant context hardening delivered (commit de2d80d). Phase 2b exit is closed; bounded planner/executor/retry/rollback record delivered as part of Phase 3 Batch 1. RLC-3 bounded RLS integration validated (commit 42cdbe2): migration_integration 1/1 passed on fresh DB; rls_integration --ignored 4/4 passed; migration 009 consolidation delivered with FORCE RLS; bounded handler RLS wiring for create_graph_edge/create_orchestration_run/create_forensic_bundle delivered as local working-tree change (pending commit).
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
| **Commits** | fe2a1f6 (POST sim), de2d80d (tenant), 7b05c5b (forensic counts), 42cdbe2 (RLC-3 bounded RLS) | Phase 3 exit gate pending |

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
- **Delivered:** Bounded DLQ metrics worker (`DlqMetricsWorker`) behind `INTENT_API_NATS_DLQ_WORKER=true` gate
  - Emits `intent_api_dlq_messages_current` gauge (depth)
  - Emits `intent_api_dlq_message_age_seconds` gauge (oldest message age)
  - Uses lightweight peek (no_ack=true) to count without consuming
  - Requires `INTENT_API_NATS_CONSUMER=true` and `NATS_URL`
- **NOT delivered:** Full DLQ replay worker (remains Phase 4+ future work)
- **NOT delivered:** Multi-consumer chain (remains future scope)
- **NOT delivered:** S3 runtime wiring (remains Phase 4 scope)
- **NOT delivered:** Production readiness (external sign-off not claimed)

**Env gate behavior:**
- `INTENT_API_NATS_CONSUMER=true` enables consumer lifecycle (requires `NATS_URL`)
- `INTENT_API_NATS_DLQ_WORKER=true` enables DLQ metrics worker (requires `INTENT_API_NATS_CONSUMER=true`)
- Default (unset/false): HTTP startup behavior unchanged
- If NATS unavailable: fail-open with warning unless strict mode

**Bounded scope claim:** This is a bounded Phase 4 first slice implementing a single CheckpointCreatorConsumer behind a compile/runtime gate. DLQ metrics worker is bounded (depth/age gauges only). Full DLQ replay, multi-consumer chain, and S3 runtime wiring remain future work. Production deployment readiness is not claimed.

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

### SQL Graph Persistence (Bounded — Non-Production)

> **Status:** Implemented (2026-04-29) — bounded slice only, **NOT production-ready**

SQL-backed `SqlxGraphRepository` wired when `DATABASE_URL` is set:

- **Delivered:** `SqlxGraphRepository` implementing `GraphRepository` trait for core CRUD operations
- **Delivered:** Node operations: `create_node`, `get_node`, `list_nodes`, `update_node_state`
- **Delivered:** Edge operations: `create_edge`, `get_edge`, `list_edges`, `list_edges_from`, `list_edges_to`, `delete_edge`
- **Delivered:** Type conversion helpers for `NodeType`, `NodeState`, `EdgeType`, `ExternalRefType`
- **Bounded:** No bulk operations, no pagination on list operations
- **Bounded:** No transaction-based consistency checks (DB trigger enforces node existence)
- **NOT delivered:** Graph traversal operations (`find_reachable`, `find_path`, `detect_cycles`) remain on `GraphService` using repository's `list_*` methods
- **NOT delivered:** Production-scale claim NOT made

**Env gate behavior:**
- `DATABASE_URL` set: Uses `SqlxGraphRepository` (SQL-backed graph persistence)
- `DATABASE_URL` not set: Uses `InMemoryGraphRepository` (dev/testing only)

**Bounded scope claim:** This is a bounded Phase 2b slice implementing SQL-backed graph CRUD against existing `graph_nodes`/`graph_edges` schema. Graph traversal remains on `GraphService`. Production-scale deployment readiness is not claimed.

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
| JWT production guard + RLS helper scaffold | 🔄 In Progress | **Oracle-ordered P1 slices for full RLS transaction wrapping (P1-S1..S5 + RLC-4..RLC-12).** JWT tenant_id validation delivered (P3-S5 bounded slice); migration 009 consolidation with FORCE RLS delivered. **P1-S1 (RlsAwarePool shared) BOUNDED DONE (pushed f055dc5); P1-S2 (IntentService.rls_pool wiring) BOUNDED DONE (pushed); P1-S3 (RlsTransactionExt) BOUNDED DONE (pushed f055dc5); P1-S4 (graph_edge wrap) BOUNDED DONE (pushed 02de885); P1-S5 (approval handlers) PARTIAL — P1-S5a (approve/reject, update_status_with_tx), P1-S5b (expire, mark_expired_with_tx), P1-S5c (list_pending), P1-S5d (revalidate), P1-S5e (trigger handler-level) BOUNDED DONE (pushed); P1-S5f (trigger full-tx create+cancel) BOUNDED DONE (local); P1-S5g (approve/waive/reapprove + batch) BOUNDED DONE (local); P1-S5h (execute single/batch) DOCUMENTED BLOCKER (transaction-unaware executor/side_effect_repo trait boundary); P1-S5i (orchestration_runs bounded slice) BOUNDED DONE (local) — migration 015 creates `orchestration_runs` table with RLS policy, `create_run_with_tx` method added, RLS path wired in `create_orchestration_run` handler, RLC-12 test added; forensic/artifact PENDING; RLC-4..RLC-12 BOUNDED DONE (local — 13 tests passed via `cargo test --test rls_integration -- --ignored`).** JWT guard (`INTENT_API_REQUIRE_JWT=true`) fails startup if JWT_SECRET missing/weak; `build_router_with_jwt_auth(...)` called from `main.rs`; `jwt_auth_async` middleware validates `tenant_id` claim (rejects nil UUID); `RlsTenantContext` struct with `set_rls_context`/`reset_rls_context`; RLS policies enabled via migration 009 consolidation; **full RLS enforcement pending completion of all P1 slices; execute compensation RLS tx is NO-GO bounded due to transaction-unaware executor boundary** |
| Forensic bundle (model, generation, API, replay) | 🔄 In Progress | Bounded slices delivered: verification API, export API, integrity hashing, replay surface; **bundle generation (POST /forensic/bundle) with env-gated S3BundleStorage wiring (FORENSIC_BUNDLE_STORAGE=s3); default remains in-memory storage; list bundles (GET /forensic/bundles) and download API surfaces delivered as bounded in-memory; S3 retrieval/storage lifecycle, Object Lock, retention enforcement, chain-hash remain Phase 4+ deferred scope**; full replay remains open |
| NATS consumer lifecycle (Phase 4 first slice) | ✅ Bounded Delivered | Bounded multi-consumer registry (`ConsumerRegistry`) implemented; single consumer (`CheckpointCreatorConsumer`) wired behind `INTENT_API_NATS_CONSUMER=true` gate; graceful shutdown via shared watch channel; fail-open on NATS unavailability; **Bounded DLQ metrics worker (`DlqMetricsWorker`) delivered behind `INTENT_API_NATS_DLQ_WORKER=true` gate — emits depth/age gauges; full DLQ replay NOT enabled (Phase 4+ scope)** |
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

## Local Verification Matrix

**Remote GitHub Actions CI is intentionally disabled** — no automatic runs on push or pull_request. This is a deliberate choice to avoid CI costs on a personal project with no collaborators.

Local verification is the source of truth:

| Check | Command | Expected |
|-------|---------|----------|
| Format | `cargo fmt --all -- --check` | No diff |
| Clippy | `cargo clippy --all-features -- -D warnings` | No warnings |
| Type check | `cargo check --all` | Success |
| Unit tests | `cargo test --all-features` | All pass |
| OpenAPI spec | `npx spectral lint docs/04-api/openapi.yaml` | No errors |
| Git check | `git diff --check` | No conflicts |

**Full test suite** (requires Postgres via docker-compose):
```bash
docker compose -f infrastructure/local/docker-compose.yml up -d
cargo test -p intent-service --test migration_integration -- --ignored
```

**To manually trigger CI in GitHub Actions** (if ever needed):
1. Go to the **Actions** tab
2. Select **CI** or **Smoke Test** workflow
3. Click **Run workflow**

---

## Prioritized Next Steps

For detailed P0/P1/P2 production readiness backlog, see [Production Readiness Backlog](./17-production-readiness-backlog.md).

| Priority | Area | Action | Owner |
|----------|------|--------|-------|
| **P0** | CI/Actions intentionally disabled | Remote CI disabled by design; local gates are source of truth | Backend Lead |
| **P1** | RLS transaction wrapping | Execute P1-S1 (RlsAwarePool shared), P1-S3 (RlsTransactionExt), P1-S4 (graph_edge), P1-S5 (compensation/forensic/orchestration/approval/artifact); expand RLC-4..RLC-9 | Backend Lead |
| **P1** | External SRE sign-off | Obtain external SRE review and approval | SRE |
| **P1** | External security sign-off | Obtain external security review and approval | Security |
| **P1** | Production infra | Provision and verify production infrastructure | SRE |
| **P1** | Load testing (L3-L5) | Execute staged and production load tests | SRE |
| **P1** | Penetration testing | Engage external pen test and remediate findings | Security |
| **P2** | Panic hardening (local-executable) | Add panic handlers and graceful degradation | Backend Lead |
| **P2** | File decomposition (local-executable) | Large module decomposition for maintainability | Backend Lead |
| **P2** | DLQ/NATS lifecycle | Implement full DLQ replay worker (after G1-G5 gates) | Backend Lead |
| **P2** | Cross-process trace propagation | Revisit when Temporal SDK supports safe per-request gRPC metadata injection | Backend Lead / SRE |
| **P2/Phase4** | Forensic replay | Full replay capability + Object Lock/chain-hash for snapshots | Backend Lead |

> **Note:** Phase 3 bounded commits are pushed to origin/main. Local canonical gates are the source of truth: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check --workspace`, `cargo test --workspace --all-features -j 1`, targeted nats/s3_snapshot/snapshot_creator/trace_context/approval_revalidation/forensic tests, and `git diff --check` all pass. RLC-3 bounded RLS validation passed locally: migration_integration 1/1 passed, rls_integration --ignored 4/4 passed. **GitHub Actions CI is intentionally disabled by design** — no automatic runs on push or pull_request to avoid CI costs. This is a deliberate choice for a personal project with no collaborators. Production readiness is not claimed.

---

## Related Docs

- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Batch 0 Execution](./06-phase-3-batch-0-execution.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
- [Phase 3 Completion Execution Plan](./15-phase-3-completion-execution-plan.md)
- [10 Completion Proposals Tracker](./09-completion-proposals-tracker.md)
- [Production Readiness Backlog](./17-production-readiness-backlog.md) — P0/P1/P2 blockers for production deployment
- [CI/CD](../09-operations/02-ci-cd.md) — Actual vs aspirational CI/CD pipeline state
