# Current Project Status

## Executive Summary

**Current Phase:** Phase 3 — Compensation + Production Hardening, Batch 1 largely delivered.  
**Phase 2b status:** In progress; Phase 3 full execution is gated on Phase 2b exit.  
**Phase 3 Batch 1 delivered:** Side effect ledger, compensation-actions CRUD + APIs, batch orchestration, policy gate, orchestration dashboard, orchestration coordination view, dry-run planner, and single-shot orchestration runtime (HTTP + CLI). Full planner/executor/retry/rollback record remains gated on Phase 2b exit.  
**Production readiness:** Not yet production-ready. Phase 3 Batch 1 delivers bounded API surfaces; SRE hardening, tenant isolation, forensic replay, and performance work are still open.

---

## Implemented Phases on Main

| Phase | Status | Key Delivered |
|-------|--------|----------------|
| Phase 0 — Foundations | ✓ Complete | Repo scaffold, ADRs, architecture baseline, local dev, CI |
| Phase 1 — Core Control Plane MVP | ✓ Complete | Intent schema + versioning (PR #21), Graph HTTP API (PR #22), Observability v1 (PR #23), Security v1 (PR #24) |
| Phase 2 — Runtime-Integrated Rebase | ⚠️ Partial | Phase 2a runtime adapter delivered; Phase 2b in-flight (apply endpoint, risk classification, graph update, replay API, event streaming) |
| Phase 3 — Compensation + Hardening | 🔄 Active | Batch 0 scaffold + planning ✅; Batch 1 largely delivered ⚠️; Batches 2–4 not started |

---

## Phase 3 Batch 1 — Delivered Surfaces

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
- Execute API: `POST /compensation-actions/{action_id}/execute` — executor gate: only Approved actions execute; stub executor returns success
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

---

## Key Open Areas

| Area | Status | Blocking |
|------|--------|----------|
| Phase 2b exit gate | In progress | Required before Phase 3 Batch 2+ |
| Side effect rollback record (compensation applied, result) | Not started | |
| Compensation planner (full — stub delivered) | Not started | |
| Compensation executor (real rollback/counter-action — stub delivered) | Not started | |
| Compensation audit trail | Not started | |
| SLO definitions + alerting + error budget | Not started | |
| Distributed tracing across Phase 2→3 | Not started | |
| Performance benchmarks | Not started | |
| Runbooks | Not started | |
| Tenant isolation verification tests | Not started | |
| Forensic bundle (model, generation, API, replay) | Not started | |
| Threat model v2, penetration testing | Not started | |
| Load testing | Not started | |

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

## Related Docs

- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Batch 0 Execution](./06-phase-3-batch-0-execution.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
- [10 Completion Proposals Tracker](./09-completion-proposals-tracker.md)
