# Roadmap

## Phase 0 — Foundations (2–4 tuần)
- product spec finalized
- domain model finalized
- repo scaffolding
- ADRs
- architecture baseline
- local dev environment
- CI baseline

## Phase 1 — Core Control Plane MVP (4–8 tuần)
- intent schema + versioning
- semantic diff v1
- graph model v1
- rebase preview only
- console basic
- audit baseline

## Phase 2 — Runtime-Integrated Rebase (6–10 tuần)
- runtime adapter v1
- checkpoint mapping
- apply rebase for low/medium risk
- approvals revalidation
- artifact invalidation + quarantine

## Phase 3 — Compensation + Production Hardening (6–10 tuần)
- side effect ledger
- compensation engine
- SRE/observability
- tenant isolation hardening
- forensic replay bundle
- performance work
- **ADR-11 bounded MVP delivered (non-production):** `GET /policy-snapshots/{snapshot_id}/impact-report` — read-only ImpactReport for a policy snapshot's intent, reusing ADR-10 semantics. Full `PolicyRebaseAdapter` deferred to Phase 4+.

> **Phase 3 Batch 1 delivered:** side effect ledger (model/query/idempotency/capture-on-write), compensation actions CRUD + approve/waive/execute APIs, DLQ, batch orchestration (approve/reapprove/execute), policy gate evaluation, orchestration dashboard, orchestration coordination view, orchestration dry-run, single-shot orchestration runtime (HTTP + CLI). Phase 2b exit is closed; full planner/executor/retry/rollback record delivered as part of Phase 3 Batch 1 bounded slices.

**Current status:** See [Current Project Status](./00-current-status.md) for a detailed snapshot of what is delivered and what remains open.

**10 completion proposals:** See [Completion Proposals Tracker](./09-completion-proposals-tracker.md) for a structured view of all remaining major work items.

## Phase 4 — Enterprise Expansion (ongoing)
- policy simulation
- advanced adapters
- cross-workflow intent families
- trust scoring by source
- enterprise integrations
