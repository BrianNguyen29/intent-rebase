# Intent Rebase Engine

Bootstrap repository for the Intent Rebase Engine documentation set.

See [`docs/README.md`](./docs/README.md) for the v1 documentation index, architecture notes, ADRs, delivery checklists, and governance pack.

## Phase 1 PR Slices

| PR | Feature | Status |
|----|---------|--------|
| PR #21 | Intent Schema Validation | ✓ Complete |
| PR #22 | Graph HTTP API | ✓ Complete |
| PR #23 | Observability v1 | ✓ Complete |
| PR #24 | Security v1 | ✓ Complete |

## Phase 3 Status

**Phase 3 Batch 1 delivered slices:** side effect ledger (model, query API, idempotency, capture-on-write), compensation actions query/approve/waive/execute APIs, DLQ API, batch approve/reapprove/execute, policy gate evaluation, orchestration dashboard, orchestration coordination view, orchestration dry-run, single-shot orchestration runtime (HTTP + CLI). Phase 2b exit is closed; full planner/executor/retry/rollback record delivered as part of Phase 3 Batch 1 bounded slices.

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `intent-rebase-types` | Core type definitions |
| `intent-service` | Intent processing service |
| `intent-api` | HTTP API server |
| `rebase-engine` | Rebase decision engine |
| `graph-service` | Dependency graph service |
| `runtime-adapter` | Runtime execution adapter |
| `rebase-orchestrator` | Orchestration coordination |
| `compensation-service` | Compensation action management |
| `forensic-service` | Forensic bundle replay |
| `tenant-service` | Multi-tenant onboarding and quota management |
| `intent-cli` | CLI for orchestration runs |

## Deferred / Fixed Issues

| Issue | Description | Status |
|-------|-------------|--------|
| parent_version_id | Fixed version chain integrity in intent versioning | ✓ Fixed |
