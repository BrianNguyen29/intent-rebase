# 10 Project-Completion Proposals Tracker

Tracks the 10 major work proposals required to move the Intent Rebase Engine from current state to full Phase 3 completion and Phase 4 entry. Each proposal is a logical grouping of related work, owned by a named role (not necessarily a specific person). Update status, progress, and next steps as work advances.

> **Last updated:** April 2026  
> **Gate:** Most proposals are gated on Phase 2b exit. Proposal 1 represents the Phase 2b exit itself.

---

## Proposal Index

| ID | Title | Status | Priority |
|----|-------|--------|----------|
| [P1](#p1--phase-2b-exit-gate) | Phase 2b Exit Gate | ✅ Approved (name/date pending) | Critical |
| [P2](#p2--phase-3-batch-2--observability--sre) | Phase 3 Batch 2 — Observability + SRE | 🔄 In Progress (Slice P2-S4 delivered) | High |
| [P3](#p3--phase-3-batch-3a--tenant-isolation-hardening) | Phase 3 Batch 3a — Tenant Isolation Hardening | 🔄 In Progress (P3-S3, P3-S4, P3-S5 delivered) | High |
| [P4](#p4--phase-3-batch-3b--forensic-replay-bundle) | Phase 3 Batch 3b — Forensic Replay Bundle | 🔄 In Progress (P4 bounded slice delivered) | High |
| [P5](#p5--phase-3-batch-4a--performance-work) | Phase 3 Batch 4a — Performance Work | ⬜ Not Started | Medium |
| [P6](#p6--phase-3-batch-4b--security-hardening) | Phase 3 Batch 4b — Security Hardening | ⬜ Not Started | High |
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
| **Status** | 🔄 In Progress (Slice P2-S4 delivered; local baseline numbers captured) |
| **Priority** | High |
| **Owner** | SRE / Platform |
| **Suggested Next Step** | Define SLO targets (intent processing latency, rebase latency, approval wait time); set up provisional Grafana dashboard |
| **Progress Notes** | Batch 2 gated on Phase 2b exit and basic compensation engine path verified. Provisional SLO targets documented in `09-operations/04-sre-and-slos.md`; external SRE confirmation still open. **P2-S4 slice delivered:** benchmark harness infrastructure (criterion + benches/diff_latency.rs + CI benchmark job + baseline template) and RB6 rebase-stuck runbook. **P2-S5 local baseline numbers captured:** actual `compute_diff_sync` latency measured on local dev hardware (p50 range: 3.78–6.09 µs across 5 benchmark scenarios). Values recorded in `docs/11-quality/benchmark-baseline-results.md`. CI-averaged baseline and production load testing (k6/Artillery) remain gated on P2 full completion. |

**Items:**
- [ ] SLO definitions (intent processing latency, rebase latency, approval wait time)
- [ ] Alerting rules (warning, critical thresholds)
- [ ] Error budget tracking dashboard + runbook
- [ ] Distributed tracing across all services (full Phase 2 → Phase 3 trace)
- [~] Performance benchmarks: rebase latency p50/p95/p99 — **bounded slice delivered:** benchmark harness infrastructure (CI job + criterion reports + baseline template). Actual targets and production load testing remain gated on P2 full completion.
- [~] Runbooks: rebase-stuck (RB6 delivered); approval-backlog, artifact-quarantine-fail, compensation-timeout remain open

---

## P3 — Phase 3 Batch 3a — Tenant Isolation Hardening

| Field | Value |
|-------|-------|
| **ID** | P3 |
| **Title** | Phase 3 Batch 3a — Tenant Isolation Hardening |
| **Purpose** | Verify and enforce tenant isolation across all surfaces: access control, data visibility, quotas, audit log separation, and data residency. |
| **Status** | 🔄 In Progress (P3-S3, P3-S4, P3-S5 bounded slices delivered) |
| **Priority** | High |
| **Owner** | Security / Platform |
| **Suggested Next Step** | Draft tenant isolation verification test plan; audit existing API endpoints for tenant-scoped enforcement |
| **Progress Notes** | Batch 3a gated on Phase 2b exit. No hard dependency on Batch 1/2 but benefits from them. **P3-S3 bounded slice delivered:** TenantRulePackRepository trait + InMemory impl for tenant-scoped rule pack isolation (8 tenant isolation tests passing). **P3-S4 bounded slice delivered:** tenant-scoped audit query API (GET /audit/events, GET /audit/events/{event_id}) with cross-tenant isolation tests. **P3-S5 bounded slice delivered:** tenant-service scaffold (Tenant model + repository trait + InMemory impl), tenant onboarding procedure skeleton. Remaining work: quota enforcement, tenant isolation verification tests, data residency, full onboarding/offboarding API. |

**Items:**
- [ ] Tenant isolation verification tests (cross-tenant access blocked, no data leakage)
- [ ] Resource quota enforcement (intents per tenant, artifacts per tenant)
- [x] Tenant-specific rule pack isolation — ✅ P3-S3 bounded slice delivered
- [x] Tenant audit log separation (S3 tenant-scoped buckets/prefixes) — ✅ P3-S4 bounded slice delivered (audit query API; S3 archival Phase 4+)
- [ ] Data residency: tenant data stays in assigned region (update threat model)
- [x] Tenant onboarding/offboarding procedures documented — ~ P3-S5 bounded slice (skeleton/runner only; full API/automation future phase)

---

## P4 — Phase 3 Batch 3b — Forensic Replay Bundle

| Field | Value |
|-------|-------|
| **ID** | P4 |
| **Title** | Phase 3 Batch 3b — Forensic Replay Bundle |
| **Purpose** | Deliver forensic bundle capability: bundle model, generation (intent versions + artifacts + audit events + graph state), integrity verification, replay, retention, and export. |
| **Status** | 🔄 In Progress (P4 bounded slice delivered — content collection + integrity hashing) |
| **Priority** | High |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Implement bundle generation service (Phase 4 scope — collection from actual services, S3 storage, API) |
| **Progress Notes** | **P4 bounded slice delivered:** BundleStatus enum with status transition validation, BundleRepository trait with CRUD + status tracking methods, InMemoryBundleRepository implementation, forensic bundle model scaffold extended with status field. **Content collection + integrity hashing (P4 bounded slice):** bundle_hasher.rs (SHA-256 hashing, BundleIntegrityHash, section hash input types, verify_bundle_integrity), bundle_generator.rs (BundleGeneratorService, GenerateBundleRequest, BundleGenerationResult). 46 tests pass (deterministic hashing, content counts, tamper detection). **This bounded slice scope:** persistence primitives, status tracking, and content collection types with deterministic integrity hashing only. S3 storage, HTTP API, actual content collection from services, and replay remain Phase 4 scope. |

**Items:**
- [x] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) — ✅ P4 bounded slice delivered
- [x] Bundle content collection primitives + integrity hashing — ✅ P4 bounded slice delivered
- [x] Bundle integrity verification (hash chain) — ✅ P4 bounded slice delivered (verify_bundle_integrity for all 5 sections)
- [ ] Bundle generation: collect intent versions, artifacts, audit events, graph state — Phase 4 scope
- [ ] Bundle generation API: `POST /api/v1/forensic/bundle` (role: `forensic-access`) — Phase 4 scope
- [ ] Bundle replay capability (replay bundle to reproduce state) — Phase 4 scope
- [ ] Bundle retention policy (configurable per tenant, compliance) — Phase 4 scope
- [ ] Forensic bundle export: `GET /api/v1/forensic/bundles/{id}/download` — Phase 4 scope

---

## P5 — Phase 3 Batch 4a — Performance Work

| Field | Value |
|-------|-------|
| **ID** | P5 |
| **Title** | Phase 3 Batch 4a — Performance Work |
| **Purpose** | Optimize intent diff, graph traversal, and database queries; configure connection pooling; run load tests to validate Phase 3 production readiness. |
| **Status** | 🔄 In Progress (P5-S1 graph benchmark groundwork delivered) |
| **Priority** | Medium |
| **Owner** | Backend Lead / SRE |
| **Suggested Next Step** | Profile current intent diff and graph traversal paths; identify top bottlenecks before Batch 4 begins |
| **Progress Notes** | Batch 4a gated on Batch 2 (observability) complete and full stack available. **P5-S1 bounded slice delivered:** criterion benchmark harness for graph-service traversal/cycle-detection paths (`crates/graph-service/benches/graph_ops.rs`), capturing local baseline numbers (path_chain_20: ~6.6µs, cycle_detection_with_cycle: ~390ns). Load testing and production optimization claims remain gated on P5 full completion. |

**Items:**
- [ ] Intent diff optimization (caching, parallel computation) — benchmark target: diff < 100ms
- [~] **Graph traversal benchmarks** — ✅ **P5-S1 bounded slice delivered:** criterion harness with deterministic fixtures (chain-20, diamond, cycle graphs). Local baseline captured. Production optimization gated on P5 full completion.
- [ ] Graph traversal optimization (indexing, query optimization) — benchmark target: traversal < 50ms for 10k node graph
- [ ] Database query optimization (indexes, query plans) — `EXPLAIN ANALYZE` on critical queries
- [ ] Connection pooling (Postgres, NATS) — no connection exhaustion under load
- [ ] Load testing: simulate Phase 3 production load (10x normal, no SLO violations) — report: `load-test-results.md`

---

## P6 — Phase 3 Batch 4b — Security Hardening

| Field | Value |
|-------|-------|
| **ID** | P6 |
| **Title** | Phase 3 Batch 4b — Security Hardening |
| **Purpose** | Complete threat model v2, penetration testing, security review for all Phase 3 features, compliance checklist, incident response plan, and data retention/deletion verification — grounded in RR-04..RR-10 residual risks. |
| **Status** | 🔄 In Progress (threat model v2 delivered; RR-04..RR-10 registered; hardening items remain) |
| **Priority** | High |
| **Owner** | Security Team |
| **Suggested Next Step** | Scope penetration test to cover RR-04 (event delivery), RR-05 (notification), RR-06 (artifact custody), RR-09 (cross-tenant) as priority targets |
| **Progress Notes** | **Threat model v2 ✅ delivered** (`06-threat-model-v2.md`). **Residual risk register ✅ updated** with Phase 2b findings (RR-04–RR-10 in `13-residual-risk-spec.md`). RR-04..RR-06, RR-09 are High residual risks requiring monthly review cadence. Batch 4b gated on Batch 2 (observability) complete. |

**Items — grounded in RR-04..RR-10:**

| Item | Description | Risk Grounding |
|------|-------------|----------------|
| [x] | Threat model v2 (updated from Phase 1) — doc: `../../14-governance/06-threat-model-v2.md` | ✅ Delivered — covers RR-04..RR-10 attack surfaces |
| [ ] | **Penetration testing: event delivery path (JetStream/DLQ)** — verify RR-04 mitigations; confirm no event injection or loss in gap period | RR-04: Event Delivery Failure Detection Latency |
| [ ] | **Penetration testing: notification delivery path** — verify RR-05 mitigations; confirm operators actually receive alerts | RR-05: Operator Notification Not Actually Delivered |
| [ ] | **Penetration testing: artifact custody boundary** — verify RR-06 mitigations; confirm quarantine = actual storage isolation | RR-06: Artifact Custody Not Actual (Metadata Only) |
| [ ] | **Penetration testing: cross-tenant isolation** — verify RR-09 mitigations; active probe for cross-tenant data leakage | RR-09: Cross-Tenant Data Exposure Through Incomplete Enforcement |
| [ ] | **Penetration testing: trust boundary traversal** — verify RR-10 mitigations; confirm service scaffolds behave as isolated trust domains | RR-10: Moving Trust Boundaries During Phase 3 |
| [ ] | **Security review: forensic replay bounded semantics** — confirm RR-07 scope is clearly documented in user-facing docs and fails gracefully outside bounds | RR-07: Forensic Replay Bounded to Cooperative Checkpoint Replay |
| [ ] | **Security review: snapshot evidence integrity** — confirm RR-08 fallback snapshot quality is documented and source-tightening is tracked | RR-08: Snapshot Evidence Integrity Under Degraded Event Payloads |
| [ ] | Security review for all Phase 3 features — sign-off: security-team |  |
| [ ] | Compliance checklist (SOC2/GDPR if applicable) | |
| [x] | **Incident response plan documented** — doc: `../14-governance/14-incident-response-plan.md`; data freeze: `../14-governance/11-incident-freeze.md` | ✅ Delivered |
| [ ] | Data retention and deletion verified — S3 lifecycle policies enforced | |

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
| April 2026 | (owner) | Initial creation |

---

## Related Docs

- [Current Project Status](./00-current-status.md)
- [Roadmap](./01-roadmap.md)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [Phase 3 Checklist](./checklists/checklist-phase-3.md)
