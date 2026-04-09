# Phase 3 — Hardening (Staged Execution Plan)

## Phase Overview

Phase 3 builds on Phase 2b completion. Full execution is dependency-gated; however, Batch 0 may start in parallel with Phase 2b.

**Hard Dependency:** Phase 2b exit gate  
**Optional Parallel Track:** Batch 0 (planning, scaffold, dependency audit) — may begin while Phase 2b is in progress

---

## Batch 0 — Planning + Hardening Scaffold (Parallel with Phase 2b)

*Gate: None — may start immediately. Must complete before Batch 1 starts.*

**Status:** `IN PROGRESS — scaffold slice delivered, planning/admin items remain` ⚠️

| Item | Description | Notes |
|------|-------------|-------|
| B0-1 | Finalize section ownership and review sign-offs | Placeholder owners can be recorded now; final named sign-offs remain open |
| B0-2 | Review Phase 2b artifact model for compensation coverage gaps | Captured as dependency/security-prep input; implementation follow-up remains |
| B0-3 | Stub compensation service package structure | `compensation-service/` scaffold ✅ delivered |
| B0-4 | Stub forensic bundle service package structure | `forensic-service/` scaffold ✅ delivered |
| B0-5 | Dependency audit: identify cross-service assumptions introduced in Phase 2b | Recorded in `07-phase-3-dependency-audit.md` ✅ |
| B0-6 | Confirm SLO targets with SRE team | Provisional targets documented; external confirmation still open ⚠️ |
| B0-7 | Threat model update v2 — gather Phase 2b findings | Input captured in `08-phase-2b-security-findings-input.md` ✅ |

Current execution tracking: see [06-phase-3-batch-0-execution.md](./06-phase-3-batch-0-execution.md).

---

## Batch 1 — Side Effect Ledger + Compensation Engine (Gated: Phase 2b Complete)

*Gate: Phase 2b exit gate confirmed. All Phase 2 acceptance criteria met.*

**Status:** `Batch 1 IN PROGRESS — side effect ledger and query API groundwork delivered`

| Item | Description | Notes |
|------|-------------|-------|
| 1-1 | Side effect model (`effect_id`, `intent_id`, `intent_version`, `effect_type`, `target`, `timestamp`, `tenant_id`) | Schema + repository groundwork ✅ delivered (Phase 2) |
| 1-2 | Side effect capture on all artifact-producing operations | Delivered: artifact-ingest only via POST /v1/graph/artifacts with optional `side_effect_context`. Other artifact-producing operations remain open for future coverage. |
| 1-3 | Side effect query API: `GET /intents/{intent_id}/side-effects` | ✅ delivered (Phase 3 Batch 1) |
| 1-4 | Side effect idempotency keys | Tenant-scoped atomic idempotency ✅ delivered in service/repository path. Broader artifact-service coverage remains open. |
| 1-5 | Side effect rollback record (compensation applied, compensation result) | Schema migration TBD |
| 1-6 | Compensation action model (`action_type`, `target`, `parameters`, `status`) | Scaffold ✅ delivered (Phase 3 Batch 0). Now includes `intent_id`, `trigger_context`, and `execution_result_payload` fields for bounded Phase 3 design. |
| 1-7 | Compensation planner: generate compensation plan from side effects | Stub ✅ skeleton contract delivered (Phase 3 Batch 1) |
| 1-8 | Compensation executor: execute compensation actions | Stub ✅ skeleton contract delivered (Phase 3 Batch 1) |
| 1-9 | Compensation retry logic (max retries, backoff, dead-letter) | |
| 1-10 | Compensation audit trail (`compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed`) | |

---

## Batch 2 — Observability + SRE (Gated: Phase 2b Complete + Batch 1 Checkpoint)

*Gate: Compensation engine basic path verified. Phase 2b event streaming available.*

| Item | Description | Notes |
|------|-------------|-------|
| 2-1 | SLO definitions (intent processing latency, rebase latency, approval wait time) | Dashboard: Grafana SLO dashboard |
| 2-2 | Alerting rules (warning, critical thresholds) | Alertmanager config; test alerts fire |
| 2-3 | Error budget tracking dashboard + runbook | |
| 2-4 | Distributed tracing across all services (full Phase 2 → Phase 3 trace) | OTel trace context across all service boundaries |
| 2-5 | Performance benchmarks: rebase latency p50/p95/p99 | Target: p95 < 60s for low/medium risk |
| 2-6 | Runbooks for: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout | Dry-run each runbook |

---

## Batch 3 — Tenant Isolation + Forensic (Gated: Phase 2b Complete)

*Gate: Phase 2b exit gate confirmed. No hard dependency on Batch 1/2 but benefits from them.*

| Item | Description | Notes |
|------|-------------|-------|
| 3-1 | Tenant isolation verification tests (cross-tenant access blocked, no data leakage) | |
| 3-2 | Resource quota enforcement (intents per tenant, artifacts per tenant) | |
| 3-3 | Tenant-specific rule pack isolation | |
| 3-4 | Tenant audit log separation | S3 tenant-scoped buckets/prefixes |
| 3-5 | Data residency: tenant data stays in assigned region | Update threat model |
| 3-6 | Tenant onboarding/offboarding procedures documented | |
| 3-7 | Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) | |
| 3-8 | Bundle generation: collect intent versions, artifacts, audit events, graph state | S3: `forensic-bundles/{tenant}/{bundle_id}/` |
| 3-9 | Bundle generation API: `POST /api/v1/forensic/bundle` | Role: `forensic-access` |
| 3-10 | Bundle integrity verification (hash chain) | |
| 3-11 | Bundle replay capability (replay bundle to reproduce state) | |
| 3-12 | Bundle retention policy (configurable per tenant, compliance) | S3 lifecycle |
| 3-13 | Forensic bundle export: `GET /api/v1/forensic/bundles/{id}/download` | |

---

## Batch 4 — Performance + Security Hardening (Gated: Batch 2 Complete + Full Stack Available)

*Gate: Observability stack in place. Load testing requires complete system.*

| Item | Description | Notes |
|------|-------------|-------|
| 4-1 | Intent diff optimization (caching, parallel computation) | Benchmark: diff < 100ms for typical intent |
| 4-2 | Graph traversal optimization (indexing, query optimization) | Benchmark: traversal < 50ms for 10k node graph |
| 4-3 | Database query optimization (indexes, query plans) | `EXPLAIN ANALYZE` on critical queries; optimization migration number TBD |
| 4-4 | Connection pooling (Postgres, NATS) | No connection exhaustion under load |
| 4-5 | Load testing: simulate Phase 3 production load (10x normal, no SLO violations) | Report: `load-test-results.md` |
| 4-6 | Threat model v2 | Doc: `../../14-governance/06-threat-model-v2.md` |
| 4-7 | Penetration testing completed | All critical/high findings remediated |
| 4-8 | Security review for all Phase 3 features | Sign-off: security-team |
| 4-9 | Compliance checklist (SOC2/GDPR if applicable) | |
| 4-10 | Incident response plan documented | Runbook: `incident-response.md` |
| 4-11 | Data retention and deletion verified | S3 lifecycle policies enforced |

---

## Exit Gate

```
ALL BATCHES COMPLETE: □ Yes □ No

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
