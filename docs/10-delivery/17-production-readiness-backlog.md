# Production Readiness Backlog

> **Status:** Non-production — Phase 3 closed (2026-05-11)
> **Scope:** Production readiness items only; feature delivery tracked separately
> **Last Updated:** 2026-05-11

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
| **RLC-4..RLC-9** | RLC test expansion (cross-tenant isolation) | ✅ BOUNDED VERIFIED LOCALLY | 12 rls_integration --ignored tests passed locally; full RLS enforcement pending all slices |

##### P1-S5 Sub-Slice Breakdown (Approval Handlers)

| Sub-Slice | Description | Status | Notes |
|-----------|-------------|--------|-------|
| **P1-S5a** | Approve/reject full RLS tx | ✅ BOUNDED DONE (pushed) | `begin_with_tenant → update_status_with_tx` for approve/reject |
| **P1-S5b** | Expire full RLS tx | ✅ BOUNDED DONE (pushed) | `begin_with_tenant → mark_expired_with_tx` for expire |
| **P1-S5c** | List pending handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on list_pending |
| **P1-S5d** | Revalidate handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on revalidate |
| **P1-S5e** | Trigger handler-level check | ✅ BOUNDED DONE (pushed) | Handler validates tenant scope on trigger |
| **P1-S5f** | Trigger full-tx create+cancel | ✅ BOUNDED VERIFIED LOCALLY | `begin_with_tenant → insert_request_with_tx → cancel_approved_by_intent_with_tx → commit`; handler-level guards delivered; full RLS tx deferred |
| **P1-S5g** | Compensation approve/waive/reapprove + batch approve/reapprove | ✅ BOUNDED VERIFIED LOCALLY | Handler-level guards for approve/waive/reapprove/batch status transitions; execute RLS tx covered separately by P1-S5h |
| **P1-S5h** | Compensation execute single + batch RLS tx | ✅ BOUNDED DONE (pushed 7167223) | Single execute RLS tx: `begin_with_tenant → executor (read-only) → record_result_with_tx + create_with_tx → commit`; non-JWT/non-RLS fallback calls `service.execute_action`; batch execute RLS tx: per-item `begin_with_tenant → executor → record_result_with_tx + create_with_tx → commit` with partial-success aggregation |
| **P1-S5i** | Forensic/orchestration/artifact full RLS tx | 🟡 PARTIAL — bounded orchestration/replay guard slices + artifact RLS tx + forensic bundle app-level RLS tx delivered | Migration 015 creates `orchestration_runs` table with RLS policy; `create_run_with_tx` method added to `SqlxOrchestrationRunRepository`; RLS path wired in `create_orchestration_run` handler; RLC-12 test added; `replay_intent` JWT tenant guard delivered with fail-closed mismatch check; `ingest_artifact` RLS tx wiring delivered (`begin_with_tenant → ingest_artifact_with_tx → commit`); forensic bundle app-level RLS tx bounded delivered for create/list/download handlers; in-memory/non-RLS fallback preserved |

**No overclaim:** S1-S4 are BOUNDED DONE (pushed commits). S5a..S5e are BOUNDED DONE (pushed). S5f (trigger full-tx) BOUNDED VERIFIED LOCALLY. S5g (approve/waive/reapprove + batch) BOUNDED VERIFIED LOCALLY. S5h (execute single + batch RLS tx) BOUNDED DONE (pushed 7167223) — `side_effect_repo()` accessor added, single execute uses `begin_with_tenant → executor → record_result_with_tx + create_with_tx → commit`; batch execute uses per-item sequential RLS tx with partial-success aggregation. S5i (orchestration_runs bounded slice + `replay_intent` handler guard) BOUNDED VERIFIED LOCALLY; `ingest_artifact` graph RLS tx BOUNDED DONE (pushed ee5510b) — `SqlxGraphRepository::ingest_artifact_with_tx` added; side-effect recording remains out-of-tx/best-effort for this bounded slice; `SqlxBundleRepository` exists (migration 016 with RLS); forensic bundle app-level RLS tx bounded delivered for create/list/download handlers; in-memory/non-RLS fallback preserved; full forensic replay/S3 Object Lock remain Phase 4+. RLC-4..RLC-12 are BOUNDED DONE (local — 13 tests passed via `cargo test --test rls_integration -- --ignored`). Full RLS enforcement is not complete until all slices pass. Remote CI not confirmed green.

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
| **Current State** | L1 (bounded HTTP harness with in-memory repos) and L2 (SQLx local-live with docker-compose Postgres) delivered; L4 bounded slices delivered: 6 core metrics scraped by Prometheus, 90s sustained-load smoke test passed (0% error, +4.0% RSS, FD flat), 10min extended sustained-load test passed (30,005 requests, 0% error, +4.7% RSS, FD flat), `IntentVersionCreationLowSuccessRate` alert fired via fault injection (180 errors over 6min, Alertmanager received and routed), Grafana dashboards provisioned (2 dashboards, datasource healthy, queries correct), Alertmanager receivers blocked (localhost placeholders only) |
| **Evidence Required** | L3: Staged environment k6/Artillery results; L4: 30min sustained load + all alert types triggered + real receivers; L5: Production load test results |
| **Owner** | Backend Lead (L1-L4 bounded); SRE (L3-L5 staged/production) |
| **Status** | 🟡 WAIVED-SOLO (non-production Phase 3 only) — L1-L4 bounded local evidence collected; 10min sustained and one alert firing validated; 30min sustained, remaining alert types, real receivers remain pending; L3-L5 staged/production gated on infrastructure |
| **Target Dates** | L4 30min sustained + all alert types + real receivers: Phase 4 (2026-Q3); L3-L5: gated on production infra provisioning |
| **Evidence Strength** | L1/L2/L4 bounded are local-docker only; do not represent as staging or production load test results |

**No overclaim:** L1/L2/L4 bounded harness results are not staging or production load test results. The 10-minute test is stronger than 90s but still not equivalent to 30min+ sustained load. Only one availability alert was triggered; latency, compensation, DLQ, and error budget alerts were not. Alertmanager receivers are localhost placeholders — no real external notification validated.

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
| **Current State** | ✅ BOUNDED IMPLEMENTED — ADR-09 accepted; bounded D1–D7 delivered at commit `d98c7dc`: `BlockedManualReview` approval create/cancel RLS slice verified; `SqlxGraphRepository::update_node_state_with_tx` graph RLS seam added and wired as primary path; checkpoint reads wrapped in read-only RLS tx (D1); graph updates run inside primary read-write RLS tx via caller-side orchestration (D2/D3/D4); post-hoc helper removed (D5); RLC-14 RLS integration test added (D6); non-RLS fallback preserved and tested (D7); runtime signal dispatch remains post-commit/out-of-transaction by design; fallback preserved when no RLS pool/claims/SQL repo |
| **Evidence** | `docs/13-adrs/09-rebase-apply-rls-transaction-boundary.md`; `crates/intent-api/src/rebase_apply_handlers.rs`; `crates/intent-service/src/approval_request_repo.rs`; `crates/graph-service/src/sqlx_graph_repository.rs`; bounded slice adds `begin_with_tenant → create_approval_request_with_tx → cancel_*_with_tx → commit` for the manual-review approval path; graph slice adds `update_node_state_with_tx` as primary RLS path for AutoProceeded paths (post-hoc helper removed); RLC-14: tenant isolation test in `crates/intent-api/tests/rls_integration.rs` |
| **Owner** | Backend Lead |
| **Status** | ✅ BOUNDED IMPLEMENTED per ADR-09 — D1–D7 delivered at `d98c7dc`; no longer blocked waiting for design or implementation |
| **Dependencies** | External SRE/security/load/pen gates remain independent blockers; ADR-08 Option A bounded implemented for SQL/RLS path; forensic bundle app-level RLS tx bounded delivered for create/list/download handlers; in-memory/non-RLS fallback preserved; S3 Object Lock/full replay remain Phase 4+ |

**No overclaim:** This is a bounded D1–D7 slice, not full production-ready `rebase_apply` RLS coverage. Runtime signal dispatch remains intentionally post-commit and out-of-transaction. ADR-08 Option A bounded implemented for SQL/RLS path with non-RLS fallback preserved; forensic bundle app-level RLS tx bounded delivered for create/list/download handlers with in-memory/non-RLS fallback preserved; S3 Object Lock, chain-hash, and full replay remain Phase 4+ scope. External gates (SRE sign-off, security review, load/pen testing) remain independent blockers before any production readiness claim.

---

#### P2-L4: Artifact Side-Effect Transaction Boundary (ADR-08)

| Field | Value |
|-------|-------|
| **Description** | Transactional artifact side-effect recording inside the existing RLS transaction with fail-closed semantics |
| **Current State** | ADR-08 Option A bounded implemented: `SideEffectRepository::as_sqlx_repo()` + `SqlxSideEffectRepository::create_with_tx` / `get_or_create_idempotent_with_tx` added; `SideEffectService::repo()` accessor added; `ingest_artifact` RLS path records side effects inside same tx before commit; side-effect write failure aborts artifact ingest (fail-closed); non-RLS fallback preserved with best-effort semantics |
| **Evidence** | `docs/13-adrs/08-artifact-side-effect-tx-boundary.md` (updated); `crates/compensation-service/src/side_effect_repo.rs` (`create_with_tx`, `get_or_create_idempotent_with_tx`, `as_sqlx_repo`); `crates/compensation-service/src/side_effect_service.rs` (`repo()`); `crates/intent-api/src/ingest_handlers.rs` (in-tx side-effect recording before commit with early return) |
| **Owner** | Backend Lead |
| **Status** | ✅ BOUNDED IMPLEMENTED — Option A delivered for SQL/RLS path; non-RLS fallback unchanged; no DLQ/background worker/Object Lock/production-ready overclaim |
| **Dependencies** | None (local code only) |

**No overclaim:** This is a bounded Option A implementation for the SQL/RLS path only. Async reconciliation (Option B), DLQ pipeline (Option C), Object Lock, and background workers remain Phase 4+ scope. Non-RLS fallback intentionally preserves best-effort semantics.

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
| **Current State** | Bounded forensic bundle generation/export/download delivered; bounded replay evidence slice delivered (per-section integrity hashes persisted in manifest, replay verification endpoint); default storage remains in-memory; env-gated S3 bundle storage exists; full runtime replay, Object Lock, retention enforcement, and chain-hash remain deferred |
| **Owner** | Backend Lead / Security |
| **Status** | 🟡 BOUNDED DELIVERED — replay evidence slice (per-section hashes + replay verification API) complete; full runtime replay and Object Lock remain Phase 4+ deferred |
| **External Dependency** | S3 Object Lock infrastructure, chain-hash implementation for full lifecycle |

**No overclaim:** Bounded replay evidence (stored per-section hashes + read-only verification) is NOT full runtime replay or production-grade immutable evidence storage.

---

#### P2-6: Webhook Delivery Production Hardening

| Field | Value |
|-------|-------|
| **Description** | Production-grade webhook delivery with outbox pattern, HMAC signing, key rotation, subscription CRUD API, and dedicated delivery worker |
| **Current State** | Bounded non-production slice delivered (B3-B18): payload/header builders, env-gated dispatcher (`INTENT_API_WEBHOOK_DELIVERY`, default disabled), in-process sequential retry loop, metrics counters, RB13 runbook, local alert rule, RLS test/helpers, docs sync, dead_code cleanup. Commits 5dcdd36 (apply-level wiremock 200-success/500-failure) and 2ab1c4b (verified bounded baseline) complete the locally verified baseline: `cargo test -p intent-api --lib webhook_delivery_tests` 57/57 passed; `cargo test -p intent-api --lib rebase_apply_handler_tests` 9/9 passed. No outbox guarantees, no HMAC, no key rotation, no subscription management API, no background worker |
| **Owner** | Backend Lead |
| **Status** | 🔴 DEFERRED — production delivery guarantees require outbox + worker infrastructure |
| **External Dependency** | None for local implementation; production deployment requires infrastructure for background workers and secret management |

**No overclaim:** The current webhook delivery is a bounded non-production slice. It runs in-process with best-effort dispatch and no delivery guarantee. Production hardening requires an outbox table, a background delivery worker, HMAC signature generation with per-subscription secrets, key rotation, and a subscription CRUD API. All of these remain Phase 4+ scope.

**Phase 4 Planning Slices (Deferred — Planning Only)**

| Slice | Description | Status |
|-------|-------------|--------|
| **P2-6a** | Outbox schema — detailed design below | 🔴 Deferred — schema design only; no migration or code |
| **P2-6b** | Background delivery worker lifecycle — detailed design below | 🔴 Deferred — design only |
| **P2-6c** | HMAC signing + key rotation — detailed design below | 🔴 Deferred — design only |
| **P2-6d** | Subscription CRUD API — detailed design below | 🔴 Deferred — design only |
| **P2-6e** | Retry / dead-letter semantics — detailed design below | 🔴 Deferred — design only |
| **P2-6f** | Rollback plan — detailed design below | 🔴 Deferred — design only |

#### P2-6a: Outbox Schema Design

> **Scope:** Schema design only. No migration DDL, no Rust code, no worker implementation, no production readiness claim.

**Columns / Types / Constraints**

| Column | Type | Constraints | Notes |
|--------|------|-------------|-------|
| `id` | UUID | PRIMARY KEY | Delivery identifier (same as `delivery_id` in the event envelope). |
| `tenant_id` | UUID | NOT NULL | RLS isolation key. |
| `intent_id` | UUID | NOT NULL | Logical FK to `intents`; enforce at application layer or defer to migration. |
| `subscription_id` | UUID | NOT NULL | Logical FK to `webhook_subscriptions`; enforce at application layer or defer to migration. |
| `event_type` | TEXT | NOT NULL | e.g., `intent_changed`, `rebase.plan_created`. |
| `payload` | JSONB | NOT NULL | Event payload envelope. |
| `status` | TEXT | NOT NULL | `pending`, `claimed`, `delivered`, `failed`. |
| `attempt_count` | INT | NOT NULL DEFAULT 0 | Incremented per delivery attempt. |
| `max_attempts` | INT | NOT NULL DEFAULT 3 | Configurable per subscription or system default. |
| `scheduled_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | Next delivery attempt time; supports back-off scheduling. |
| `locked_at` | TIMESTAMPTZ | NULL | Claim timestamp for worker concurrency. |
| `locked_by` | TEXT | NULL | Worker identity token (e.g., hostname + pid). |
| `delivered_at` | TIMESTAMPTZ | NULL | Set on final success. |
| `last_error` | TEXT | NULL | Error message from the last failed attempt. |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | Insert timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | Updated on any state change. |
| `lock_version` | INT | NOT NULL DEFAULT 0 | Optimistic locking for claim/release. |

**Indexes**

```sql
-- Pending due queue (primary worker polling index)
CREATE INDEX idx_webhook_outbox_pending_due
  ON webhook_outbox (scheduled_at, id)
  WHERE status = 'pending';

-- Tenant + intent lookup (support idempotency / replay queries)
CREATE INDEX idx_webhook_outbox_tenant_intent
  ON webhook_outbox (tenant_id, intent_id, created_at DESC);

-- Claimed rows (stale-claim recovery)
CREATE INDEX idx_webhook_outbox_claimed
  ON webhook_outbox (locked_at, locked_by)
  WHERE status = 'claimed';
```

**RLS Policy Draft**

```sql
ALTER TABLE webhook_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_outbox FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON webhook_outbox
  USING (tenant_id = current_tenant_id());
```

- `current_tenant_id()` is the existing RLS helper (or session variable) used by `RlsAwarePool`.
- `FORCE` ensures all queries respect the policy, including table owners.

**State Machine**

```
pending → claimed → delivered
            ↓
          failed
claimed --(stale recovery)--> pending
```

- `pending`: Awaiting delivery.
- `claimed`: Worker has taken ownership via optimistic lock (`lock_version` compare-and-set). Worker must set `locked_at`, `locked_by`, and increment `lock_version`.
- `delivered`: Final success; immutable. Worker sets `delivered_at`.
- `failed`: Exhausted `max_attempts` or non-retryable error. Worker sets `last_error`.

**Stale-Claim Recovery / Concurrency Note**

- Workers heartbeat via `locked_at`. A supervisor or scheduled sweep reclaims rows where `status = 'claimed'` and `locked_at < now() - interval '<stale_threshold>'` (e.g., 5 minutes).
- Reclaim transitions `claimed → pending` and clears `locked_at`/`locked_by` without incrementing `attempt_count`.
- Multiple workers may race; optimistic locking (`lock_version`) prevents double-delivery within the same claim window. At-least-once semantics are assumed; idempotency must be handled by subscribers.
- This design does **not** guarantee exactly-once delivery. Production hardening for stronger semantics remains Phase 4+.

**Retention / Partitioning Note**

- Phase 4+ should consider time-based partitioning or a retention job for delivered/failed rows to prevent unbounded growth.
- A separate `propagation_records` table already tracks high-level propagation status; `webhook_outbox` is the delivery mechanism, not the source of truth for propagation state.

**Relationship to Existing Tables**

- `propagation_records`: High-level propagation status per intent. `webhook_outbox` rows are generated from propagation signals but do not replace `propagation_records`.
- `webhook_subscriptions`: Subscriber configuration (URL, secret, event filters). `subscription_id` references this table. FK enforcement is deferred to implementation; schema design does not mandate it.

**Explicit Non-Goals**

- No migration DDL file (this is design-only).
- No Rust code, worker implementation, or `tokio::spawn` fire-and-forget conversion.
- No HMAC signing or key rotation (P2-6c).
- No subscription CRUD API (P2-6d).
- No per-attempt log table (P2-6e deferred).
- No DLQ / external queue integration (P2-3, P2-6e).
- No production readiness or at-least-once guarantee claim as current behavior.

**No overclaim:** This subsection is a schema design draft to guide a future migration. It is not a migration, not executable code, and does not confer any production readiness.

#### P2-6b: Background Delivery Worker Lifecycle

> **Scope:** Design-only. No Rust implementation, no `tokio::spawn` wiring, no production readiness claim.

**Startup / Gating**

- Env gate: `INTENT_API_WEBHOOK_OUTBOX_WORKER` (boolean).
- **Default:** `false` (conservative). Must be explicitly enabled.
- **Behavior when enabled:** On HTTP server startup, spawn a background task that polls the `webhook_outbox` table (P2-6a) and delivers pending rows.
- **Behavior when disabled:** No background task is spawned; outbox rows accumulate until a future worker starts or until manual intervention.

**Tokio Task Lifecycle**

```
main startup
  └─> if INTENT_API_WEBHOOK_OUTBOX_WORKER == true:
        spawn background_delivery_worker(shutdown_rx)
          └─> loop until shutdown_rx.changed() == true
                poll & claim rows
                dispatch HTTP requests
                record outcomes
```

- The worker is a single top-level `tokio::task` (or a small `tokio::spawn` family). Multiple worker instances are possible for horizontal scaling, but a single instance is the bounded design starting point.
- Each claimed row spawns a short-lived delivery sub-task. The parent worker uses a `JoinSet` (or equivalent) to track in-flight deliveries.

**Cancellation / Shutdown**

- Use the existing `watch::Receiver<bool>` shutdown pattern (same as `CheckpointCreatorConsumer`).
- On `SIGINT`/`SIGTERM`, the shutdown sender flips to `true`.
- The worker loop checks `shutdown_rx.has_changed()` (or `changed().await`) between poll iterations.
- **Graceful shutdown behavior:**
  1. Stop polling for new rows.
  2. Wait up to `OUTBOX_SHUTDOWN_DRAIN_SECONDS` (e.g., 30s) for in-flight `JoinSet` tasks to complete.
  3. After drain timeout, abort remaining tasks.
  4. Tasks that complete within the window record their outcome normally; tasks that are aborted record no outcome (best-effort; will be reclaimed as stale on next startup).
- No `CancellationToken` per sub-task is required for the bounded design; abort-on-drop is acceptable.

**Polling / Claim Loop**

```sql
-- Illustrative claim query (pseudocode)
WITH next_row AS (
  SELECT id, tenant_id, subscription_id, payload, lock_version
  FROM webhook_outbox
  WHERE status = 'pending'
    AND scheduled_at <= NOW()
  ORDER BY scheduled_at, id
  LIMIT <batch_size>
  FOR UPDATE SKIP LOCKED
)
UPDATE webhook_outbox
SET status = 'claimed',
    locked_at = NOW(),
    locked_by = :worker_id,
    lock_version = lock_version + 1
FROM next_row
WHERE webhook_outbox.id = next_row.id
  AND webhook_outbox.lock_version = next_row.lock_version
RETURNING webhook_outbox.*;
```

- **Worker identity (`:worker_id`):** A short token such as `{hostname}:{pid}:{task_id}` to aid stale-claim diagnosis.
- **Batch size:** 10–50 rows per poll to balance throughput and lock contention.
- **Poll interval:** 1–5 seconds (configurable via `OUTBOX_POLL_INTERVAL_MS`).
- **Claim semantics:** Optimistic locking via `lock_version` (see P2-6a). Only rows matching the pre-selected `lock_version` are updated; races are resolved by the `WHERE` clause.

**In-Flight Tracking**

- A `JoinSet<DeliveryResult>` (or equivalent) holds handles for active delivery sub-tasks.
- When a sub-task completes, it is removed from the set and its outcome is recorded via `record_delivery_outcome` (see P2-6a / P2-6e).
- The readiness probe reports `outbox_outstanding_count = join_set.len()`.

**Health / Readiness Probes**

- Extend the existing `GET /ready` response with two optional fields:
  - `outbox_worker_healthy`: `true` if the worker task is running and the last poll succeeded within `2 * poll_interval`.
  - `outbox_outstanding_count`: number of in-flight deliveries (`JoinSet` size).
- If the worker is disabled by env gate, `outbox_worker_healthy` may be omitted or set to `null`.
- Liveness: if the worker task panics or the claim loop errors repeatedly, the main process may choose to exit (fail-closed) or log and retry (fail-open). The bounded design recommends fail-open with `tracing::error!` and backoff.

**Metrics / Logging**

| Metric Name | Type | Description |
|---|---|---|
| `intent_api_outbox_claimed_total` | Counter | Rows successfully claimed by this worker. |
| `intent_api_outbox_delivered_total` | Counter | Rows delivered successfully (2xx). |
| `intent_api_outbox_failed_total` | Counter | Rows moved to `failed` (exhausted or non-retryable). |
| `intent_api_outbox_stale_reclaimed_total` | Counter | Rows reclaimed from stale workers (stale-claim recovery). |
| `intent_api_outbox_outstanding_count` | Gauge | Current in-flight delivery count. |

- Log at `info` level: worker startup, shutdown start, shutdown complete.
- Log at `debug` level: each poll iteration, claim count, per-row delivery start.
- Log at `warn` level: delivery failures, stale claims detected, claim races lost.
- Log at `error` level: worker panic, repeated DB connection failures, shutdown drain timeout.

**Failure Handling**

| Scenario | Behavior | Recorded State |
|---|---|---|
| DB connection lost during poll | Back off and retry poll loop | None (no rows claimed) |
| Claim race lost (lock_version mismatch) | Skip row; it will be picked up next poll | None |
| HTTP 2xx | Success | `delivered` |
| HTTP 4xx (non-429) | Non-retryable; mark failed | `failed` |
| HTTP 429 | Retry once with backoff; then failed if still 429 | `pending` → `failed` |
| HTTP 5xx / timeout / network error | Retry per policy; mark failed if exhausted | `pending` → `failed` |
| Worker panic during delivery | Task aborts; row remains `claimed` until stale recovery | `claimed` → `pending` (via stale reclaim) |
| Graceful shutdown with in-flight tasks | Wait for drain timeout; abort remaining | Best-effort (may leave `claimed` rows) |

**Relationship to P2-6a**

- This worker design depends on the `webhook_outbox` schema defined in P2-6a.
- `lock_version`, `locked_at`, `locked_by`, and `scheduled_at` are the coordination primitives.
- Stale-claim recovery (reclaiming `claimed` rows from crashed workers) is shared semantics between P2-6a and P2-6b.

**Explicit Non-Goals**

- No Rust code or `tokio::spawn` wiring is included in this subsection.
- No migration DDL (covered by P2-6a).
- No HMAC signing or key rotation (P2-6c).
- No subscription CRUD API (P2-6d).
- No per-attempt delivery log table (P2-6e).
- No DLQ / external retry queue (P2-3, P2-6e).
- No production readiness or at-least-once guarantee claim as current behavior.

**No overclaim:** This subsection is a design draft to guide future implementation. It is not code, not a migration, and does not confer any production readiness.

#### P2-6c: HMAC Signing + Key Rotation

> **Scope:** Design-only. No Rust implementation, no secret backend integration, no production readiness claim.

**Header Format**

```
X-Webhook-Signature: t=<unix_timestamp>,v1=<hmac_hex>,kid=<key_id>
```

- `t`: Unix timestamp (seconds since epoch) of the signing time.
- `v1`: HMAC-SHA256 of the canonical signing string, rendered as lowercase hexadecimal.
- `kid`: Key identifier (UUID or short slug) that identifies which secret was used. Enables dual-key grace periods and audit.

**Canonical Signing String**

The canonical string is a newline-delimited concatenation of the following fields in order:

```
delivery_id
event_id
event_type
occurred_at
tenant_id
workflow_id
body_hash
```

- `body_hash`: SHA-256 hash of the raw request body bytes, lowercase hex.
- Fields are concatenated with a single newline (`\n`) between each.
- No trailing newline after the last field.
- If a field is absent, use an empty string for that line.

**Algorithm / Versioning**

- Version: `v1` (HMAC-SHA256).
- Future versions may introduce `v2` with a different algorithm (e.g., Ed25519). The `v1` prefix in the header allows version negotiation.
- The consumer must reject signatures with an unrecognized version prefix.

**Key Identifier (`kid`)**

- `kid` is a stable identifier for the secret used to generate the signature.
- It is **not** the secret itself; it is a lookup key for the consumer (and for the service’s secret store).
- During rotation, two `kid` values may be valid simultaneously (dual-key grace window).

**Secret Storage Boundary**

- Per-subscription secrets are stored in a production secret manager (e.g., HashiCorp Vault, AWS Secrets Manager, Kubernetes Secrets).
- The service retrieves the secret by `(tenant_id, subscription_id, kid)` at delivery time.
- **Local dev:** Secrets may be stored in environment variables or a local secrets file (never committed).
- **No secret material is included in this document.** No keys, no placeholders, no examples of real secrets.
- Secret access must be logged at `info` level (access event, not the secret value).

**Rotation Workflow**

1. **Generate new key:**
   - Create a new secret for the subscription.
   - Assign a new `kid` (e.g., UUID v4).
   - Store the new secret in the secret manager.
   - Update the subscription record to mark the new `kid` as active.

2. **Dual-key grace window:**
   - Both the old `kid` and the new `kid` remain valid for a configurable grace period (default: 24 hours).
   - Deliveries during the grace window use the new `kid` (and new secret).
   - Consumers must accept signatures generated with either `kid` during the grace window.

3. **Consumer notification:**
   - Notify the consumer out-of-band (e.g., via the subscription management API or email) of the new `kid` and its effective date.
   - The consumer updates their verification logic to accept the new `kid`.

4. **Revoke old key:**
   - After the grace period expires, mark the old `kid` as revoked.
   - The service stops accepting the old `kid` for new deliveries.
   - The old secret may be deleted from the secret manager after a retention period (e.g., 7 days) to allow for audit or emergency rollback.

**Replay Protection / Timestamp Tolerance**

- Consumers must validate the `t` field to prevent replay attacks.
- **Tolerance window:** ±300 seconds (5 minutes) from the current time.
- Signatures with `t` outside the tolerance window must be rejected.
- Consumers should cache recently seen `delivery_id` values for at least the tolerance window to detect duplicate deliveries.

**Consumer Verification Guidance (Pseudocode)**

```
function verify_signature(header, body, known_secrets):
    parts = parse_header(header)  // t, v1, kid
    if abs(now() - parts.t) > 300:
        return "timestamp out of tolerance"
    secret = known_secrets[parts.kid]
    if secret is None:
        return "unknown key id"
    canonical = build_canonical_string(body)
    expected = hmac_sha256(secret, canonical).hex()
    if not constant_time_compare(parts.v1, expected):
        return "signature mismatch"
    if delivery_id_seen_recently(parts.delivery_id):
        return "duplicate delivery"
    return "ok"
```

- Use constant-time comparison (`constant_time_compare`) to prevent timing attacks.
- `known_secrets` is a map of `kid → secret` maintained by the consumer.

**Failure Behavior**

| Scenario | Service Behavior | Consumer Behavior |
|---|---|---|
| Secret missing from store | Skip delivery; log error; mark row `failed` | N/A |
| Secret retrieval fails | Retry per delivery policy; mark `failed` if exhausted | N/A |
| Consumer rejects signature (4xx) | Mark `failed`; no auto-retry | Return 4xx with reason |
| Consumer timestamp tolerance reject | Mark `failed` | Return 4xx with `X-Webhook-Error: timestamp_tolerance` |
| Rotation in progress, old `kid` used | Accept during grace window; prefer new `kid` for new deliveries | Accept both `kid`s during grace window |

**Relationship to P2-6a / P2-6b**

- P2-6c depends on the `webhook_outbox` table (P2-6a) for delivery state and on the background worker (P2-6b) for dispatch timing.
- The signing step occurs inside the delivery sub-task immediately before the HTTP request is sent.
- `kid` and secret retrieval are part of the delivery task, not part of the outbox schema.

**Explicit Non-Goals**

- No Rust code or HMAC implementation is included in this subsection.
- No migration DDL (covered by P2-6a).
- No secret backend implementation or Vault integration.
- No real secret material, placeholders, or example keys.
- No subscription CRUD API (P2-6d).
- No per-attempt delivery log table (P2-6e).
- No production readiness or signed-delivery guarantee claim as current behavior.

**No overclaim:** This subsection is a design draft to guide future implementation. It is not code, not a secret, and does not confer any production readiness.

#### P2-6d: Subscription CRUD API Design

> **Scope:** Design-only. No API implementation, no handler code, no OpenAPI contract implementation, no production readiness claim.

**Endpoints Summary**

| Method | Path | Purpose | Auth |
|--------|------|---------|------|
| `POST` | `/webhooks/subscriptions` | Create a new subscription | JWT + tenant isolation |
| `GET` | `/webhooks/subscriptions` | List subscriptions for the tenant | JWT + tenant isolation |
| `GET` | `/webhooks/subscriptions/{subscription_id}` | Get a single subscription | JWT + tenant isolation |
| `PATCH` | `/webhooks/subscriptions/{subscription_id}` | Update subscription fields | JWT + tenant isolation |
| `DELETE` | `/webhooks/subscriptions/{subscription_id}` | Soft-delete (disable) a subscription | JWT + tenant isolation |

**Request / Response Schemas**

*Create (`POST /webhooks/subscriptions`)*
```json
{
  "webhook_url": "https://downstream.example.com/webhooks",
  "event_types": ["intent_changed", "rebase.plan_created"],
  "downstream_system_id": "github-prod",
  "max_attempts": 3
}
```

*Response (all routes)*
```json
{
  "subscription_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "webhook_url": "https://downstream.example.com/webhooks",
  "event_types": ["intent_changed"],
  "downstream_system_id": "github-prod",
  "max_attempts": 3,
  "status": "active",
  "created_at": "2026-05-13T12:00:00Z",
  "updated_at": "2026-05-13T12:00:00Z"
}
```

**Lifecycle States**

| State | Meaning | Transitions |
|-------|---------|-------------|
| `active` | Subscription is eligible for delivery | `active` → `disabled` (PATCH or DELETE) |
| `disabled` | Subscription is paused; no new deliveries | `disabled` → `active` (PATCH) |
| `deleted` | Soft-deleted; retained for audit | `deleted` → none (immutable) |

- Hard delete is **not** supported to preserve audit lineage.
- `deleted` subscriptions are filtered out of the list endpoint by default.

**Tenant Isolation / RLS Behavior**

- `tenant_id` is required on every request and is sourced from the JWT claim.
- The SQL layer enforces tenant isolation via RLS (`tenant_id = current_tenant_id()`), identical to the P2-6a `webhook_outbox` pattern.
- Cross-tenant subscription access returns `404 Not Found` (fail-closed) rather than `403`, to avoid leaking subscription existence.

**Auth / Authorization**

- All endpoints require a valid JWT with `tenant_id` claim.
- No additional role-based checks for Phase 4 baseline; all authenticated tenant users may manage subscriptions.
- Future scope: role-based access control (e.g., `webhook:admin` scope) may be added later.

**Validation Rules**

| Field | Rule | Error |
|-------|------|-------|
| `webhook_url` | Valid URL; HTTPS required in production | `400` — `invalid_url` |
| `webhook_url` | Max length 2048 characters | `400` — `url_too_long` |
| `event_types` | Non-empty array; each element must be a known event type | `400` — `unknown_event_type` |
| `max_attempts` | Integer 1–10 (inclusive) | `400` — `max_attempts_out_of_range` |
| `downstream_system_id` | Non-empty string; max length 256 | `400` — `invalid_downstream_system_id` |

**Secret Redaction / HMAC Boundary**

- The `POST` response must **not** include the secret or `kid`. The secret is returned exactly once during creation (in the `secret` field of the `201 Created` response) and is never retrievable again.
- The `GET` response omits the `secret` and `kid` fields entirely.
- HMAC signing details (header format, canonical string, rotation) are defined in P2-6c. This subsection only covers the subscription management surface.
- Secret storage is delegated to the production secret manager; the subscription record stores only the `active_kid` and `revoked_kid` references.

**Error Semantics**

| Status | Scenario | Response Body |
|--------|----------|---------------|
| `201` | Subscription created | Full response including one-time `secret` |
| `200` | List / get / update success | Response without `secret` |
| `204` | Soft-delete success | Empty body |
| `400` | Validation failure | `{"error": "invalid_request", "details": [...]}` |
| `401` | Missing or invalid JWT | `{"error": "unauthorized"}` |
| `404` | Subscription not found or tenant mismatch | `{"error": "not_found"}` |
| `409` | Duplicate `downstream_system_id` for tenant | `{"error": "duplicate_downstream_system"}` |
| `429` | Rate limit exceeded | `{"error": "rate_limited", "retry_after": 60}` |

**Relationship to P2-6a / P2-6b / P2-6c**

- P2-6d is the management surface that creates the subscriptions used by the outbox (P2-6a) and the delivery worker (P2-6b).
- The `subscription_id` in `webhook_outbox` references the subscription created here.
- HMAC signing (P2-6c) depends on the `active_kid` stored in the subscription record.

**Explicit Non-Goals**

- No Rust handler code or route wiring is included in this subsection.
- No OpenAPI endpoint definitions claiming availability (the spec remains deferred).
- No migration DDL (covered by P2-6a).
- No secret backend implementation.
- No production readiness or current subscription CRUD availability claim.
- No role-based access control or advanced authorization.

**No overclaim:** This subsection is a design draft to guide future implementation. It is not an API contract implementation, not code, and does not confer any production readiness.

#### P2-6e: Retry / Dead-Letter Semantics Design

> **Scope:** Design-only. No queue implementation, no migration DDL, no Rust code, no production readiness claim.

**Critical Distinction: Two DLQ Concepts**

| DLQ | Source | Metrics Prefix | Current State |
|-----|--------|----------------|---------------|
| **NATS JetStream DLQ** | NATS consumer `max_deliver` exhaustion | `intent_api_dlq_*` | Bounded `DlqMetricsWorker` exists (depth/age gauges only); full replay worker deferred (P2-3). See `14-dlq-retry-design.md` for NATS-specific semantics. |
| **Webhook Outbox DLQ** | `webhook_outbox` rows that exhaust `max_attempts` | `intent_api_outbox_dlq_*` | Design-only. No table, no topic, no worker. This subsection defines the future semantics. |

> **Do not conflate the two.** NATS DLQ handles message-stream retries; webhook outbox DLQ handles HTTP delivery retries. They may coexist but are independent.

**Per-Attempt Delivery Log Table Concept (`propagation_delivery_attempts`)**

A future table may capture per-attempt detail for audit and debugging:

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID | PRIMARY KEY |
| `tenant_id` | UUID | RLS isolation key |
| `outbox_id` | UUID | Logical FK to `webhook_outbox` |
| `attempt_number` | INT | 1-indexed |
| `attempted_at` | TIMESTAMPTZ | When the HTTP request was sent |
| `http_status` | INT | Response status code (NULL if timeout/network error) |
| `duration_ms` | INT | Round-trip duration |
| `failure_reason` | TEXT | Sanitized error detail |
| `created_at` | TIMESTAMPTZ | Insert timestamp |

- **RLS:** Same `tenant_isolation` policy as `webhook_outbox`.
- **Retention:** Time-based partitioning or TTL recommended to prevent unbounded growth.
- **Scope:** This is a design concept only. No migration or code is included.

**Retry Queue Semantics**

- **Source:** `webhook_outbox` rows with `status = 'failed'` and `attempt_count < max_attempts`.
- **Retry trigger:** A background worker (P2-6b) or a dedicated retry scheduler picks up failed rows after a backoff delay.
- **Backoff policy:** Same as Slice 3 bounded policy (exponential backoff with full jitter, base 2s, multiplier 2.0, max delay 30s). See P2-6b for worker polling semantics.
- **Retry cap:** `max_attempts` (default 3, configurable per subscription 1–10).
- **Ordering:** Retries are ordered by `scheduled_at` (earliest first). No strict FIFO guarantee across tenants.

**DLQ Topic / Table Semantics**

- **Entry condition:** `attempt_count >= max_attempts` and final attempt returned a retryable error, OR a non-retryable error occurred on any attempt.
- **Webhook outbox DLQ options:**
  1. **DLQ table:** A `webhook_outbox_dlq` table with the same schema as `webhook_outbox` plus `dlq_reason` and `dlq_entered_at`.
  2. **DLQ topic:** A NATS/SQS topic `webhook.dlq` where exhausted rows are published as events for external systems.
  3. **Hybrid:** Rows moved to DLQ table; an async publisher emits events to the topic for integration with external observability.
- **Exit condition:** Manual operator review, automatic replay after a cooldown, or subscription update that resolves the root cause.
- **No auto-retry from DLQ:** By default, DLQ rows remain until an operator or explicit replay job acts on them.

**Replay / Operator Actions**

| Action | Behavior | Required State |
|--------|----------|----------------|
| **Manual replay** | Operator selects a DLQ row and triggers a new delivery attempt | `dlq` → `pending` (reset `attempt_count` to 0) |
| **Bulk replay** | Operator replays all DLQ rows for a subscription or tenant | Batch update to `pending` |
| **Purge** | Operator permanently removes DLQ rows (audit retention policy applies) | `dlq` → `deleted` |
| **Auto-replay (future)** | Scheduled job replays DLQ rows after a cooldown period (e.g., 1 hour) | Deferred to Phase 4+ |

**Metrics / Alerts (Design-Only)**

| Metric | Type | Description |
|--------|------|-------------|
| `intent_api_outbox_dlq_entries_total` | Counter | Rows entering the webhook outbox DLQ. |
| `intent_api_outbox_dlq_replayed_total` | Counter | Rows manually or bulk-replayed from DLQ. |
| `intent_api_outbox_dlq_purged_total` | Counter | Rows purged from DLQ. |
| `intent_api_outbox_dlq_current_count` | Gauge | Current DLQ depth. |
| `intent_api_outbox_dlq_oldest_age_seconds` | Gauge | Age of oldest DLQ row. |

- **Alert (design-only):** `WebhookOutboxDLQDepthHigh` — fired when `intent_api_outbox_dlq_current_count > threshold` for `duration`.
- **Alert (design-only):** `WebhookOutboxDLQStale` — fired when `intent_api_outbox_dlq_oldest_age_seconds > threshold`.
- These metrics and alerts are **not instrumented** and have **no local rules**. They are documented here for future SRE implementation.

**Relationship to P2-6a / P2-6b / P2-6d**

- P2-6e depends on the `webhook_outbox` schema (P2-6a) for retry source data and the delivery worker (P2-6b) for execution.
- Subscriptions (P2-6d) define `max_attempts` and the downstream URL, which influence retry and DLQ entry behavior.
- NATS DLQ semantics (P2-3, `14-dlq-retry-design.md`) are a separate concern and should not be confused with webhook outbox DLQ.

**Explicit Non-Goals**

- No NATS/SQS queue implementation or DLQ topic creation.
- No migration DDL for `propagation_delivery_attempts` or `webhook_outbox_dlq`.
- No Rust retry worker or DLQ consumer code.
- No automatic replay implementation.
- No production readiness or current DLQ availability claim.
- No delivery guarantee claim.

**No overclaim:** This subsection is a design draft to guide future implementation. It is not code, not a migration, and does not confer any production readiness.

#### P2-6f: Rollback Plan Design

> **Scope:** Design-only. No automation scripts, no code, no production readiness claim.

**Rollback Scenarios Summary**

| Scenario | Trigger | Rollback Action |
|----------|---------|-----------------|
| **A. Feature disable** | Webhook delivery causing instability | Set env gates to `false`; restart |
| **B. Worker drain** | Need to stop background worker safely | Graceful shutdown with drain timeout |
| **C. Subscription disable** | Single downstream system misbehaving | PATCH subscription to `disabled` |
| **D. Subscription deregister** | Downstream system permanently decommissioned | Soft-delete subscription; retain audit |
| **E. Outbox state recovery** | Corrupted or stuck outbox rows | Manual SQL update per rollback matrix |
| **F. Full rollback** | Catastrophic failure of webhook subsystem | Disable gates + disable all subscriptions + drain |

**Env-Gate Disable Procedure**

Two independent env gates control webhook delivery:

| Gate | Default | Disable Action | Effect |
|------|---------|----------------|--------|
| `INTENT_API_WEBHOOK_DELIVERY` | `false` | Set `false` (or unset) | Stops in-process dispatch inside apply handler |
| `INTENT_API_WEBHOOK_OUTBOX_WORKER` | `false` | Set `false` (or unset) | Stops background worker polling |

- **Rollback (A / F):** Set both gates to `false` and restart the service.
- **No data loss:** Outbox rows remain in `pending`/`claimed` status; they are not deleted.
- **No in-flight loss:** In-process deliveries complete before the handler returns; background worker drain is best-effort (see below).

**Worker Drain / Shutdown**

- Background worker uses `watch::Receiver<bool>` shutdown pattern (P2-6b).
- On shutdown signal:
  1. Stop polling for new rows.
  2. Wait up to `OUTBOX_SHUTDOWN_DRAIN_SECONDS` (default 30s) for in-flight `JoinSet` tasks to complete.
  3. After timeout, abort remaining tasks.
- **Rollback (B / F):** Trigger shutdown via `SIGTERM` or programmatic signal; wait for drain timeout.
- **Best-effort:** Tasks aborted after timeout leave rows in `claimed` status; stale-claim recovery (P2-6a) reclaims them on next worker startup.

**Subscription Disable / Deregister Without Data Loss**

- **Disable (C):** `PATCH /webhooks/subscriptions/{id}` → `{ "status": "disabled" }`.
  - Disabled subscriptions are skipped by the delivery worker.
  - Existing outbox rows for this subscription remain; they are not automatically cancelled.
- **Deregister (D):** `DELETE /webhooks/subscriptions/{id}` → soft-delete to `deleted` status.
  - Soft-delete retains the subscription record for audit.
  - New outbox rows will not reference this subscription.
  - Existing outbox rows referencing this subscription will fail delivery (404 or DNS failure) and eventually exhaust retries → DLQ (P2-6e).

**Outbox State Rollback Matrix**

| Current State | Desired Rollback State | Manual SQL (illustrative) | Notes |
|---------------|------------------------|---------------------------|-------|
| `claimed` (stuck) | `pending` | `UPDATE webhook_outbox SET status='pending', locked_at=NULL, locked_by=NULL WHERE status='claimed' AND locked_at < NOW() - INTERVAL '5 minutes';` | Use stale-claim recovery query |
| `failed` (transient) | `pending` | `UPDATE webhook_outbox SET status='pending', attempt_count=0, scheduled_at=NOW() WHERE status='failed' AND subscription_id = ?;` | Bulk retry after fixing root cause |
| `delivered` | N/A | None | Immutable; do not rollback |
| `dlq` (future) | `pending` | `UPDATE webhook_outbox_dlq SET status='pending', attempt_count=0, scheduled_at=NOW() WHERE ...;` | Replay from DLQ (P2-6e) |

- **Data loss prevention:** All state transitions are updates; no rows are deleted.
- **Audit:** `updated_at` is touched on every state change.

**DLQ Replay / Rollback Interaction**

- DLQ rows (P2-6e) can be replayed to `pending` after the root cause is resolved.
- Replay does **not** automatically re-enable subscriptions; if the subscription is `disabled` or `deleted`, the replayed delivery will fail again.
- Operator must verify subscription status before bulk replay.

**Rollback Verification Checklist**

- [ ] Both env gates (`INTENT_API_WEBHOOK_DELIVERY`, `INTENT_API_WEBHOOK_OUTBOX_WORKER`) are set to `false`.
- [ ] Service has restarted and health checks pass.
- [ ] Background worker is no longer polling (check logs for "worker shutdown complete").
- [ ] In-flight deliveries have drained or timed out (check `outbox_outstanding_count` gauge if available).
- [ ] Subscriptions are in expected state (`active`/`disabled`/`deleted`).
- [ ] Outbox rows are in expected state (no unexpected `claimed` rows older than stale threshold).
- [ ] No webhook HTTP requests are being sent (verify via network logs or downstream metrics).
- [ ] Apply path is unaffected (rebase apply returns 200 without delivery errors).

**Failure Escalation Path**

| Severity | Condition | Action | Owner |
|----------|-----------|--------|-------|
| P1 | Webhook delivery causing apply path failures | Immediate disable both gates; escalate to Backend Lead | On-call engineer |
| P2 | Persistent downstream 5xx for single subscription | Disable subscription; open incident | On-call engineer |
| P3 | Worker panic or claim loop failure | Restart worker; check logs; escalate if repeated | On-call engineer |
| P4 | Metrics anomaly (spike in failed deliveries) | Investigate downstream health; no immediate rollback needed | SRE (future) |

**Operator Runbook References**

- RB13 — Webhook Delivery Failures: diagnosis and mitigation for delivery errors.
- P2-6a — Outbox Schema Design: stale-claim recovery query and state machine.
- P2-6b — Background Delivery Worker Lifecycle: graceful shutdown and drain semantics.
- P2-6c — HMAC Signing + Key Rotation: secret revocation during rollback.
- P2-6d — Subscription CRUD API: disable/deregister endpoints and lifecycle states.
- P2-6e — Retry / Dead-Letter Semantics: DLQ replay and bulk retry procedures.

**Relationship to P2-6a..P2-6e**

- This rollback plan depends on the outbox schema (P2-6a), worker lifecycle (P2-6b), subscription management (P2-6d), and DLQ semantics (P2-6e).
- It does **not** introduce new schema or code; it documents operator procedures using the primitives defined in prior slices.

**Explicit Non-Goals**

- No automation scripts or operator tooling.
- No code changes or migration DDL.
- No infra changes (e.g., load balancer rules, DNS changes).
- No production readiness or automated rollback claim.
- No guarantee that rollback is instant or lossless.

**No overclaim:** This subsection is a design draft to guide future operator runbooks. It is not automation, not code, and does not confer any production readiness.

---

## Production Readiness Summary

| Priority | Item | Status | Evidence Required |
|----------|------|--------|------------------|
| **P0** | CI/Actions disabled by design | ✅ INTENTIONAL | Local gates are source of truth |
| **P1** | RLS transaction wrapping (P1-S1..S5 + RLC-4..13) | 🟡 BOUNDED LOCAL VERIFIED | S1-S4 BOUNDED DONE (pushed); S5a..S5e BOUNDED DONE (pushed); S5f/S5g/S5h bounded slices delivered; S5i orchestration/artifact graph bounded slices delivered; forensic SQL bundle repo + migration 016 delivered; targeted live RLC-13 passed on isolated local Postgres |
| **P1** | External SRE sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External SRE name/date/statement required before production claim |
| **P1** | External security sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External reviewer name/date/statement required before production claim |
| **P1** | Production infra | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production env verified required before deployment |
| **P1** | Load testing (L3-L5) | 🟡 WAIVED-SOLO (non-production Phase 3 only) | L1-L4 bounded local evidence collected; 10min sustained + one alert firing validated; 30min sustained + all alert types + real receivers pending Phase 4; staged/production results required before production claim |
| **P1** | Penetration testing | 🟡 WAIVED-SOLO (non-production Phase 3 only) | External pen test report required before production claim |
| **P2** | SqlxBundleRepository + forensic bundle RLS | ✅ BOUNDED VERIFIED | Local engineering backlog (P2-L1); migration 016 + SqlxBundleRepository + targeted live RLC-13 passed |
| **P2** | OpenAPI batch-execute RLS semantics | ✅ DOCUMENTED | Local engineering backlog (P2-L2); documentation complete |
| **P2** | rebase_apply handler review | ✅ BOUNDED IMPLEMENTED | Local engineering backlog (P2-L3); ADR-09 accepted; D1–D7 bounded implemented at commit `d98c7dc`; external gates still open |
| **P2** | Artifact side-effect tx boundary ADR | ✅ BOUNDED IMPLEMENTED | ADR-08 Option A delivered for SQL/RLS path; non-RLS fallback preserved |
| **P2** | Panic hardening (local-executable) | 🟡 BOUNDED SLICE DELIVERED | Bounded panic hook; full hardening Phase 4 scope |
| **P2** | File decomposition (local-executable) | 🟡 BOUNDED SLICES DELIVERED | Handler test groups extracted; `handler_tests.rs` reduced to router smoke test; `build_router_with_jwt_auth` deduplicated (delegates to `build_router`); broader router route grouping/split remains Phase 4 |
| **P2** | DLQ/NATS lifecycle | 🔴 DEFERRED | G1-G5 gates + Phase 4 infra |
| **P2** | Cross-process trace propagation | 🔴 DEFERRED | SDK support required |
| **P2** | Forensic replay + immutable storage lifecycle | 🟡 BOUNDED DELIVERED — replay evidence slice complete; full runtime replay + Object Lock Phase 4+ | Phase 4+ scope |
| **P2** | Webhook delivery production hardening | 🔴 DEFERRED — outbox, HMAC, key rotation, subscription CRUD, background worker | Phase 4+ scope; bounded B3-B18 + 5dcdd36 + 2ab1c4b form the locally verified non-production baseline; P2-6a..P2-6f design baseline complete (design-only, no implementation) |

---

## External Evidence Packets (Pending)

The following external evidence/gates remain pending and are not yet available:

| Packet | Status | Blocking |
|--------|--------|----------|
| External SRE sign-off | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| External security review | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| Penetration test report | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment |
| Load test L3-L5 results | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Production deployment; L4 10min sustained + one alert firing validated; 30min sustained + all alert types + real receivers pending Phase 4 |
| Production infrastructure | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Staging/production deployment |
| DLQ replay worker | 🔴 DEFERRED | Phase 4 (requires G1-G5 gates) |
| Cross-process trace propagation | 🔴 DEFERRED | Phase 4 (requires SDK fix) |
| Forensic replay + Object Lock | 🟡 BOUNDED DELIVERED — replay evidence slice (per-section hashes + replay verification API) complete; full runtime replay + Object Lock remain Phase 4+ | Phase 4+ |

**WAIVED-SOLO Policy:** External gates marked WAIVED-SOLO are accepted for non-production Phase 3 close-out only. Solo self-review is weaker evidence than external verification. All WAIVED-SOLO items must be revisited and closed with named external evidence before any production deployment or production-readiness claim.

## Local Engineering Backlog (Phase 3 Residual — P2 Priority)

The following items are local-executable and do not require external dependencies. They can proceed in parallel with external sign-off collection.

| Item | Priority | Status | Notes |
|------|----------|--------|-------|
| SqlxBundleRepository + forensic bundle RLS | P1 | ✅ BOUNDED VERIFIED | P2-L1 in this doc; migration 016 + SqlxBundleRepository + targeted live RLC-13 passed |
| OpenAPI batch-execute RLS semantics | P2 | ✅ DOCUMENTED | P2-L2 in this doc; documentation complete |
| rebase_apply handler review | P2 | ✅ BOUNDED IMPLEMENTED | P2-L3 in this doc; ADR-09 accepted; D1–D7 bounded implemented at commit `d98c7dc`; external gates remain open |
| Artifact side-effect tx boundary | P2 | ✅ BOUNDED IMPLEMENTED | ADR-08 Option A delivered for SQL/RLS path; non-RLS fallback preserved |
| Phase 4 deferred forensic S3/DLQ/trace | P2 | 🔴 DEFERRED | Phase 4+ scope |
| Forensic replay real-repo evidence | P2 | ✅ BOUNDED DELIVERED | Per-section integrity hashes persisted in manifest; replay verification endpoint; tests cover generate→store→retrieve→replay cycle |

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
- [ADR-09: Rebase Apply RLS Transaction Boundary](../13-adrs/09-rebase-apply-rls-transaction-boundary.md) — Accepted design for rebase_apply RLS transaction boundary; bounded D1–D7 implemented at commit `d98c7dc`; external gates and remaining blockers still explicit
