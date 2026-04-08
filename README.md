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

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `intent-rebase-types` | Core type definitions |
| `intent-service` | Intent processing service |
| `intent-api` | HTTP API server |
| `rebase-engine` | Rebase decision engine |
| `graph-service` | Dependency graph service |
| `runtime-adapter` | Runtime execution adapter (Task 5) |

## Deferred / Fixed Issues

| Issue | Description | Status |
|-------|-------------|--------|
| parent_version_id | Fixed version chain integrity in intent versioning | ✓ Fixed |
