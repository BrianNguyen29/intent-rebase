# 10 Project-Completion Proposals Tracker

Tracks the 10 major work proposals required to move the Intent Rebase Engine from current state to full Phase 3 completion and Phase 4 entry. Each proposal is a logical grouping of related work, owned by a named role (not necessarily a specific person). Update status, progress, and next steps as work advances.

> **Last updated:** April 2026  
> **Gate:** Phase 2b exit gate is CLOSED (Phase 2b APPROVED — all three reviewers signed off). Most Phase 3 proposals are actively in progress. Phase 4 proposals are gated on Phase 3 exit.

---

## Proposal Index

| ID | Title | Status | Priority |
|----|-------|--------|----------|
| [P1](#p1--phase-2b-exit-gate) | Phase 2b Exit Gate | ✅ Approved (Brian Nguyen / 2026-04-28 — single-signer) | Critical |
| [P2](#p2--phase-3-batch-2--observability--sre) | Phase 3 Batch 2 — Observability + SRE | 🔄 In Progress (Slice 1, Slice 2, Slice 3, Slice 4, Slice 5, Slice 6, Slice 7, Slice 8, Slice 9 delivered) | High |
| [P3](#p3--phase-3-batch-3a--tenant-isolation-hardening) | Phase 3 Batch 3a — Tenant Isolation Hardening | 🔄 In Progress (bounded slices delivered: P3-S1, P3-S2, P3-S3, P3-S4, P3-S5) | High |
| [P4](#p4--phase-3-batch-3b--forensic-replay-bundle) | Phase 3 Batch 3b — Forensic Replay Bundle | 🔄 In Progress (bounded verification slice delivered) | High |
| [P5](#p5--phase-3-batch-4a--performance-work) | Phase 3 Batch 4a — Performance Work | 🔄 In Progress (bounded slices delivered: P5-S1 graph traversal benchmarks, P5-S2 DB query benchmarks, bounded HTTP load harness) | Medium |
| [P6](#p6--phase-3-batch-4b--security-hardening) | Phase 3 Batch 4b — Security Hardening | 🔄 In Progress (bounded slices delivered: JWT auth, RLS policies, audit immutability trigger, retention verification types) | High |
| [P7](#p7--phase-3-batch-1-closure--compensation-plannerexecutor) | Phase 3 Batch 1 Closure — Compensation Planner/Executor | ✅ Closed (Phase 3 Batch 1) | Critical |
| [P8](#p8--phase-4-enterprise-expansion--policy-simulation) | Phase 4 — Enterprise Expansion: Policy Simulation | ⬜ Not Started | Medium |
| [P9](#p9--phase-4-enterprise-expansion--advanced-adapters--cross-workflow) | Phase 4 — Advanced Adapters + Cross-Workflow Families | ⬜ Not Started | Medium |
| [P10](#p10--phase-4-enterprise-expansion--trust-scoring--integrations) | Phase 4 — Trust Scoring + Enterprise Integrations | ⬜ Not Started | Low |

---

## P1 — Phase 2b Exit Gate

| Field | Value |
|-------|-------|
| **ID** | P1 |
| **Title** | Phase 2b Exit Gate |
| **Purpose** | Phase 2b exit gate — completed and closed. Phase 2b scope: runtime adapter external implementation, apply endpoint, risk classification, graph update, replay API, event streaming. |
| **Status** | ✅ **APPROVED — Phase 2b exit gate closed; Brian Nguyen sole signer (2026-04-28)** |
| **Priority** | Critical |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Phase 3 Batch 2+ work is now unblocked — begin P2 (Phase 3 Batch 2 — Observability + SRE) |
| **Progress Notes** | Phase 2a runtime adapter delivered. Phase 2b Slice A (evidence verification) complete — all gates green. **Slice B (residual risk & Phase 3 deferral register) delivered — see [Phase 2b Residual Risk & Phase 3 Deferral Register](./10-phase-2b-residual-risk-deferral-register.md).** **Phase 2b is APPROVED — Brian Nguyen sole signer (personal project) signed off on all three roles (Product Owner, Security, Runtime Integration) on 2026-04-28. Phase 2b exit gate CLOSED. Phase 3 Batch 2+ is now unblocked.** **Bounded approval invalidation wired:** `trigger_reapproval` endpoint cancels existing `Approved` approvals (status → `Cancelled`) when creating a replacement `Pending` request; `rebase_apply` with `BlockedManualReview` outcome also cancels existing `Approved` approvals for the same tenant+intent. Approval revalidation classifier (`classify_approvals`) is wired in intent-api via `crates/intent-api/src/lib.rs`. Risk-based invalidation rules in `crates/rebase-engine/src/approval_revalidation.rs`. Notifications/NATS, DLQ/retry worker, immutable S3 snapshot blobs (currently `memory://` placeholder), and cross-process trace propagation remain future/deferred work.** |

### Slice A — Evidence Verification ✅ GREEN

All three canonical gates passed with zero warnings as errors:

| Command | Result |
|---------|--------|
| `cargo test --all-features` | ✅ All tests pass |
| `cargo check --all` | ✅ No errors |
| `cargo clippy --all-features -- -D warnings` | ✅ Clean |

Phase 2b scoped slices (runtime adapter, apply endpoint, risk classification, graph update, replay API, event streaming, bounded external surfaces) are delivered and gate-verified. Exit gate sign-off was pending Slice B residual close-out (now resolved).

---

## P2 — Phase 3 Batch 2 — Observability + SRE

| Field | Value |
|-------|-------|
| **ID** | P2 |
| **Title** | Phase 3 Batch 2 — Observability + SRE |
| **Purpose** | Deliver SLO definitions, alerting rules, error budget tracking, distributed tracing across Phase 2→3, performance benchmarks, and runbooks for common failure scenarios. |
| **Status** | 🔄 In Progress (Slice 1–9 delivered; Slice B delivered; Slice C, D documented) |
| **Priority** | High |
| **Owner** | SRE / Platform |
| **Suggested Next Step** | Slice B delivered: production load testing; SRE approval gate |
| **Progress Notes** | Batch 2 Slice 1 (SLO foundation + Grafana dashboard scaffold) delivered. Batch 2 Slice 2 (bounded OTEL propagation) delivered: request-id middleware, service method instrumentation, optional OTLP export (when OTEL_EXPORTER_OTLP_ENDPOINT is set), W3C trace context extraction from inbound requests, traceparent/tracestate response headers, background task span propagation. Batch 2 Slice 3 (alerting rules + runbook foundation) delivered: Prometheus alerting rules, Alertmanager config, Grafana provisioning, metrics instrumentation now active (metrics-exporter-prometheus 0.18.1 with metrics 0.24), runbook scenarios RB6-RB13. Webhook delivery bounded slice (B3-B18) delivered post-Batch 2: env-gated dispatcher (`INTENT_API_WEBHOOK_DELIVERY`, default disabled), retry loop with incrementing `attempt_number`, metrics counters, RB13 runbook, `WebhookDeliveryFailureRate` local alert rule, webhook_subscriptions RLS test/helpers, docs sync, dead_code cleanup. Batch 2 Slice 4 (rebase-engine sync benchmark) delivered: criterion-based benchmark harness, sync diff + plan across low/medium/high complexity (~490ns-4.2µs observed, all well under 100ms target). Batch 2 Slice 5 (error budget tracking panels) delivered: preview + apply 1h burn-rate stat panels backed by intent_api_rebase_preview_requests_total and intent_api_rebase_apply_requests_total. Batch 2 Slice 6 (graph + HTTP + DB benchmarks) delivered: graph traversal (BFS, path finding, cycle detection), intent-api sync path (diff compute, validation), and intent-service DB operations (live run: p50 25ms create_intent, 1.6ms create_version, <1ms get_intent/get_versions_by_intent against live Postgres). Batch 2 Slice 7 (multi-window burn-rate alerting) delivered: 1h/6h/3d burn-rate alerting rules for preview and apply paths. Batch 2 Slice 8 (bounded Temporal adapter tracing) delivered. Batch 2 Slice 9 (bounded sqlx repository tracing) delivered. **Slice B (NATS publisher + bounded JetStream seam) delivered:** async-nats 0.47 decision complete, core publisher emits W3C traceparent, bounded JetStream stream initializer is wired fail-safe, and `NatsPullConsumerAdapter` provides native traceparent extraction; consumer lifecycle and DLQ/retry worker remain open (see [12-trace-propagation-blocker-matrix.md](./12-trace-propagation-blocker-matrix.md)). **Slice C (S3/MinIO snapshot blob spec) documented:** target design for S3-backed immutable blob storage with key structure, JSON format, upload/retrieval contract, retention/lifecycle, and memory:// migration path (see [S3 snapshot blob spec](../14-governance/05-s3-snapshot-blob-spec.md)). **Slice D (trace blocker matrix) delivered:** detailed blocker matrix documents Temporal SDK, sqlx, NATS publisher/consumer, and HTTP forwarding blockers with unblock conditions (see [12-trace-propagation-blocker-matrix.md](./12-trace-propagation-blocker-matrix.md)).

**Items:**
- [x] SLO definitions (intent processing latency, rebase latency, approval wait time) — Batch 2 Slice 1
- [x] Grafana dashboard scaffold — Batch 2 Slice 1
- [x] Bounded OTEL propagation (request-id extraction + service method instrumentation + optional OTLP export + W3C trace-context + background task span propagation) — Batch 2 Slice 2
- [x] Alerting rules (warning, critical thresholds) — Batch 2 Slice 3
- [x] Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn, propagation-signal-failures (RB12), webhook-delivery-failures (RB13) — Batch 2 Slice 3 + B12-B13
- [x] Bounded metrics instrumentation (intent version creation, rebase preview/apply counters and histograms — definitions scaffolded, emission now active with metrics-exporter-prometheus 0.18.1 + metrics 0.24) — Batch 2 Slice 3
- [x] Rebase-engine sync diff + plan benchmark harness (criterion, low/medium/high complexity) — Batch 2 Slice 4
- [x] Error budget tracking panels (preview + apply 1h burn-rate stat panels) — Batch 2 Slice 5
- [x] Graph traversal benchmark harness (BFS, path finding, cycle detection across small/medium/large graphs) — Batch 2 Slice 6
- [x] Intent-api sync path benchmark harness (diff compute, validation, intent service create) — Batch 2 Slice 6
- [x] Intent-api HTTP server benchmark harness (real HTTP requests with in-memory repos) — Batch 2 Slice 6
- [x] Intent-service DB benchmark harness (live run complete — p50 25ms create, 1.6ms version, <1ms get_intent/get_versions_by_intent against live Postgres) — Batch 2 Slice 6
- [x] Multi-window burn-rate alerting (1h/6h/3d) — Batch 2 Slice 7
- [x] Bounded in-process OTEL propagation (optional OTLP export + W3C trace-context + background task span) — Batch 2 Slice 2 (delivered)
- [x] Phase 3 bounded trace continuity (trace_id/span_id in audit events and published event envelopes) — delivered
- [x] Phase 3 bounded Temporal adapter tracing (local span correlation around Temporal gRPC calls) — Batch 2 Slice 8 delivered
- [x] Phase 3 bounded sqlx repository tracing (local span correlation around high-value sqlx transactions: create_intent_tx, create_version_with_occ) — delivered
- [ ] Cross-process trace propagation across all service boundaries — **Slice B PARTIALLY RESOLVED:** See [12-trace-propagation-blocker-matrix.md](./12-trace-propagation-blocker-matrix.md) for full blocker analysis. Temporal SDK (B-01), sqlx (B-02), NATS publisher (B-03 — partially resolved), NATS consumer (B-04 — Phase 3 scope), HTTP forwarding (B-05). Slice B publisher side delivered; JetStream consumers remain Phase 3.
- [x] Slice B (bounded NATS core publisher) — ✅ DELIVERED: async-nats 0.47 + core publish, NatsEventPublisher with W3C trace-context header injection, fail-open, bounded timeouts. JetStream consumers/DLQ remain Phase 3. See [12-trace-propagation-blocker-matrix.md](./12-trace-propagation-blocker-matrix.md) § B-03.
- [ ] Slice C (S3/MinIO snapshot blob spec) — ✅ Document delivered: see [05-s3-snapshot-blob-spec.md](../14-governance/05-s3-snapshot-blob-spec.md) (target design, not implemented).
- [ ] Slice D (trace propagation blocker matrix) — ✅ Document delivered: see [12-trace-propagation-blocker-matrix.md](./12-trace-propagation-blocker-matrix.md).
- [ ] Full production load testing (bounded HTTP load harness delivered — intent-api HTTP server benchmark with in-memory repos; full production load testing remains gated on P2 full completion)

---

## P3 — Phase 3 Batch 3a — Tenant Isolation Hardening

| Field | Value |
|-------|-------|
| **ID** | P3 |
| **Title** | Phase 3 Batch 3a — Tenant Isolation Hardening |
| **Purpose** | Verify and enforce tenant isolation across all surfaces: access control, data visibility, quotas, audit log separation, and data residency. |
| **Status** | 🔄 In Progress (bounded slices delivered: tenant isolation tests P3-S1, quota enforcement P3-S2, rule pack isolation P3-S3, audit query isolation P3-S4, tenant service/onboarding scaffold P3-S5, artifact ingest tenant isolation) |
| **Priority** | High |
| **Owner** | Security / Platform |
| **Suggested Next Step** | Data residency verification; broader artifact-service tenant coverage extension |
| **Progress Notes** | Phase 2b exit is closed; Batch 3a is actively in progress. Bounded slices delivered: P3-S1 (tenant isolation verification tests), P3-S2 (quota enforcement), P3-S3 (tenant-specific rule pack isolation), P3-S4 (tenant audit log separation via scoped audit query API), P3-S5 (tenant service scaffold + onboarding procedure skeleton). **Artifact ingest side-effect tenant isolation tests now pass in intent-api lib tests.** Data residency note added to threat model (bounded verification/planning — single-region today, target-region metadata exists, enforcement/routing is future work). Broader artifact-service coverage remains open. |

**Items:**
- [x] Tenant isolation verification tests (cross-tenant access blocked, no data leakage) — P3-S1 bounded slice delivered
- [x] Resource quota enforcement (intents per tenant, artifacts per tenant) — P3-S2 bounded slice delivered
- [x] Tenant-specific rule pack isolation — P3-S3 bounded slice delivered
- [x] Tenant audit log separation (tenant-scoped audit query API) — P3-S4 bounded slice delivered; S3 cold storage remains Phase 4+ scope
- [x] Artifact ingest tenant isolation (cross-tenant query isolation on ingest with side_effect_context) — ✅ delivered in intent-api lib tests
- [~] Data residency: tenant data stays in assigned region — bounded verification note added to threat model (single-region today, target-region metadata exists, enforcement/routing is future work)
- [~] Tenant onboarding/offboarding procedures documented — P3-S5 bounded slice (skeleton/runner only); full API, S3 bucket provisioning, NATS account creation, RBAC setup remain future scope

---

## P4 — Phase 3 Batch 3b — Forensic Replay Bundle

| Field | Value |
|-------|-------|
| **ID** | P4 |
| **Title** | Phase 3 Batch 3b — Forensic Replay Bundle |
| **Purpose** | Deliver forensic bundle capability: bundle model, generation (intent versions + artifacts + audit events + graph state), integrity verification, replay, retention, and export. |
| **Status** | 🔄 In Progress (Phase 3 Batch 3b bounded slice + P4 bounded generation/storage slice delivered) |
| **Priority** | High |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Full replay capability; async generation orchestration; S3-backed retrieval and storage lifecycle hardening |
| **Progress Notes** | **Phase 3 Batch 3b bounded slice delivered:** forensic-service with ForensicVerificationService trait, InMemoryForensicVerificationService, request/response types, and coverage structs. API endpoint `POST /forensic/verify` integrated in intent-api with tests. OpenAPI documentation updated. **P4 bounded generation/storage slice delivered:** `POST /forensic/bundle` bounded synchronous path — ForensicDataCollector collects real data from intent/graph/audit repositories, BundleGeneratorService generates manifest with integrity hashes, InMemoryBundleStorage persists bundle JSON in-memory at runtime (S3BundleStorage seam exists but is not wired; S3 lifecycle/retrieval deferred to Phase 4), bundle status=Ready recorded in repository. |

**Items:**
- [x] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) — ✅ Batch 0 scaffold delivered
- [x] Forensic verification API: `POST /forensic/verify` (bounded request-driven verification) — ✅ Phase 3 Batch 3b bounded slice delivered
- [x] Forensic archive export API: `POST /forensic/export` (bounded in-memory generation with scaffolded data) — ✅ Phase 3 Batch 3b bounded slice delivered
- [x] Bundle generation: collect intent versions, artifacts, audit events, graph state from real services — ✅ P4 bounded slice delivered (ForensicDataCollector + real repository calls)
- [x] Bundle generation API: `POST /forensic/bundle` (role: `forensic-access`) — ✅ P4 bounded slice delivered; bounded synchronous path with real collection + in-memory storage (S3BundleStorage seam deferred to Phase 4)
- [x] Bundle integrity verification (hash chain) — ✅ Phase 3 Batch 3b bounded slice delivered (verify_bundle_integrity function)
- [ ] Bundle replay capability (replay bundle to reproduce state)
- [ ] Bundle retention policy (configurable per tenant, compliance) — retention policy metadata model delivered; S3 lifecycle enforcement remains Phase 4+ scope
- [ ] Forensic bundle export from storage: `GET /forensic/bundles/{bundle_id}/download` (S3-backed retrieval) — download endpoint exists for in-memory/exportable bundles; S3-backed retrieval remains Phase 4 scope

---

## P5 — Phase 3 Batch 4a — Performance Work

| Field | Value |
|-------|-------|
| **ID** | P5 |
| **Title** | Phase 3 Batch 4a — Performance Work |
| **Purpose** | Optimize intent diff, graph traversal, and database queries; configure connection pooling; run load tests to validate Phase 3 production readiness. |
| **Status** | 🔄 In Progress (bounded slices delivered: P5-S1 graph traversal benchmarks, P5-S2 DB query benchmarks, in-memory HTTP load test, SQLx-backed local-live load test; full production load testing remains open) |
| **Priority** | Medium |
| **Owner** | Backend Lead / SRE |
| **Suggested Next Step** | Proceed with DB query optimization if needed; full production load testing gated on P5 full completion |
| **Progress Notes** | **P5-S1 delivered:** rebase-engine benchmark harness with criterion. Benchmarks sync diff + plan path across low/medium/high complexity (~490ns-4.2µs, all under 100ms target). **P5-S2 delivered:** graph traversal benchmark (BFS, path finding, cycle detection across small/medium/large graphs), intent-api sync path benchmark (diff compute, validation, intent service create), and intent-service DB benchmark with live Postgres run (p50 25ms create_intent, 1.6ms create_version_with_occ, 873µs get_intent, 959µs get_versions_by_intent). **In-memory HTTP load test delivered:** 3 load levels against intent-api HTTP server with in-memory repos; L1 p95 5ms, L2 p95 33ms, L3 p95 60ms, all 0% errors. **SQLx-backed local-live load test delivered:** docker-compose Postgres; SQLx-L1 (5 clients/500 req) p95 5ms, 0% errors; SQLx-L2 (10 clients/1000 req) p95 4ms, 0% errors. Full production load testing (k6/Artillery against staging/production) remains gated on P5 full completion. |

**Items:**
- [x] Rebase-engine sync diff + plan benchmark harness — P5-S1 bounded slice delivered
- [x] Graph traversal benchmarks (BFS, path finding, cycle detection) — P5-S1 bounded slice delivered
- [x] DB query benchmarks with live Postgres — P5-S2 bounded slice delivered
- [x] Intent-api HTTP server benchmarks (in-memory repos) — bounded slice delivered; L1 p95 5ms, L2 p95 33ms, L3 p95 60ms, all 0% errors
- [x] SQLx-backed local-live load test (docker-compose Postgres) — bounded slice delivered; SQLx-L1 p95 5ms, SQLx-L2 p95 4ms, 0% errors
- [ ] Intent diff optimization (caching, parallel computation) — benchmark target: diff < 100ms (baseline: ~490ns-2.6µs, target met; may not be needed)
- [ ] Graph traversal optimization (indexing, query optimization)
- [ ] Database query optimization (indexes, query plans)
- [ ] Connection pooling (Postgres, NATS) — local-live groundwork delivered (pool config: max_connections=20, min_connections=2, acquire_timeout=30s, idle_timeout=600s); production pool sizing remains open
- [ ] Full production load testing (bounded HTTP harness and SQLx local-live test delivered; full k6/Artillery production load testing remains gated on P5 full completion)

## P6 — Phase 3 Batch 4b — Security Hardening

| Field | Value |
|-------|-------|
| **ID** | P6 |
| **Title** | Phase 3 Batch 4b — Security Hardening |
| **Purpose** | Complete threat model v2, penetration testing, security review for all Phase 3 features, compliance checklist, incident response plan, and data retention/deletion verification. |
| **Status** | 🔄 In Progress (bounded slices delivered: JWT auth middleware, RLS migration definitions, audit immutability trigger, retention verification types; pen test and external review remain open) |
| **Priority** | High |
| **Owner** | Security Team |
| **Suggested Next Step** | Pen test scope definition; external security review engagement |
| **Progress Notes** | **Bounded slices delivered:** JWT authentication middleware implemented (intent-api HTTP server), PostgreSQL RLS policy migration definitions documented, audit immutability trigger implemented (prevent UPDATE/DELETE on audit_events), retention verification types and S3 lifecycle config template (P6-S1 bounded slice). **Threat model v2** input captured in `08-phase-2b-security-findings-input.md`. **Pen test and external security review remain open.** |

**Items:**
- [x] JWT authentication middleware implemented — P6 bounded slice delivered
- [x] PostgreSQL RLS policies defined — P6 bounded slice delivered
- [x] Audit immutability trigger (prevent UPDATE/DELETE) — P6 bounded slice delivered
- [x] Data retention and deletion verification (bounded — retention verification types + S3 lifecycle config template) — P6-S1 bounded slice delivered; live S3 enforcement remains Phase 4+ scope
- [x] Threat model v2 documented
- [x] Compliance checklist (bounded — SOC2/GDPR/ISO27001 control tracking)
- [x] Incident response plan documented
- [~] Penetration testing scope defined — bounded planning artifact; actual pen testing remains Phase 3/4 future work
- [ ] Penetration testing completed
- [ ] External security review sign-off

---

## P7 — Phase 3 Batch 1 Closure — Compensation Planner/Executor

| Field | Value |
|-------|-------|
| **ID** | P7 |
| **Title** | Phase 3 Batch 1 Closure — Compensation Planner/Executor |
| **Purpose** | Deliver bounded compensation planner (plan generation from side effects), four bounded executors (rollback/counter-action logic), and compensation audit trail. |
| **Status** | ✅ Closed (Phase 3 Batch 1) |
| **Priority** | Critical |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Proceed to Batch 2 observability + SRE hardening |
| **Progress Notes** | Four bounded executors delivered: RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). Audit trail events delivered. S2 planner/executor alignment resolved: S2ExternalReversible routes to CounterAction+SemiAutomatic. N4-4 bounded simulation API slice delivered: `GET /intents/{intent_id}/rebase-simulation` with deterministic/stochastic mode, version ordering validation, and invalid-mode fallback to deterministic. Full N4 (live executors in simulation mode) remains Phase 4 scope. |

**Items:**
- [x] Side effect rollback record (compensation applied, compensation result) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [x] Compensation planner: generate compensation plan from side effects — ✅ delivered (Phase 3 Batch 1 bounded; class-based strategy routing; S2 routes to CounterAction+SemiAutomatic; fail-closed on unsupported strategy classes)
- [x] Four bounded compensation executors (RollbackExecutor, CounterActionExecutor, FollowupNoticeExecutor, EscalationExecutor) — ✅ delivered (Phase 3 Batch 1 bounded slice; each executor is strategy+feasibility-gated; acknowledges against side effect ledger; all other combos fail closed)
- [x] Compensation audit trail (`compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed`) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [x] **S2 planner/executor alignment** — ✅ resolved: S2ExternalReversible now routes to CounterAction + SemiAutomatic, aligned with CounterActionExecutor gate
- [x] N4-4 Compensation simulation API slice (`GET /intents/{intent_id}/rebase-simulation`) — ✅ delivered (Phase 3 bounded slice; deterministic/stochastic mode with seed; version ordering validation; invalid-mode fallback; read-only mock simulation; full N4 scope remains Phase 4)

---

## P8 — Phase 4 — Enterprise Expansion: Policy Simulation

| Field | Value |
|-------|-------|
| **ID** | P8 |
| **Title** | Phase 4 — Enterprise Expansion: Policy Simulation |
| **Purpose** | Deliver policy simulation capability: dry-run policy changes against production state, what-if analysis for rule pack changes, impact preview before applying policy updates. |
| **Status** | ⬜ Not Started |
| **Priority** | Medium |
| **Owner** | Backend Lead / Product |
| **Suggested Next Step** | Define policy simulation use cases and success criteria with product team |
| **Progress Notes** | Phase 4 gated on Phase 3 exit gate. Policy simulation enables safer policy changes in multi-tenant production environments. |

**Items:**
- [ ] Policy simulation model and API surface
- [ ] What-if analysis for rule pack changes
- [ ] Impact preview before applying policy updates
- [ ] Policy simulation dashboard / read API

---

## P9 — Phase 4 — Advanced Adapters + Cross-Workflow Families

| Field | Value |
|-------|-------|
| **ID** | P9 |
| **Title** | Phase 4 — Advanced Adapters + Cross-Workflow Families |
| **Purpose** | Support cross-workflow intent families (shared intent lineage across multiple workflows), advanced runtime adapters beyond the initial Temporal integration, and multi-adapter coordination. |
| **Status** | ⬜ Not Started |
| **Priority** | Medium |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Survey runtime adapter requirements from early adopters; design adapter abstraction |
| **Progress Notes** | Phase 4 gated on Phase 3 exit gate. Current runtime adapter v1 is Temporal-focused. Advanced adapters and cross-workflow families require broader design discussion. |

**Items:**
- [ ] Advanced runtime adapters (beyond Temporal v1)
- [ ] Cross-workflow intent families (shared lineage across workflows)
- [ ] Multi-adapter coordination API
- [ ] Adapter registry and versioning

---

## P10 — Phase 4 — Trust Scoring + Enterprise Integrations

| Field | Value |
|-------|-------|
| **ID** | P10 |
| **Title** | Phase 4 — Trust Scoring + Enterprise Integrations |
| **Purpose** | Introduce trust scoring by source (e.g., human agent, AI agent, automated system), enterprise integrations (SSO, SCIM, audit log forwarding), and elevated enterprise tier capabilities. |
| **Status** | ⬜ Not Started |
| **Priority** | Low |
| **Owner** | Product / Backend Lead |
| **Suggested Next Step** | Define trust scoring taxonomy with product team; identify enterprise integration priorities |
| **Progress Notes** | Phase 4 gated on Phase 3 exit gate. Trust scoring is a future Phase 4 item; currently no trust level classification in the intent model. |

**Items:**
- [ ] Trust scoring taxonomy by source (human agent, AI agent, automated system)
- [ ] Trust-score-informed risk classification
- [ ] Enterprise SSO integration (SAML/OIDC)
- [ ] SCIM provisioning for enterprise tenants
- [ ] Audit log forwarding to enterprise SIEM
- [ ] Enterprise tier dashboard and reporting

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| May 2026 | (fixer) | P2 updated: Webhook delivery bounded slice B3-B18 delivered — env-gated dispatcher, retry loop, metrics, RB13 runbook, `WebhookDeliveryFailureRate` alert, RLS tests, docs sync, dead_code cleanup; runbook references updated RB6-RB13 |
| May 2026 | (fixer) | Commit 5dcdd36: apply-level wiremock success/failure outcome coverage delivered (200-success and 500-failure paths via `create_propagation_signals_after_apply_with_resolver` test seam in `rebase_apply_handler_tests.rs`) |
| April 2026 | (orchestrator) | P2 updated: Slice B marked delivered for bounded NATS publisher + bounded JetStream seam; consumer lifecycle and DLQ/retry worker remain open; Slice C (S3/MinIO snapshot blob spec) documented; Slice D (trace propagation blocker matrix) documented; cross-process trace propagation item updated with blocker matrix reference |
| April 2026 | (orchestrator) | P2 updated: cross-process trace propagation investigated and deferred (Temporal SDK limitation, sqlx limitation, NATS not yet implemented) |
| April 2026 | (owner) | Initial creation |

---

## Related Docs

- [Current Project Status](./00-current-status.md)
- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Completion Execution Plan](./15-phase-3-completion-execution-plan.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
