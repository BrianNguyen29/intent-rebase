# Project Completion Roadmap

> **Status:** P0 complete, P1 complete, P2 docs audit complete (benchmarks deferred), **Phase 3 CLOSED — Non-Production Only**. Canonical source: [`docs/10-delivery/00-current-status.md`](./00-current-status.md).
> **Last updated:** 2026-06-21
> **Non-production caveat:** This project is explicitly **NOT production-ready**. All phases below are bounded non-production feature delivery. Production readiness requires external sign-off (SRE, Security, Runtime Integration), load testing, pen testing, and compliance audit — none of which are claimed here.

---

## Overview

This document tracks the remaining work to bring the Intent Rebase Engine from its current state to a clean, well-documented, and test-complete non-production codebase. Work is organized into four priority batches (P0–P3). **P0, P1, and P2 docs audit are complete.** Phase 3 is **CLOSED — Non-Production Only (2026-05-11)**. P3 is split into achieved private-only evidence (§P3.1) and still-open public-production gates (§P3.2). **No production readiness is claimed.**

| Batch | Theme | Status | Scope |
|-------|-------|--------|-------|
| **P0** | Quality & Cleanup | ✅ Complete | Fast verification, test extraction, CI smoke, code hygiene |
| **P1** | Test Completeness | ✅ Complete | Remaining inline test extractions, router decomposition Stage 1 |
| **P2** | Observability & Docs | ✅ Docs Complete (benchmarks deferred) | Module-level documentation audit for extracted and handler modules; cross-link consistency review; benchmark integration deferred |
| **P3** | Production Readiness | 🟡 Phase 3 Closed — Private-Only; Public Gates Open | Split: private-only evidence achieved (§P3.1) vs public-production gates still open (§P3.2) |

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
- [x] Extract `rebase_apply_handlers.rs` inline tests → `rebase_apply_handler_tests.rs` (delivered; module registered in `lib.rs`)
- [x] Ensure all test modules follow consistent import style (`crate::...` paths) — handler and non-handler
- [x] Router Stage 2 (route-group split) → deferred to P1/P2 per bounded scope constraints

### Acceptance Criteria
- `cargo fmt --all -- --check` passes
- `cargo check --workspace --all-features` passes
- `cargo clippy --workspace --all-features -- -D warnings` passes
- `cargo test --workspace --lib --all-features` passes
- `scripts/verify-fast.sh` runs in <5 minutes on a clean checkout without external services

---

## P1 — Test Completeness (Complete)

**Goal:** Extract all remaining inline test modules and decompose oversized files.

**Items:**
- [x] Extract `rebase_apply_handlers.rs` inline tests → `rebase_apply_handler_tests.rs` (delivered; module registered in `lib.rs`)
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
- [x] Router Stage 2: route-group split → domain-grouped route modules under `crates/intent-api/src/routes/` (intent, graph, compensation, approval, forensic, policy, webhook, propagation, health) delivered as A-09 decomposition slice
- [x] Normalize `super::...` references to `crate::...` paths in all extracted test modules

**Constraints:**
- Do NOT change production behavior
- Preserve `#[cfg(all(test, feature = "jwt-auth"))]` semantics exactly
- Follow proven pattern from `batch_handler_tests.rs`, `compensation_mutation_handler_tests.rs`, etc.

---

## P2 — Observability & Documentation (Docs Complete — benchmarks deferred)

**Goal:** Integrate benchmarks into CI, complete documentation gaps, and harden the local-dev experience.

**Items:**
- [ ] Integrate criterion benchmarks into CI (non-blocking, informational only) — deferred; real criterion source files exist in 4 crates (`intent-service`, `rebase-engine`, `intent-api`, `graph-service`) but CI wiring not implemented
- [ ] Add `cargo bench` step to `verify-fast.sh` as optional/skippable flag — deferred; bench compile guard passes (`cargo check --benches`) but harness execution not wired into `verify-fast.sh`
- [x] Review and update module-level documentation (`//!` headers) for recently extracted modules — completed for router/auth_middleware, router/jwt_builders, nats_jetstream/consumer, nats_jetstream/tests_*
- [x] Ensure recently extracted modules have brief doc comments explaining bounded scope — completed
- [x] Review and update `//!` headers for remaining handler modules — completed (added bounded/non-production caveats to the weakest modules; all 21 listed handler modules have concise `//!` docs)
- [x] Review `docs/10-delivery/` for stale references and update cross-links — completed for delivery/quality docs in the P2 cross-link consistency review
- [x] Add `justfile` alternative to `scripts/verify-fast.sh` for teams using `just` — delivered at root `justfile` with fmt-check, check, clippy, test-lib, verify-fast targets

**Constraints:**
- Benchmarks must not require live Postgres by default (use in-memory repos)
- Documentation updates must preserve non-production caveats

---

## P3 — Production Readiness (Phase 3 CLOSED — Non-Production Only)

**Goal:** Distinguish achieved private-only solo completion evidence from still-open public-production and commercial-readiness gates.

> **IMPORTANT:** Phase 3 is **CLOSED — Non-Production Only (2026-05-11)**. P3 is **not** out of current scope; it is reframed into two sub-sections: private-only evidence achieved (§P3.1) and public-production gates still open (§P3.2). No production readiness, commercial readiness, or external sign-off claims are made. See ADR-16 (`docs/13-adrs/16-solo-private-operation-waiver.md`) for solo/private waiver terms.

### P3.1 — Private-Only Solo Completion Evidence (Achieved)

The following evidence was achieved as part of the final private close-out (commit `845b8b1`):

- **verify-fast equivalent:** `cargo fmt`/`check`/`clippy`/`test --lib` all pass; `scripts/verify-fast.sh` and `just verify-fast` are the canonical local gates.
- **Staging clone deletion:** PITR validation clone `a07-staging-postgres-20260618143121` deleted after use.
- **Final DR smoke:** Clone `dr-final-smoke-20260620051418` created, app deployed, health/ready endpoints passed, authenticated API create+read passed, clone deleted.

**Phase 4 bounded slices delivered (non-production only):**
- NATS multi/full consumer bounded work (`INTENT_API_NATS_FULL_CONSUMER=true`, `DlqMetricsWorker`, `DlqReplayWorker`, `ConsumerRegistry`)
- JWT dual-key support (kid-based rotation scaffold, `JWT_SECRET` + `JWT_SECRET_PREVIOUS` env support)
- Webhook outbox/subscription/HMAC/retry/DLQ bounded work (migrations 019–022, `WebhookOutboxWorker`, `WebhookDeliveryDispatcher`, HMAC-SHA256 signing, retry/backoff classification, DLQ list/replay/bulk-replay/stats endpoints, subscription CRUD API)
- Forensic chain-hash (ADR-14, `chain_hash.rs` pure module with tests, `BundleIntegrity.previous_bundle_hash`)
- Worker/router decomposition/hardening (route-group split under `routes/`, `propagation_handlers.rs`, `panic_hardening.rs` with `process_panics_total` metric)

These are **bounded non-production slices** and do not constitute production readiness.

### P3.2 — Public Production & Commercial Readiness Gates (Still Open)

| Gate | Status | Blocker |
|------|--------|---------|
| External SRE sign-off (A-03) | 🔴 Open — WAIVED-SOLO | Historical `APPROVED WITH CONDITIONS` (DuongNguyen, 2026-06-15) on record; **external re-signoff NOT obtained** per ADR-16 |
| External Security sign-off (A-04) | 🔴 Open — WAIVED-SOLO | Historical `APPROVED WITH CONDITIONS` (DuongNguyen, 2026-06-15) on record; **external re-signoff NOT obtained** per ADR-16 |
| Penetration testing (A-07) | 🔴 Open — WAIVED-SOLO / PRIVATE-ONLY | ZAP self-scan prep only (0 FAIL, 1 WARN); **external pen test NOT executed**; no public-ingress pen test |
| Full production load testing (L3–L5) | 🔴 Open | Staging 30-min business-path load passed; production/public-ingress load NOT done |
| Tenant isolation verification across all surfaces | 🔴 Open | Per-tenant JetStream streams (ADR-15) design complete; no production rollout |
| Compliance audit (SOC2/GDPR/ISO27001) | 🔴 Open | Requires external auditor engagement |
| Cross-process trace propagation | 🔴 Deferred | Temporal SDK limitation; upstream SDK-blocked |
| S3-backed forensic bundle retrieval and lifecycle | 🔴 Open | Object Lock not deployed; S3 runtime wiring not deployed |
| Production DLQ replay | 🔴 Open | Exponential backoff, poison-message detection, batch replay; requires production NATS topology |
| Public ingress / TLS / Cloud Armor | 🔴 Intentionally deferred | Private-only decision; no public domain or ingress |
| Commercial readiness (SLA/SLO, team, IR) | 🔴 Open | Requires team/on-call, SLA/SLO commitments, SBOM/dependency audit, incident-response drills, data deletion/residency, customer docs |

**Gate:** Phase 3 close-out is gated on P0, P1, and P2 completion (all achieved). Phase 4 entry requires closing external evidence gates above. **No production-ready claim is made.**

---

## Non-Production Caveat

This codebase delivers **bounded non-production features** per phase. The following are explicitly NOT claimed:

| Claim | Status |
|-------|--------|
| Production-ready | ❌ Not claimed |
| External security sign-off (A-04) | ❌ Not claimed — SELF-ATTESTED-SOLO / WAIVED-SOLO per ADR-16 |
| External SRE sign-off (A-03) | ❌ Not claimed — SELF-ATTESTED-SOLO / WAIVED-SOLO per ADR-16 |
| Penetration testing passed (A-07) | ❌ Not claimed — WAIVED-SOLO / PRIVATE-ONLY per ADR-16 |
| Full load testing | ❌ Not claimed (bounded harness + staging 30-min only) |
| Cross-process trace propagation | ❌ Not claimed (partial/in-process only) |
| S3 runtime wiring | ❌ Not claimed (seam exists, not wired) |
| Live NATS consumer production hardening | ❌ Not claimed (local-dev gates only) |
| Commercial readiness | ❌ Not claimed |
| Public ingress / TLS / Cloud Armor | ❌ Intentionally deferred (private-only) |

---

## Related Documents

- [Current Project Status](./00-current-status.md)
- [Completion Proposals Tracker](./09-completion-proposals-tracker.md)
- [Phase 2b Residual Risk & Deferral Register](./10-phase-2b-residual-risk-deferral-register.md)
- [Production Readiness Backlog](./17-production-readiness-backlog.md)
- [Agent Safety Rebase Roadmap](./18-agent-safety-rebase-roadmap.md)
- [Phase 4 Entry Plan](./22-phase-4-entry-plan.md) — detailed A-01..A-13 todo-list and execution roadmap; includes evidence packet plan and decomposition progress
- [Project Assessment and Execution Tracker](./23-project-assessment-and-execution-tracker.md) — consolidated contradiction register, risk register, phase execution plan, and validation matrix
- [Strategic Roadmap and Execution Checklist](./24-strategic-roadmap-and-checklist.md) — internal planning companion covering the latest strategic evaluation recommendations (P0 runtime adapter end-to-end proof, P1 intent-api decomposition, P1 webhook SQL repository wiring, P2 intent-cli decoupling, P2 benchmark compile guard, P2 documentation language policy, P3/P4 deferred); **internal only**, not a public support doc, not a production-readiness claim
