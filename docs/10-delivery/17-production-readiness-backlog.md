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

### P0-1: CI/Actions Disabled by Design

| Field | Value |
|-------|-------|
| **Description** | GitHub Actions CI is intentionally disabled — no automatic runs on push or pull_request |
| **Impact** | Remote CI is not used; local verification is the source of truth |
| **Rationale** | Personal project with no collaborators; CI costs avoided by design |
| **Owner** | Backend Lead |
| **Status** | ✅ INTENTIONAL — not a blocker; local gates are the verification source |

**No overclaim:** CI/Actions being disabled is a deliberate choice, not a defect. Local canonical gates are the verification source.

---

## P1 — RLS Transaction Wrapping (Ordered Slices)

P1 items address the full RLS transaction wrapping plan from oracle design. These are P1 because they block production deployment (cross-tenant isolation), but the oracle plan provides a clear ordered execution path.

### P1-0: Full RLS Transaction Wrapping — Oracle Design (HIGH confidence)

| Field | Value |
|-------|-------|
| **Description** | Oracle-ordered plan for wiring RLS context into all SQL query execution paths |
| **Impact** | Tenant data isolation not enforced at the database layer in all code paths |
| **Evidence** | RLC-3 validation passed locally: migration_integration 1/1, rls_integration --ignored 4/4 (commit 42cdbe2) |

#### Ordered Implementation Slices

| Slice | Description | Status | Notes |
|-------|-------------|--------|-------|
| **P1-S1** | Move `RlsAwarePool` to shared location | ✅ BOUNDED DONE (pushed f055dc5) | RlsAwarePool shared via intent-rebase-types; enables RLS-aware pool wrapping |
| **P1-S2** | Wire `IntentService.rls_pool` | ✅ BOUNDED DONE (pushed) | IntentService.rls_pool wired; cargo fmt/check/test with/without jwt-auth passed (221 tests) |
| **P1-S3** | Add `RlsTransactionExt` trait | ✅ BOUNDED DONE (pushed f055dc5) | RlsTransactionExt trait enables `begin_with_tenant` on any sqlx::Transaction |
| **P1-S4** | Wrap `create_graph_edge` handler | ✅ BOUNDED DONE (pushed 02de885) | `begin_with_tenant → create_edge_with_tx → commit` wired; tenant mismatch rejection |
| **P1-S5** | Wrap compensation, forensic, orchestration, approval, artifact handlers | 🔴 PARTIAL — approval sub-slices delivered | See sub-slice breakdown below |
| **RLC-4..RLC-9** | RLC test expansion (cross-tenant isolation) | ✅ BOUNDED DONE (local) | 12 rls_integration --ignored tests passed locally; full RLS enforcement pending all slices |

##### P1-S5 Sub-Slice Breakdown (Approval Handlers)

| Sub-Slice | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **P1-S5a** | Approve/reject full RLS tx | ✅ BOUNDED DONE (pushed) | `begin_with_tenant → update_status_with_tx` for approve/reject |
| **P1-S5b** | Expire full RLS tx | ✅ BOUNDED DONE (pushed) | `begin_with_tenant → mark_expired_with_tx` for expire |
| **P1-S5c** | List pending handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on list_pending |
| **P1-S5d** | Revalidate handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on revalidate |
| **P1-S5e** | Trigger handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on trigger |
| **P1-S5f** | Trigger full-tx create+cancel | ✅ BOUNDED DONE (local) | `begin_with_tenant → insert_request_with_tx → cancel_approved_by_intent_with_tx → commit`; handler-level guards delivered; full RLS tx deferred |
| **P1-S5g** | Compensation approve/waive/reapprove + batch approve/reapprove | ✅ BOUNDED DONE (local) | Handler-level guards for approve/waive/reapprove/batch status transitions; execute RLS tx covered separately by P1-S5h |
| **P1-S5h** | Compensation execute single + batch RLS tx | ✅ BOUNDED DONE (pushed 7167223) | Single execute RLS tx: `begin_with_tenant → executor (read-only) → record_result_with_tx + create_with_tx → commit`; non-JWT/non-RLS fallback calls `service.execute_action`; batch execute RLS tx: per-item `begin_with_tenant → executor → record_result_with_tx + create_with_tx → commit` with partial-success aggregation |
| **P1-S5i** | Forensic/orchestration/artifact full RLS tx | 🟡 PARTIAL — bounded orchestration/replay guard slices + artifact RLS tx delivered | Migration 015 creates `orchestration_runs` table with RLS policy; `create_run_with_tx` method added to `SqlxOrchestrationRunRepository`; RLS path wired in `create_orchestration_run` handler; RLC-12 test added; `replay_intent` JWT tenant guard delivered with fail-closed mismatch check; `ingest_artifact` RLS tx wiring delivered (`begin_with_tenant → ingest_artifact_with_tx → commit`); forensic/artifact full tx wrapping PENDING |

**No overclaim:** S1-S4 are BOUNDED DONE (pushed commits). S5a..S5e are BOUNDED DONE (pushed). S5f (trigger full-tx) BOUNDED DONE (local). S5g (approve/waive/reapprove + batch) BOUNDED DONE (local). S5h (execute single + batch RLS tx) BOUNDED DONE (pushed 7167223) — `side_effect_repo()` accessor added, single execute uses `begin_with_tenant → executor → record_result_with_tx + create_with_tx → commit`; batch execute uses per-item sequential RLS tx with partial-success aggregation. S5i (orchestration_runs bounded slice + `replay_intent` handler guard) BOUNDED DONE (local); `ingest_artifact` graph RLS tx BOUNDED DONE (pushed ee5510b) — `SqlxGraphRepository::ingest_artifact_with_tx` added; side-effect recording remains out-of-tx/best-effort for this bounded slice; forensic bundle full SQL RLS tx is not applicable until a `SqlxBundleRepository` exists; `download_forensic_bundle` remains low-priority/read-only/in-memory. RLC-4..RLC-12 are BOUNDED DONE (local — 13 tests passed via `cargo test --test rls_integration -- --ignored`). Full RLS enforcement is not complete until all slices pass. Remote CI not confirmed green.

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

## P2 — Phase 4 Scope

P2 items are important but not blocking Phase 3 exit. They split into **local-executable** (can be done without external dependencies) and **deferred** (require external factors or Phase 4 infrastructure).

### Local-Executable (Can Begin After Phase 3 Exit)

These items can be started without waiting for external dependencies.

#### P2-L1: SqlxBundleRepository + Forensic Bundle RLS Wiring

| Field | Value |
|-------|-------|
| **Description** | Implement `SqlxBundleRepository` with RLS policy for forensic bundles; wire into forensic-service runtime |
| **Current State** | ✅ BOUNDED VERIFIED — SQL-backed forensic bundle repository, RLS migration, runtime SQL wiring, and targeted live RLC-13 tenant isolation test delivered |
| **Evidence** | `infrastructure/migrations/016_create_forensic_bundles.sql`; `crates/forensic-service/src/bundle_repo.rs` (`SqlxBundleRepository`); `crates/intent-api/src/main.rs` SQL wiring; `crates/intent-api/tests/rls_integration.rs` RLC-13 |
| **Owner** | Backend Lead |
| **Status** | ✅ BOUNDED VERIFIED — local code slice complete; targeted live RLC-13 passed on isolated local Postgres |
| **Dependencies** | Broader live RLS suite still requires local PostgreSQL; external sign-off remains separate |
| **Implementation Notes** | Migration 016 creates `forensic_bundles` with tenant RLS enabled/forced; runtime SQL path uses `SqlxBundleRepository`; in-memory fallback remains |

**No overclaim:** This closes the bounded local SQL/RLS repository slice only. Production deployment still requires external SRE/security review, infrastructure, and live integration evidence.

---

#### P2-L2: OpenAPI Batch-Execute RLS Semantics Documentation

| Field | Value |
|-------|-------|
| **Description** | Document RLS semantics for batch-execute endpoints (`POST /compensation-actions/batch-execute`) in OpenAPI spec |
| **Current State** | ✅ DOCUMENTED — OpenAPI spec updated with per-item RLS transaction semantics, partial-success aggregation, and best-effort rollback record semantics |
| **Evidence** | `docs/04-api/openapi.yaml` — batch-execute description updated with: (1) per-item RLS tx pattern `begin_with_tenant → executor dispatch → record_result_with_tx + create_with_tx → commit`; (2) partial-success semantics for tenant mismatch/not_found/executor failures; (3) fail-open best-effort rollback record creation |
| **Owner** | Backend Lead |
| **Status** | ✅ DOCUMENTED — P2-L2 closed |
| **Dependencies** | None (documentation only) |

**No overclaim:** OpenAPI spec update is documentation-only. Implementation already exists with per-item RLS transactions.

---

#### P2-L3: rebase_apply Handler Review

| Field | Value |
|-------|-------|
| **Description** | Review rebase_apply handler for RLS transaction wrapping and tenant isolation correctness |
| **Current State** | ✅ DESIGN RESOLVED — ADR-09 accepted; bounded Slice 1/2 delivered: `BlockedManualReview` approval create/cancel RLS slice verified; `SqlxGraphRepository::update_node_state_with_tx` graph RLS seam added; JWT AutoProceeded/AutoProceededWithNotification post-hoc RLS tx check/update applied after successful graph updates; fallback preserved when no RLS pool/claims/SQL repo |
| **Evidence** | `docs/13-adrs/09-rebase-apply-rls-transaction-boundary.md`; `crates/intent-api/src/rebase_apply_handlers.rs`; `crates/intent-service/src/approval_request_repo.rs`; `crates/graph-service/src/sqlx_graph_repository.rs`; bounded slice adds `begin_with_tenant → create_approval_request_with_tx → cancel_*_with_tx → commit` for the manual-review approval path; graph slice adds `update_node_state_with_tx` with post-hoc JWT RLS check/update for AutoProceeded paths; RLC-14: tenant mismatch rejection test extracted to `crates/intent-api/src/rebase_apply_handler_tests.rs` |
| **Owner** | Backend Lead |
| **Status** | ✅ DESIGN RESOLVED per ADR-09 — implementation deferred to Phase 4 D1–D7; no longer blocked waiting for design |
| **Dependencies** | D1–D7 implementation is Phase 4 scope; external SRE/security/load/pen gates remain independent blockers |

**No overclaim:** This is not full `rebase_apply` RLS coverage. The `AutoProceeded` graph update uses a post-hoc RLS check/update after the graph mutation, not a single atomic tx covering checkpoint and runtime signal. ADR-09 records the accepted design for Phase 4 D1–D7 implementation. Checkpoint alignment and runtime signal remain outside the RLS transaction until D1–D7 are implemented.

---

#### P2-L4: Artifact Side-Effect Transaction Boundary (ADR-08)

| Field | Value |
|-------|-------|
| **Description** | Design note for artifact side-effect out-of-transaction/best-effort semantics |
| **Current State** | ADR-08 created; documents current design (best-effort side-effect recording) and three options for Phase 4+ |
| **Evidence** | `docs/13-adrs/08-artifact-side-effect-tx-boundary.md` |
| **Owner** | Backend Lead |
| **Status** | ✅ DESIGN NOTE CREATED — implementation deferred to Phase 4+ |
| **Dependencies** | None (design only) |

**No overclaim:** ADR-08 is a design note. Implementation is Phase 4+ scope.

---

#### P2-1: Panic Hardening

| Field | Value |
|-------|-------|
| **Description** | Panic handler registration, graceful degradation on unexpected panics |
| **Current State** | Bounded local-executable slice delivered — panic hook registered at startup, sanitized logging, no production alerting claims |
| **Owner** | Backend Lead |
| **Status** | 🟡 IN PROGRESS (bounded local slice) — Phase 4 full hardening deferred |
| **Dependencies** | None (local code only) |
| **Implementation** | `init_panic_hook()` in `intent_api::panic_hardening` module (extracted from lib.rs as first file decomposition slice); re-exported from crate root for backward compatibility; called before `init_tracing()` in `main.rs`; sanitizes JWT/DB/creds/bearer tokens in panic payloads |
| **No overclaim** | Bounded local panic hook is not production alerting. Full panic hardening (worker lifecycle, alerting, graceful shutdown) remains Phase 4 scope. |

---

#### P2-2: File Decomposition

| Field | Value |
|-------|-------|
| **Description** | Large module decomposition for maintainability |
| **Current State** | Bounded slices delivered — `panic_hardening.rs`, DTO/type extraction, handler decomposition through intent read/validation/diff/error/approval helper/mutation/rebase-preview slices, handler test-module extractions complete (rebase simulation, approval invalidation, compensation simulation, rebase preview), and `build_router_with_jwt_auth` deduplication (`commit dbd8758`); `build_router_with_jwt_auth` now delegates to canonical `build_router` with `rls_pool: None` and layers JWT middleware, eliminating duplicate route registration; `handler_tests.rs` reduced to router smoke/residual wiring test; remaining higher-risk work is broader router route grouping/split |
| **Owner** | Backend Lead |
| **Status** | 🟡 IN PROGRESS (bounded decomposition slices delivered) — Phase 4 continues with broader router decomposition and additional bounded slices |
| **Dependencies** | None (local code only) |
| **Implementation** | `panic_hardening.rs` created; `init_panic_hook()` re-exported from `intent_api` crate root for backward compatibility; DTOs/types and many API handlers moved into focused modules; handler test groups extracted to `rebase_simulation_tests.rs`, `approval_invalidation_tests.rs`, `compensation_simulation_tests.rs`, and `rebase_preview_tests.rs`; `handler_tests.rs` now contains only the router smoke test and trace-context comment. Router JWT builder deduplication (`dbd8758`) removes duplicated route/state/middleware body from `build_router_with_jwt_auth` by delegating to `build_router(..., None)` and layering JWT middleware using the same pattern as `build_router_with_sql_audit_and_approval_jwt`. Broader router route grouping or file split remains a higher-risk production refactor deferred to Phase 4. No production-readiness claim is implied by this maintainability work. |

---

### Deferred (Require External Factors or Phase 4 Infrastructure)

These items cannot proceed until specific external conditions are met.

#### P2-3: DLQ/NATS Lifecycle Implementation

| Field | Value |
|-------|-------|
| **Description** | Full NATS consumer lifecycle with DLQ routing and automatic replay worker |
| **Current State** | Bounded CheckpointCreatorConsumer behind `INTENT_API_NATS_CONSUMER=true` gate; DlqMetricsWorker delivered; G1-G5 design gates passed (solo self-review) |
| **Status** | 🔴 DEFERRED — implementation gated on G1-G5 evidence; G1 self-reviewed, G2 validated, G3 stubs, G4 RB11, G5 bounded tests |
| **Requirements** | G1-G5 gates must pass before any DLQ worker implementation begins |
| **External Dependency** | Requires Phase 4 infrastructure and design review completion |

**Note:** DLQ design is approved; DLQ worker implementation is future work gated on G1-G5.

---

#### P2-4: Trace Propagation (Cross-Process)

| Field | Value |
|-------|-------|
| **Description** | Distributed trace propagation across service boundaries (Temporal SDK, sqlx per-query context) |
| **Current State** | Bounded in-process OTEL propagation delivered; cross-process propagation investigated and deferred |
| **Evidence** | Temporal SDK 0.2.0 shares `Arc<RwLock>` race on `Connection::set_headers`; sqlx lacks per-query context propagation; NATS publisher not yet implemented |
| **Owner** | Backend Lead / SRE |
| **Status** | 🔴 DEFERRED — revisit when SDK support improves |
| **External Dependency** | Temporal SDK fix required for safe per-request gRPC metadata injection |

---

#### P2-5: Forensic Replay + Immutable Storage Lifecycle

| Field | Value |
|-------|-------|
| **Description** | Full forensic replay capability plus production-grade immutable bundle storage lifecycle |
| **Current State** | Bounded forensic bundle generation/export/download delivered; default storage remains in-memory; env-gated S3 bundle storage exists; full replay, Object Lock, retention enforcement, and chain-hash remain deferred |
| **Owner** | Backend Lead / Security |
| **Status** | 🔴 DEFERRED — Phase 4+ scope |
| **External Dependency** | Requires S3 Object Lock infrastructure, chain-hash implementation |

**No overclaim:** Forensic bundle generation and integrity checks are not equivalent to full replay or production-grade immutable evidence storage.

---

## Production Readiness Summary

| Priority | Item | Status | Evidence Required |
|----------|------|--------|------------------|
| **P0** | CI/Actions disabled by design | ✅ INTENTIONAL | Local gates are source of truth |
| **P1** | RLS transaction wrapping (P1-S1..S5 + RLC-4..13) | 🟡 BOUNDED LOCAL VERIFIED | S1-S4 BOUNDED DONE (pushed); S5a..S5e BOUNDED DONE (pushed); S5f/S5g/S5h bounded slices delivered; S5i orchestration/artifact graph bounded slices delivered; forensic SQL bundle repo + migration 016 delivered; targeted live RLC-13 passed on isolated local Postgres |
| **P1** | External SRE sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External SRE name/date/statement required before production claim |
| **P1** | External security sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External reviewer name/date/statement required before production claim |
| **P1** | Production infra | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production env verified required before deployment |
| **P1** | Load testing (L3-L5) | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Staged/production results required before production claim |
| **P1** | Penetration testing | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External pen test report required before production claim |
| **P2** | SqlxBundleRepository + forensic bundle RLS | ✅ BOUNDED VERIFIED | Local engineering backlog (P2-L1); migration 016 + SqlxBundleRepository + targeted live RLC-13 passed |
| **P2** | OpenAPI batch-execute RLS semantics | ✅ DOCUMENTED | Local engineering backlog (P2-L2); documentation complete |
| **P2** | rebase_apply handler review | ✅ DESIGN RESOLVED | Local engineering backlog (P2-L3); ADR-09 accepted; design no longer blocked; implementation deferred to Phase 4 D1–D7 |
| **P2** | Artifact side-effect tx boundary ADR | ✅ DESIGN NOTE CREATED | ADR-08 created; implementation Phase 4+ |
| **P2** | Panic hardening (local-executable) | 🟡 BOUNDED SLICE DELIVERED | Bounded panic hook; full hardening Phase 4 scope |
| **P2** | File decomposition (local-executable) | 🟡 BOUNDED SLICES DELIVERED | Handler test groups extracted; `handler_tests.rs` reduced to router smoke test; `build_router_with_jwt_auth` deduplicated (delegates to `build_router`); broader router route grouping/split remains Phase 4 |
| **P2** | DLQ/NATS lifecycle | 🔴 DEFERRED | G1-G5 gates + Phase 4 infra |
| **P2** | Cross-process trace propagation | 🔴 DEFERRED | SDK support required |
| **P2** | Forensic replay + immutable storage lifecycle | 🔴 DEFERRED | Phase 4+ scope |

---

## External Evidence Packets (Pending)

The following external evidence/gates remain pending and are not yet available:

| Packet | Status | Blocking |
|--------|--------|----------|
| External SRE sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| External security review | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| Penetration test report | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| Load test L3-L5 results | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| Production infrastructure | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Staging/production deployment |
| DLQ replay worker | 🔴 DEFERRED | Phase 4 (requires G1-G5 gates) |
| Cross-process trace propagation | 🔴 DEFERRED | Phase 4 (requires SDK fix) |
| Forensic replay + Object Lock | 🔴 DEFERRED | Phase 4+ |

**WAIVED-SOLO Policy:** External gates marked WAIVED-SOLO are accepted for non-production Phase 3 close-out only. Solo self-review is weaker evidence than external verification. All WAIVED-SOLO items must be revisited and closed with named external evidence before any production deployment or production-readiness claim.

## Local Engineering Backlog (Phase 3 Residual — P2 Priority)

The following items are local-executable and do not require external dependencies. They can proceed in parallel with external sign-off collection.

| Item | Priority | Status | Notes |
|------|----------|--------|-------|
| SqlxBundleRepository + forensic bundle RLS | P1 | ✅ BOUNDED VERIFIED | P2-L1 in this doc; migration 016 + SqlxBundleRepository + targeted live RLC-13 passed |
| OpenAPI batch-execute RLS semantics | P2 | ✅ DOCUMENTED | P2-L2 in this doc; documentation complete |
| rebase_apply handler review | P2 | ✅ DESIGN RESOLVED | P2-L3 in this doc; ADR-09 accepted; design no longer blocked; implementation deferred to Phase 4 D1–D7 |
| Artifact side-effect tx boundary | P2 | ✅ DESIGN NOTE | ADR-08 created; implementation Phase 4+ |
| Phase 4 deferred forensic S3/DLQ/trace | P2 | 🔴 DEFERRED | Phase 4+ scope |
| Forensic replay real-repo evidence | P2 | 📋 CONSIDERED | Next candidate slice for real-repo validation; not yet implemented |

---

## Forbidden Claims

The following must NOT appear in any documentation:

| Forbidden Claim | Correct Wording |
|-----------------|----------------|
| `production-ready` | `non-production feature completion` or `bounded slice delivered` |
| `remote CI passed` | `local canonical gates are the required source of truth` or `remote CI startup_failure` |
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
- [External Review Packet](../09-operations/10-external-review-packet.md) — SRE/security review packet template
- [Pen/Load Test Packet](../09-operations/11-pen-load-test-packet.md) — Pen/load test execution packet template
- [ADR-08: Artifact Side-Effect Tx Boundary](../13-adrs/08-artifact-side-effect-tx-boundary.md) — Design note for artifact side-effect transaction boundary
- [ADR-09: Rebase Apply RLS Transaction Boundary](../13-adrs/09-rebase-apply-rls-transaction-boundary.md) — Accepted design for rebase_apply RLS transaction boundary; implementation deferred to Phase 4 D1–D7
