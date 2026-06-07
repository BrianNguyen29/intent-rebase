# Webhook SQL Repository Wiring Evidence (Internal)

> **Status:** INTERNAL EVIDENCE — bounded local wiring proof, not a production claim
> **Date:** 2026-06-07
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** P1 Webhook SQL Repository Wiring — first bounded slice of the strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §6).
> **Caveat:** This document records a **local, bounded** wiring change. It is **not** a production-readiness claim, **not** a CI-green / external sign-off, and **not** a claim that webhook production hardening (secret manager, retry/DLQ semantics, tenant-scoped pattern matching, horizontal scaling, leases) is complete. All external/production gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. The webhook outbox background worker remains **default-off** behind `INTENT_API_WEBHOOK_OUTBOX_WORKER`.

---

## 1. Purpose

Strategic roadmap item **P1 — Webhook SQL Repository Wiring** (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §6) calls for wiring the existing `SqlxWebhookSubscriptionRepository` and `SqlxWebhookOutboxRepository` into the SQL-backed `intent-api` router startup path so they are no longer `None` placeholders. This file is the bounded local evidence note for that wiring slice.

It does **not**:

- Touch the in-memory router (`build_inmemory_router`) — in-memory behavior is preserved.
- Enable the webhook outbox background worker by default; the existing `INTENT_API_WEBHOOK_OUTBOX_WORKER` env gate remains default-off.
- Introduce new env gates or new dependencies.
- Modify any public doc (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`).
- Claim production-readiness, CI-green status, or external sign-off.

---

## 2. What Changed

In `crates/intent-api/src/main.rs`:

1. Added the import `webhook_subscription_repo::SqlxWebhookSubscriptionRepository` to the `intent_api::{...}` use block (it was not previously imported).
2. In `build_sql_router_with_consumer_jwt` (JWT SQL router path), replaced the two `None` placeholders for `webhook_subscription_repo` and `webhook_outbox_repo` with `Some(Arc::new(SqlxWebhookSubscriptionRepository::new(pool.clone())))` and `Some(Arc::new(SqlxWebhookOutboxRepository::new(pool.clone())))` respectively.
3. In `build_sql_router_with_consumer_impl` (non-JWT SQL router path), applied the same two `Some(Arc::new(...))` replacements.
4. Removed the stale "local-dev only, not wired in main.rs yet" comments at both call sites.

The in-memory router (`build_inmemory_router`) was intentionally left untouched (continues to pass `None` for both webhook repos). The webhook outbox worker startup path (lines ~1093–1125 in `main.rs`) is also untouched and remains default-off.

---

## 3. What Path Is Now Wired

| Router path | Webhook subscription repo | Webhook outbox repo |
|-------------|---------------------------|---------------------|
| In-memory (`build_inmemory_router`) | `None` (unchanged) | `None` (unchanged) |
| SQL non-JWT (`build_sql_router_with_consumer_impl`) | `Some(Arc::new(SqlxWebhookSubscriptionRepository::new(pool.clone())))` | `Some(Arc::new(SqlxWebhookOutboxRepository::new(pool.clone())))` |
| SQL JWT (`build_sql_router_with_consumer_jwt`) | `Some(Arc::new(SqlxWebhookSubscriptionRepository::new(pool.clone())))` | `Some(Arc::new(SqlxWebhookOutboxRepository::new(pool.clone())))` |

The webhook handler / DLQ handler routes read through the `Option<Arc<dyn …>>` repository fields on `RouterState` (`crates/intent-api/src/lib.rs` lines 320–324). Previously, the SQL router started up with both fields set to `None`, so subscription CRUD and DLQ endpoints returned empty/503 stubs. After this slice, both fields are populated with concrete `Sqlx*Repository` instances sharing the same `sqlx::PgPool` that the rest of the SQL router uses.

---

## 4. Verification (Local, Bounded)

Sequential gates (all run from `/home/uong_guyen/work/intent-rebase/intent-rebase`):

| # | Command | Result |
|---|---------|--------|
| 1 | `cargo fmt --all -- --check` | ✅ pass (no diff) |
| 2 | `cargo check -p intent-api --all-features` | ✅ pass (29.46s) |
| 3 | `cargo clippy -p intent-api --all-features -- -D warnings` | ✅ pass (6.86s, no warnings) |
| 4 | `cargo test -p intent-api --lib` | ✅ 442 passed; 0 failed; 17 ignored (4.99s) |
| 5 | `git diff --check` | ✅ clean |
| 6 | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` | ✅ 1 passed (0.95s) |

The optional ignored integration test (gate 6) ran against the local docker-compose Postgres with the migrations already applied to `intent_rebase_phase1_fix`. The default `intent_rebase` database was **not** used because the test fixture migrations live in `intent_rebase_phase1_fix` per the Phase 1 fix evidence recorded in `docs/10-delivery/23-project-assessment-and-execution-tracker.md`. This is local-dev evidence only and is **not** a production DB claim.

The existing `test_webhook_sqlx_outbox_pipeline_success` integration test in `crates/intent-api/tests/webhook_integration.rs` is the same test that the strategic roadmap §6 already names as the canonical end-to-end webhook SQL path exercise. Its pass here is recorded as **local bounded evidence** that the new wiring does not break the existing integration path; the test does **not** itself drive `main.rs` startup. No new test was added in this slice because the wiring is a startup-only change and the existing test suite already covers the runtime SQL paths.

---

## 5. Caveats Preserved

- **Default-off worker:** The webhook outbox background worker remains default-off; this slice did not touch `maybe_start_webhook_outbox_worker` or the `INTENT_API_WEBHOOK_OUTBOX_WORKER` env gate. The worker only starts when `INTENT_API_WEBHOOK_OUTBOX_WORKER=true` is set, which is still an explicit local-dev bounded path (see `main.rs` lines ~841–858).
- **Local-dev bounded:** Both `SqlxWebhookSubscriptionRepository` and `SqlxWebhookOutboxRepository` remain local-dev only. They are not production-hardened: no secret manager, no subscription validation, no tenant-scoped pattern matching, no horizontal scaling, no lease semantics, no backpressure (see existing module-level doc comments in `crates/intent-api/src/webhook_subscription_repo.rs` and `crates/intent-api/src/webhook_outbox_repo/sqlx.rs`).
- **In-memory path unchanged:** The in-memory router still constructs and serves the same `Router`; only the SQL paths gained concrete `Sqlx*Repository` wiring. Any test or dev workflow that did not set `DATABASE_URL` is unaffected.
- **P0 changes preserved:** The uncommitted P0 changes (`crates/rebase-orchestrator/src/orchestrator_tests.rs`, `docs/10-delivery/20-project-completion-roadmap.md`, `docs/10-delivery/23-project-assessment-and-execution-tracker.md`, the new `docs/10-delivery/24-strategic-roadmap-and-checklist.md`, and `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md`) were not reverted or modified by this slice.
- **External gates still blocked:** A-12 (webhook production hardening), A-03 / A-04 / A-05 / A-06 / A-07 / A-10 / A-13 all remain blocked.

---

## 6. Companion Updates

- `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §3 (status row) and §6.5 (action checklist) — status row updated to reflect the bounded wiring slice; action checklist 4 of 4 items checked off (item 3 was the actual SQL router startup wiring newly delivered by this slice; items 1, 2, 4 were pre-existing Slice 4a/4b + WEB-LOCAL-1 evidence and the canonical ignored integration test).
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (update log) — new row recording the P1 wiring slice and verification gates.

**Not touched (per slice handoff):** `docs/getting-started/configuration.md`, `README.md`, `README.vi.md`, `docs/README.md`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`.
