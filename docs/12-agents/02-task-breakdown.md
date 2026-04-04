# Task Breakdown for AI Agents

## Epic 1 — Intent Registry
- [x] Create DB migrations for intents, intent_versions, intent_clauses (Phase 1 - baseline)
- [x] Implement create intent endpoint (Phase 1 first slice - in-memory repo)
- [x] Implement create version endpoint (Phase 1 first slice)
- [x] Implement get current/head/version history (Phase 1 first slice)
- [x] Add optimistic concurrency checks (Phase 1 - PR #4)
- [x] SQL-backed repository (Phase 1 - PR #4, uses sqlx with PostgreSQL)

## Epic 2 — Semantic Diff
- Implement structured diff for scope/constraints/acceptance/authority
- Implement severity assignment rules
- Implement confidence and manual-review triggers
- Add diff API
- Add regression fixtures

## Epic 3 — Graph and Impact
- Create graph nodes/edges storage
- Build artifact/approval/side effect edge ingestors
- Implement impact traversal
- Implement classification output

## Epic 4 — Rebase Planner
- Build preview-only planner
- Add decision classes A–E
- Add checkpoint selection heuristics
- Add approval revalidation hooks

## Epic 5 — Adapter
- Define capability contract
- Implement chosen runtime adapter
- Support pause/resume/checkpoint lookup
- Support apply preview -> apply transitions

## Epic 6 — Console
- Intent version history
- Semantic diff view
- Rebase preview page
- Workflow timeline
- Approval stale alerts

## Epic 7 — Audit and Replay
- Append-only audit pipeline
- Forensic export endpoint
- Timeline UI

---

## Phase 1 First Slice - Completed Items
- [x] Intent domain types matching `docs/03-spec/01-intent-model.md`
- [x] Intent service with repository trait (in-memory implementation for tests)
- [x] `create_intent` operation
- [x] `create_version` operation
- [x] `get_intent_head` operation
- [x] `list_versions` operation
- [x] Migration baseline: `001_create_intents.sql`
- [x] Migration baseline: `002_create_intent_versions.sql`
- [x] Migration baseline: `003_create_intent_clauses.sql`
- [x] OpenAPI spec: `docs/04-api/openapi.yaml` (manually wired)
- [x] Unit tests for service layer
- [x] SQL-backed repository (`SqlxIntentRepository`) with PostgreSQL/sqlx (PR #4)
- [x] Optimistic concurrency control (OCC) via `X-Expected-Version` / `X-Expected-Row-Version` headers (PR #4)
- [x] HTTP transport layer with axum (`intent-api` crate, PR #4)
