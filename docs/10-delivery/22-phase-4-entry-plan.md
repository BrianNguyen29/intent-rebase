# Phase 4 Entry Plan

**Status:** PLANNING — Phase 4 entry criteria and execution roadmap
**Date:** 2026-05-16
**Owner:** BrianNguyen (Backend Lead, solo practitioner)

---

## Purpose

This document provides a comprehensive todo-list and execution plan for entering Phase 4 of the Intent Rebase Engine. It is a **planning artifact only** — it does not claim that any Phase 4 work has been implemented or that the system is production-ready.

> **⚠️ Non-Production Caveat**
>
> Phase 4 entry requires closing or revisiting all external evidence gates that were WAIVED-SOLO during Phase 3 close-out. This plan enumerates those gates and local hardening items without claiming they are complete.

---

## Phase 4 Sub-Phase Grouping

| Sub-Phase | Theme | Items |
|-----------|-------|-------|
| **4a** | External Gates & Infrastructure | A-03 through A-07 |
| **4b** | Core Production Hardening | A-02, A-08 through A-12 |
| **4c** | Enterprise Expansion & Advanced Features | A-01, A-13, plus P8–P10 planning |

---

## A-01 — CI/Actions Decision

| Field | Value |
|-------|-------|
| **Description** | Decision on remote CI (GitHub Actions) vs. local gates as source of truth |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P0-1), `docs/10-delivery/00-current-status.md` |
| **Design/Implementation Status** | ✅ INTENTIONAL — GitHub Actions disabled by design; local gates are verification source |
| **Dependencies** | None |
| **Owner** | Backend Lead |
| **Validation Path** | `scripts/verify-fast.sh` passes; local canonical gates documented |
| **Non-Production Caveat** | Remote CI is not required for non-production feature delivery; production may require remote CI for audit traceability |

---

## A-02 — Full RLS Transaction Wrapping

| Field | Value |
|-------|-------|
| **Description** | Complete RLS-aware transaction wrapping across all SQL query paths (remaining P1-S5i, NATS tenant isolation, production certification) |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-0), `docs/08-security/02-authn-authz.md` |
| **Design/Implementation Status** | 🟡 BOUNDED PARTIAL — P1-S1..S5h delivered; P1-S5i (forensic/orchestration/artifact full RLS tx) partial; NATS tenant isolation pending; production certification pending |
| **Dependencies** | Local PostgreSQL, RLC test suite, oracle-ordered P1 slices |
| **Owner** | Backend Lead |
| **Validation Path** | `cargo test --test rls_integration -- --ignored` passes; all handlers use `begin_with_tenant` |
| **Non-Production Caveat** | Bounded slices verify local SQL paths only; full production RLS enforcement requires staged integration testing |

---

## A-03 — External SRE Sign-Off

| Field | Value |
|-------|-------|
| **Description** | External SRE review and approval of observability stack, SLO definitions, alerting rules, runbooks |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-1), `docs/09-operations/10-external-review-packet.md` (G-EXT-1) |
| **Design/Implementation Status** | 🔴 PENDING — solo self-review only; external sign-off not obtained |
| **Dependencies** | Production infrastructure provisioned; Grafana/Alertmanager deployed with real receivers; 30min sustained load + all alert types validated |
| **Owner** | SRE |
| **Validation Path** | External SRE reviewer signs Section H of external review packet; named evidence required |
| **Non-Production Caveat** | WAIVED-SOLO for Phase 3 close-out; must be revisited with named external evidence before production readiness claim |

---

## A-04 — External Security Review Sign-Off

| Field | Value |
|-------|-------|
| **Description** | External security reviewer approval of JWT auth, RLS policies, tenant isolation, threat model v2 |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-2), `docs/09-operations/10-external-review-packet.md` (G-EXT-2) |
| **Design/Implementation Status** | 🔴 PENDING — solo self-review only; external review not engaged |
| **Dependencies** | Threat model v2 accepted as internal planning artifact; pen test scope defined; RLS wrapping complete |
| **Owner** | Security |
| **Validation Path** | External security reviewer signs Section H of external review packet; named evidence required |
| **Non-Production Caveat** | WAIVED-SOLO for Phase 3 close-out; must be revisited with named external evidence before production readiness claim |

---

## A-05 — Production Infrastructure

| Field | Value |
|-------|-------|
| **Description** | Provision production-grade infrastructure: Postgres with connection pooling, NATS with JetStream, S3 storage, monitoring stack |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-3), `docs/09-operations/10-external-review-packet.md` (G-OPS-3) |
| **Design/Implementation Status** | 🔴 BLOCKED — local docker-compose only; production infra not provisioned |
| **Dependencies** | Cloud provider account; Terraform/CDK or equivalent IaC; SRE sign-off |
| **Owner** | SRE |
| **Validation Path** | Production environment verified operational; deployment runbook executed |
| **Non-Production Caveat** | docker-compose local is not production-equivalent; provisioning requires external infrastructure and budget |

---

## A-06 — Load Testing (L3–L5)

| Field | Value |
|-------|-------|
| **Description** | Staged and production load testing to validate performance under production-like load |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-4), `docs/11-quality/load-test-results.md`, `docs/09-operations/10-external-review-packet.md` (G-EXT-4) |
| **Design/Implementation Status** | 🟡 WAIVED-SOLO — L1/L2 bounded local evidence collected; L4 bounded slices delivered (10min sustained, one alert firing); L3/L5 blocked |
| **Dependencies** | Staging/production infrastructure; k6/Artillery harness; SRE sign-off |
| **Owner** | Backend Lead / SRE |
| **Validation Path** | L3: staged k6/Artillery results; L4: 30min sustained load + all alert types + real receivers; L5: production load test results |
| **Non-Production Caveat** | Bounded local harness results are not staging or production load test results; 10min test is not equivalent to 30min+ sustained load |

---

## A-07 — Penetration Testing

| Field | Value |
|-------|-------|
| **Description** | External penetration testing engagement and findings remediation |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P1-5), `docs/08-security/06-pen-test-scope.md`, `docs/09-operations/10-external-review-packet.md` (G-EXT-3) |
| **Design/Implementation Status** | 🔴 BLOCKED — threat model v2 and pen test scope accepted as internal planning artifacts; no pen test executed |
| **Dependencies** | External pen test team; staging environment; security review sign-off |
| **Owner** | Security |
| **Validation Path** | External pen test report (PDF + JSON); HIGH/CRITICAL findings remediated with evidence |
| **Non-Production Caveat** | Threat model documentation and pen test scope definition are not pen test execution; WAIVED-SOLO for Phase 3 close-out |

---

## A-08 — Panic Hardening

| Field | Value |
|-------|-------|
| **Description** | Panic handler registration, graceful degradation on unexpected panics, production alerting integration |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-1), `docs/10-delivery/05-phase-3-hardening.md` |
| **Design/Implementation Status** | 🟡 BOUNDED SLICE DELIVERED — panic hook registered at startup, sanitized logging; full hardening (worker lifecycle, alerting, graceful shutdown) remains Phase 4 scope |
| **Dependencies** | None (local-executable) |
| **Owner** | Backend Lead |
| **Validation Path** | `cargo test --workspace --lib --all-features` passes; panic hook test verifies sanitized output |
| **Non-Production Caveat** | Bounded local panic hook is not production alerting; full panic hardening remains Phase 4 scope |

---

## A-09 — File Decomposition

| Field | Value |
|-------|-------|
| **Description** | Large module decomposition for maintainability (router route-group split, remaining handler extractions) |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-2), `docs/10-delivery/20-project-completion-roadmap.md` (P1/P2) |
| **Design/Implementation Status** | 🟡 BOUNDED SLICES DELIVERED — many modules decomposed; broader router route grouping/split remains Phase 4 |
| **Dependencies** | None (local-executable) |
| **Owner** | Backend Lead |
| **Validation Path** | `cargo check --workspace --all-features` and `cargo test --workspace --lib --all-features` pass after each decomposition |
| **Non-Production Caveat** | Maintainability work does not imply production readiness; no production-readiness claim is made |

---

## A-10 — DLQ/NATS Lifecycle (Production-Grade)

| Field | Value |
|-------|-------|
| **Description** | Full NATS consumer lifecycle with production-grade DLQ routing, exponential backoff, poison-message detection, batch replay |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-3), `docs/10-delivery/16-solo-ops-evidence-plan.md` (G1-G5), `docs/09-operations/10-external-review-packet.md` (G-DLQ-1) |
| **Design/Implementation Status** | 🟡 BOUNDED LOCAL-DEV DELIVERED — `DlqMetricsWorker`, `DlqReplayWorker`, and full-consumer gate exist behind env gates; production-grade replay worker remains Phase 4+ deferred |
| **Dependencies** | G1-G5 gates closed; external SRE sign-off; production NATS/JetStream topology |
| **Owner** | Backend Lead |
| **Validation Path** | G1-G5 pass; promtool validates alerting rules; fault injection validates DLQ routing and replay |
| **Non-Production Caveat** | Local-dev gates are not production-ready; full DLQ replay worker requires external SRE sign-off before production deployment |

---

## A-11 — Cross-Process Trace Propagation

| Field | Value |
|-------|-------|
| **Description** | Distributed trace propagation across service boundaries (Temporal gRPC metadata, sqlx per-query context, NATS headers, HTTP forwarding) |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-4), `docs/10-delivery/12-trace-propagation-blocker-matrix.md` |
| **Design/Implementation Status** | 🔴 DEFERRED — Temporal SDK 0.2.0 lacks safe per-request gRPC metadata injection; sqlx lacks per-query context propagation; NATS publisher not yet implemented |
| **Dependencies** | Temporal SDK fix; sqlx feature addition; NATS publisher implementation |
| **Owner** | Backend Lead / SRE |
| **Validation Path** | End-to-end trace IDs visible across service boundaries in OTLP backend; integration tests verify header propagation |
| **Non-Production Caveat** | Bounded in-process OTEL propagation is not cross-process distributed tracing; revisit when SDK support improves |

---

## A-12 — Webhook Delivery Production Hardening

| Field | Value |
|-------|-------|
| **Description** | Production-grade webhook delivery with outbox pattern, background worker, HMAC signing, key rotation, subscription CRUD API |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-6), `docs/10-delivery/19-propagation-status-implementation-plan.md`, `docs/09-operations/10-external-review-packet.md` (G-WEB-1) |
| **Design/Implementation Status** | 🟡 SLICES 1–2 DELIVERED — outbox schema (migration 019), repository trait, in-memory implementation, bounded tests (Slice 1); env-gated worker with claim/list-pending flow and bounded tests (Slice 2). Slices 3–5 remain deferred. |
| **Dependencies** | Slice 1: local Postgres for migration; Slices 2–5: background worker, secret manager, subscription CRUD API |
| **Owner** | Backend Lead |
| **Validation Path** | Slice 1: `cargo test -p intent-api --lib webhook_outbox_repo` passes. Slice 2: `cargo test -p intent-api --lib webhook_outbox_worker` passes. Slices 3–5: end-to-end delivery test with real subscriber; HMAC verification; key rotation grace window test |
| **Non-Production Caveat** | Current webhook delivery is a bounded non-production slice (in-process, best-effort, no delivery guarantee). Outbox foundation (Slice 1) is local-dev only; production hardening requires Slices 2–5. |

### Slice Execution Checklist

| Slice | Description | Status | Evidence |
|-------|-------------|--------|----------|
| **Slice 1** | Outbox schema + repository foundation | 🟡 DELIVERED | Migration `019_create_webhook_outbox.sql`; `crates/intent-api/src/webhook_outbox_repo.rs` (trait + in-memory impl + tests) |
| **Slice 2** | Background delivery worker lifecycle | 🟡 DELIVERED | `crates/intent-api/src/webhook_outbox_worker.rs` (env-gated `WebhookOutboxWorker` trait, claim/list-pending `process_once`, in-memory tests) |
| **Slice 3** | HMAC signing + key rotation | 🔴 DEFERRED | Design documented in `docs/10-delivery/17-production-readiness-backlog.md` (P2-6c) |
| **Slice 4** | Subscription CRUD API | 🔴 DEFERRED | Design documented in `docs/10-delivery/17-production-readiness-backlog.md` (P2-6d) |
| **Slice 5** | Retry / dead-letter semantics | 🔴 DEFERRED | Design documented in `docs/10-delivery/17-production-readiness-backlog.md` (P2-6e) |

### Next Action

Wire `WebhookOutboxWorker` into application state and a background task loop when Slice 3 (HTTP dispatch + HMAC signing) begins. Slice 2 does not alter the existing env-gated dispatcher behavior; the worker is local-dev only and not wired into app startup.

---

## A-13 — Forensic Replay + Immutable Storage Lifecycle

| Field | Value |
|-------|-------|
| **Description** | Full forensic replay capability plus production-grade immutable bundle storage lifecycle (S3 Object Lock, chain-hash, retention enforcement) |
| **Source Refs** | `docs/10-delivery/17-production-readiness-backlog.md` (P2-5), `docs/10-delivery/00-current-status.md` (Batch 3b) |
| **Design/Implementation Status** | 🟡 BOUNDED DELIVERED — replay evidence slice (per-section integrity hashes + replay verification API) complete; full runtime replay and Object Lock remain Phase 4+ deferred |
| **Dependencies** | S3 Object Lock infrastructure; chain-hash implementation; retention policy enforcement |
| **Owner** | Backend Lead / Security |
| **Validation Path** | Replay verification API returns integrity pass; S3 Object Lock prevents object deletion; chain-hash detects tampering |
| **Non-Production Caveat** | Bounded replay evidence (stored per-section hashes + read-only verification) is NOT full runtime replay or production-grade immutable evidence storage |

---

## P8–P10 — Enterprise Expansion (Planning Only)

These completion proposals from `docs/10-delivery/09-completion-proposals-tracker.md` are tracked for Phase 4+ but are **not in scope for Phase 4 entry**:

| ID | Title | Status | Source Refs |
|----|-------|--------|-------------|
| P8 | Policy Simulation | ⬜ Planned | `docs/10-delivery/09-completion-proposals-tracker.md` |
| P9 | Advanced Adapters + Cross-Workflow Families | ⬜ Planned | `docs/10-delivery/09-completion-proposals-tracker.md` |
| P10 | Trust Scoring + Enterprise Integrations | ⬜ Planned | `docs/10-delivery/09-completion-proposals-tracker.md` |

---

## Recommended Execution Order

1. **Local-executable hardening (no external deps)** — A-02 (RLS completion), A-08 (panic hardening), A-09 (file decomposition)
2. **External evidence collection** — A-03 (SRE sign-off), A-04 (security sign-off), A-05 (production infra), A-06 (load testing), A-07 (pen testing)
3. **Core system hardening (requires staging)** — A-10 (DLQ/NATS production-grade), A-12 (webhook hardening), A-13 (forensic replay)
4. **Deferred SDK/infrastructure items** — A-11 (trace propagation) when Temporal SDK supports safe metadata injection
5. **Enterprise expansion** — P8–P10 after core system is production-hardened

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Phase 4 implemented` | `Phase 4 entry plan documented; implementation pending` |
| `Production-ready` | `Non-production feature completion; external gates remain open` |
| `SRE sign-off obtained` | `External SRE sign-off pending; solo self-review only` |
| `Security sign-off obtained` | `External security review pending; solo self-review only` |
| `Pen test passed` | `Pen test scope defined; execution pending external engagement` |
| `Load testing passed` | `L1/L2 bounded local evidence; L3-L5 blocked` |
| `Full RLS enforced` | `RLS policies defined; full wiring pending P1-S5i completion` |
| `DLQ production-ready` | `Local-dev DLQ gates delivered; full replay worker deferred` |
| `Webhook delivery production-ready` | `Bounded non-production slice delivered; outbox/worker/HMAC deferred` |
| `Forensic replay production-ready` | `Bounded replay evidence delivered; full runtime replay deferred` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/17-production-readiness-backlog.md` | Source of P1/P2 backlog items mapped to A-01..A-13 |
| `docs/10-delivery/00-current-status.md` | Current project status and prioritized next steps |
| `docs/09-operations/10-external-review-packet.md` | External review packet with readiness gate checklist (G-EXT-1..G-WEB-1) |
| `docs/10-delivery/09-completion-proposals-tracker.md` | P8–P10 enterprise expansion proposals |
| `docs/10-delivery/18-agent-safety-rebase-roadmap.md` | Agent Safety Rebase roadmap Phase 4+ items |
| `docs/10-delivery/20-project-completion-roadmap.md` | Project completion roadmap P3 (production readiness) |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-05-16 | BrianNguyen (via authorized assistant fixer) | Initial Phase 4 entry plan — A-01..A-13 mapped from production readiness backlog and current status; sub-phase grouping 4a/4b/4c; recommended execution order; forbidden claims; P8–P10 planning placeholders |
