# Phase 1 — Core Control Plane MVP Checklist

**Exit Gate:** Phase 1 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 0 exit gate passed.

**Trạng thái:** `NOT STARTED`  
**Phase:** Phase 1  
**Target Duration:** 4–8 tuần

---

## 1. Intent Schema & Versioning

```
[ ] Intent data model implemented
    Evidence:
    - PR merged: <link>
    - Code: intent-service/intent.rs
    - Tests: intent-service/tests/intent_tests.rs
    - Docs: ../../03-spec/01-intent-model.md (updated)

[ ] Intent versioning (create, update, list versions) implemented
    Evidence:
    - PR merged: <link>
    - Code: intent-service/version.rs
    - Tests: intent-service/tests/version_tests.rs
    - Migration: 001_intent_versioning.sql

[ ] Intent ID generation and uniqueness enforced
    Evidence:
    - Tests: uniqueness tests pass

[ ] Intent schema validation (JSON Schema or equivalent)
    Evidence:
    - PR merged: <link>
    - Code: intent-service/schema_validation.rs
    - Tests: validation tests pass
```

---

## 2. Semantic Diff v1

```
[ ] Diff computation algorithm implemented
    Evidence:
    - PR merged: <link>
    - Code: rebase-engine/diff.rs
    - Tests: rebase-engine/tests/diff_tests.rs

[ ] Diff threshold configuration (via rule pack)
    Evidence:
    - Rule pack v1 includes diff rules
    - Tests: threshold behavior verified

[ ] Diff API endpoint: POST /api/v1/intents/{id}/diff
    Evidence:
    - OpenAPI spec updated: ../../04-api/01-rest-api.md
    - Integration test passes

[ ] Diff output includes: added fields, removed fields, modified fields, similarity score
    Evidence:
    - Test: output schema validated
    - Docs: ../../03-spec/02-semantic-diff.md (updated)
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
[ ] All Phase 1 schemas migrated in order
    Evidence:
    - Migration files: 001-006 numbered
    - Test: fresh DB from migrations passes all tests

[ ] Schema migrations are idempotent (safe to re-run)
    Evidence:
    - Tests: migration re-run passes

[ ] Post-migration data validation tests
    Evidence:
    - Tests: seed data validates correctly

[ ] Rollback plan documented for each migration
    Evidence:
    - Migration comments include rollback steps
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
[ ] OpenAPI 3.1 spec for all Phase 1 endpoints
    Evidence:
    - File: ../../04-api/openapi.yaml
    - Validation: openapi-validate passes

[ ] API change policy: OpenAPI spec must update with code
    Evidence:
    - CI: openapi-validate in pipeline
    - Doc: ../../12-agents/01-agent-implementation-guide.md (updated)

[ ] Event contracts documented
    Evidence:
    - Doc: ../../04-api/02-events.md (updated)
    - NATS subjects documented

[ ] Webhook payload schemas documented
    Evidence:
    - Doc: ../../04-api/03-webhooks.md (updated)
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