# Production Readiness Backlog

> **Status:** Non-production — Phase 3 in progress
> **Scope:** Production readiness items only; feature delivery tracked separately
> **Last Updated:** April 2026

---

## Purpose

This document captures the prioritized production readiness backlog. It distinguishes between **non-production feature completion** (what has been implemented) and **production readiness** (what remains before production deployment is safe).

> **Key Distinction:** Feature delivery ≠ Production readiness. A bounded slice may be delivered but not production-ready. Do not conflate the two.

---

## P0 — Critical Blockers (Must Resolve Before Phase 3 Exit)

P0 items block Phase 3 exit gate and any production deployment.

### P0-1: Remote CI Startup Failure

| Field | Value |
|-------|-------|
| **Description** | GitHub Actions CI runs report `startup_failure` before jobs are created |
| **Impact** | Remote CI is not passing; code quality gates cannot run remotely |
| **Evidence** | GitHub Actions push run shows `startup_failure` status |
| **Owner** | Backend Lead |
| **Status** | 🔴 BLOCKED |
| **Workaround** | Local canonical gates pass: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check --workspace`, `cargo test --workspace` |

**No overclaim:** Remote CI `startup_failure` is not a green CI. Do not represent local gate passes as remote CI passing.

---

### P0-2: Full RLS (Row-Level Security) Coverage

| Field | Value |
|-------|-------|
| **Description** | PostgreSQL RLS policies defined but not fully wired into SQL query execution path |
| **Impact** | Tenant data isolation not enforced at the database layer in all code paths |
| **Current State** | JWT tenant_id validation delivered (P3-S5 bounded slice); automatic repository transaction wrapping to wire `RlsTenantContext` into SQL query execution path is PENDING |
| **Evidence** | `RlsTenantContext` struct exists with `set_rls_context`/`reset_rls_context` methods; RLS policies enabled in migration 013; `jwt_auth_async` middleware validates tenant_id claim |
| **Owner** | Backend Lead |
| **Status** | 🔴 IN PROGRESS — bounded JWT/RLS scaffold delivered; full wiring PENDING |
| **Requirements** | Automatic `SET app.current_tenant` before SQL queries; verified no cross-tenant data leakage |

**No overclaim:** JWT guard and RLS policy definitions do not constitute full RLS enforcement. Full repository transaction wrapping is required.

---

## P1 — High Priority (Must Resolve Before Production Deployment)

P1 items are required for safe production deployment but may be addressed in parallel with production infrastructure setup.

### P1-1: External SRE Sign-Off

| Field | Value |
|-------|-------|
| **Description** | External SRE review and approval of observability stack, SLO definitions, alerting rules |
| **Current State** | Solo self-review completed; provisional SLO targets, Grafana dashboard, Alertmanager config self-reviewed |
| **Evidence Required** | External SRE name, date, and sign-off statement |
| **Owner** | SRE |
| **Status** | 🔴 PENDING — solo self-review only; external sign-off not obtained |

**No overclaim:** Solo self-review is weaker evidence. External SRE sign-off is a distinct, higher-confidence milestone.

---

### P1-2: External Security Review Sign-Off

| Field | Value |
|-------|-------|
| **Description** | External security reviewer approval of JWT auth, RLS policies, tenant isolation, threat model v2 |
| **Current State** | Solo self-review completed; JWT auth, RLS, audit immutability, tenant isolation self-reviewed |
| **Evidence Required** | External reviewer name, date, and sign-off statement |
| **Owner** | Security |
| **Status** | 🔴 PENDING — solo self-review only; external review not engaged |

**No overclaim:** Solo self-review does not substitute for external security review.

---

### P1-3: Production Infrastructure

| Field | Value |
|-------|-------|
| **Description** | Production-grade infrastructure: Postgres with connection pooling, NATS with JetStream, S3 storage, monitoring stack |
| **Current State** | Local docker-compose environment available; production infra not provisioned |
| **Evidence Required** | Production environment verified operational; deployment runbook executed |
| **Owner** | SRE |
| **Status** | 🔴 BLOCKED — requires production environment provisioning |

**No overclaim:** docker-compose local is not production-equivalent.

---

### P1-4: Load Testing (L3–L5)

| Field | Value |
|-------|-------|
| **Description** | Staged and production load testing to validate performance under production-like load |
| **Current State** | L1 (bounded HTTP harness with in-memory repos) and L2 (SQLx local-live with docker-compose Postgres) delivered |
| **Evidence Required** | L3: Staged environment k6/Artillery results; L4: Alternative tool results; L5: Production load test results |
| **Owner** | SRE |
| **Status** | 🔴 BLOCKED — L3-L5 gated on staging/production infra |
| **Evidence Strength** | L1/L2 are local-docker only; do not represent as staging or production load test results |

**No overclaim:** L1/L2 bounded harness results are not staging or production load test results.

---

### P1-5: Penetration Testing (L3–L5)

| Field | Value |
|-------|-------|
| **Description** | External penetration testing engagement and findings remediation |
| **Current State** | Threat model v2 documented; pen test scope defined |
| **Evidence Required** | External pen test report; evidence of HIGH/CRITICAL findings remediated |
| **Owner** | Security |
| **Status** | 🔴 BLOCKED — requires external engagement |

**No overclaim:** Threat model documentation and pen test scope definition are not pen test execution.

---

## P2 — Phase 4 Scope (Deferred Until Phase 3 Exit)

P2 items are important but not blocking Phase 3 exit. They are Phase 4 candidates.

### P2-1: DLQ/NATS Lifecycle Implementation

| Field | Value |
|-------|-------|
| **Description** | Full NATS consumer lifecycle with DLQ routing and automatic replay worker |
| **Current State** | Bounded CheckpointCreatorConsumer behind `INTENT_API_NATS_CONSUMER=true` gate; DlqMetricsWorker delivered; G1-G5 design gates passed (solo self-review) |
| **Status** | 🔴 BLOCKED — implementation gated on G1-G5 evidence; G1 self-reviewed, G2 validated, G3 stubs, G4 RB11, G5 bounded tests |
| **Requirements** | G1-G5 gates must pass before any DLQ worker implementation begins |

**Note:** DLQ design is approved; DLQ worker implementation is future work gated on G1-G5.

---

### P2-2: Panic Hardening

| Field | Value |
|-------|-------|
| **Description** | Panic handler registration, graceful degradation on unexpected panics |
| **Current State** | Not started |
| **Owner** | Backend Lead |
| **Status** | 🔴 PENDING — Phase 4 candidate |

---

### P2-3: Trace Propagation (Cross-Process)

| Field | Value |
|-------|-------|
| **Description** | Distributed trace propagation across service boundaries (Temporal SDK, sqlx per-query context) |
| **Current State** | Bounded in-process OTEL propagation delivered; cross-process propagation investigated and deferred |
| **Evidence** | Temporal SDK 0.2.0 shares `Arc<RwLock>` race on `Connection::set_headers`; sqlx lacks per-query context propagation; NATS publisher not yet implemented |
| **Owner** | Backend Lead / SRE |
| **Status** | 🔴 DEFERRED — revisit when SDK support improves |

---

### P2-4: File Decomposition

| Field | Value |
|-------|-------|
| **Description** | Large module decomposition for maintainability |
| **Current State** | Not started |
| **Owner** | Backend Lead |
| **Status** | 🔴 PENDING — Phase 4 candidate |

---

### P2-5: Forensic Replay + Immutable Storage Lifecycle

| Field | Value |
|-------|-------|
| **Description** | Full forensic replay capability plus production-grade immutable bundle storage lifecycle |
| **Current State** | Bounded forensic bundle generation/export/download delivered; default storage remains in-memory; env-gated S3 bundle storage exists; full replay, Object Lock, retention enforcement, and chain-hash remain deferred |
| **Owner** | Backend Lead / Security |
| **Status** | 🔴 DEFERRED — Phase 4+ scope |

**No overclaim:** Forensic bundle generation and integrity checks are not equivalent to full replay or production-grade immutable evidence storage.

---

## Production Readiness Summary

| Priority | Item | Status | Evidence Required |
|----------|------|--------|------------------|
| **P0** | Remote CI startup failure | 🔴 BLOCKED | Remote CI passing |
| **P0** | Full RLS coverage | 🔴 IN PROGRESS | Cross-tenant isolation verified |
| **P1** | External SRE sign-off | 🔴 PENDING | SRE name/date/statement |
| **P1** | External security sign-off | 🔴 PENDING | Reviewer name/date/statement |
| **P1** | Production infra | 🔴 BLOCKED | Production env verified |
| **P1** | Load testing (L3-L5) | 🔴 BLOCKED | Staged/production results |
| **P1** | Penetration testing | 🔴 BLOCKED | External pen test report |
| **P2** | DLQ/NATS lifecycle | 🔴 BLOCKED | G1-G5 gates passed |
| **P2** | Panic hardening | 🔴 PENDING | Phase 4 scope |
| **P2** | Cross-process trace propagation | 🔴 DEFERRED | SDK support required |
| **P2** | File decomposition | 🔴 PENDING | Phase 4 scope |
| **P2** | Forensic replay + immutable storage lifecycle | 🔴 DEFERRED | Phase 4+ scope |

---

## Forbidden Claims

The following must NOT appear in any documentation:

| Forbidden Claim | Correct Wording |
|-----------------|----------------|
| `production-ready` | `non-production feature completion` or `bounded slice delivered` |
| `remote CI passed` | `local canonical gates pass` or `remote CI startup_failure` |
| `remote CI green` | `remote CI reports startup_failure` |
| `full RLS enforced` | `RLS policies defined; full wiring pending` |
| `production load test passed` | `L1/L2 bounded local evidence; L3-L5 blocked` |
| `SRE sign-off complete` | `solo self-review completed; external SRE sign-off pending` |
| `Security sign-off complete` | `solo self-review completed; external security review pending` |
| `pen test passed` | `threat model v2 documented; pen test scope defined; pen test not executed` |
| `staging environment` (when referring to docker-compose) | `docker-compose local (staging-like)` |

---

## Related Documents

- [Current Status](./00-current-status.md) — Feature delivery tracking
- [SRE Approval Checklist](./sre-approval-checklist.md) — Detailed SRE review items
- [CI/CD](../09-operations/02-ci-cd.md) — Actual vs aspirational CI/CD state
- [Solo Ops Evidence Plan](./16-solo-ops-evidence-plan.md) — Solo self-review evidence templates
