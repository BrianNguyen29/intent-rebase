# Phase 3 — Compensation + Production Hardening Checklist

**Exit Gate:** Phase 3 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 2b exit gate passed. Phase 2b scope includes: runtime adapter external implementation, apply endpoint, risk classification, graph update, replay API, event streaming. Phase 3 Batch 0 (hardening planning and scaffold prep) may proceed in parallel while Phase 2b is in progress — see [05-phase-3-hardening.md](../05-phase-3-hardening.md) for batch structure.

**Trạng thái:** `BATCH 0 COMPLETE, BATCH 1 IN PROGRESS` — Batch 0 code scaffolds delivered; Batch 0 planning/admin items remain open. Batch 1 side effect ledger groundwork delivered (model, query API, idempotency, capture-on-write for ingest_artifact). Compensation actions query API delivered (read-only). Formal Batch 1 completion (full planner, executor, retry, rollback record) remains gated on Phase 2b exit and remaining Batch 0 prep. See [05-phase-3-hardening.md](../05-phase-3-hardening.md) and [06-phase-3-batch-0-execution.md](../06-phase-3-batch-0-execution.md) for the current execution split.  
**Phase:** Phase 3  
**Target Duration:** 6–10 tuần

---

## Batch 0 Progress Snapshot

```
[x] Batch 0 scaffold: compensation service package structure created
    Evidence:
    - Code: crates/compensation-service/Cargo.toml
    - Code: crates/compensation-service/src/lib.rs
    - Code: crates/compensation-service/src/side_effect.rs
    - Code: crates/compensation-service/src/compensation_action.rs
    - Tests: cargo test -p compensation-service --all-features

[x] Batch 0 scaffold: forensic service package structure created
    Evidence:
    - Code: crates/forensic-service/Cargo.toml
    - Code: crates/forensic-service/src/lib.rs
    - Code: crates/forensic-service/src/bundle.rs
    - Code: crates/forensic-service/src/bundle_contents.rs
    - Tests: cargo test -p forensic-service --all-features

[x] Batch 0 groundwork: Phase 3 audit taxonomy extended for compensation/forensic flows
    Evidence:
    - Code: crates/intent-rebase-types/src/audit.rs
    - Code: crates/intent-rebase-types/src/audit_repo.rs
    - Scope: additive event taxonomy only; no producer/consumer wiring yet

[~] Batch 0 planning/admin items partially prepared
    Evidence:
    - Dependency audit artifact: ../07-phase-3-dependency-audit.md
    - Phase 2b security input artifact: ../08-phase-2b-security-findings-input.md
    - Provisional SLO prep: ../../09-operations/04-sre-and-slos.md
    - Ownership/sign-off still awaits named assignees and external confirmation
    - Final SRE/security/compliance sign-off remains open
    - Tracking plan: ../06-phase-3-batch-0-execution.md
```

---

## 1. Side Effect Ledger

```
[x] Side effect model (effect_id, intent_id, intent_version, effect_type, target, timestamp, tenant_id)
    Evidence:
    - PR merged: <link>
    - Code: crates/compensation-service/src/side_effect.rs (tenant_id added)
    - Code: crates/compensation-service/src/side_effect_repo.rs (persist/query groundwork only)
    - Schema: infrastructure/migrations/010_create_side_effects_ledger.sql
    - Note: This slice delivers persistence + repository groundwork only. Capture-on-write, API, idempotency enforcement, and rollback records remain open below.

[~] Side effect capture on artifact-producing operations (Phase 3 Batch 1 groundwork)
    Evidence:
    - Code: crates/intent-rebase-types/src/graph.rs (SideEffectCaptureContext added)
    - Code: crates/graph-service/src/lib.rs (side_effect_context field on ArtifactIngestRequest, documentation updated)
    - Code: crates/intent-api/src/lib.rs (ingest_artifact endpoint with optional side effect capture, 16 validation tests for side_effect_context)
    - Tests: cargo test -p graph-service --all-features (78 tests pass), cargo test -p intent-api --all-features (67 tests pass)
    - Note: Delivered capture path is artifact-ingest only (via POST /v1/graph/artifacts with side_effect_context). Broader capture across other artifact-producing operations remains open and requires artifact-service integration.

[x] Side effect query API for compensation planning (Phase 3 Batch 1 groundwork)
    Evidence:
    - PR merged: <link>
    - Code: crates/compensation-service/src/side_effect_service.rs (service facade with list_side_effects_by_intent)
    - Code: crates/intent-api/src/lib.rs (GET /intents/{intent_id}/side-effects endpoint)
    - Tests: cargo test -p compensation-service --all-features (30 tests pass), cargo test -p intent-api --all-features (67 tests pass)

[x] Side effect idempotency keys (Phase 3 Batch 1 groundwork - via service facade)
    Evidence:
    - Code: crates/compensation-service/src/side_effect_service.rs (atomic record_side_effect_with_idempotency)
    - Code: crates/compensation-service/src/side_effect_repo.rs (tenant-scoped get_or_create_idempotent)
    - Tests: cargo test -p compensation-service --all-features (idempotency tests pass, including concurrent duplicate protection)
    - Note: Atomic tenant-scoped idempotency is now implemented in the service/repository path. Broader artifact-service coverage remains open.

[ ] Side effect rollback record (compensation applied, compensation result)
    Evidence:
    - Code: compensation-service/rollback.rs
    - Schema: side-effect rollback migration TBD
```

---

## 2. Compensation Engine

```
[~] Compensation action model and repository (Batch 1 scaffold)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action.rs (model)
    - Code: crates/compensation-service/src/compensation_action_repo.rs (trait + InMemory/SQL implementations)
    - Schema: infrastructure/migrations/011_create_compensation_actions.sql
    - Tests: cargo test -p compensation-service --all-features
    - Note: Planner and executor remain as stubs (Batch 1+ scope)

[x] Compensation actions query API (Phase 3 Batch 1 bounded read-only slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /intents/{intent_id}/compensation-actions endpoint)
    - Code: crates/compensation-service/src/compensation_action_service.rs (list_by_intent method)
    - Tests: cargo test -p intent-api --all-features (67 tests pass)
    - Note: This endpoint is READ-ONLY - does not trigger compensation execution. Planner/executor remain Batch 1+ scope.

[ ] Compensation planner: generate compensation plan from side effects
    Evidence:
    - Code: compensation-service/planner.rs (stub scaffold only)
    - Status: Batch 1+ scope

[ ] Compensation executor: execute compensation actions
    Evidence:
    - Code: compensation-service/executor.rs (stub scaffold only)
    - Status: Batch 1+ scope

[ ] Compensation retry logic (max retries, backoff, dead-letter)
    Evidence:
    - Code: compensation-service/retry.rs (stub scaffold only)
    - Status: Batch 1+ scope

[ ] Compensation audit trail
    Evidence:
    - Audit events: compensation.planned, compensation.started, compensation.completed, compensation.failed
    - Doc: ../../14-governance/01-audit-event-spec.md (updated)
    - Status: Batch 1+ scope
```

---

## 3. SRE & Observability

```
[ ] SLO definitions (intent processing latency, rebase latency, approval wait time)
    Evidence:
    - PR merged: <link>
    - Doc: ../../09-operations/04-sre-and-slos.md (updated)
    - Dashboard: Grafana SLO dashboard

[ ] Alerting rules (warning, critical thresholds)
    Evidence:
    - PR merged: <link>
    - Alert rules: alertmanager.yml or equivalent
    - Tests: alerting tests pass (send test alerts)

[ ] Error budget tracking
    Evidence:
    - Dashboard: error budget dashboard
    - Runbook: error budget exceeded response

[ ] Distributed tracing across all services (full Phase 2 → Phase 3 trace)
    Evidence:
    - PR merged: <link>
    - OTel: trace context propagated across all service boundaries
    - Jaeger/Zipkin: trace searchable

[ ] Performance benchmarks: rebase latency p50/p95/p99
    Evidence:
    - Benchmark results: rebase_engine benchmarks
    - Target: p95 < 60s for low/medium risk

[ ] Runbooks for common failure scenarios
    Evidence:
    - Doc: ../../09-operations/05-runbooks.md (updated with Phase 3 scenarios)
    - Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout
```

---

## 4. Tenant Isolation Hardening

```
[ ] Tenant isolation verification tests
    Evidence:
    - PR merged: <link>
    - Tests: cross-tenant access attempts blocked
    - Tests: data leakage tests (tenant A cannot see tenant B data)

[ ] Resource quota enforcement (intents per tenant, artifacts per tenant)
    Evidence:
    - PR merged: <link>
    - Code: tenant-service/quota.rs
    - Tests: quota enforcement tests pass

[ ] Tenant-specific rule pack isolation
    Evidence:
    - Code: rule-pack-service/tenant_isolation.rs
    - Tests: pack isolation tests pass

[ ] Tenant audit log separation
    Evidence:
    - Tests: tenant A events not visible in tenant B queries
    - S3: tenant-scoped buckets/prefixes

[ ] Data residency: tenant data stays in assigned region
    Evidence:
    - Doc: ../../08-security/01-threat-model.md (updated)
    - Code: multi-region routing

[ ] Tenant onboarding/offboarding procedures documented
    Evidence:
    - Doc: tenant operations runbook
    - Tests: offboarding removes all tenant data
```

---

## 5. Forensic Replay Bundle

```
[ ] Forensic bundle model (bundle_id, intent_id, time_range, contents)
    Evidence:
    - PR merged: <link>
    - Code: forensic-service/bundle.rs

[ ] Bundle generation: collect intent versions, artifacts, audit events, graph state
    Evidence:
    - PR merged: <link>
    - Code: forensic-service/generator.rs
    - S3: forensic-bundles/{tenant}/{bundle_id}/

[ ] Bundle generation API: POST /api/v1/forensic/bundle
    Evidence:
    - OpenAPI spec updated
    - Role required: forensic-access

[ ] Bundle integrity verification (hash chain)
    Evidence:
    - Code: forensic-service/integrity.rs
    - Tests: integrity verification tests pass

[ ] Bundle replay capability (replay bundle to reproduce state)
    Evidence:
    - Code: forensic-service/replay.rs
    - Tests: replay tests pass

[ ] Bundle retention policy (configurable per tenant, compliance)
    Evidence:
    - Code: forensic-service/retention.rs
    - S3 lifecycle: configurable retention period

[ ] Forensic bundle export (download as tar.gz)
    Evidence:
    - API: GET /api/v1/forensic/bundles/{id}/download
    - Tests: download tests pass
```

---

## 6. Performance Work

```
[ ] Intent diff optimization (caching, parallel computation)
    Evidence:
    - PR merged: <link>
    - Benchmark: diff computation < 100ms for typical intent
    - Code: rebase-engine/diff_cache.rs

[ ] Graph traversal optimization (indexing, query optimization)
    Evidence:
    - PR merged: <link>
    - Benchmark: graph traversal < 50ms for 10k node graph
    - Code: graph-service/indexing.rs

[ ] Database query optimization (indexes, query plans)
    Evidence:
    - PR merged: <link>
    - EXPLAIN ANALYZE on critical queries
    - New indexes: optimization index migration TBD

[ ] Connection pooling (Postgres, NATS)
    Evidence:
    - Code: connection pool configured
    - Benchmark: no connection exhaustion under load

[ ] Load testing: simulate Phase 3 production load
    Evidence:
    - Load test results: 10x normal load sustained
    - No SLO violations under load
    - Report: load-test-results.md
```

---

## 7. Security Hardening

```
[ ] Threat model v2 (updated from Phase 1)
    Evidence:
    - PR merged: <link>
    - Doc: ../../14-governance/06-threat-model-v2.md

[ ] Penetration testing completed
    Evidence:
    - Report: penetration-test-results.md
    - All critical/high findings remediated

[ ] Security review for all Phase 3 features
    Evidence:
    - Review sign-off: security-team
    - Findings: none critical/high unmitigated

[ ] Compliance checklist (if applicable: SOC2, GDPR, etc.)
    Evidence:
    - Doc: compliance-checklist.md
    - All items checked

[ ] Incident response plan documented
    Evidence:
    - Doc: ../../14-governance/11-incident-freeze.md
    - Runbook: incident-response.md

[ ] Data retention and deletion verified
    Evidence:
    - Tests: deletion removes data within SLA
    - S3 lifecycle policies enforced
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Phase 3 Exit Gate Review Date: ___________
Reviewed By: ___________
Product Owner Sign-off: ___________
Security Sign-off: ___________
SRE Sign-off: ___________
Compliance Sign-off: ___________

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 4 — Enterprise Expansion](./checklist-phase-4.md)
