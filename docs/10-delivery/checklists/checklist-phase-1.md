# Phase 1 — Core Control Plane MVP Checklist

**Exit Gate:** Phase 1 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 0 exit gate passed.

**Trạng thái:** `NOT STARTED`  
**Phase:** Phase 1  
**Target Duration:** 4–8 tuần

---

## 1. Intent Schema & Versioning

```
[x] Intent data model implemented
    Evidence:
    - Branch: phase1/db-http-wiring (PR pending)
    - Code: crates/intent-rebase-types/src/intent.rs (Intent, IntentVersion, IntentPayload types)
    - Tests: crates/intent-service/src/lib.rs unit tests
    - Docs: docs/03-spec/01-intent-model.md (already present)

[x] Intent versioning (create, update, list versions) implemented
    Evidence:
    - Branch: phase1/db-http-wiring (PR pending)
    - Code: crates/intent-service/src/lib.rs (IntentService.create_intent, create_version, list_versions)
    - Tests: test_create_version, test_list_versions, test_list_versions_descending_order, test_get_specific_version
    - Migration: infrastructure/migrations/002_create_intent_versions.sql

[x] Intent ID generation and uniqueness enforced
    Evidence:
    - Tests: test_in_memory_repo_persistence (verifies in-memory repo persistence with shared state)
    - DB: intents.intent_id is PRIMARY KEY in 001_create_intents.sql

[ ] Intent schema validation (JSON Schema or equivalent)
    Note: IntentPayload deserialization is validated via serde; explicit JSON Schema not yet added
    (Deferred to future PR if Phase 1 validation needed)
```

---

## 2. Semantic Diff v1

```
[x] Diff computation algorithm implemented (engine-core only — PR #5)
    Evidence:
    - PR merged: #5
    - Code: crates/rebase-engine/src/diff.rs
    - Tests: crates/rebase-engine/src/diff.rs and crates/rebase-engine/src/lib.rs

[x] Risk analysis: severity, confidence, manual-review triggers (engine-core only — PR #6)
    Evidence:
    - Branch: phase1/diff-risk-rules-v1 (PR pending)
    - Code: crates/rebase-engine/src/risk.rs, crates/rebase-engine/src/rules.rs
    - Tests: risk.rs, rules.rs, and lib.rs cover severity levels, confidence scoring, manual-review triggers, and engine API behavior

[ ] Diff threshold configuration (via rule pack)
    Evidence:
    - Rule pack v1 includes diff rules
    - Tests: threshold behavior verified
    (Deferred to future PR — not in scope for engine-core slice)

[ ] Diff API endpoint: POST /api/v1/intents/{id}/diff
    Evidence:
    - OpenAPI spec updated: ../../04-api/01-rest-api.md
    - Integration test passes
    (Deferred to future PR — HTTP layer not in scope for this slice)

[x] Structured diff output for scope/constraints/acceptance/authority
    Evidence:
    - Code: crates/rebase-engine/src/diff.rs (IntentVersionDiff and related types)
    - Tests: tests covering section add/remove/modify, determinism, ambiguity fallback, duplicate identity handling
    - NOT including: similarity score (deferred to future PRs)

[x] Engine-local diff risk rules (severity, confidence, manual-review)
    Evidence:
    - Code: crates/rebase-engine/src/risk.rs (DiffRiskAnalysis, Severity, RiskConfig)
    - Code: crates/rebase-engine/src/rules.rs (analyze_diff_risk, matching stats)
    - Tests: tests covering all severity levels, confidence thresholds, manual-review triggers, and engine API behavior
```

---

## 3. Graph Model v1

```
[ ] Graph data model (nodes, edges, labels) implemented
    Evidence:
    - PR merged: <link>
    - Code: graph-service/model.rs
    - Schema: 005_dependency_graph.sql

[ ] Graph CRUD operations (add node, add edge, query) implemented
    Evidence:
    - PR merged: <link>
    - Code: graph-service/crud.rs
    - Tests: graph-service/tests/crud_tests.rs

[ ] Graph traversal (BFS, path finding) implemented
    Evidence:
    - PR merged: <link>
    - Code: graph-service/traversal.rs
    - Tests: traversal tests pass

[ ] Graph propagation rules from rule pack applied
    Evidence:
    - Code: graph-service/propagation.rs
    - Tests: propagation rule tests pass

[ ] Graph API endpoints: GET /api/v1/graph, POST /api/v1/graph/nodes, POST /api/v1/graph/edges
    Evidence:
    - OpenAPI spec updated
    - Integration tests pass
```

---

## 4. Rebase Preview Only

```
[ ] Rebase plan computation implemented
    Evidence:
    - PR merged: <link>
    - Code: rebase-engine/planner.rs
    - Tests: rebase-engine/tests/planner_tests.rs

[ ] Rebase preview endpoint: POST /api/v1/intents/{id}/rebase-preview
    Evidence:
    - OpenAPI spec updated
    - Response schema: { affected_artifacts, affected_approvals, compensation_actions, risk_level }

[ ] Rebase preview includes: affected artifacts list, approval invalidation list, compensation recommendations
    Evidence:
    - Test: preview output schema validated
    - Docs: ../../03-spec/04-rebase-engine.md (updated)

[ ] NO rebase apply in Phase 1 — preview only
    Evidence:
    - API returns 404 or error if apply attempted
    - Error message: "Apply rebase not enabled until Phase 2"
```

---

## 5. Console Basic (Frontend MVP)

```
[ ] Intent list view (list all intents, filterable)
    Evidence:
    - PR merged: <link>
    - UI: Next.js page /intents

[ ] Intent detail view (version history, diff viewer)
    Evidence:
    - PR merged: <link>
    - UI: Next.js page /intents/[id]

[ ] Rebase preview display (affected artifacts, approvals, risk)
    Evidence:
    - PR merged: <link>
    - UI: Next.js component RebasePreview

[ ] Basic authentication (login/logout, session management)
    Evidence:
    - PR merged: <link>
    - Auth: nextauth.js or equivalent

[ ] Tenant context switching (for multi-tenant dev testing)
    Evidence:
    - Dev mode: tenant selector in UI
    - Production: tenant extracted from auth token
```

---

## 6. Audit Baseline

```
[ ] Audit event model defined (event_type, actor, target, timestamp, metadata)
    Evidence:
    - Doc: ../../14-governance/01-audit-event-spec.md
    - Code: audit-service/event.rs

[ ] Audit event append API implemented
    Evidence:
    - PR merged: <link>
    - Code: audit-service/append.rs
    - Tests: audit-service/tests/append_tests.rs

[ ] Audit event query API implemented (filter by tenant, time range, event type)
    Evidence:
    - PR merged: <link>
    - Code: audit-service/query.rs
    - Tests: query tests pass

[ ] Audit events persisted to PostgreSQL
    Evidence:
    - Migration: 006_audit_events.sql
    - No UPDATE/DELETE on audit table (append-only)

[ ] Audit event streaming to NATS (if ADR-04 uses NATS)
    Evidence:
    - Code: audit-service/stream.rs
    - Integration test: events appear in NATS subject
```

---

## 7. Data Schema & Migrations

```
[x] Phase 1 schemas migrated in order (001-003)
    Evidence:
    - Migration files: infrastructure/migrations/001_create_intents.sql,
      002_create_intent_versions.sql, 003_create_intent_clauses.sql
    - Migrations use IF NOT EXISTS and are idempotent (CREATE TABLE IF NOT EXISTS)
    - Test: unit tests pass with in-memory repo; SQL repo tests require live DB

[ ] Post-migration data validation tests
    Note: Integration tests with live DB not yet added to CI (PR #4 SQL repo is implemented
    but DB integration tests are skipped in CI; can be added in future PR)
    Evidence:
    - SQL repo implementation: crates/intent-service/src/sqlx_repository.rs
    - Unit tests: crates/intent-service/src/sqlx_repository.rs tests (deserialization,
      helper functions — no live DB required)
    - Note: Live DB integration tests to be added in follow-up PR

[ ] Rollback plan documented for each migration
    Note: Rollback steps not explicitly documented in migration files
    Evidence:
    - Migration comments document forward semantics only
```

---

## 8. Observability v1

```
[ ] Structured JSON logging implemented in all services
    Evidence:
    - PR merged: <link>
    - Code: all services use tracing/log structured

[ ] Prometheus metrics exposed on /metrics
    Evidence:
    - All HTTP services expose /metrics
    - Metrics: intent_operations_total, rebase_previews_total, graph_operations_total

[ ] Health check endpoints: /health, /ready
    Evidence:
    - All services expose /health
    - Kubernetes readiness probe configured

[ ] OTel tracing (basic span instrumentation)
    Evidence:
    - PR merged: <link>
    - Spans: intent.create, intent.update, diff.compute, graph.traverse

[ ] Loki or equivalent log aggregation configured
    Evidence:
    - Dev: loki container in docker-compose
    - Prod: cloud logging configured
```

---

## 9. Security v1

```
[ ] API authentication: API key + JWT validation
    Evidence:
    - PR merged: <link>
    - Middleware: auth.rs
    - Tests: auth tests pass

[ ] Tenant isolation: tenant_id extracted from token, not request
    Evidence:
    - Tests: cross-tenant access blocked
    - Doc: ../../08-security/02-authn-authz.md (updated)

[ ] Input validation on all API endpoints
    Evidence:
    - Code: all endpoints validate input
    - Tests: invalid input rejected

[ ] No PII in logs (tenant_id only, no user email/name)
    Evidence:
    - Log review: no PII present
    - Doc: ../../08-security/03-privacy-and-data-handling.md (updated)
```

---

## 10. API Contract & Documentation

```
[x] OpenAPI 3.0 spec for Phase 1 intent/version endpoints
    Evidence:
    - File: docs/04-api/openapi.yaml (Phase 1 endpoints: createIntent, getIntentHead,
      createVersion, listVersions, getVersion — with OCC headers documented)
    - Routes manually wired in crates/intent-api/src/lib.rs

[ ] Event contracts documented
    Note: NATS subjects not yet defined (deferred to Phase 2+)
    Evidence:
    - Doc: docs/04-api/02-events.md (skeleton only)

[ ] Webhook payload schemas documented
    Note: Webhooks not yet implemented (deferred to Phase 2+)
    Evidence:
    - Doc: docs/04-api/03-webhooks.md (skeleton only)

[x] API change policy: OpenAPI spec must update with code
    Note: No CI enforcement yet; policy documented in agent guide
    Evidence:
    - Doc: docs/12-agents/01-agent-implementation-guide.md (existing)
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Phase 1 Exit Gate Review Date: ___________
Reviewed By: ___________
Product Owner Sign-off: ___________
Security Sign-off: ___________

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 2 — Runtime-Integrated Rebase](./checklist-phase-2.md)
