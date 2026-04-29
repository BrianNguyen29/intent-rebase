# Phase 3 — Hardening (Staged Execution Plan)

## Phase Overview

Phase 3 builds on Phase 2b completion. Full execution is dependency-gated; however, Batch 0 may start in parallel with Phase 2b.

**Hard Dependency:** Phase 2b exit gate  
**Optional Parallel Track:** Batch 0 (planning, scaffold, dependency audit) — may begin while Phase 2b is in progress

---

## Batch 0 — Planning + Hardening Scaffold (Parallel with Phase 2b)

*Gate: None — may start immediately. Must complete before Batch 1 starts.*

**Status:** `Batch 0 Complete` ✅ — scaffold and planning items delivered. Batch 1 and subsequent batches are gated on Phase 2b exit (Phase 2b exit is now CLOSED per [deferral register](./10-phase-2b-residual-risk-deferral-register.md)).

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

**Status:** `Batch 1 IN PROGRESS — side effect ledger, side-effects query API, compensation-actions query API, and bounded execution slice delivered`

| Item | Description | Notes |
|------|-------------|-------|
| 1-1 | Side effect model (`effect_id`, `intent_id`, `intent_version`, `effect_type`, `target`, `timestamp`, `tenant_id`) | Schema + repository groundwork ✅ delivered (Phase 2) |
| 1-2 | Side effect capture on all artifact-producing operations | Delivered: artifact-ingest only via POST /v1/graph/artifacts with optional `side_effect_context`. Other artifact-producing operations remain open for future coverage. |
| 1-3 | Side effect query API: `GET /intents/{intent_id}/side-effects` | ✅ delivered (Phase 3 Batch 1) |
| 1-4 | Side effect idempotency keys | Tenant-scoped atomic idempotency ✅ delivered in service/repository path. Broader artifact-service coverage remains open. |
| 1-5 | Side effect rollback record (compensation applied, compensation result) | ✅ delivered (Phase 3 Batch 1 bounded slice). Schema + repository for compensation applied/result fields; fail-closed on unknown/invalid side effects. |
| 1-6 | Compensation action model (`action_type`, `target`, `parameters`, `status`) | Scaffold ✅ delivered (Phase 3 Batch 0). Now includes `intent_id`, `trigger_context`, and `execution_result_payload` fields for bounded Phase 3 design. |
| 1-6b | Compensation actions query API: `GET /intents/{intent_id}/compensation-actions` | ✅ delivered (Phase 3 Batch 1). READ-ONLY endpoint. |
| 1-6c | Compensation action approve API: `POST /compensation-actions/{action_id}/approve` | ✅ delivered (Phase 3 Batch 1 bounded execution slice). Transitions Pending → Approved. Executor gate ensures only Approved actions can execute. |
| 1-6d | Compensation action waive API: `POST /compensation-actions/{action_id}/waive` | ✅ delivered (Phase 3 Batch 1 bounded execution slice). Transitions Pending → Waived. |
| 1-6e | Compensation action execute API: `POST /compensation-actions/{action_id}/execute` | ✅ delivered (Phase 3 Batch 1 bounded execution slice). Executor gate: only Approved actions can execute. Four strategy-specific executors: RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). All four acknowledge against side effect ledger; all other combos fail closed. |
| 1-6f | Status transition validation matrix | ✅ delivered (Phase 3 Batch 1 bounded execution slice). Explicit validation with fail-closed semantics. |
| 1-7 | Compensation planner: generate compensation plan from side effects | ✅ delivered (Phase 3 Batch 1 bounded slice). Fail-closed on non-Rollback strategies or unsupported side effect types. Acknowledgment-only (does not reverse; confirms compensation was applied). |
| 1-8 | Compensation executor: real rollback/acknowledgment logic | ✅ delivered (Phase 3 Batch 1 bounded four-executor slice). RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). Acknowledged success summaries (confirmed against side effect ledger; not reversed). All four executors fail closed on non-matching strategy/feasibility combos. S2 planner/executor alignment resolved: S2ExternalReversible routes to CounterAction+SemiAutomatic. |
| 1-10 | Compensation audit trail (`compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed`) | ✅ delivered (Phase 3 Batch 1 bounded slice). Acknowledgment-only events; fail-closed on unknown side effects. |
| 1-11 | Failed → Pending reapproval path | ✅ delivered (Phase 3 Batch 1 bounded manual retry slice). POST /compensation-actions/{action_id}/reapprove with fail-closed policy gates. Only retryable errors AND remaining budget allow reapproval. |

---

## Batch 2 — Observability + SRE (Gated: Phase 2b Complete + Batch 1 Checkpoint)

*Gate: Compensation engine basic path verified. Phase 2b event streaming available.*

**Status:** `Batch 2 IN PROGRESS — Slice 1 (SLO foundation + Grafana dashboard scaffold) delivered, Slice 2 (tracing foundation) delivered, Slice 3 (alerting rules + runbook foundation) delivered, Slice 4 (rebase-engine sync benchmark) delivered, Slice 5 (error budget tracking panels) delivered, Slice 6 (graph + HTTP + DB benchmark harnesses) delivered, Slice 7 (multi-window burn-rate alerting) delivered, Slice 8 (forensic bundle generation API) delivered, Slice 9 (forensic bundle download) delivered — external SRE sign-off gates remain`

### Batch 2 Slice 1 (delivered)

| Item | Description | Notes |
|------|-------------|-------|
| 2-1a | SLO definitions documented | `docs/09-operations/04-sre-and-slos.md` — provisional targets, awaiting SRE confirmation |
| 2-1b | Grafana dashboard scaffold | `docs/09-operations/06-slo-dashboard.md` — 16 panels, all referencing metrics that require instrumentation |

### Batch 2 Slice 2 (delivered — bounded OTEL propagation)

| Item | Description | Notes |
|------|-------------|-------|
| 2-4a | HTTP request-id extraction middleware | Extracts `X-Request-ID` header or generates UUID; stores in request extensions for downstream correlation |
| 2-4b | Service method instrumentation | `#[tracing::instrument]` on key intent-service, rebase-engine, and compensation-service methods |
| 2-4c | Optional OTLP export | OTLP export activated when `OTEL_EXPORTER_OTLP_ENDPOINT` env var is set; JSON logging fallback otherwise |
| 2-4d | W3C trace-context propagation | Extracts `traceparent`/`tracestate` from inbound requests; adds to responses |
| 2-4e | Background task span propagation | Spawned background work inherits current span context via `tracing::Instrument` |

**Slice 2 scope (bounded/truthful):**
- Request-id extraction middleware in intent-api HTTP layer ✅
- `#[tracing::instrument]` on key service methods ✅
- Optional OTLP export (activated via env var) ✅
- W3C trace-context extraction and propagation (in-process only) ✅
- Background task span propagation ✅
- **NOT in scope for Slice 2:** Cross-process trace propagation, full distributed trace across service boundaries

### Batch 2 Slice 3 (delivered — alerting rules + runbook foundation)

| Item | Description | Notes |
|------|-------------|-------|
| 2-2a | Alerting rules | `infrastructure/local/prometheus/rules/intent_api_alerts.yml` — warning/critical thresholds for availability and latency SLOs |
| 2-2b | Alertmanager config | `infrastructure/local/alertmanager/alertmanager.yml` — placeholder receivers for local dev only |
| 2-3 | Error budget runbook | `docs/09-operations/05-runbooks.md` RB10 — error budget burn rate runbook |
| 2-6 | Runbooks | `docs/09-operations/05-runbooks.md` RB6-RB10 — rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn |
| Metrics | Bounded metrics instrumentation (active emission) | `intent_api_intent_version_created_total`, `intent_api_rebase_preview_requests_total`, `intent_api_rebase_apply_requests_total`, `intent_api_diff_compute_duration_seconds`, `intent_api_rebase_preview_duration_seconds`, `intent_api_rebase_apply_duration_seconds` — metric definitions scaffolded and actively recorded in intent-api (metrics-exporter-prometheus 0.18.1 with metrics 0.24) |

**Slice 3 scope (bounded/truthful):**
- Local dev alerting infrastructure (Prometheus, Alertmanager, Grafana) ✅
- Bounded metrics instrumentation (definitions scaffolded, emission active with metrics-exporter-prometheus 0.18.1 + metrics 0.24) ✅
- Runbook scenarios for common failure modes ✅
- **NOT in scope for Slice 3:** Full metrics coverage across all flows, production alerting deployment, error budget tracking dashboard, performance benchmarks

### Batch 2 Slice 4 (delivered — rebase-engine sync benchmark)

| Item | Description | Notes |
|------|-------------|-------|
| 2-5a | Benchmark harness for rebase-engine | `crates/rebase-engine/benches/rebase_latency.rs` (criterion) |
| 2-5b | Low/medium/high complexity benchmark cases | Sync diff + plan path; synthetic fixtures |
| 2-5c | Benchmark results captured | ~490ns-4.2µs observed (target < 100ms: MET) |

**Slice 4 scope (bounded/truthful):**
- Sync CPU-bound rebase-engine diff + plan path only ✅
- **NOT in scope:** Graph traversal benchmarks, DB query benchmarks, HTTP/API benchmarks, load testing

### Batch 2 Slice 5 (delivered — error budget tracking panels)

| Item | Description | Notes |
|------|-------------|-------|
| 2-7a | Preview path 1h burn rate stat panel | `intent_api_rebase_preview_requests_total` — query: `sum(rate(...{status!="success"}[1h])) / sum(rate(...[1h]))` |
| 2-7b | Apply path 1h burn rate stat panel | `intent_api_rebase_apply_requests_total` — same burn-rate query pattern |

**Slice 5 scope (bounded/truthful):**
- Preview + apply 1-hour burn rate stat panels backed by metrics emitted in Slice 3 ✅
- **NOT in scope:** Multi-window burn-rate alerting (1h/6h/3d), budget depletion forecasting, 30-day budget tracking panel, SLO composite panels

### Batch 2 Slice 7 (delivered — multi-window burn-rate alerting)

| Item | Description | Notes |
|------|-------------|-------|
| 2-7c | Preview path 1h burn-rate alert | `PreviewPathBurnRate1h` — 1h window, threshold 0.5%, for 10m |
| 2-7d | Apply path 1h burn-rate alert | `ApplyPathBurnRate1h` — 1h window, threshold 1.0%, for 10m |
| 2-7e | Preview path 6h burn-rate alert | `PreviewPathBurnRate6h` — 6h window, threshold 0.6%, for 30m |
| 2-7f | Apply path 6h burn-rate alert | `ApplyPathBurnRate6h` — 6h window, threshold 1.2%, for 30m |
| 2-7g | Preview path 3d burn-rate alert | `PreviewPathBurnRate3d` — 3d window, threshold 0.8%, for 2h |
| 2-7h | Apply path 3d burn-rate alert | `ApplyPathBurnRate3d` — 3d window, threshold 1.6%, for 2h |
| 2-7i | 6h burn-rate stat panels | Preview + apply path 6h burn-rate panels in Grafana dashboard |
| 2-7j | 3d burn-rate stat panels | Preview + apply path 3d burn-rate panels in Grafana dashboard |

**Slice 7 scope (bounded/truthful):**
- Multi-window burn-rate alerting rules (1h/6h/3d) for preview and apply paths ✅
- 6h and 3d burn-rate dashboard panels ✅
- **NOT in scope:** Budget depletion forecasting, 30-day budget tracking panel, SLO composite panels, production Alertmanager deployment, SRE-approved production rollout

### Batch 2 Slice 6 (delivered — graph + HTTP + DB benchmark harnesses)

| Item | Description | Notes |
|------|-------------|-------|
| 2-8a | Graph-service traversal benchmark | `crates/graph-service/benches/graph_traversal.rs` — BFS, path finding, cycle detection |
| 2-8b | Intent-api sync path benchmark | `crates/intent-api/benches/http_handlers.rs` — diff compute, validation |
| 2-8c | Intent-service DB benchmark harness (live) | `crates/intent-service/benches/db_operations.rs` — live run complete; p50 25ms create, 1.6ms version, <1ms get/list |

**Slice 6 scope (bounded/truthful):**
- Graph traversal: in-memory repository benchmarks (BFS, path finding, cycle detection across small/medium/large graphs) ✅
- Intent-api sync path: diff compute, validation, intent service create ✅
- Intent-api HTTP server: real HTTP requests against live axum server with in-memory repos ✅
- Intent-service DB: live benchmark against Postgres; p50 25ms create, 1.6ms version, <1ms get/list ✅
- **NOT in scope:** SQL-backed graph benchmarks, concurrent DB benchmarks, load testing

### Batch 2 remaining work (not yet delivered)

| Item | Description | Notes |
|------|-------------|-------|
| 2-1 (remainder) | SLO definitions — SRE approval gate | External SRE sign-off still open |
| 2-2 (remainder) | Production alerting deployment | Local dev infrastructure only — production requires SRE confirmation |
| 2-4 (remainder) | Distributed tracing across all services | Bounded in-process OTEL propagation delivered (Slice 2); Phase 3 bounded trace continuity slice delivers trace_id/span_id in audit events and published event envelopes; Phase 3 bounded Temporal adapter tracing slice delivers local tracing span correlation around Temporal gRPC calls; cross-process propagation **investigated and deferred** — Temporal SDK 0.2.0 lacks safe per-request gRPC metadata injection (shared `Arc<RwLock>` race on `Connection::set_headers`), sqlx lacks per-query context propagation, NATS publisher not yet implemented. Revisit when SDK support improves. |
| 2-5 (remainder) | Graph traversal and DB query benchmarks | Graph traversal and DB benchmarks — both delivered |
| 2-5 (remainder) | HTTP/API benchmarks | ✅ Delivered (real HTTP with in-memory repos); full production load testing remains future scope |
| 2-7 (remainder) | Multi-window burn-rate alerting (1h/6h/3d) | ✅ Delivered (Slice 7); 1h/6h/3d windows for preview and apply paths |
| 2-7 (remainder) | Budget depletion forecasting / 30-day budget panel | Future scope after single-window burn-rate panels are validated |

---

## Batch 3 — Tenant Isolation + Forensic (Gated: Phase 2b Complete)

*Gate: Phase 2b exit gate confirmed. No hard dependency on Batch 1/2 but benefits from them.*

| Item | Description | Notes |
|------|-------------|-------|
| 3-1 | Tenant isolation verification tests (cross-tenant access blocked, no data leakage) | ✅ P3-S1 bounded slice delivered — cross-tenant access blocked, no data leakage tests passing |
| 3-2 | Resource quota enforcement (intents per tenant, artifacts per tenant) | ✅ P3-S2 bounded slice delivered — QuotaService with InMemoryQuotaRepository; quota checks on create_intent and ingest_artifact |
| 3-3 | Tenant-specific rule pack isolation | ✅ P3-S3 bounded slice delivered — TenantRulePackRepository trait + InMemory impl; 8 tenant isolation tests passing. Full upload/management API and S3 integration remain future phase scope. |
| 3-4 | Tenant audit log separation | ✅ P3-S4 bounded slice delivered — tenant-scoped audit query API (GET /audit/events, GET /audit/events/{event_id}); cross-tenant isolation tests passing. S3 cold storage and archival remain Phase 4+ scope. |
| 3-5 | Data residency: tenant data stays in assigned region | Not started — future phase scope; update threat model |
| 3-6 | Tenant onboarding/offboarding procedures documented | ~ P3-S5 bounded slice delivered (skeleton/runner only) — tenant-service scaffold (Tenant model + repository + InMemory impl), tenant onboarding procedure skeleton in `09-operations/06-tenant-onboarding.md`. Full API, S3 bucket provisioning, NATS account creation, and RBAC setup remain future phase scope. |
| 3-7 | Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) | ✅ delivered (Batch 0 scaffold) |
| 3-8 | Bundle generation: collect intent versions, artifacts, audit events, graph state from real services | ✅ P4 bounded slice delivered — ForensicDataCollector + real repository calls |
| 3-9 | Bundle generation API: `POST /forensic/bundle` | ✅ P4 bounded slice delivered — bounded synchronous path: collect + generate manifest + persist in-memory (S3 seam deferred to Phase 4) + record Ready |
| 3-10 | Bundle integrity verification (hash chain) | ✅ Phase 3 Batch 3b bounded slice delivered — verify_bundle_integrity function |
| 3-11 | Bundle replay capability (replay bundle to reproduce state) | 🔄 Phase 4 scope — bounded replay verification surface (read-only integrity check) delivered; full runtime replay is Phase 4 |
| 3-12 | Bundle retention policy (configurable per tenant, compliance) | 🔄 Phase 4 scope — retention policy metadata model delivered; S3 lifecycle enforcement future work |
| 3-13 | Forensic bundle export: `GET /forensic/bundles/{bundle_id}/download` | ✅ Phase 3 Batch 3b bounded slice delivered — bounded local/exportable download; S3-backed retrieval is Phase 4 scope |

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
