# 10 Project-Completion Proposals Tracker

Tracks the 10 major work proposals required to move the Intent Rebase Engine from current state to full Phase 3 completion and Phase 4 entry. Each proposal is a logical grouping of related work, owned by a named role (not necessarily a specific person). Update status, progress, and next steps as work advances.

> **Last updated:** April 2026  
> **Gate:** Most proposals are gated on Phase 2b exit. Proposal 1 represents the Phase 2b exit itself.

---

## Proposal Index

| ID | Title | Status | Priority |
|----|-------|--------|----------|
| [P1](#p1--phase-2b-exit-gate) | Phase 2b Exit Gate | ✅ Approved (name/date pending) | Critical |
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
| **Purpose** | Complete Phase 2b to unblock Phase 3 Batch 2+. Phase 2b scope: runtime adapter external implementation, apply endpoint, risk classification, graph update, replay API, event streaming. |
| **Status** | ✅ **APPROVED — Phase 2b exit gate closed; name/date pending documentation per user instruction** |
| **Priority** | Critical |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Phase 3 Batch 2+ work is now unblocked — begin P2 (Phase 3 Batch 2 — Observability + SRE) |
| **Progress Notes** | Phase 2a runtime adapter delivered. Phase 2b Slice A (evidence verification) complete — all gates green. **Slice B (residual risk & Phase 3 deferral register) delivered — see [Phase 2b Residual Risk & Phase 3 Deferral Register](./10-phase-2b-residual-risk-deferral-register.md).** **Phase 2b is APPROVED — Product Owner ✅, Security ✅, Runtime Integration ✅ — name/date pending documentation per user instruction. Phase 2b exit gate CLOSED. Phase 3 Batch 2+ is now unblocked.** |

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
| **Status** | 🔄 In Progress (Slice 1, Slice 2, Slice 3, Slice 4, Slice 5, Slice 6, Slice 7, Slice 8, Slice 9 delivered) |
| **Priority** | High |
| **Owner** | SRE / Platform |
| **Suggested Next Step** | Cross-process trace propagation investigated and deferred (SDK limitation); production load testing; SRE approval gate |
| **Progress Notes** | Batch 2 Slice 1 (SLO foundation + Grafana dashboard scaffold) delivered. Batch 2 Slice 2 (bounded OTEL propagation) delivered: request-id middleware, service method instrumentation, optional OTLP export (when OTEL_EXPORTER_OTLP_ENDPOINT is set), W3C trace context extraction from inbound requests, traceparent/tracestate response headers, background task span propagation. Batch 2 Slice 3 (alerting rules + runbook foundation) delivered: Prometheus alerting rules, Alertmanager config, Grafana provisioning, metrics instrumentation now active (metrics-exporter-prometheus 0.18.1 with metrics 0.24), runbook scenarios RB6-RB10. Batch 2 Slice 4 (rebase-engine sync benchmark) delivered: criterion-based benchmark harness, sync diff + plan across low/medium/high complexity (~490ns-4.2µs observed, all well under 100ms target). Batch 2 Slice 5 (error budget tracking panels) delivered: preview + apply 1h burn-rate stat panels backed by intent_api_rebase_preview_requests_total and intent_api_rebase_apply_requests_total. Batch 2 Slice 6 (graph + HTTP + DB benchmarks) delivered: graph traversal (BFS, path finding, cycle detection), intent-api sync path (diff compute, validation), and intent-service DB operations (live run: p50 25ms create_intent, 1.6ms create_version, <1ms get_intent/get_versions_by_intent against live Postgres). Batch 2 Slice 7 (multi-window burn-rate alerting) delivered: 1h/6h/3d burn-rate alerting rules for preview and apply paths. Batch 2 Slice 8 (bounded Temporal adapter tracing) delivered. Batch 2 Slice 9 (bounded sqlx repository tracing) delivered.

**Items:**
- [x] SLO definitions (intent processing latency, rebase latency, approval wait time) — Batch 2 Slice 1
- [x] Grafana dashboard scaffold — Batch 2 Slice 1
- [x] Bounded OTEL propagation (request-id extraction + service method instrumentation + optional OTLP export + W3C trace-context + background task span propagation) — Batch 2 Slice 2
- [x] Alerting rules (warning, critical thresholds) — Batch 2 Slice 3
- [x] Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn — Batch 2 Slice 3
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
- [ ] Cross-process trace propagation across all service boundaries — investigated and deferred: Temporal SDK lacks per-request gRPC metadata injection (shared `Arc<RwLock>` race); sqlx lacks per-query context propagation; NATS publisher not yet implemented. Revisit when temporalio-client adds interceptor support or upgrades to a version with per-request metadata.
- [ ] Full production load testing (bounded HTTP load harness delivered — intent-api HTTP server benchmark with in-memory repos; full production load testing remains gated on P2 full completion)

---

## P3 — Phase 3 Batch 3a — Tenant Isolation Hardening

| Field | Value |
|-------|-------|
| **ID** | P3 |
| **Title** | Phase 3 Batch 3a — Tenant Isolation Hardening |
| **Purpose** | Verify and enforce tenant isolation across all surfaces: access control, data visibility, quotas, audit log separation, and data residency. |
| **Status** | 🔄 In Progress (bounded slices delivered: tenant isolation tests P3-S1, quota enforcement P3-S2, rule pack isolation P3-S3, audit query isolation P3-S4, tenant service/onboarding scaffold P3-S5) |
| **Priority** | High |
| **Owner** | Security / Platform |
| **Suggested Next Step** | Data residency verification; artifact-service tenant coverage extension |
| **Progress Notes** | Batch 3a gated on Phase 2b exit. Bounded slices delivered: P3-S1 (tenant isolation verification tests), P3-S2 (quota enforcement), P3-S3 (tenant-specific rule pack isolation), P3-S4 (tenant audit log separation via scoped audit query API), P3-S5 (tenant service scaffold + onboarding procedure skeleton). Broader artifact-service coverage and data residency remain open. |

**Items:**
- [x] Tenant isolation verification tests (cross-tenant access blocked, no data leakage) — P3-S1 bounded slice delivered
- [x] Resource quota enforcement (intents per tenant, artifacts per tenant) — P3-S2 bounded slice delivered
- [x] Tenant-specific rule pack isolation — P3-S3 bounded slice delivered
- [x] Tenant audit log separation (tenant-scoped audit query API) — P3-S4 bounded slice delivered; S3 cold storage remains Phase 4+ scope
- [ ] Data residency: tenant data stays in assigned region (update threat model)
- [~] Tenant onboarding/offboarding procedures documented — P3-S5 bounded slice (skeleton/runner only); full API, S3 bucket provisioning, NATS account creation, RBAC setup remain future scope

---

## P4 — Phase 3 Batch 3b — Forensic Replay Bundle

| Field | Value |
|-------|-------|
| **ID** | P4 |
| **Title** | Phase 3 Batch 3b — Forensic Replay Bundle |
| **Purpose** | Deliver forensic bundle capability: bundle model, generation (intent versions + artifacts + audit events + graph state), integrity verification, replay, retention, and export. |
| **Status** | 🔄 In Progress (Phase 3 Batch 3b bounded slice delivered) |
| **Priority** | High |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Implement bundle generation API and S3 storage integration |
| **Progress Notes** | **Phase 3 Batch 3b bounded slice delivered:** forensic-service with ForensicVerificationService trait, InMemoryForensicVerificationService, request/response types, and coverage structs. API endpoint `POST /forensic/verify` integrated in intent-api with tests. OpenAPI documentation updated.

**Items:**
- [x] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) — ✅ Batch 0 scaffold delivered
- [x] Forensic verification API: `POST /forensic/verify` (bounded request-driven verification) — ✅ Phase 3 Batch 3b bounded slice delivered
- [x] Forensic archive export API: `POST /forensic/export` (bounded in-memory generation with scaffolded data) — ✅ Phase 3 Batch 3b bounded slice delivered
- [ ] Bundle generation: collect intent versions, artifacts, audit events, graph state from real services
- [ ] Bundle generation API: `POST /api/v1/forensic/bundle` (role: `forensic-access`)
- [ ] Bundle integrity verification (hash chain)
- [ ] Bundle replay capability (replay bundle to reproduce state)
- [ ] Bundle retention policy (configurable per tenant, compliance)
- [ ] Forensic bundle export from storage: `GET /api/v1/forensic/bundles/{id}/download`

**Bounded verification + export slices (delivered):**

---

## P5 — Phase 3 Batch 4a — Performance Work

| Field | Value |
|-------|-------|
| **ID** | P5 |
| **Title** | Phase 3 Batch 4a — Performance Work |
| **Purpose** | Optimize intent diff, graph traversal, and database queries; configure connection pooling; run load tests to validate Phase 3 production readiness. |
| **Status** | 🔄 In Progress (bounded slices delivered: P5-S1 graph traversal benchmarks, P5-S2 DB query benchmarks, bounded HTTP load harness/report; full production load testing remains open) |
| **Priority** | Medium |
| **Owner** | Backend Lead / SRE |
| **Suggested Next Step** | Proceed with DB query optimization if needed; full production load testing gated on P5 full completion |
| **Progress Notes** | **P5-S1 delivered:** rebase-engine benchmark harness with criterion. Benchmarks sync diff + plan path across low/medium/high complexity (~490ns-4.2µs, all under 100ms target). **P5-S2 delivered:** graph traversal benchmark (BFS, path finding, cycle detection across small/medium/large graphs), intent-api sync path benchmark (diff compute, validation, intent service create), and intent-service DB benchmark with live Postgres run (p50 25ms create_intent, 1.6ms create_version_with_occ, 873µs get_intent, 959µs get_versions_by_intent). **Bounded HTTP load harness delivered:** intent-api HTTP server benchmarks with in-memory repos (3 load levels, all passed; scope: in-memory repos only, no live Postgres). Full production load testing remains gated on P5 full completion. |

**Items:**
- [x] Rebase-engine sync diff + plan benchmark harness — P5-S1 bounded slice delivered
- [x] Graph traversal benchmarks (BFS, path finding, cycle detection) — P5-S1 bounded slice delivered
- [x] DB query benchmarks with live Postgres — P5-S2 bounded slice delivered
- [x] Intent-api HTTP server benchmarks (bounded — in-memory repos) — bounded slice delivered; full production load testing remains open
- [ ] Intent diff optimization (caching, parallel computation) — benchmark target: diff < 100ms (baseline: ~490ns-2.6µs, target met; may not be needed)
- [ ] Graph traversal optimization (indexing, query optimization)
- [ ] Database query optimization (indexes, query plans)
- [ ] Connection pooling (Postgres, NATS)
- [ ] Full production load testing (bounded HTTP harness delivered; full k6/Artillery production load testing remains gated on P5 full completion)

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
| **Progress Notes** | Four bounded executors delivered: RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). Audit trail events delivered. S2 planner/executor alignment resolved: S2ExternalReversible routes to CounterAction+SemiAutomatic. |

**Items:**
- [x] Side effect rollback record (compensation applied, compensation result) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [x] Compensation planner: generate compensation plan from side effects — ✅ delivered (Phase 3 Batch 1 bounded; class-based strategy routing; S2 routes to CounterAction+SemiAutomatic; fail-closed on unsupported strategy classes)
- [x] Four bounded compensation executors (RollbackExecutor, CounterActionExecutor, FollowupNoticeExecutor, EscalationExecutor) — ✅ delivered (Phase 3 Batch 1 bounded slice; each executor is strategy+feasibility-gated; acknowledges against side effect ledger; all other combos fail closed)
- [x] Compensation audit trail (`compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed`) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [x] **S2 planner/executor alignment** — ✅ resolved: S2ExternalReversible now routes to CounterAction + SemiAutomatic, aligned with CounterActionExecutor gate

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
| April 2026 | (orchestrator) | P2 updated: cross-process trace propagation investigated and deferred (Temporal SDK limitation, sqlx limitation, NATS not yet implemented) |
| April 2026 | (owner) | Initial creation |

---

## Related Docs

- [Current Project Status](./00-current-status.md)
- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
