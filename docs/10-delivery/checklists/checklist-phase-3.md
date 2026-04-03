# Phase 3 — Compensation + Production Hardening Checklist

**Exit Gate:** Phase 3 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 2 exit gate passed.

**Trạng thái:** `NOT STARTED`  
**Phase:** Phase 3  
**Target Duration:** 6–10 tuần

---

## 1. Side Effect Ledger

```
[ ] Side effect model (effect_id, intent_id, intent_version, effect_type, target, timestamp)
    Evidence:
    - PR merged: <link>
    - Code: compensation-service/side_effect.rs
    - Schema: 008_side_effects_ledger.sql

[ ] Side effect capture on all artifact-producing operations
    Evidence:
    - PR merged: <link>
    - Code: artifact-service/effect_capture.rs
    - Tests: effect capture tests pass

[ ] Side effect query API for compensation planning
    Evidence:
    - PR merged: <link>
    - API: GET /api/v1/intents/{id}/side-effects
    - Tests: query tests pass

[ ] Side effect idempotency keys (prevent duplicate compensation)
    Evidence:
    - Code: compensation-service/idempotency.rs
    - Tests: idempotency tests pass

[ ] Side effect rollback record (compensation applied, compensation result)
    Evidence:
    - Code: compensation-service/rollback.rs
    - Schema: 009_side_effect_rollbacks.sql
```

---

## 2. Compensation Engine

```
[ ] Compensation action model (action_type, target, parameters, status)
    Evidence:
    - PR merged: <link>
    - Code: compensation-service/action.rs

[ ] Compensation planner: generate compensation plan from side effects
    Evidence:
    - PR merged: <link>
    - Code: compensation-service/planner.rs
    - Tests: planner tests pass

[ ] Compensation executor: execute compensation actions
    Evidence:
    - PR merged: <link>
    - Code: compensation-service/executor.rs
    - Integration test: compensation executed end-to-end

[ ] Compensation retry logic (max retries, backoff, dead-letter)
    Evidence:
    - Code: compensation-service/retry.rs
    - Tests: retry tests pass

[ ] Compensation audit trail
    Evidence:
    - Audit events: compensation.planned, compensation.started, compensation.completed, compensation.failed
    - Doc: ../../14-governance/01-audit-event-spec.md (updated)
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
    - New indexes: 010_optimization_indexes.sql

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