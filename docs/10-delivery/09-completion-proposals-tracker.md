# 10 Project-Completion Proposals Tracker

Tracks the 10 major work proposals required to move the Intent Rebase Engine from current state to full Phase 3 completion and Phase 4 entry. Each proposal is a logical grouping of related work, owned by a named role (not necessarily a specific person). Update status, progress, and next steps as work advances.

> **Last updated:** April 2026  
> **Gate:** Most proposals are gated on Phase 2b exit. Proposal 1 represents the Phase 2b exit itself.

---

## Proposal Index

| ID | Title | Status | Priority |
|----|-------|--------|----------|
| [P1](#p1--phase-2b-exit-gate) | Phase 2b Exit Gate | ✅ Approved (name/date pending) | Critical |
| [P2](#p2--phase-3-batch-2--observability--sre) | Phase 3 Batch 2 — Observability + SRE | ⬜ Not Started | High |
| [P3](#p3--phase-3-batch-3a--tenant-isolation-hardening) | Phase 3 Batch 3a — Tenant Isolation Hardening | ⬜ Not Started | High |
| [P4](#p4--phase-3-batch-3b--forensic-replay-bundle) | Phase 3 Batch 3b — Forensic Replay Bundle | ⬜ Not Started | High |
| [P5](#p5--phase-3-batch-4a--performance-work) | Phase 3 Batch 4a — Performance Work | ⬜ Not Started | Medium |
| [P6](#p6--phase-3-batch-4b--security-hardening) | Phase 3 Batch 4b — Security Hardening | ⬜ Not Started | High |
| [P7](#p7--phase-3-batch-1-closure--compensation-plannerexecutor) | Phase 3 Batch 1 Closure — Compensation Planner/Executor | ⬜ Not Started | Critical |
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

Phase 2b scoped slices (runtime adapter, apply endpoint, risk classification, graph update, replay API, event streaming, bounded external surfaces) are delivered and gate-verified. Exit gate sign-off is still pending Slice B residual close-out.

---

## P2 — Phase 3 Batch 2 — Observability + SRE

| Field | Value |
|-------|-------|
| **ID** | P2 |
| **Title** | Phase 3 Batch 2 — Observability + SRE |
| **Purpose** | Deliver SLO definitions, alerting rules, error budget tracking, distributed tracing across Phase 2→3, performance benchmarks, and runbooks for common failure scenarios. |
| **Status** | ⬜ Not Started |
| **Priority** | High |
| **Owner** | SRE / Platform |
| **Suggested Next Step** | Define SLO targets (intent processing latency, rebase latency, approval wait time); set up provisional Grafana dashboard |
| **Progress Notes** | Batch 2 gated on Phase 2b exit and basic compensation engine path verified. Provisional SLO targets documented in `09-operations/04-sre-and-slos.md`; external SRE confirmation still open. |

**Items:**
- [ ] SLO definitions (intent processing latency, rebase latency, approval wait time)
- [ ] Alerting rules (warning, critical thresholds)
- [ ] Error budget tracking dashboard + runbook
- [ ] Distributed tracing across all services (full Phase 2 → Phase 3 trace)
- [ ] Performance benchmarks: rebase latency p50/p95/p99 (target: p95 < 60s for low/medium risk)
- [ ] Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout

---

## P3 — Phase 3 Batch 3a — Tenant Isolation Hardening

| Field | Value |
|-------|-------|
| **ID** | P3 |
| **Title** | Phase 3 Batch 3a — Tenant Isolation Hardening |
| **Purpose** | Verify and enforce tenant isolation across all surfaces: access control, data visibility, quotas, audit log separation, and data residency. |
| **Status** | ⬜ Not Started |
| **Priority** | High |
| **Owner** | Security / Platform |
| **Suggested Next Step** | Draft tenant isolation verification test plan; audit existing API endpoints for tenant-scoped enforcement |
| **Progress Notes** | Batch 3a gated on Phase 2b exit. No hard dependency on Batch 1/2 but benefits from them. Tenant-scoped idempotency already implemented in compensation-service path. Broader artifact-service coverage remains open. |

**Items:**
- [ ] Tenant isolation verification tests (cross-tenant access blocked, no data leakage)
- [ ] Resource quota enforcement (intents per tenant, artifacts per tenant)
- [ ] Tenant-specific rule pack isolation
- [ ] Tenant audit log separation (S3 tenant-scoped buckets/prefixes)
- [ ] Data residency: tenant data stays in assigned region (update threat model)
- [ ] Tenant onboarding/offboarding procedures documented

---

## P4 — Phase 3 Batch 3b — Forensic Replay Bundle

| Field | Value |
|-------|-------|
| **ID** | P4 |
| **Title** | Phase 3 Batch 3b — Forensic Replay Bundle |
| **Purpose** | Deliver forensic bundle capability: bundle model, generation (intent versions + artifacts + audit events + graph state), integrity verification, replay, retention, and export. |
| **Status** | ⬜ Not Started |
| **Priority** | High |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Finalize forensic bundle model with legal/compliance input; confirm S3 layout |
| **Progress Notes** | forensic-service scaffold delivered (`forensic-service/` package). Batch 3b gated on Phase 2b exit. |

**Items:**
- [ ] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`)
- [ ] Bundle generation: collect intent versions, artifacts, audit events, graph state
- [ ] Bundle generation API: `POST /api/v1/forensic/bundle` (role: `forensic-access`)
- [ ] Bundle integrity verification (hash chain)
- [ ] Bundle replay capability (replay bundle to reproduce state)
- [ ] Bundle retention policy (configurable per tenant, compliance)
- [ ] Forensic bundle export: `GET /api/v1/forensic/bundles/{id}/download`

---

## P5 — Phase 3 Batch 4a — Performance Work

| Field | Value |
|-------|-------|
| **ID** | P5 |
| **Title** | Phase 3 Batch 4a — Performance Work |
| **Purpose** | Optimize intent diff, graph traversal, and database queries; configure connection pooling; run load tests to validate Phase 3 production readiness. |
| **Status** | ⬜ Not Started |
| **Priority** | Medium |
| **Owner** | Backend Lead / SRE |
| **Suggested Next Step** | Profile current intent diff and graph traversal paths; identify top bottlenecks before Batch 4 begins |
| **Progress Notes** | Batch 4a gated on Batch 2 (observability) complete and full stack available. Load testing requires complete system. |

**Items:**
- [ ] Intent diff optimization (caching, parallel computation) — benchmark target: diff < 100ms
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
| **Purpose** | Complete threat model v2, penetration testing, security review for all Phase 3 features, compliance checklist, incident response plan, and data retention/deletion verification. |
| **Status** | ⬜ Not Started |
| **Priority** | High |
| **Owner** | Security Team |
| **Suggested Next Step** | Schedule threat model v2 review; begin penetration testing scope definition |
| **Progress Notes** | Threat model v2 input captured in `08-phase-2b-security-findings-input.md`. Batch 4b gated on Batch 2 (observability) complete. |

**Items:**
- [ ] Threat model v2 (updated from Phase 1) — doc: `../../14-governance/06-threat-model-v2.md`
- [ ] Penetration testing completed — all critical/high findings remediated
- [ ] Security review for all Phase 3 features — sign-off: security-team
- [ ] Compliance checklist (SOC2/GDPR if applicable)
- [ ] Incident response plan documented — doc: `../../14-governance/11-incident-freeze.md`; runbook: `incident-response.md`
- [ ] Data retention and deletion verified — S3 lifecycle policies enforced

---

## P7 — Phase 3 Batch 1 Closure — Compensation Planner/Executor

| Field | Value |
|-------|-------|
| **ID** | P7 |
| **Title** | Phase 3 Batch 1 Closure — Compensation Planner/Executor |
| **Purpose** | Complete the compensation planner stub (full plan generation from side effects) and replace the stub executor with real rollback/counter-action logic. Also add compensation audit trail. |
| **Status** | 🔄 Bounded Executors Delivered — S2 Planner/Executor Alignment Gap Remains (Phase 3 Batch 1) |
| **Priority** | Critical |
| **Owner** | Backend Lead |
| **Suggested Next Step** | Close S2 planner/executor alignment gap; then proceed to Batch 2 observability + SRE hardening |
| **Progress Notes** | Four bounded executors delivered: RollbackExecutor (Rollback+Automatic), CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). Audit trail events delivered. **Alignment gap:** planner routes S2ExternalReversible → Rollback strategy + SemiAutomatic feasibility; no bounded executor gate accepts this combination (RollbackExecutor accepts Rollback+Automatic only). Planner/executor alignment for S2 remains to close before Phase 3 exit gate. |

**Items:**
- [x] Side effect rollback record (compensation applied, compensation result) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [x] Compensation planner: generate compensation plan from side effects — ✅ delivered (Phase 3 Batch 1 bounded; class-based strategy routing with `default_rollback_strategy`; fail-closed on non-Rollback/unsupported strategies; S2 alignment gap remains open)
- [x] Four bounded compensation executors (RollbackExecutor, CounterActionExecutor, FollowupNoticeExecutor, EscalationExecutor) — ✅ delivered (Phase 3 Batch 1 bounded slice; each executor is strategy+feasibility-gated; acknowledges against side effect ledger; all other combos fail closed)
- [x] Compensation audit trail (`compensation.planned`, `compensation.started`, `compensation.completed`, `compensation.failed`) — ✅ delivered (Phase 3 Batch 1 bounded slice)
- [ ] **S2 planner/executor alignment** — open: planner routes S2ExternalReversible → Rollback + SemiAutomatic; no executor gate accepts this combo; must be resolved before Phase 3 exit gate

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
