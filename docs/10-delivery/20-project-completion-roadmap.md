# Project Completion Roadmap

> **Status:** P0 complete, P1 complete, P2 module-doc audit in progress  
> **Last updated:** 2026-05-16  
> **Non-production caveat:** This project is explicitly **NOT production-ready**. All phases below are bounded non-production feature delivery. Production readiness requires external sign-off (SRE, Security, Runtime Integration), load testing, pen testing, and compliance audit — none of which are claimed here.

---

## Overview

This document tracks the remaining work to bring the Intent Rebase Engine from its current state to a clean, well-documented, and test-complete non-production codebase. Work is organized into four priority batches (P0–P3). **P0 and P1 are complete; P2 module-doc audit is in progress.** P3 is tracked for future planning and is NOT committed.

| Batch | Theme | Status | Scope |
|-------|-------|--------|-------|
| **P0** | Quality & Cleanup | ✅ Complete | Fast verification, test extraction, CI smoke, code hygiene |
| **P1** | Test Completeness | ✅ Complete | Remaining inline test extractions, router decomposition Stage 1 |
| **P2** | Observability & Docs | 🔄 In Progress | Module-level documentation audit for recently extracted modules; benchmark integration deferred |
| **P3** | Production Readiness | ⬜ Planned | External gates, load testing, pen testing, compliance (future scope) |

---

## P0 — Quality & Cleanup (Complete)

**Goal:** Establish a fast, reliable local verification loop and extract all low-risk inline test modules.

### Delivered
- [x] `scripts/verify-fast.sh` — executable script running `fmt`, `check`, `clippy`, and in-memory `test --lib` without requiring Postgres/NATS
- [x] `.github/workflows/smoke.yml` — real lightweight smoke check (fmt + check + clippy + lib tests) replacing the stub `echo` workflow
- [x] Extract `error_response.rs` inline tests → `error_response_tests.rs`
- [x] Extract `panic_hardening.rs` inline tests → `panic_hardening_tests.rs`
- [x] Extract `approval_invalidation.rs` inline tests → `approval_invalidation_tests.rs`
- [x] Extract `nats_event_publisher.rs` inline tests → `event_publisher_tests.rs`
- [x] Extract `auth.rs` inline tests → `auth_tests.rs` (preserving `jwt-auth` feature gating)
- [x] Register extracted test modules in `lib.rs`
- [x] Router Stage 1: extract JWT builders and auth middleware from `router.rs` into `router/jwt_builders.rs` and `router/auth_middleware.rs` (preserving `jwt-auth` feature gating and public signatures)

### Remaining (P0)
- [ ] Extract `rebase_apply_handlers.rs` inline tests → deferred per task constraints
- [x] Ensure all test modules follow consistent import style (`crate::...` paths) — handler and non-handler
- [ ] Router Stage 2 (route-group split) → deferred to P1/P2 per bounded scope constraints

### Acceptance Criteria
- `cargo fmt --all -- --check` passes
- `cargo check --workspace --all-features` passes
- `cargo clippy --workspace --all-features -- -D warnings` passes
- `cargo test --workspace --lib --all-features` passes
- `scripts/verify-fast.sh` runs in <5 minutes on a clean checkout without external services

---

## P1 — Test Completeness (Planned)

**Goal:** Extract all remaining inline test modules and decompose oversized files.

**Items:**
- [ ] Extract `rebase_apply_handlers.rs` inline tests → `rebase_apply_handler_tests.rs` (currently deferred)
- [x] Extract any remaining inline `#[cfg(test)] mod tests` blocks from handler files (all cleared)
- [x] Extract propagation-signal helper block from `rebase_apply_handlers.rs` → `propagation_signals.rs`
- [x] Extract DLQ error types + `validate_nats_subject` from `nats_jetstream.rs` → `nats_jetstream/dlq.rs` (S2 only)
- [x] Extract `JetStreamInitializer` from `nats_jetstream.rs` → `nats_jetstream/stream.rs` (S3 only)
- [x] Extract `DlqHelper` + DLQ header constants from `nats_jetstream.rs` → `nats_jetstream/dlq.rs` (S4 only)
- [x] Extract `DlqMetricsWorker` family from `nats_jetstream.rs` → `nats_jetstream/dlq_metrics_worker.rs` (S5 only)
- [x] Extract `DlqReplayWorker` family from `nats_jetstream.rs` → `nats_jetstream/dlq_replay_worker.rs` (S6 only)
- [x] Extract `NatsPullConsumerAdapter` + consumer registry family from `nats_jetstream.rs` → `nats_jetstream/consumer.rs` (S7 only)
- [x] Relocate `tests` module from `nats_jetstream.rs` → `nats_jetstream/tests_unit.rs` (A1)
- [x] Relocate `live_integration_tests` module from `nats_jetstream.rs` → `nats_jetstream/tests_live_integration.rs` (A2)
- [x] Relocate `lifecycle_tests` module from `nats_jetstream.rs` → `nats_jetstream/tests_lifecycle.rs` (A3)
- [x] Router Stage 1: JWT builders and auth middleware extracted from `router.rs` → `router/jwt_builders.rs` + `router/auth_middleware.rs` (preserves public API, gated by `jwt-auth` feature)
- [ ] Router Stage 2: route-group split (e.g., graph-routes, compensation-routes, forensic-routes) → deferred to P2; requires evaluation of maintainability thresholds and API stability
- [x] Normalize `super::...` references to `crate::...` paths in all extracted test modules

**Constraints:**
- Do NOT change production behavior
- Preserve `#[cfg(all(test, feature = "jwt-auth"))]` semantics exactly
- Follow proven pattern from `batch_handler_tests.rs`, `compensation_mutation_handler_tests.rs`, etc.

---

## P2 — Observability & Documentation (Planned)

**Goal:** Integrate benchmarks into CI, complete documentation gaps, and harden the local-dev experience.

**Items:**
- [ ] Integrate criterion benchmarks into CI (non-blocking, informational only) — deferred, no benchmarks exist yet
- [ ] Add `cargo bench` step to `verify-fast.sh` as optional/skippable flag — deferred
- [x] Review and update module-level documentation (`//!` headers) for recently extracted modules — completed for router/auth_middleware, router/jwt_builders, nats_jetstream/consumer, nats_jetstream/tests_*
- [x] Ensure recently extracted modules have brief doc comments explaining bounded scope — completed
- [ ] Review and update `//!` headers for remaining handler modules — deferred (consciously scoped to recently extracted modules only)
- [ ] Review `docs/10-delivery/` for stale references and update cross-links — deferred
- [ ] Add `justfile` alternative to `scripts/verify-fast.sh` for teams using `just` — deferred

**Constraints:**
- Benchmarks must not require live Postgres by default (use in-memory repos)
- Documentation updates must preserve non-production caveats

---

## P3 — Production Readiness (Future Scope)

**Goal:** Close external gates required for production deployment.

> **IMPORTANT:** P3 is explicitly out of current scope. It is tracked here for roadmap completeness only. No production readiness claims are made.

**Items:**
- [ ] External SRE sign-off (observability, alerting, runbooks)
- [ ] External Security sign-off (pen testing, threat model v2 validation)
- [ ] Full production load testing (k6/Artillery against staging)
- [ ] Tenant isolation verification across all surfaces
- [ ] Compliance audit (SOC2/GDPR/ISO27001 control validation)
- [ ] Cross-process trace propagation (Temporal SDK, sqlx, NATS consumer, HTTP forwarding)
- [ ] S3-backed forensic bundle retrieval and lifecycle
- [ ] Production DLQ replay (exponential backoff, poison-message detection)

**Gate:** P3 entry is gated on P0 and P1 completion.

---

## Non-Production Caveat

This codebase delivers **bounded non-production features** per phase. The following are explicitly NOT claimed:

| Claim | Status |
|-------|--------|
| Production-ready | ❌ Not claimed |
| External security sign-off | ❌ Not claimed |
| External SRE sign-off | ❌ Not claimed |
| Full load testing | ❌ Not claimed (bounded harness only) |
| Cross-process trace propagation | ❌ Not claimed (partial/in-process only) |
| S3 runtime wiring | ❌ Not claimed (seam exists, not wired) |
| Live NATS consumer production hardening | ❌ Not claimed (local-dev gates only) |

---

## Related Documents

- [Current Project Status](./00-current-status.md)
- [Completion Proposals Tracker](./09-completion-proposals-tracker.md)
- [Phase 2b Residual Risk & Deferral Register](./10-phase-2b-residual-risk-deferral-register.md)
- [Production Readiness Backlog](./17-production-readiness-backlog.md)
- [Agent Safety Rebase Roadmap](./18-agent-safety-rebase-roadmap.md)
