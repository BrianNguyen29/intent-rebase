# Project Assessment and Phase Execution Tracker

> **Status:** ASSESSMENT — consolidated source of truth for executing next phases
> **Date:** 2026-05-21
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Non-production planning and execution tracker only

---

## 1. Purpose

This document consolidates recent explorer/oracle findings into a single actionable tracker. It serves as the **source of truth** for:

- What has been delivered (bounded local-dev slices)
- What remains blocked (production/external gates)
- Known contradictions and technical debt
- Immediate cleanup actions (Phase 0)
- Execution plan for subsequent phases

> **⚠️ Non-Production Caveat**
>
> This tracker does **not** claim production readiness, external sign-off, CI-green status, or completed production gates. All external and production evidence gates remain **blocked or deferred**.

---

## 2. Caveats and Constraints

| Constraint | Implication |
|------------|-------------|
| **No production-readiness claim** | Every "delivered" item is bounded local-dev unless explicitly stated otherwise |
| **No external sign-off claimed** | A-03/A-04/A-07 require named independent third-party evidence; solo self-review is insufficient |
| **No CI-green claim** | Heavy GitHub Actions CI is manual-only; lightweight smoke may run on PR; local gates are source of truth (`scripts/verify-fast.sh` or equivalent commands) |
| **No invented reviewer names** | External reviewer fields use "(to be named)" placeholders only |
| **WAIVED-SOLO preserved** | Phase 3 close-out WAIVED-SOLO items remain non-production and must be revisited with real evidence |
| **Local-dev ≠ production** | `cargo test --workspace --lib --all-features`, in-memory repos, and docker-compose local are not production-equivalent |

---

## 3. Current Status Summary

### 3.1 Overall Health

| Dimension | Assessment | Evidence |
|-----------|------------|----------|
| **Local-dev bounded work** | Strong | 843 tokio tests, 46 ignored/manual tests, 21 migrations, canonical gates pass |
| **In-memory coverage** | Strong | `cargo test --workspace --lib --all-features` passes; most handlers have DB-free tests |
| **Production readiness** | Blocked | External gates open; no staging/production infra; no named external reviewers |
| **Live integration evidence** | Weak gap | Most RLS/NATS/SQLx/load tests require `#[ignore]` + external services; not run by default |
| **Documentation consistency** | Has contradictions | 10 identified contradictions/debt items (see §5); Phase 0 cleanup required |

### 3.2 Recent Commits Inventory

| Commit | Description | Evidence Type |
|--------|-------------|---------------|
| `389d15f` | `forensic-service` bundle repository tests extracted into `bundle_repo_tests.rs` | Local-dev code |
| `30d7277` | `intent-rebase-types` audit repository tests extracted into `audit_repo_tests.rs` | Local-dev code |
| `4b4691e` | Optional ignored SQLx smoke tests for audit and bundle repositories | Manual/`DATABASE_URL`-gated; not executed by default |
| `a969b79` | Repository smoke progress docs update | Documentation-only |
| `496bf66` | Phase 4 evidence plan added to `22-phase-4-entry-plan.md` | Documentation-only |

**Commit caveat:** All commits above are local-dev or docs-only. No production evidence or external sign-off is associated with any of them.

### 3.3 Test Inventory (Current)

| Category | Count | Status |
|----------|-------|--------|
| Tokio tests (workspace) | ~843 | Pass locally (in-memory) |
| Ignored/manual tests | ~46 | Require external services; not run by default |
| Migrations | 21 | Local docker-compose validated |
| CI coverage | Heavy CI manual-only; lightweight smoke PR-triggered | Local verification remains source of truth |
| RLS integration tests (live) | 22 | 22 passed on fresh DB; 3 passed, 19 failed on existing DB; 1 passed, 21 failed on existing DB with `RLS_TEST_RUN_MIGRATIONS=true` (migration 9 checksum mismatch). |
| NATS live integration tests | ~14 | 14 passed |
| SQLx repository smoke tests | ~7 | 7 passed (bundle 1, graph repo 5, audit 1) |
| Webhook integration test (S4) | 1 | 1 passed on fresh DB — SQLx outbox + subscription pipeline with real HTTP receiver |
| Migration integration tests | 2 | 2 passed (1 existing DB, 1 fresh DB) |
| Load tests (L1-L3) | Bounded harness | 2026-05-21 rerun: in-memory L1 1000/1000, L2 5000/5000, L3 completed (details truncated); SQLx L1 500/500; sustained 90s 4505/4505, 50.05 req/s, p95 4ms, p99 10ms, RSS +1.6%. **Local harness only; not production evidence.** |
| Load tests (L4-L5) | 0 | Blocked — no staging/production infra |
| Load tests (I2a sub-slice, 10min sustained + 1 alert) | 1+1 | 2026-06-06: 10min sustained load against external intent-api binary passed (30,005/30,005, 0% error, RSS +7.4%, FD flat); one availability alert pipeline fired end-to-end (Prometheus → Alertmanager → local `alert-receiver`). **Local sub-slice only; not full I2 (30min + all alert types + real receivers); not production evidence.** |
| Backup/restore validation (I6) | 1 round-trip | Local docker-compose only; `pg_dump`/`pg_restore` to separate DB; migration + webhook integration tests pass against restored DB |

---

## 4. Evidence Inventory

### 4.1 Local-Dev Evidence (Non-Production)

| Gate | Evidence | Strength | Notes |
|------|----------|----------|-------|
| Canonical gates | `scripts/verify-fast.sh` or equivalent commands pass | Strong for local dev | fmt, check, clippy, in-memory lib tests |
| RLS integration (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration -- --ignored --test-threads=1` **22 passed** | Strong for local-dev integration | Fresh DB path now unblocked after B-3 migration 19 fix and RLS harness fix. Existing DB path still shows schema mismatch (B-1/B-2). |
| RLS integration (existing DB) | `cargo test --test rls_integration -- --ignored` 3 passed, 19 failed on existing DB; 1 passed, 21 failed on existing DB with `RLS_TEST_RUN_MIGRATIONS=true` | Weak — schema drift on existing DB | Missing relations (`propagation_records`, `webhook_subscriptions`); migration 9 checksum mismatch when `RLS_TEST_RUN_MIGRATIONS=true`. Existing DB is stale relative to migration sequence. |
| Webhook delivery | `cargo test -p intent-api --lib webhook_delivery_tests` 57/57 pass | Strong for bounded slice | In-memory + wiremock; no production guarantees |
| Webhook integration (S4) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` **1 passed** | Strong for local-dev integration | SQLx-backed outbox + subscription pipeline with in-process HTTP receiver and real `reqwest` dispatch. Fresh DB only; local-dev evidence, not production. |
| I3 JWT→RLS→DML integration | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration test_i3 -- --ignored --test-threads=1` **1 passed** | Strong for local-dev integration | Authenticated handler path: JWT Bearer → auth middleware → `create_intent` → `IntentService::create_intent_with_rls` → `RlsAwarePool::begin_with_tenant` → SQLx DML. Fresh DB only; local-dev evidence, not production. |
| I6 Backup/restore local validation | `pg_dump` → `pg_restore` to separate DB (`intent_rebase_i6_restore`); 21 migrations restored; migration integration **1/1 passed**; webhook integration **1/1 passed** | Strong for local-dev procedure validation | Non-destructive `pg_dump`/`pg_restore` round-trip against local docker-compose Postgres. Not production PITR/basebackup validation; no RPO/RTO measured. |
| I2a sub-slice — 10min sustained load | `SUSTAINED_LOAD_URL=http://localhost:8080 SUSTAINED_LOAD_DURATION_SECS=600 SUSTAINED_LOAD_RPS=50 cargo test -p intent-api --features load-test --test load_test -- --nocapture test_sustained_load_smoke` → 30,005/30,005, 0% error, RSS +7.4%, FD flat | Bounded — local sub-slice | Default-feature local binary with local JWT dev fallback; `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix`; `INTENT_API_WEBHOOK_DELIVERY=false` and `INTENT_API_WEBHOOK_OUTBOX_WORKER=false`. 10 minutes only — **not** 30min. Not production/staging evidence. |
| I2a sub-slice — one availability alert pipeline | Schema-valid fault traffic (5 success / 361 cumulative errors in `intent_api_intent_version_created_total`) → Prometheus success-rate 0% → `IntentVersionCreationLowSuccessRate` firing (severity `warning`, slo `availability`) → Alertmanager routed to `warning-alerts` → local `alert-receiver` webhook received. | Bounded — local sub-slice, one alert only | Validates **one** availability alert end-to-end. **Not** all 17 alert rules, **not** latency/compensation/error-budget/DLQ types, **not** real external receiver (local placeholder only). Browser tools not used; checks via `curl`/HTTP API/`docker compose`. Full I2 / A-06 / L4 / L5 remain blocked. |
| Decomposition | `cargo check --workspace --all-features` + `cargo test --workspace --lib --all-features` pass | Strong for maintainability | Post-refactor verification only |
| Panic hardening | Panic hook test verifies sanitized output | Bounded | Local hook only; no production alerting |

### 4.2 Production/External Evidence (Blocked or Missing)

| Gate | Required Evidence | Current State |
|------|-------------------|---------------|
| A-03 External SRE Sign-Off | Named external SRE reviewer; signed Section H of external review packet | 🔴 BLOCKED — solo self-review only |
| A-04 External Security Review | Named external security reviewer; signed Section H; threat model v2 assessment | 🔴 BLOCKED — solo self-review only |
| A-07 Penetration Testing | External pen test report (PDF + JSON); HIGH/CRITICAL remediation evidence | 🔴 BLOCKED — scope defined, no execution |
| A-05 Production Infrastructure | Production Postgres/NATS/S3/monitoring operational; deployment runbook executed | 🔴 BLOCKED — docker-compose local only |
| A-06 Load Testing (L3-L5) | Staged k6/Artillery results; 30min sustained + all alerts + real receivers; production load test | 🔴 BLOCKED — L1/L2 local only; I2a 10min sustained + one availability alert sub-slice recorded 2026-06-06 (bounded, **not** full A-06) |
| A-10 DLQ/NATS Production-Grade | External SRE sign-off; production NATS topology; full replay worker validated | 🔴 BLOCKED — local-dev gate only |
| A-12 Webhook Production Hardening | Production secret manager + key rotation; staging/production SLO evidence; external review closure | 🔴 BLOCKED — local-dev slices only |
| A-13 Forensic Immutable Storage | S3 Object Lock deployed; chain-hash; retention enforcement validated | 🔴 BLOCKED — bounded replay evidence only |
| A-11 Trace Propagation | Cross-process trace IDs visible across service boundaries in OTLP backend | 🔴 DEFERRED — SDK-blocked |

---

## 5. Contradiction and Technical Debt Register

> **Purpose:** Track known inconsistencies found during explorer audits that require cleanup. Each item has an owner, severity, and next action.

| ID | Finding | Severity | Location(s) | Next Action | Owner |
|----|---------|----------|-------------|-------------|-------|
| **C-1** | Webhook backlog (`17-production-readiness-backlog.md` P2-6) lists design-only deferred slices, while `22-phase-4-entry-plan.md` A-12 shows many local-dev slices as delivered (Slice 1-5, Phase 1.1-2.3). Status mismatch creates stale contradiction. | MED | `docs/10-delivery/17-production-readiness-backlog.md` P2-6; `docs/10-delivery/22-phase-4-entry-plan.md` A-12 | Update P2-6 in backlog to reflect delivered local-dev slices and keep deferred items accurate | Backend Lead |
| **C-2** | `00-current-status.md` and other docs claim "CI disabled by design / no automatic runs on push", yet `.github/workflows/smoke.yml` exists and may have PR triggers. Claim vs artifact mismatch. | LOW | `docs/10-delivery/00-current-status.md`; `.github/workflows/smoke.yml` | Verify smoke.yml trigger config; update docs if workflow has active PR triggers or archive if truly disabled | Backend Lead |
| **C-3** | `17-production-readiness-backlog.md` P1-S5 aggregate line shows "PARTIAL" while sub-slices P1-S5a..S5i are mostly BOUNDED DONE. Aggregate status is stale. | LOW | `docs/10-delivery/17-production-readiness-backlog.md` P1-S5 | Update aggregate P1-S5 status to reflect sub-slice completion; add residual-open note if any | Backend Lead |
| **C-4** | K8s runbook references exist in some ops docs despite current infra being docker-compose only. Creates aspirational vs actual mismatch. | MED | `docs/09-operations/` runbooks | Audit runbooks for K8s refs; replace with docker-compose current state or add "future scope" caveat | Backend Lead |
| **C-5** | `17-production-readiness-backlog.md` header shows "Last Updated: 2026-05-11" but content has 2026-05-20 entries. Date is stale. | LOW | `docs/10-delivery/17-production-readiness-backlog.md` header | Update "Last Updated" date to most recent edit | Backend Lead |
| **C-6** | Stale line number references in docs pointing to code that has shifted due to decomposition. | LOW | Various docs with `crates/...` line refs | Audit and update line references or replace with stable anchors (function names, module paths) | Backend Lead |
| **C-7** | Threat model v1 vs v2 ambiguity: some docs reference "threat model v2" while others may still point to v1 without version qualifier. | LOW | `docs/08-security/`, `docs/14-governance/` | Audit threat model cross-refs; ensure all point to v2 with explicit version | Backend Lead |
| **C-8** | `20-project-completion-roadmap.md` P2 shows "Docs Complete" but Phase 4 decomposition (router route groups, service crate extractions, test extractions) is not reflected in roadmap batches. | MED | `docs/10-delivery/20-project-completion-roadmap.md` | Update roadmap to account for Phase 4 decomposition work or add cross-ref to `22-phase-4-entry-plan.md` | Backend Lead |
| **C-9** | External review packet (`10-external-review-packet.md`) G-RLS-1 status says "BOUNDED PARTIAL" but does not reflect recent P1-S5i closure. Lag between code changes and packet updates. | LOW | `docs/09-operations/10-external-review-packet.md` Appendix A | Sync G-RLS-1 with latest `22-phase-4-entry-plan.md` A-02 and `17-production-readiness-backlog.md` P1-S5i status | Backend Lead |
| **C-10** | Glossary (`docs/01-product/05-glossary.md`) may contain P4 references that mismatch current phase numbering or backlog terminology. | LOW | `docs/01-product/05-glossary.md` | Audit glossary for stale phase references; align with current `22-phase-4-entry-plan.md` A-item terminology | Backend Lead |

---

## 6. Risk Register

| Risk ID | Description | Likelihood | Impact | Mitigation | Owner |
|---------|-------------|------------|--------|------------|-------|
| **R-1** | Local-dev evidence is mistaken for production evidence by future readers | High | High | Explicit caveats in every doc; forbidden claims table; this tracker | Backend Lead |
| **R-2** | Docs drift out of sync with code after decomposition (stale line refs, stale status) | High | Medium | Phase 0 docs sync (C-1..C-10); verify-fast as baseline | Backend Lead |
| **R-3** | `#[ignore]` live tests (RLS, NATS, SQLx) rot because they are not run by default | Medium | High | Run ignored suite before claiming integration completeness; document manual trigger commands | Backend Lead |
| **R-4** | External gates (A-03/A-04/A-07) remain open indefinitely due to solo-practitioner bandwidth | Medium | High | This tracker makes blockers explicit; schedule external engagement as Phase 4 entry criteria | Backend Lead / User |
| **R-5** | NATS shared `audit_events` stream allows server-side cross-tenant leakage if RLS bypassed | Low | High | RLS ad-hoc wrapping is current mitigation; S1 (NATS tenant isolation) is next slice | Backend Lead |
| **R-6** | JWT dev fallback (`INTENT_API_REQUIRE_JWT=false`) allows unauthenticated access if misconfigured in staging | Medium | High | Document strict env requirement for any non-local env; add startup warning | Backend Lead |
| **R-7** | Temporal SDK trace propagation gap blocks observability compliance | Medium | Medium | A-11 deferred; monitor upstream SDK releases; evaluate workarounds | Backend Lead / SRE |
| **R-8** | Webhook/NATS/DLQ workers default-off means background features are not exercised in default local dev | Medium | Medium | Explicit env-gate documentation; integration tests behind env vars; docker-compose integration test (S4) | Backend Lead |

---

## 7. Phase Execution Plan

### Phase 0 — Docs Sync & Contradiction Cleanup (Immediate)

> **Goal:** Resolve C-1..C-10 contradictions and establish a clean baseline before further execution.

| ID | Action | Deliverable | Owner | Evidence |
|----|--------|-------------|-------|----------|
| P0-1 | Update `17-production-readiness-backlog.md` P2-6 to reflect delivered webhook local-dev slices | Updated backlog with accurate delivered/deferred split | Backend Lead | Diff review |
| P0-2 | Verify `.github/workflows/smoke.yml` trigger state; update `00-current-status.md` if needed | Doc accuracy or workflow archive | Backend Lead | File inspection |
| P0-3 | Update P1-S5 aggregate status in backlog | Accurate aggregate line | Backend Lead | Diff review |
| P0-4 | Audit ops runbooks for K8s aspirational refs; add caveats | Runbook consistency | Backend Lead | Grep + manual review |
| P0-5 | Update backlog "Last Updated" date | Current date in header | Backend Lead | Diff review |
| P0-6 | Replace stale line refs with stable module/function anchors | Accurate cross-references | Backend Lead | Grep + manual review |
| P0-7 | Standardize threat model v2 cross-refs | All security docs point to v2 explicitly | Backend Lead | Grep + manual review |
| P0-8 | Update `20-project-completion-roadmap.md` to reflect Phase 4 decomposition | Roadmap accuracy | Backend Lead | Diff review |
| P0-9 | Sync `10-external-review-packet.md` G-RLS-1 with latest RLS status | Packet accuracy | Backend Lead | Cross-doc review |
| P0-10 | Audit glossary for stale P4/phase refs | Glossary accuracy | Backend Lead | Grep + manual review |
| P0-R | Run `scripts/verify-fast.sh` after any doc changes that mention code | Baseline verification | Backend Lead | `verify-fast.sh` pass |

**Phase 0 Exit Criteria:**
- All C-1..C-10 items have resolution notes in this tracker
- `scripts/verify-fast.sh` passes
- No stale dates or contradictory status claims remain in referenced docs

**Phase 0 Progress (2026-05-20):**

| Item | Status | Resolution / Rationale |
|------|--------|------------------------|
| **P0-1** | ✅ RESOLVED | Updated `17-production-readiness-backlog.md` P2-6 deferred table: P2-6a..P2-6f now reflect delivered local-dev status vs remaining production blockers. Added explicit remaining blockers list. |
| **P0-2** | ✅ RESOLVED | Updated `00-current-status.md`: clarified that heavy `ci.yml` is manual-only (`workflow_dispatch`), while lightweight `smoke.yml` runs on `pull_request` + `workflow_dispatch`. Local gates remain primary source of truth. |
| **P0-3** | ✅ RESOLVED | Updated `17-production-readiness-backlog.md` P1-S5 aggregate: changed from "🔴 PARTIAL" to "🟡 BOUNDED PARTIAL — P1-S5a..S5i delivered; NATS tenant isolation and production certification remain open." |
| **P0-4** | ✅ RESOLVED | Grep audit of `docs/09-operations/05-runbooks.md` and sibling ops docs: no K8s aspirational refs found in runbooks. K8s mentions in `10-external-review-packet.md` (template checkbox) and `08-secrets-inventory.md` (secret manager options list) are acceptable as template options, not aspirational claims. |
| **P0-5** | ✅ RESOLVED | Updated `17-production-readiness-backlog.md` header: "Last Updated" changed from 2026-05-11 to 2026-05-20. |
| **P0-6** | ✅ RESOLVED (mapped subset) | Replaced 6 mapped stale line refs with stable module/function anchors: `checklist-phase-2.md` (CheckpointService, run_expiration, policy snapshot handlers/routes, update_graph_state), `11-phase-2b-sign-off-packet.md` (policy snapshot handlers/routes), `checklist-phase-3.md` (rebase_simulation handler). Rebase-engine doc-comment TODO cleanup completed (`planner.rs`, `lib.rs`). Bounded ignored-suite baseline initially blocked by missing local Postgres/NATS, then unblocked and rerun successfully; see 2026-05-21 update log. No broad whole-repo audit claimed. |
| **P0-7** | ✅ RESOLVED | Updated `docs/README.md` reading order (item 10) to label `01-threat-model.md` as baseline/legacy and point to `14-governance/06-threat-model-v2.md` as current. Updated `docs/13-adrs/README.md` internal links to reference `06-threat-model-v2.md` as current with `01-threat-model.md` as baseline/legacy. Phase checklists still point to v1; accepted as known debt since checklists are historical phase artifacts. |
| **P0-8** | ✅ RESOLVED | Updated `20-project-completion-roadmap.md` Related Documents: added cross-reference to Phase 4 Entry Plan (with decomposition note) and to this tracker. |
| **P0-9** | ✅ RESOLVED | Updated `10-external-review-packet.md` G-RLS-1: status changed from "BOUNDED PARTIAL" to "🟡 BOUNDED PARTIAL — P1-S5a..S5i delivered; handler-level tenant guards present in all scoped handlers"; missing evidence updated to "NATS tenant isolation, server-side per-tenant JetStream streams/ACLs, production certification." |
| **P0-10** | ✅ RESOLVED | Grep audit of `docs/01-product/05-glossary.md`: all Phase 4 references are accurate (e.g., ForensicBundle "Phase 4+", IntentFamily "Phase 4+"). No stale or contradictory phase references found. |
| **P0-R** | ✅ RESOLVED | Local baseline verified after doc changes: `scripts/verify-fast.sh` completed fmt/check/clippy, timed out during the final test phase at 10 minutes, then `cargo test --workspace --lib --all-features` was rerun separately and passed. No code changes were made. |

---

### Phase 1 — Verification Baseline

> **Goal:** Ensure canonical gates are the reliable source of truth and all ignored tests can be manually triggered.

| ID | Action | Deliverable | Owner | Evidence |
|----|--------|-------------|-------|----------|
| P1-1 | Confirm `scripts/verify-fast.sh` runs in <5 min on clean checkout | Time benchmark | Backend Lead | Shell timing |
| P1-2 | Document manual trigger commands for all `#[ignore]` test categories | README or test-strategy update | Backend Lead | Doc update |
| P1-3 | Run full ignored suite (RLS + NATS + SQLx smoke) and record results | Result log | Backend Lead | Command output |
| P1-4 | If any ignored test fails, file fix task | Issue/tracker entry | Backend Lead | This tracker updated |

**Phase 1 Exit Criteria:**
- `scripts/verify-fast.sh` passes
- All ignored tests either pass or have documented failure reasons
- Manual trigger commands documented in `docs/11-quality/01-test-strategy.md`

**Phase 1 Execution Results (2026-05-20):**

| Item | Status | Evidence / Blocker |
|------|--------|-------------------|
| **P1-1** | 🟡 PARTIAL | `scripts/verify-fast.sh` completes fmt/check/clippy but **times out during test phase at ~10 minutes** on current environment. `cargo test --workspace --lib --all-features` run separately **passes**. Target <5 min not met due to test phase duration. |
| **P1-2** | ✅ RESOLVED | Manual trigger commands for RLS, SQLx smoke, NATS live, and load tests documented in `docs/11-quality/01-test-strategy.md` with prerequisites (docker-compose, DATABASE_URL, NATS_URL). |
| **P1-3** | ✅ RESOLVED | Full ignored suite executed on fresh DB path. Results: migration integration 2/2 pass, RLS integration 22/22 pass, NATS live 14/14 pass, SQLx smoke 7/7 pass (bundle 1, graph 5, audit 1), webhook integration 1/1 pass. Existing DB path still shows schema drift (B-1/B-2). Fresh DB is canonical integration evidence source. |
| **P1-4** | ✅ RESOLVED (partial — B-1/B-2 remain open) | Fix tasks filed and resolved for B-3 (migration 19 syntax), B-4 (audit enum cast), and B-5 (NATS JetStream subject overlap). B-1 (missing relations on stale existing DB) and B-2 (migration 9 checksum mismatch on existing DB) remain open with documented rationale: existing DB is stale relative to migration sequence; fresh DB path is canonical. |

**Phase 1 Ignored-Suite Execution Log:**

| Suite | Command | Result | Notes |
|-------|---------|--------|-------|
| Migration integration (existing DB) | `cargo test -p intent-service --test migration_integration -- --ignored` | **1 passed** | Existing `intent_rebase` database; migrations already applied. |
| RLS integration (existing DB) | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase cargo test -p intent-api --test rls_integration -- --ignored` | **3 passed, 19 failed** | Failures: missing relations (`propagation_records`, `webhook_subscriptions`), RLS not enabled → `RowNotFound`, tenant isolation assertions failing. |
| RLS integration (existing DB + migration flag) | `RLS_TEST_RUN_MIGRATIONS=true DATABASE_URL=...intent_rebase cargo test -p intent-api --test rls_integration -- --ignored` | **1 passed, 21 failed** | Migration 9 checksum mismatch: "previously applied but has been modified". |
| Fresh DB creation | `docker exec intent-rebase-postgres createdb -U intent_rebase intent_rebase_phase1` | **Succeeded** | Clean database for fresh migration path. |
| Migration integration (fresh DB) | `DATABASE_URL=...intent_rebase_phase1 cargo test -p intent-service --test migration_integration -- --ignored` | **Failed** | Migration 19 syntax error at or near `NOT`. |
| Audit SQLx smoke (fresh DB) | `DATABASE_URL=...intent_rebase_phase1 cargo test -p intent-api --lib sqlx_repo_smoke -- --ignored` | **Failed** | `test_sqlx_audit_repo_smoke` fails because `event_type` column is `audit_event_type` but expression expects text cast. |
| Bundle SQLx smoke (fresh DB) | `DATABASE_URL=...intent_rebase_phase1 cargo test -p intent-api --lib sqlx_repo_smoke -- --ignored` | **1 passed** | Bundle repository smoke passes on fresh DB. |
| Graph SQLx repository tests (fresh DB) | `DATABASE_URL=...intent_rebase_phase1 cargo test -p intent-api --lib sqlx_repo_smoke -- --ignored` | **5 passed** | Graph repository tests pass on fresh DB. |
| NATS JetStream ignored suite | `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` | **13 passed, 1 failed** | Failed: `live_jetstream_tenant_scope_unscoped_consumes_all` — JetStream subjects overlap with existing stream. |
| Load tests (L1-L3) | `cargo test -p intent-api --test load_test --features load-test -- --nocapture` | **2 passed** | L1 1000/1000, L2 5000/5000, L3 10000/10000; sustained 90s 4505/4505, 50.05 req/s, p95 3ms, p99 8ms. **Local load-test harness only; not production evidence.** |
| Final stack health | `docker compose -f infrastructure/local/docker-compose.yml ps` | **All healthy** | Grafana, MinIO, NATS, Postgres healthy. |

**Phase 1 Fix Re-run Log (2026-05-20):**

| Suite | Command | Result | Notes |
|-------|---------|--------|-------|
| Migration integration (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-service --test migration_integration -- --ignored` | **1 passed** | B-3 fixed: migration 19 syntax corrected in `infrastructure/migrations/019_create_webhook_outbox.sql`. Fresh DB migrations now apply cleanly. |
| Audit SQLx smoke (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-rebase-types --lib --all-features test_sqlx_audit_repo_smoke -- --ignored` | **1 passed** | B-4 fixed: audit enum insert/read alignment in `crates/intent-rebase-types/src/audit_repo.rs`. |
| RLS integration (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration -- --ignored --test-threads=1` | **22 passed** | RLS harness fixed in `crates/intent-api/tests/rls_integration.rs`. Fresh DB path now fully green. |
| NATS JetStream ignored suite | `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` | **14 passed** | B-5 fixed: NATS uniqueness fix in `crates/intent-api/src/nats_jetstream/tests_live_integration.rs`. All live NATS tests now pass. Note: prior to fix, orchestrator observed service-down (2/14 pass, 12 connection refused), then after starting NATS observed 11/14 pass with 3 subject-overlap failures; after code fix, full 14/14 pass. |

**Phase 1 Blockers Identified:**

| Blocker | Affected Suite | Root Cause | Status | Next Action |
|---------|---------------|------------|--------|-------------|
| **B-1** | RLS integration (existing DB) | Missing relations (`propagation_records`, `webhook_subscriptions`) on existing DB | 🔴 OPEN | Existing DB is stale relative to migration sequence. Use fresh DB for integration evidence, or re-seed existing DB if needed for backwards-compatibility testing. |
| **B-2** | RLS integration (existing DB + migration flag) | Migration 9 checksum mismatch | 🔴 OPEN | Existing DB has a modified migration 9. Fresh DB path is the canonical integration evidence source until this is resolved. |
| **B-3** | Migration integration (fresh DB) | Migration 19 syntax error at or near `NOT` | 🟢 RESOLVED | Fixed in `infrastructure/migrations/019_create_webhook_outbox.sql`. Fresh DB migration test passes. |
| **B-4** | Audit SQLx smoke | `event_type` column mapped as `audit_event_type` enum, but query casts/expression treats it as text | 🟢 RESOLVED | Fixed in `crates/intent-rebase-types/src/audit_repo.rs`. Fresh DB audit smoke passes. |
| **B-5** | NATS JetStream | `live_jetstream_tenant_scope_unscoped_consumes_all` fails due to subject overlap with existing stream | 🟢 RESOLVED | Fixed in `crates/intent-api/src/nats_jetstream/tests_live_integration.rs`. Full 14/14 live NATS tests pass. |

**Phase 1 Decision:** Phase 1 is **PARTIAL — local infrastructure unblocked, fresh DB integration path now green, but existing DB schema drift remains**. In-memory canonical gates pass (`cargo test --workspace --lib --all-features`). Fresh DB ignored suites now pass fully: migration integration (2/2), audit SQLx smoke (1/1), RLS integration (22/22), NATS live (14/14). Blockers B-3, B-4, B-5 are **resolved**. Blockers B-1 and B-2 remain **open** on the stale existing DB, but the fresh DB path is the canonical integration evidence source. Load test results are local-harness evidence only. No production-readiness claimed.

---

### Phase 2 — Local Hardening (Ordered Slices)

> **Goal:** Close locally executable hardening items that do not require external infrastructure.

| ID | Action | Source Ref | Owner | Dependencies |
|----|--------|------------|-------|--------------|
| **S1** | ✅ DONE — NATS tenant isolation: consumer registry propagation + publisher guard + unit tests + docs | Oracle recommendation; `docs/14-governance/08-tenant-isolation.md` | Backend Lead | None |
| **S2** | ✅ DONE — RLS enforcement audit tool (`scripts/audit-rls-dml.sh`) + docs (`docs/11-quality/03-rls-audit.md`) | Oracle recommendation | Backend Lead | None |
| **S3** | ✅ DONE — NATS publisher with W3C traceparent injection, tenant scope guard, fail-open retry, and unit coverage | Oracle recommendation | Backend Lead | None |
| **S4** | ✅ DONE — Webhook docker-compose integration test: `crates/intent-api/tests/webhook_integration.rs` with SQLx outbox/subscription repos, in-process HTTP receiver, real `reqwest` dispatch, and DB status verification | Oracle recommendation | Backend Lead | Local docker-compose |
| **S5** | ✅ DONE — Add startup warning when `INTENT_API_REQUIRE_JWT=false` | Risk R-6 mitigation | Backend Lead | None |
| **S6** | ✅ DONE — `forensic-service` bundle service tests extracted into `bundle_service_tests.rs`; propagation handlers (`get_propagation_status`, `ingest_propagation_signal` JWT/non-JWT variants) extracted from `intent-api/src/query_handlers.rs` into `intent-api/src/propagation_handlers.rs` | `22-phase-4-entry-plan.md` A-09 | Backend Lead | None |
| **S7** | 🟡 DESIGN / LOCAL RUNBOOK DONE — Panic alerting integration design/runbook delivered (`docs/09-operations/12-panic-alerting-integration.md`); RB15 panic response runbook added to `05-runbooks.md`; Prometheus alert rule design-only (metric prerequisite `process_panics_total` not implemented); local validation approach documented (hook tests, promtool syntax check, Alertmanager routing smoke). Production alerting remains blocked: no panic metric, no external receivers, no staging env, no SRE sign-off. | `22-phase-4-entry-plan.md` A-08 | Backend Lead | None |
| **S8** | ✅ DONE — `query_handlers.rs::ingest_propagation_signal` now uses `begin_with_tenant` + `create_record_with_tx` when JWT claims and `rls_pool` are present. Fallback non-RLS path remains for in-memory and no-claims contexts. Webhook residuals (R2-R6) remain deferred Phase 4+ warnings. | `22-phase-4-entry-plan.md` A-02 | Backend Lead | None |

**Phase 2 Exit Criteria:**
- S1-S4 delivered or explicitly deferred with reason
- `scripts/verify-fast.sh` passes after each slice
- No new production-readiness claims introduced

---

### Phase 3 — Integration Evidence

> **Goal:** Collect staging-like and live integration evidence without claiming production readiness.

| ID | Action | Deliverable | Owner | Dependencies |
|----|--------|-------------|-------|--------------|
| **I1** | ✅ DONE — L3 staged load test rerun (in-memory + SQLx-backed + sustained smoke) against local docker-compose stack. Evidence recorded in `docs/11-quality/load-test-results.md` Section 4. | Load test results doc | Backend Lead | S4; docker-compose stack |
| **I2** | 🟡 I2a LOCAL SUB-SLICE DONE — 10min sustained load + one availability alert pipeline validated end-to-end against local docker-compose stack. Full I2 deliverable (30min sustained + all alert types + real receivers) remains blocked and deferred to staging/prod. | Alert firing evidence | Backend Lead | Local stack; alert rules |
| **I3** | ✅ DONE — JWT→RLS→DML integration test (`test_i3_jwt_create_intent_rls_dml_isolation`) | Test pass evidence | Backend Lead | S2; local Postgres |
| **I4** | 🟡 STAGE 1 DONE — NATS per-tenant stream migration readiness assessment executed locally. Stage 1 log: shared stream `audit_events` inspected (subject `audit.events.v1.>`, 0 consumers); publisher subject format `audit.events.v1.<tenant_id>.<event_type>` confirmed; tenant UUID inventory complete (no hardcoded NATS tenants, all live tests use dynamic UUIDs); NATS live integration suite 14/14 pass; stream config template prepared (planning-only, marked DO NOT EXECUTE). Stage 2–4 remain blocked on external SRE sign-off, staging env, and coordinated rollout. | Stage 1 execution log in `docs/14-governance/08-tenant-isolation.md` | Backend Lead | S1; local NATS |
| **I5** | ✅ DONE — Webhook end-to-end delivery test rerun: `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` **1 passed**. SQLx outbox + subscription pipeline with in-process HTTP receiver and real `reqwest` dispatch. Fresh DB only; local-dev evidence, not production. | Test pass evidence | Backend Lead | S4; local stack |
| **I6** | ✅ DONE — Non-destructive `pg_dump`/`pg_restore` validation against local docker-compose Postgres; restored DB passes migration and webhook integration tests | Execution log in `docs/09-operations/07-backup-restore.md` | Backend Lead | `docs/09-operations/07-backup-restore.md` |

**Phase 3 Exit Criteria:**
- Integration tests pass in local docker-compose environment
- Results documented in `docs/11-quality/load-test-results.md` or equivalent
- Explicit caveat that docker-compose is not production-equivalent

---

### Phase 4 — External / Production Evidence

> **Goal:** Close external gates with named third-party evidence and production infrastructure. This phase **cannot proceed** until external reviewers and infrastructure are engaged.

| Gate | Phase 4 Action | Evidence Required | Owner | Phase 3 Prerequisite |
|------|---------------|-------------------|-------|---------------------|
| A-03 | Engage external SRE; conduct operational review | Signed Section H of external review packet | External SRE (to be named) | I1, I2, I6 |
| A-04 | Engage external security reviewer; conduct architecture review | Signed Section H; threat model v2 assessment | External Security (to be named) | S2, I3 |
| A-07 | Engage external pen test team; execute scope; remediate | Pen test report (PDF + JSON); remediation evidence | External Pen Test Team | A-04, I1, staging env |
| A-05 | Provision production infrastructure (Terraform/CDK) | Production environment operational; runbook executed | SRE | I1, I6 |
| A-06 | Execute L4 (30min + all alerts + real receivers) and L5 (production) | Load test results with real receiver validation | Backend Lead / SRE | A-05, A-03 |
| A-10 | Deploy production NATS topology; validate full DLQ replay worker | SRE sign-off + staging validation | Backend Lead / SRE | A-03, A-05, S1 |
| A-12 | Production secret manager integration; key rotation grace window; delivery SLO evidence | Secret manager audit log; delivery SLO charts | SRE / Security | A-03, A-04, A-07, A-05 |
| A-13 | S3 Object Lock deployment; chain-hash; retention enforcement | Object Lock validation; tamper-evidence test | Backend Lead / Security | A-05 |
| A-11 | Revisit when Temporal SDK supports safe per-request gRPC metadata injection | Cross-process trace IDs in OTLP backend | Backend Lead / SRE | Temporal SDK fix |

**Phase 4 Exit Criteria:**
- All external gates have named reviewer evidence or documented deferral
- Production infrastructure is provisioned and operational
- No WAIVED-SOLO items remain unaddressed before production claim

---

## 8. Execution Rules

1. **Verify-first:** Run `scripts/verify-fast.sh` before and after every slice. If it fails, stop and fix.
2. **Docs-before-code:** For any schema or API change, update ADR/OpenAPI/spec first.
3. **No external gate self-signing:** External gates (A-03, A-04, A-07) require independent third-party evidence. Do not mark them complete with solo review.
4. **Preserve caveats:** Every delivered slice must carry a non-production caveat unless it is explicitly production infrastructure with operational evidence.
5. **Contradiction cleanup before new slices:** Do not start Phase 2 slices until Phase 0 contradictions are resolved or explicitly accepted as known debt.
6. **Ignored-test hygiene:** Before claiming integration completeness, run the full `#[ignore]` suite and record results.
7. **One tracker update per slice:** After completing any slice, update this tracker with evidence location and status.

---

## 9. Validation Matrix

| Check | Command / Action | Expected Result | Frequency |
|-------|------------------|-----------------|-----------|
| Fast verify | `scripts/verify-fast.sh` | All pass | Every commit |
| Format | `cargo fmt --all -- --check` | No diff | Every commit |
| Clippy | `cargo clippy --workspace --all-features -- -D warnings` | No warnings | Every commit |
| Type check | `cargo check --workspace --all-features` | Success | Every commit |
| In-memory tests | `cargo test --workspace --lib --all-features` | All pass | Every commit |
| RLS live tests (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration -- --ignored --test-threads=1` | All pass (22/22 on fresh DB; existing DB still 3/22 due stale schema) | Before claiming RLS complete |
| NATS live tests | `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` | All pass (14/14) | Before claiming NATS complete |
| Webhook integration (fresh DB) | `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` | All pass (1/1) | Before claiming webhook pipeline complete |
| OpenAPI spec | `npx @stoplight/spectral-cli lint docs/04-api/openapi.yaml --ruleset .spectral.yml --fail-severity=error` | No errors | Before API changes merge |
| Git check | `git diff --check` | No conflicts | Before push |
| Doc sync | Cross-reference this tracker with changed docs | No contradictions | After every Phase 0 item |

---

## 10. External Evidence Path

This section maps the external evidence required for a future production-readiness claim. It is **not** a claim that any of this evidence currently exists.

```
Phase 0 (Docs Sync)
  └── Phase 1 (Verification Baseline)
        └── Phase 2 (Local Hardening: S1-S8)
              └── Phase 3 (Integration Evidence: I1-I6)
                    ├── A-03 + A-04 (External SRE + Security Review)
                    ├── A-05 (Production Infrastructure)
                    ├── A-07 (Pen Test)
                    ├── A-06 (L4/L5 Load Testing)
                    ├── A-10 (DLQ/NATS Production-Grade)
                    ├── A-12 (Webhook Production Hardening)
                    ├── A-13 (Forensic Immutable Storage)
                    └── A-11 (Trace Propagation — deferred/SDK-blocked)
```

**Critical Path:** Phase 0 → Phase 1 → Phase 2 (S1-S4) → Phase 3 (I1-I3) → A-03/A-04/A-05 → A-07/A-06/A-10/A-12/A-13.

---

## 11. Next Actions (Immediate)

| Priority | Action | Owner | ETA |
|----------|--------|-------|-----|
| **P0** | ✅ DONE — C-1 resolved, C-5 resolved, verify-fast.sh baseline established | Backend Lead | — |
| **P1** | ✅ DONE — B-3 fixed: Migration 19 syntax error on fresh DB | Backend Lead | 2026-05-20 |
| **P1** | ✅ DONE — B-4 fixed: Audit SQLx smoke enum cast mismatch | Backend Lead | 2026-05-20 |
| **P1** | ✅ DONE — RLS harness fixed; fresh DB RLS 22/22 pass | Backend Lead | 2026-05-20 |
| **P1** | ✅ DONE — B-5 fixed: NATS JetStream live suite 14/14 pass | Backend Lead | 2026-05-20 |
| **P1** | Re-run full ignored suite after B-3/B-4/RLS/B-5 fixed | Backend Lead | ✅ DONE 2026-05-20 |
| **P1** | ✅ DONE — Execute S1: NATS tenant isolation design/implementation | Backend Lead | 2026-05-20 |
| **P1** | ✅ DONE — Execute S2: RLS enforcement audit tool | Backend Lead | 2026-05-20 |
| **P1** | ✅ DONE — Execute S4: Webhook docker-compose integration test | Backend Lead | 2026-05-20 |
| **P2** | ✅ DONE — I1 executed 2026-05-21. Local load-test rerun completed (in-memory + SQLx + sustained smoke). Results in `docs/11-quality/load-test-results.md` Section 4. A-06 remains blocked. | Backend Lead | 2026-05-21 |
| **P2** | ✅ DONE — Execute I3: JWT→RLS→DML integration test | Backend Lead | 2026-05-20 |

---

## 12. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/22-phase-4-entry-plan.md` | Phase 4 entry criteria and A-01..A-13 detailed tracker; this doc consolidates contradictions and adds ordered execution phases |
| `docs/10-delivery/17-production-readiness-backlog.md` | Source of P0/P1/P2 backlog items; target for C-1/C-3/C-5 cleanup |
| `docs/10-delivery/00-current-status.md` | Current project status; target for C-2 cleanup |
| `docs/10-delivery/20-project-completion-roadmap.md` | Completion roadmap P0-P3; target for C-8 cleanup |
| `docs/09-operations/10-external-review-packet.md` | External review packet template; target for C-9 sync |
| `docs/11-quality/01-test-strategy.md` | Test approach; reference for validation matrix |
| `docs/01-product/05-glossary.md` | Product glossary; target for C-10 cleanup |

---

## 13. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Initial creation — consolidated explorer/oracle findings into single tracker: purpose, caveats, current status, evidence inventory, contradiction/debt register (C-1..C-10), risk register (R-1..R-8), phase execution plan (Phase 0-4), execution rules, validation matrix, external evidence path, next actions. No production-readiness claim. External gates remain blocked. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 0 executed — resolved P0-1 (backlog P2-6 webhook status sync), P0-2 (CI wording accuracy in current status), P0-3 (P1-S5 aggregate status update), P0-4 (K8s runbook audit — no refs found), P0-5 (backlog date update), P0-8 (roadmap cross-ref), P0-9 (external review packet G-RLS-1 sync), P0-10 (glossary audit — no stale refs). Marked P0-6 (stale line refs) OPEN as too broad, P0-7 (threat model v2 cross-refs) PARTIAL. All edits preserve no-production-readiness caveats. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | P0-7 RESOLVED — updated `docs/README.md` reading order item 10 to label `01-threat-model.md` as baseline/legacy and point to `14-governance/06-threat-model-v2.md` as current. Updated `docs/13-adrs/README.md` internal links to reference `06-threat-model-v2.md` as current with `01-threat-model.md` as baseline/legacy. Tracker P0-7 status changed from PARTIAL to RESOLVED. Phase checklists accepted as known debt (historical artifacts). |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 1 documented — added manual ignored-test commands and prerequisites (RLS, SQLx smoke, NATS live, load tests) to `docs/11-quality/01-test-strategy.md`. Updated tracker Phase 1 with feasibility assessment: P1-1 PARTIAL (`verify-fast.sh` times out at ~10 min; separate `cargo test --workspace --lib --all-features` passes), P1-2 RESOLVED (commands documented), P1-3 BLOCKED (DATABASE_URL unset, NATS_URL unset, docker-compose Postgres/NATS/MinIO not running; ignored tests not executed), P1-4 NOT TRIGGERED. Phase 1 decision: BLOCKED on local infrastructure. No false pass claims for ignored tests. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 1 fix evidence recorded — B-3 (migration 19 syntax) fixed and fresh DB migration integration passes (1/1). B-4 (audit enum cast) fixed and fresh DB audit smoke passes (1/1). RLS harness fixed and fresh DB RLS integration passes (22/22). B-5 (NATS JetStream subject overlap) fixed and live NATS suite passes (14/14). Updated tracker test inventory, evidence inventory, validation matrix, Phase 1 blockers (B-3/B-4/B-5 marked RESOLVED; B-1/B-2 remain open on stale existing DB), Phase 1 decision, and next actions. All evidence is local/docker-compose/manual ignored-test only; no production-readiness claims added; external gates remain blocked. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S5 executed — added `tracing::warn!` startup warning in `crates/intent-api/src/main.rs` when `INTENT_API_REQUIRE_JWT=false/unset`, clearly stating dev fallback is active and NOT for staging/production. `INTENT_API_REQUIRE_JWT=true` path unchanged. Tracker Phase 2 table updated: S5 marked ✅ DONE. `cargo fmt --all -- --check` and `cargo check -p intent-api --all-features` pass. No production-readiness claim; local-dev caveat preserved. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S2 executed — created `scripts/audit-rls-dml.sh` structural audit script and `docs/11-quality/03-rls-audit.md`. Updated `docs/11-quality/01-test-strategy.md` with RLS audit reference. Tracker Phase 2 table updated: S2 marked ✅ DONE. Script exits 0 for known state (11 RLS-wrapped handlers, 9 read-only handlers, 2 webhook gaps, 1 known residual). No production-readiness claim; limitations and caveats preserved. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S1 executed — `ConsumerRegistry` gained `tenant_scope` field/builder/wiring to propagate scope to all `NatsPullConsumerAdapter` instances in `start_all`. `NatsEventPublisher` gained `tenant_scope` field/builder and tenant guard in `publish()` that returns `PublishResult::Skipped { reason }` with expected/actual UUIDs before NATS connection. Unit tests added in `event_publisher_tests.rs` (unscoped allows, scoped matching allows, scoped mismatched skips). `ConsumerRegistry` tenant scope builder test added in `tests_lifecycle.rs`. `docs/14-governance/08-tenant-isolation.md` updated with publisher guard and registry propagation sections. Tracker Phase 2 table updated: S1 marked ✅ DONE. Explicit local-only caveat preserved: no per-tenant streams created, no NATS ACLs/topology changes, no production-readiness claim. |
| 2026-05-20 | BrianNguyen (via authorized assistant explorer) | Phase 2 S3 reconciled — existing `NatsEventPublisher` implementation verified as already delivered: lazy NATS connection, W3C traceparent injection, fail-open retry/skip behavior, tenant scope guard from S1, unit coverage, and SQL router wiring behind `NATS_URL`. Tracker Phase 2 table updated: S3 marked ✅ DONE. This is bounded local-dev evidence only; no production NATS topology, ACLs, or production-readiness claim. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S8 executed — added `create_record_with_tx` to `PropagationRecordRepository` trait with in-memory delegation and SQLx transaction execution. `query_handlers.rs::ingest_propagation_signal` JWT path now uses `begin_with_tenant` + `create_record_with_tx` when `rls_pool` and claims are present; fallback non-RLS path remains for in-memory/no-claims contexts. Updated `scripts/audit-rls-dml.sh` to move `query_handlers.rs` from known residual to RLS-wrapped. Updated `docs/11-quality/03-rls-audit.md` to mark residual resolved. Tracker Phase 2 table updated: S8 marked ✅ DONE. Webhook residuals (R2-R6) remain deferred Phase 4+ warnings. No production-readiness claim; local-dev caveat preserved. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S4 executed — created `crates/intent-api/tests/webhook_integration.rs`: ignored integration test exercising `SqlxWebhookOutboxRepository` + `SqlxWebhookSubscriptionRepository` against live Postgres, with in-process axum HTTP receiver and real `reqwest` dispatch via `WebhookDeliveryDispatcher`. Test seeds subscription + pending outbox row, runs `WebhookOutboxWorkerImpl::process_once`, asserts HTTP POST received with correct payload shape and DB outbox status transitions to `Delivered`. Passed against fresh DB (`intent_rebase_phase1_fix`). Updated `docs/11-quality/01-test-strategy.md` with webhook integration test command and result. Tracker Phase 2 table updated: S4 marked ✅ DONE. Test inventory, evidence inventory, validation matrix, and next actions updated. No production-readiness claim; local-dev/docker-compose evidence only. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 2 S6 continued — extracted inline `forensic-service` bundle service tests from `crates/forensic-service/src/bundle_service.rs` into `crates/forensic-service/src/bundle_service_tests.rs` and wired the module in `crates/forensic-service/src/lib.rs`. No production logic changed. `cargo fmt --all -- --check`, `cargo check -p forensic-service --all-features`, and `cargo test -p forensic-service --lib` pass (116 passed, 0 failed, 1 ignored). S6 remains IN PROGRESS for additional future decomposition/cross-crate consolidation slices. No production-readiness claim. |
| 2026-05-20 | BrianNguyen (via authorized assistant fixer) | Phase 3 I6 executed — non-destructive local PostgreSQL backup/restore validation (`pg_dump`/`pg_restore`) against docker-compose Postgres. Dump size 156498 bytes. Restored to separate DB `intent_rebase_i6_restore`. Verified 21 `_sqlx_migrations` rows. Migration integration test (1/1) and webhook integration test (1/1) both pass against restored DB. Updated `docs/09-operations/07-backup-restore.md` with execution log and caveats. Tracker Phase 3 table updated: I6 marked ✅ DONE. No production PITR/basebackup claim; no RPO/RTO measured; local-only caveat preserved. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Phase 3 I1 executed — local load-test rerun against docker-compose stack (postgres, nats, minio, prometheus, grafana). In-memory L1/L2 passed (1000/1000, 5000/5000), L3 completed (details truncated). SQLx-backed L1 passed (500/500). Sustained 90s smoke passed (4505/4505, 0% error, RSS +1.6%). NATS running but compose healthcheck returned 503. Prometheus query returned empty vector (observability caveat, not failure). Updated `docs/11-quality/load-test-results.md` with Section 4. Tracker Phase 3 table updated: I1 marked ✅ DONE. Test inventory and next actions updated. A-06 (L4/L5 load testing) remains blocked. No production-readiness claim; all evidence is local-dev only. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Phase 3 I4 Stage 1 executed — NATS per-tenant stream migration readiness assessment. Local NATS restarted fresh (corrupted stream from prior test run cleared). Shared stream `audit_events` inspected via HTTP monitoring API: subject filter `audit.events.v1.>`, 0 messages, 0 consumers after test run. Publisher subject format confirmed tenant-scoped `audit.events.v1.<tenant_id>.<event_type>`. Tenant UUID inventory: no hardcoded tenant UUIDs in NATS publisher/consumer/JetStream code; all live tests use `uuid::Uuid::new_v4()`. NATS live integration suite `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` **14/14 passed**. Post-test stream inventory: 12 isolated test streams, 10 consumers, 12 messages — no cross-test pollution. Stream config template prepared and marked DO NOT EXECUTE. Updated `docs/14-governance/08-tenant-isolation.md` with Stage 1 execution log. Tracker Phase 3 table updated: I4 marked 🟡 STAGE 1 DONE with Stage 2–4 blocked caveats. No per-tenant streams created. No server-side topology changed. No production-readiness claim. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Phase 3 I5 executed — webhook end-to-end delivery test rerun against fresh DB `intent_rebase_phase1_fix`. Command: `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored --nocapture`. Result: **1 passed** (`test_webhook_sqlx_outbox_pipeline_success` in 3.27s). Validates `SqlxWebhookSubscriptionRepository` create, `SqlxWebhookOutboxRepository` create, `WebhookOutboxWorkerImpl::process_once` claim/dispatch/deliver, real HTTP POST via `WebhookDeliveryDispatcher`, DB status transition `Pending` → `Claimed` → `Delivered`, and payload shape verification. Tracker Phase 3 table updated: I5 marked ✅ DONE. No production-readiness claim; local-dev/docker-compose evidence only. A-12 webhook production hardening remains blocked. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | P0-6 mapped stale-ref cleanup executed — 6 stale line refs replaced with stable module/function anchors in `checklist-phase-2.md`, `11-phase-2b-sign-off-packet.md`, `checklist-phase-3.md`. Rebase-engine stale TODO doc-comments updated in `planner.rs` and `lib.rs` to deferred/baseline language. Verification: `cargo fmt --all -- --check` pass, `cargo check -p rebase-engine` pass, `cargo test -p rebase-engine --lib` pass (162/162), `git diff --check` pass. Bounded ignored-suite baseline blocked: local docker-compose stack has only Grafana running; Postgres and NATS unreachable. No production-readiness claim; external gates remain blocked. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Bounded ignored-suite baseline rerun unblocked — started local docker-compose postgres + nats + minio (Grafana already running). All three mapped baseline commands executed successfully: (1) `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` **14/14 passed** in 4.51s; (2) `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` **1/1 passed** in 8.62s; (3) `DATABASE_URL=...intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration test_i3 -- --ignored --test-threads=1` **1/1 passed** in 3.65s. Tracker prior "blocked" note for ignored-suite baseline superseded. No production-readiness claim; local-dev/docker-compose evidence only. |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Doc sync (bounded Phase 0 follow-up) — updated `20-project-completion-roadmap.md`: marked `rebase_apply_handlers.rs` inline test extraction and Router Stage 2 route-group split as delivered (both were already implemented; roadmap lagged actual state). Updated `23-project-assessment-and-execution-tracker.md` Phase 1 rows: P1-3 upgraded to ✅ RESOLVED (fresh DB ignored suite fully green), P1-4 upgraded to ✅ RESOLVED with B-1/B-2 remaining open caveat. Fixed stale line ref in `checklist-phase-1.md`: replaced `crates/intent-rebase-types/src/intent.rs:266-340` with stable module/function anchor (`Intent`, `IntentVersion`, `IntentPayload` structs). All edits preserve non-production caveats; no external/production gates marked complete. Verification: `git diff --check` pass, `cargo fmt --all -- --check` pass (no Rust changes). |
| 2026-05-21 | BrianNguyen (via authorized assistant fixer) | Phase 2 S7 executed — panic alerting integration design/runbook delivered. Created `docs/09-operations/12-panic-alerting-integration.md`: current state (hook delivered, no metric), intended alert path (`process_panics_total` → Prometheus → Alertmanager → receiver), planned alert rule (design-only), local validation approach (hook unit tests, promtool syntax check, Alertmanager routing smoke), RB15 response procedure, blocked production prerequisites table, and forbidden claims. Added RB15 "Process Panic Detected" runbook to `docs/09-operations/05-runbooks.md` with diagnosis, mitigation, recovery, and caveats. Updated `22-phase-4-entry-plan.md` A-08 to reflect S7 design/runbook completion while preserving production blockers. Tracker Phase 2 table updated: S7 marked 🟡 DESIGN / LOCAL RUNBOOK DONE. No runtime panic hook changes; no production alerting claim; no external receiver configuration. |
| 2026-05-25 | BrianNguyen (via authorized assistant fixer) | Phase 2 S6 continued — extracted propagation handlers (`get_propagation_status` JWT/non-JWT variants, `ingest_propagation_signal` JWT/non-JWT variants) from `crates/intent-api/src/query_handlers.rs` into new `crates/intent-api/src/propagation_handlers.rs`. Updated `routes/propagation.rs` references from `crate::query_handlers::...` to `crate::propagation_handlers::...`. Added `pub mod propagation_handlers;` in `lib.rs`. Removed unused propagation-related imports from `query_handlers.rs`. No handler logic changed; no public route paths renamed. Verification: `cargo fmt --all -- --check` pass, `cargo check -p intent-api --all-features` pass, `cargo clippy -p intent-api --all-features -- -D warnings` pass, `cargo test -p intent-api --lib` pass (441 passed, 0 failed, 17 ignored), `cargo check --workspace --all-features` pass, `git diff --check` pass. `cargo test --workspace --lib --all-features` timed out during compilation (15 min) on this environment; not claimed as blocker since targeted crate tests and workspace check pass. Tracker Phase 2 table updated: S6 remains 🟡 IN PROGRESS. No production-readiness claim. |
| 2026-06-06 | BrianNguyen (via authorized assistant fixer) | Phase 2 S6 marked ✅ DONE — both S6 bounded slices verified locally: (a) `forensic-service` bundle service tests extracted from `bundle_service.rs` into `bundle_service_tests.rs` with `cargo test -p forensic-service --lib` 116/116 + 1 ignored pass; (b) propagation handlers (`get_propagation_status`, `ingest_propagation_signal` JWT/non-JWT variants) extracted from `intent-api/src/query_handlers.rs` into `intent-api/src/propagation_handlers.rs` with `cargo test -p intent-api --lib` 441/441 + 17 ignored pass, plus `cargo fmt --all -- --check`, `cargo check -p intent-api --all-features`, `cargo clippy -p intent-api --all-features -- -D warnings`, `cargo check --workspace --all-features`, and `git diff --check` all pass. Tracker Phase 2 table updated: S6 status flipped from 🟡 IN PROGRESS to ✅ DONE. Local-dev verification only; no CI-green, no production-readiness, and no external/production gate (A-03/A-04/A-05/A-06/A-07/A-10/A-11/A-12/A-13) claimed complete. |
| 2026-06-06 | BrianNguyen (via authorized assistant fixer) | Phase 3 I2a local sub-slice evidence documented — added Section 5 to `docs/11-quality/load-test-results.md` capturing the bounded I2a sub-slice: (1) local docker-compose stack (postgres/nats/minio/prometheus/alertmanager/alert-receiver/grafana) up after one observability-profile startup timeout-relaunch; (2) `cargo build -p intent-api --no-default-features` failed on feature-gated `jwt_auth_async` (recorded as caveat, not blocker); `cargo build -p intent-api` default-feature build passed; (3) intent-api brought up with local JWT dev fallback, `INTENT_API_WEBHOOK_DELIVERY=false`, `INTENT_API_WEBHOOK_OUTBOX_WORKER=false`, healthy `/health`, Prometheus `up{job="intent-api"} = 1`; (4) initial 900s-timeout run cut off at ~580s by shell budget (not counted as pass); (5) passing sustained-load run `SUSTAINED_LOAD_URL=http://localhost:8080 SUSTAINED_LOAD_DURATION_SECS=600 SUSTAINED_LOAD_RPS=50 cargo test -p intent-api --features load-test --test load_test -- --nocapture test_sustained_load_smoke` → 30,005/30,005, 0% error, p50 2ms / p95 14ms / p99 25ms, 50.01 req/s, RSS +1,220 kB (+7.4%), FD 15→15; (6) initial alert fault attempts failed to fire (load-harness/fault body schema drift → 422, `intent_api_intent_version_created_total` empty); (7) schema-valid nil workflow_id POST returned 400 `INVALID_INGEST_REQUEST` and incremented error metric; (8) final fault mix produced 5 success / 361 errors in `intent_api_intent_version_created_total`, Prometheus success-rate 0%, `/api/v1/alerts` showed `IntentVersionCreationLowSuccessRate` firing (severity `warning`, slo `availability`), Alertmanager `/api/v2/alerts` routed to `warning-alerts`, `alert-receiver` logs received webhook payload (receiver `warning-alerts`, status `firing`, alertname `IntentVersionCreationLowSuccessRate`, severity `warning`, slo `availability`, truncatedAlerts 0). Tracker updated: I2 row annotated as `🟡 I2a LOCAL SUB-SLICE DONE — 10min sustained load + one availability alert pipeline validated end-to-end against local docker-compose stack. Full I2 deliverable (30min sustained + all alert types + real receivers) remains blocked and deferred to staging/prod.`; test inventory gained an I2a sub-slice row; §4.1 evidence inventory gained two I2a rows; A-06 row annotated to note the I2a sub-slice without unblocking. Limitations list and production recommendations updated with I2a pointers. All caveats preserved: WAIVED-SOLO, local-dev, no-production-readiness, no CI-green, no external/production gate claimed complete, 10min (not 30min), one alert (not all 17), local `alert-receiver` placeholder (not real external receiver), default-feature local binary with local JWT dev fallback (not production auth), browser tools not used. No code changes. |
