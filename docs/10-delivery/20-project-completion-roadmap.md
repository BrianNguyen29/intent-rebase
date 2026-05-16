# Project Completion Roadmap

> **Status:** P0 in progress — quality/cleanup batch  
> **Last updated:** 2026-05-15  
> **Non-production caveat:** This project is explicitly **NOT production-ready**. All phases below are bounded non-production feature delivery. Production readiness requires external sign-off (SRE, Security, Runtime Integration), load testing, pen testing, and compliance audit — none of which are claimed here.

---

## Overview

This document tracks the remaining work to bring the Intent Rebase Engine from its current state to a clean, well-documented, and test-complete non-production codebase. Work is organized into four priority batches (P0–P3). **Only P0 is actively in scope.** P1–P3 are tracked for future planning and are NOT committed.

| Batch | Theme | Status | Scope |
|-------|-------|--------|-------|
| **P0** | Quality & Cleanup | 🔄 In Progress | Fast verification, test extraction, CI smoke, code hygiene |
| **P1** | Test Completeness | ⬜ Planned | Remaining inline test extractions, router decomposition |
| **P2** | Observability & Docs | ⬜ Planned | Benchmark integration, documentation review, CI hardening |
| **P3** | Production Readiness | ⬜ Planned | External gates, load testing, pen testing, compliance (future scope) |

---

## P0 — Quality & Cleanup (Active)

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

### Remaining (P0)
- [ ] Extract `rebase_apply_handlers.rs` inline tests → deferred per task constraints
- [x] Ensure all test modules follow consistent import style (`crate::...` paths) — handler and non-handler

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
- [ ] Router decomposition: evaluate splitting `router.rs` if it exceeds maintainability thresholds
- [x] Normalize `super::...` references to `crate::...` paths in all extracted test modules

**Constraints:**
- Do NOT change production behavior
- Preserve `#[cfg(all(test, feature = "jwt-auth"))]` semantics exactly
- Follow proven pattern from `batch_handler_tests.rs`, `compensation_mutation_handler_tests.rs`, etc.

---

## P2 — Observability & Documentation (Planned)

**Goal:** Integrate benchmarks into CI, complete documentation gaps, and harden the local-dev experience.

**Items:**
- [ ] Integrate criterion benchmarks into CI (non-blocking, informational only)
- [ ] Add `cargo bench` step to `verify-fast.sh` as optional/skippable flag
- [ ] Review and update all module-level documentation (`//!` headers)
- [ ] Ensure every handler module has a brief doc comment explaining its bounded scope
- [ ] Review `docs/10-delivery/` for stale references and update cross-links
- [ ] Add `justfile` alternative to `scripts/verify-fast.sh` for teams using `just`

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
