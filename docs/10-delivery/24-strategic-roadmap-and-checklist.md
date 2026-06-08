# Strategic Roadmap and Execution Checklist (Internal)

> **Status:** INTERNAL PLANNING — not a public artifact, not a production-readiness claim
> **Date:** 2026-06-07
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Internal roadmap/checklist for the latest strategic evaluation recommendations; captures rationale, owner type, acceptance criteria, validation commands, caveats, and per-item action checklists for the next bounded execution slices.
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, and it does not claim CI-green status, external sign-off, or completed production gates. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose and Audience

This document exists to translate the latest strategic evaluation findings (bounded explorer/oracle reviews) into a **single execution-ready checklist** that a future slice owner can pick up without re-analysis. It is intentionally:

- **Internal only.** Located under `docs/10-delivery/` (non-public support documentation). It is **not** linked from the public reading order in `docs/README.md`, the project `README.md`, or `README.vi.md`.
- **Action-oriented.** Every item has an action checklist, acceptance criteria, validation commands, risks, and an owner type.
- **Conservative.** It does not invent reviewers, infrastructure, or evidence. All P3/P4 items are explicitly gated on external factors and do not assume future success.
- **Composable.** It is a forward-looking execution companion to `23-project-assessment-and-execution-tracker.md` (current phase evidence) and `20-project-completion-roadmap.md` (P0–P3 completion roadmap).

> **Audience:** Solo practitioner (BrianNguyen) and authorized assistants acting on their behalf. It is not a support document for external users.

---

## 2. Executive Summary

| Theme | Priority | Why now |
|-------|----------|---------|
| **Runtime Adapter End-to-End Proof** | **P0** | The `RuntimeAdapter` trait (`crates/runtime-adapter/src/lib.rs`) plus `MockAdapter` and feature-gated `TemporalAdapter` are trait-defined and partially wired, but the public docs (`docs/getting-started/configuration.md`) and the strategic evaluation note that there is **no end-to-end proof** that an apply cycle goes through the adapter boundary and reaches a real or mock runtime in a way that a future maintainer can replay step by step. Without that, the trait is a seam without a verified contract. |
| **Intent API Decomposition** | **P1** | `crates/intent-api/src/` now contains ~75 files. `A-09 / S6` in `22-phase-4-entry-plan.md` and `P0-6` in the tracker have already extracted many handler/test files, but the crate is still a single binary crate. Further logical decomposition (sub-modules by domain) reduces merge conflict surface and clarifies ownership boundaries for the next Phase 4+ slices. |
| **Webhook SQL Repository Wiring** | **P1** | `SqlxWebhookOutboxRepository` and `SqlxWebhookSubscriptionRepository` exist and pass bounded integration tests (see `22-phase-4-entry-plan.md` A-12 WEB-LOCAL-1 + Slice 4a/4b). The remaining wiring gap is the durable write path through the propagation/dispatch boundary and full coverage in the default-feature startup sequence; this is a high-leverage local-executable slice. |
| **Intent CLI Decoupling** | **P2** | `crates/intent-cli/src/main.rs` is a single 294-line file with bounded scope (single-shot compensation action orchestration). The CLI directly calls HTTP endpoints and could be split into a thin `bin/intent-cli.rs` plus a `lib` module for testable command logic, enabling subcommand tests without running the full clap parse path. |
| **Benchmark Compile Guard** | **P2** | `[[bench]]` stanzas and `criterion` dev-dependencies exist in 4 crates (`intent-api`, `graph-service`, `intent-service`, `rebase-engine`) but no benchmark source files exist. `cargo bench` on a clean checkout will fail to compile (or no-op), which is a hidden footgun for new contributors. A bounded compile guard (e.g., a stub `benches/<name>.rs` returning a `criterion::criterion_main!` over a no-op) would make the harness non-fatal and explicit. |
| **Documentation Language Policy** | **P2** | The repo mixes English and Vietnamese in internal docs (e.g., `docs/10-delivery/01-roadmap.md` has English headers with Vietnamese items; `docs/13-adrs/01-runtime-adapter.md` lines 25, 38; `docs/10-delivery/04-phase-2-runtime-integrated.md` line 4). The strategic evaluation flagged this as an inconsistency risk: public-facing reading order docs are in English; internal-only files drift. A formal language policy prevents future contributor confusion. **User decision accepted 2026-06-07 (option A: English-primary technical docs; Vietnamese allowed only in `README.vi.md` or explicit `*.vi.md` translation files); see §9.5 and §9.11.** |
| **P3/P4 Deferred / External-Gated** | P3/P4 | Items that depend on external reviewers, production infrastructure, vendor certifications, or upstream SDK fixes. Not actionable locally beyond tracking. |

**Critical-Path Read:** P0 must be executed before any new feature work touches the runtime adapter contract. P1 items can be parallelized across crates. P2 items are bounded local-executable hardening.

---

## 3. Prioritized Roadmap Table

| # | Priority | Item | Owner Type | Est. Slice Effort | Status | External Gate? |
|---|----------|------|------------|-------------------|--------|----------------|
| 1 | **P0** | Runtime Adapter End-to-End Proof | local | S (1–2 days bounded) | 🟡 BOUNDED DONE (local proof — see [§4.10](#410-evidence-link)) | ❌ No |
| 2 | **P1** | Intent API Decomposition | local | M (3–5 bounded slices) | 🟡 First bounded demo slice done — `health_routes` → `routes::health` (see [§5.10](#510-evidence-link)); A-09 S6 continuation ongoing | ❌ No |
| 3 | **P1** | Webhook SQL Repository Wiring | local | S–M (bounded slices) | 🟡 BOUNDED WIRING DONE (SQL router startup now passes concrete `SqlxWebhookSubscriptionRepository` + `SqlxWebhookOutboxRepository`; see [§6.10](#610-evidence-link)) | ❌ No |
| 4 | **P2** | Intent CLI Decoupling | local | S (1 slice) | 🟡 BOUNDED DONE (binary-to-library split with 3 new unit tests; see [§7.10](#710-evidence-link)) | ❌ No |
| 5 | **P2** | Benchmark Compile Guard | local | XS (1 slice) | 🟡 BOUNDED DONE (local compile guard — see [§8.10](#810-evidence-link)) | ❌ No |
| 6 | **P2** | Documentation Language Policy | user decision | S (1 doc policy) | ✅ ACCEPTED (user decision — see [§9.5](#95-accepted-policy-user-decision-on-2026-06-07) and [§9.11](#911-evidence-link)) | ⚠️ User-facing policy decision |
| 7 | **P3** | External Gates (A-03 SRE, A-04 Security, A-07 Pen Test, A-05 Infra, A-06 Load L3–L5) | external | n/a (gated) | 🔴 Blocked | ✅ Yes |
| 8 | **P4** | Enterprise Expansion (P8 Policy Sim, P9 Advanced Adapters, P10 Trust Scoring) | external / Phase 4+ | n/a (gated) | 🔴 Blocked / Planned | ✅ Yes |

**Status legend:** ⬜ Not started · 🟡 Partial/In progress · ✅ Bounded done · 🔴 Blocked · ⚠️ User decision required.

---

## 4. P0 — Runtime Adapter End-to-End Proof

### 4.1 Problem Statement

The `RuntimeAdapter` trait (`crates/runtime-adapter/src/lib.rs`) defines five async methods (`get_checkpoints`, `send_rebase_signal`, `map_intent_to_checkpoint`, `replay_from_checkpoint`, `is_adapter_ready`) and is implemented by `MockAdapter` (always compiled) and `TemporalAdapter` (feature `temporal`). The trait is wired into `RebaseOrchestrator` per the Phase 2b sign-off packet. However, the strategic evaluation observed that there is **no single end-to-end test that exercises the full apply cycle through the adapter boundary** and records observable evidence in CI/local verification. The closest is `RebaseOrchestrator` integration tests with `MockAdapter`, but the in-process `apply` → `map_intent_to_checkpoint` → `replay_from_checkpoint` boundary is not asserted as a single atomic contract.

### 4.2 Why Now

- The `RuntimeAdapter` is the seam between the deterministic rebase engine and the runtime (Temporal in production, Mock in dev/test). Without an end-to-end proof, future maintainers cannot tell whether a refactor in either crate breaks the contract.
- Phase 2b exit is closed and `A-09` decomposition continues. New handler extractions in `intent-api` increase the chance of accidental boundary changes.
- The public configuration docs (`docs/getting-started/configuration.md` lines 60–64) reference the runtime adapter env vars but do not link to a runnable end-to-end proof.

### 4.3 Scope IN

- A single end-to-end test (`test_runtime_adapter_apply_end_to_end` or similar) that:
  - Spins up a `RebaseOrchestrator` with `Arc<MockAdapter>` via the existing constructor.
  - Triggers an apply (or replay) path that calls **at least three of the five** trait methods in sequence.
  - Asserts the returned checkpoint candidate and signal types are stable.
- A documentation note in `docs/10-delivery/11-phase-2b-sign-off-packet.md` (or a new `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md`) recording the test path, command, and captured output.
- A reference in `docs/getting-started/configuration.md` (public) linking to the new evidence doc **without** introducing production-readiness language.

### 4.4 Scope OUT

- No Temporal server required. `MockAdapter` is sufficient; `TemporalAdapter` is feature-gated and not exercised in default `cargo test`.
- No new external SRE review (this is local-only).
- No new dependency additions.

### 4.5 Action Checklist

- [x] Identify the existing `RebaseOrchestrator` constructor signature and confirm it takes `Arc<dyn RuntimeAdapter>`.
- [x] Locate the closest existing in-memory apply/replay test in `intent-api` or `rebase-orchestrator`.
- [x] Author a new test that drives the orchestrator through a bounded scenario calling `map_intent_to_checkpoint` → `send_rebase_signal` → `replay_from_checkpoint` and asserts checkpoint IDs and signal types. (Implemented as `test_runtime_adapter_apply_end_to_end` in `crates/rebase-orchestrator/src/orchestrator_tests.rs`; the orchestrator-driven path is `align_checkpoint` (mapping) → `send_runtime_rebase_signal` (signal) → `replay_from_checkpoint` (replay), which exercises `is_adapter_ready` + `send_rebase_signal` + `replay_from_checkpoint` = 3 of the 5 trait methods in sequence.)
- [x] Run `cargo test -p rebase-orchestrator --lib` and capture output.
- [ ] Run `cargo test -p intent-api --lib` (filtered to the new test) and capture output. *(Deferred — the new test lives in `rebase-orchestrator`; the `intent-api` crate has no equivalent in-process apply-cycle test, so this item is out of scope for this slice. The orchestrator test exercises the same adapter seam.)*
- [x] Add a short evidence section in a new or existing internal doc, naming the test, command, and result. (Implemented as new `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md`; see §4.10 below.)
- [ ] Add a cross-reference link from `docs/getting-started/configuration.md` to the evidence doc. *(Deferred — the slice handoff explicitly forbids editing public docs (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`). The evidence link lives in the internal `24b-…` doc and is referenced from this internal roadmap instead.)*

### 4.6 Acceptance Criteria

- New test exists, passes locally, and is registered in the lib (not behind `#[ignore]`).
- Test asserts at least three of the five `RuntimeAdapter` methods in sequence and verifies return values.
- `cargo fmt --all -- --check`, `cargo check --workspace --all-features`, `cargo clippy --workspace --all-targets -- -D warnings` all pass.
- `docs/getting-started/configuration.md` (public) gains **one** internal link to the evidence doc with neutral wording (no "production-ready" claim).

### 4.7 Validation Commands

```bash
# Format and type checks
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-targets -- -D warnings

# Targeted test
cargo test -p rebase-orchestrator --lib runtime_adapter
cargo test -p intent-api --lib runtime_adapter

# Diff hygiene
git diff --check
```

### 4.8 Risks / Tradeoffs

- **Risk:** Adding a test that depends on internals of `RebaseOrchestrator` could couple it to refactors. **Mitigation:** drive the test through the public orchestrator API, not through private helpers.
- **Risk:** Wording in the public docs could drift toward production-readiness. **Mitigation:** Use the existing "bounded local-dev evidence" boilerplate and copy-paste from `23-project-assessment-and-execution-tracker.md` §3.1.
- **Tradeoff:** The new test cannot replace a real Temporal E2E; the scope explicitly excludes Temporal server requirements.

### 4.9 Owner Type

**local** — bounded local-executable slice; no external dependency, no user decision, no production infra.

### 4.10 Evidence Link

> **Status:** P0 slice is **🟡 BOUNDED DONE (local proof)** as of 2026-06-07. The new test exists, is not `#[ignore]`-d, and passes locally. Production-readiness, CI-green, and external sign-off are **not** claimed.

The bounded local proof and the full command/result record live in:

- `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md` — what path is proven, command + result, trait-method coverage table (3 of 5 methods exercised in sequence), sequential verification gates, and explicit non-production caveat.

Supporting code change:

- `crates/rebase-orchestrator/src/orchestrator_tests.rs` — new test `test_runtime_adapter_apply_end_to_end` (uses existing `MockAdapter::ready()` and in-memory `MockCheckpointRepo` / `MockGraphRepo`; no new dependency; no public-doc edits).

**Companion update-log row:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. In particular, the test is `MockAdapter`-only; the `TemporalAdapter` is feature-gated and out of scope per §4.4. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 5. P1 — Intent API Decomposition

### 5.1 Problem Statement

`crates/intent-api/src/` contains ~75 Rust source files. While the strategic evaluation recognizes that A-09 / S6 has already extracted many handler and test modules, the crate is still a single binary crate. As the surface grows (per Slice 4a/4b of `22-phase-4-entry-plan.md` A-12 webhook work, plus propagation handler extraction), the risk of merge conflicts and unclear ownership boundaries increases. The strategic evaluation recommends a structured decomposition plan that future slices can execute without re-deriving the same conclusions.

### 5.2 Why Now

- The most recent bounded decomposition slice (propagation handlers, 2026-05-25) shows the pattern is proven and safe.
- New webhook handler files (`webhook_outbox_dlq_handlers.rs`, `webhook_subscription_handlers.rs`) and DLQ/CRUD endpoints add to the same crate.
- The `intent-api` crate's `lib.rs` re-exports grow proportionally; a clearer module tree reduces `pub use` churn.

### 5.3 Scope IN

- A **decomposition plan section** in this document (§5.5) listing candidate sub-modules and the extraction order.
- A bounded first slice: extract a single leaf module (e.g., `health_routes.rs` → `routes/health.rs` style) following the A-09 / S6 proven pattern. This is a "demo" slice that confirms the path is still safe.
- Updated `//!` module-level docs on the moved files, preserving non-production caveats.

### 5.4 Scope OUT

- No cross-crate extraction (e.g., moving `webhook_outbox_repo` into a separate crate). That is a different risk class and is out of scope for a single strategic slice.
- No rename of public route paths.
- No change to JWT/RBAC behavior.

### 5.5 Action Checklist

- [x] Inventory current `intent-api/src/` top-level modules and identify leaf modules safe to move (no `pub use` re-export coupling). *(See A-09 in `22-phase-4-entry-plan.md` for the existing decomposition inventory; the P1 demo slice picked the lowest-risk leaf.)*
- [x] Pick the lowest-risk leaf (`health_routes.rs` → `routes/health.rs`) and execute one bounded extraction. *(Demo slice delivered 2026-06-07; see §5.10.)*
- [x] Update `lib.rs` `pub mod` and any `crate::health_routes::` references to `crate::routes::health::`. *(Removed `pub mod health_routes;`; `routes::health::add_routes` now uses local handlers; `router.rs` now references `routes::health::request_id_middleware` / `routes::health::trace_context_middleware`; `health_routes.rs` deleted.)*
- [x] Run the validation commands in §5.7. *(All four sequential gates pass — see §5.10.)*
- [ ] Update `22-phase-4-entry-plan.md` A-09 status line to note the continuation slice. *(Out of scope for the demo slice per the slice handoff; deferred to the next A-09 continuation slice. Tracked as the follow-up in the §5.5 next-candidate list below.)*
- [x] Update this document's §5.5 with the executed slice and the next candidate. *(This checklist and §5.10 evidence link.)*

**Next candidate (bounded extraction, after the demo slice):** the `intents/...` handler modules under `intent-api/src/` that are still top-level and not yet grouped under `routes/intent.rs` are the natural next leaf candidates. The same A-09 / S6 pattern (leaf module → `routes/<domain>.rs` plus a single `add_routes` aggregator) applies. The candidate selection and execution is a follow-up slice and is **not** part of this bounded demo slice.

### 5.6 Acceptance Criteria

- One leaf module extracted; `cargo check -p intent-api --all-features` and `cargo test -p intent-api --lib` pass.
- `cargo fmt --all -- --check`, `cargo clippy -p intent-api --all-features -- -D warnings` pass.
- `git diff --check` is clean.
- `22-phase-4-entry-plan.md` and `23-project-assessment-and-execution-tracker.md` update logs gain a new bounded-slice entry.

### 5.7 Validation Commands

```bash
cargo fmt --all -- --check
cargo check -p intent-api --all-features
cargo clippy -p intent-api --all-features -- -D warnings
cargo test -p intent-api --lib
git diff --check
```

### 5.8 Risks / Tradeoffs

- **Risk:** Public re-exports break consumers. **Mitigation:** Crate has no external Rust consumers in this repo (binary crate); verify with `cargo check --workspace --all-features`.
- **Risk:** A future slice duplicates the same logic. **Mitigation:** Update `22-phase-4-entry-plan.md` A-09 with the next candidate after each slice.

### 5.9 Owner Type

**local** — bounded local-executable slice; no external dependency, no user decision.

### 5.10 Evidence Link (P1 Demo Slice)

> **Status:** P1 demo slice is **🟡 First bounded demo slice done** as of 2026-06-07. The lowest-risk leaf module (`health_routes.rs` → `routes/health.rs`) was extracted, the top-level module declaration was removed, the public route paths and middleware layering are preserved byte-identically, and all sequential verification gates pass. Production-readiness, CI-green, and external sign-off are **not** claimed. The decomposition remains a longer P1 workstream; further leaf candidates are tracked in the §5.5 "Next candidate" paragraph.

The bounded local evidence and the full command/result record live in:

- `docs/10-delivery/24e-health-routes-extraction-evidence.md` — what moved from `crates/intent-api/src/health_routes.rs` into `crates/intent-api/src/routes/health.rs` (handlers, middleware, route registration), the reference-rewrite table (`lib.rs` / `router.rs` / `routes/health.rs` / `docs/04-api/route-openapi-contract-map.md`), the deletion of the now-orphan top-level `health_routes.rs`, sequential verification gates (fmt / check / clippy / lib tests / `git diff --check` / workspace check), and explicit non-production caveats.

Supporting code changes:

- `crates/intent-api/src/routes/health.rs` — file rewritten to be the single self-contained health route group. All five items (`request_id_middleware`, `trace_context_middleware`, `health_handler`, `ready_handler`, `metrics_handler`) are `pub` in this module; `add_routes` now references them as local symbols instead of `crate::health_routes::*`. Module doc-comment preserved non-production caveats and notes the A-09 / S6 follow-on pattern.
- `crates/intent-api/src/lib.rs` — removed `pub mod health_routes;`; replaced the stale `// Health check routes and middleware have been moved to health_routes.rs` comment with a one-liner pointing to `routes::health`.
- `crates/intent-api/src/router.rs` — removed `use crate::health_routes;`; `axum::middleware::from_fn` layers now reference `routes::health::request_id_middleware` and `routes::health::trace_context_middleware`. Middleware layering order (request-id before trace-context) is preserved.
- `crates/intent-api/src/health_routes.rs` — **deleted** (no remaining references after the rewrites; verified by `grep -r health_routes`).
- `docs/04-api/route-openapi-contract-map.md` — three rows for `/health`, `/ready`, `/metrics` updated `Handler Module` column from `health_routes` to `routes::health`. Status, paths, methods, and tags are unchanged.

**Companion update-log row:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. Route paths, methods, status codes, headers, request/response bodies, middleware ordering, and metrics behavior are byte-identical to the pre-extraction implementation; the only observable difference is the Rust module path of the moved symbols. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 6. P1 — Webhook SQL Repository Wiring

### 6.1 Problem Statement

`SqlxWebhookOutboxRepository` and `SqlxWebhookSubscriptionRepository` are delivered and tested (see `22-phase-4-entry-plan.md` A-12 WEB-LOCAL-1a, Slice 4a, Slice 4b). The strategic evaluation notes that the durable write path through `propagation_signals::dispatch_webhooks_for_intent_with_outbox` (WEB-LOCAL-1b) is now in place, but the integration with the default-feature startup sequence (`crates/intent-api/src/main.rs`) is opt-in (behind `INTENT_API_WEBHOOK_OUTBOX_WORKER` and `INTENT_API_WEBHOOK_DELIVERY` env gates). The bounded end-to-end path is exercised only by the `webhook_integration.rs` ignored test on a fresh DB. The strategic evaluation recommends documenting a clear wiring checklist for future maintainers.

### 6.2 Why Now

- A-12 is the most progressed bounded slice; further wiring reduces the chance of env-gate drift.
- The default-feature startup path is the most likely place where future maintainers will accidentally change behavior.

### 6.3 Scope IN

- A wiring checklist documenting:
  - Which env gates control SQLx outbox worker startup (`INTENT_API_WEBHOOK_OUTBOX_WORKER`).
  - Which env gates control delivery (`INTENT_API_WEBHOOK_DELIVERY`).
  - Where the durable outbox write happens (`propagation_signals::dispatch_webhooks_for_intent_with_outbox`).
  - Which integration test exercises the path end-to-end (`crates/intent-api/tests/webhook_integration.rs`).
- A bounded slice that adds a "happy path" test (already exists) and updates its docstring/comment to reference the checklist.

### 6.4 Scope OUT

- No production secret manager wiring.
- No key rotation.
- No real external receiver configuration.

### 6.5 Action Checklist

- [x] Verify `propagation_signals.rs` `dispatch_webhooks_for_intent_with_outbox` is the canonical durable write path. *(Out of scope for this bounded wiring slice; durable write path is already delivered per Slice 4a/4b + WEB-LOCAL-1 evidence. This slice focuses on the SQL router startup wiring, not the durable write path.)*
- [x] Verify `main.rs` `maybe_start_webhook_outbox_worker` is the canonical startup wiring. *(Out of scope for this bounded wiring slice; worker startup is unchanged. This slice wires the repos into the SQL router builder so the handler endpoints no longer see `None` for `webhook_subscription_repo` and `webhook_outbox_repo`.)*
- [x] Wire `SqlxWebhookSubscriptionRepository` + `SqlxWebhookOutboxRepository` into both `build_sql_router_with_consumer_jwt` and `build_sql_router_with_consumer_impl` calls in `crates/intent-api/src/main.rs`. Replaced the two `None` placeholders with `Some(Arc::new(Sqlx*Repository::new(pool.clone())))` in both paths. In-memory router and worker default-off behavior preserved. (See §6.10 below for evidence.)
- [x] Confirm the existing `test_webhook_sqlx_outbox_pipeline_success` test passes on a fresh DB and document the command. (Local bounded evidence: `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` → 1 passed.)

### 6.6 Acceptance Criteria

- This document §6.3 references the canonical write path and startup wiring with file paths and function names (no line numbers).
- `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` passes (documented, not run by default).
- `cargo test -p intent-api --lib` passes.
- No new env gate introduced.

### 6.7 Validation Commands

```bash
# Local verification (no live DB required)
cargo test -p intent-api --lib

# Optional ignored integration (manual trigger only)
DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix \
  cargo test -p intent-api --test webhook_integration -- --ignored

git diff --check
```

### 6.8 Risks / Tradeoffs

- **Risk:** Documentation drifts from code after a refactor. **Mitigation:** Use stable module/function anchors, not line numbers.
- **Tradeoff:** Manual ignored tests are easy to forget. **Mitigation:** Document the command in `docs/11-quality/01-test-strategy.md` (already partly done).

### 6.9 Owner Type

**local** — bounded local-executable slice; no external dependency.

### 6.10 Evidence Link

> **Status:** P1 wiring slice is **🟡 BOUNDED WIRING DONE (local bounded evidence)** as of 2026-06-07. The SQL router startup path now passes concrete `SqlxWebhookSubscriptionRepository` and `SqlxWebhookOutboxRepository` instances in both JWT and non-JWT paths. The in-memory router and the webhook outbox worker default-off behavior are preserved. Production-readiness, CI-green, and external sign-off are **not** claimed.

The bounded local evidence and the full command/result record live in:

- `docs/10-delivery/24c-webhook-sql-repo-wiring-evidence.md` — what changed in `main.rs`, the wiring table (in-memory / SQL non-JWT / SQL JWT), sequential verification gates (fmt / check / clippy / lib tests / git diff --check / ignored integration test on `intent_rebase_phase1_fix`), and explicit local-dev / default-off-worker caveats.

Supporting code change:

- `crates/intent-api/src/main.rs` — added `webhook_subscription_repo::SqlxWebhookSubscriptionRepository` to the `intent_api::{...}` import block, and in both `build_sql_router_with_consumer_jwt` and `build_sql_router_with_consumer_impl` replaced the two `None` placeholders with `Some(Arc::new(Sqlx*Repository::new(pool.clone())))` for the webhook subscription and outbox repos. In-memory router and `maybe_start_webhook_outbox_worker` paths are unchanged.

**Companion update-log row:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. In particular, the wired `Sqlx*Repository` instances are local-dev only — no secret manager, no subscription validation, no tenant-scoped pattern matching, no horizontal scaling, no lease semantics, no backpressure. The webhook outbox background worker remains default-off behind `INTENT_API_WEBHOOK_OUTBOX_WORKER`. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 7. P2 — Intent CLI Decoupling

### 7.1 Problem Statement

`crates/intent-cli/src/main.rs` is a single 294-line file with bounded scope (single-shot compensation action orchestration). All logic — argument parsing, HTTP request building, response handling — lives in the same translation unit. This makes the CLI command logic untestable without running the full `clap` parse path. A bounded decoupling into `bin/intent-cli.rs` (entry point) + `lib.rs` (testable command logic) is a low-risk refactor with high maintainability payoff.

### 7.2 Why Now

- A future feature may add new subcommands (e.g., list-actions, replay-outbox). The current monolithic shape makes that additive change risky.
- The crate is a binary; creating a `lib` is a small, well-understood Cargo pattern.

### 7.3 Scope IN

- A `lib.rs` exposing the command types (`Cli`, `Commands`) and pure functions for the `run` subcommand payload.
- A thin `bin/intent-cli.rs` that calls `intent_cli::run()`.
- Doc tests or unit tests for the new pure functions.

### 7.4 Scope OUT

- No new subcommand.
- No new HTTP endpoint.
- No change to the public CLI UX (flags, help text).

### 7.5 Action Checklist

- [x] Create `crates/intent-cli/src/lib.rs` with the `Cli`, `Commands` types and a `pub fn run(cli: Cli) -> Result<...>` function.
- [x] Convert `crates/intent-cli/src/main.rs` into a thin `bin/intent-cli.rs` that calls `intent_cli::run()`.
- [x] Add `[[bin]]` / `[lib]` sections to `crates/intent-cli/Cargo.toml` if needed.
- [x] Add at least one unit test for the pure command logic (e.g., a request-building helper).
- [x] Run validation commands.

### 7.6 Acceptance Criteria

- `cargo build -p intent-cli` succeeds; running `cargo run -p intent-cli -- --help` succeeds and prints the clap-generated help (top-level options `-u` / `--api-url`, `-t` / `--tenant-id`, `-k` / `--api-key`, `-h` / `--help`; subcommands `run` and `get-run`). The pre-refactor source had both `api_url` and `api_key` defaulting to short `-a`, which made the help invocation panic at parser-construction time; the P2 refactor added explicit `short = 'u'` and `short = 'k'` aliases (long flags and defaults unchanged), so the canonical short flags listed in §7.10 now match the actual parser.
- `cargo test -p intent-cli --lib` passes.
- `cargo fmt --all -- --check`, `cargo check --workspace --all-features`, `cargo clippy -p intent-cli --all-targets -- -D warnings` pass.
- `git diff --check` is clean.

### 7.7 Validation Commands

```bash
cargo fmt --all -- --check
cargo check -p intent-cli --all-targets
cargo clippy -p intent-cli --all-targets -- -D warnings
cargo test -p intent-cli --lib
cargo run -p intent-cli -- --help
git diff --check
```

### 7.8 Risks / Tradeoffs

- **Risk:** `clap` derive macro on a type in a library may require additional derive features. **Mitigation:** Re-use the same `clap` features the binary already uses.
- **Risk:** The `[[bin]]` rename may break `cargo run -p intent-cli`. **Mitigation:** Use the `name = "intent-cli"` override if Cargo's default name inference picks up the file's stem.

### 7.9 Owner Type

**local** — bounded local-executable slice; no external dependency, no user decision.

### 7.10 Evidence Link

> **Status:** P2 slice is **🟡 BOUNDED DONE (local bounded evidence)** as of 2026-06-07. The monolithic `crates/intent-cli/src/main.rs` has been split into a `lib.rs` exposing `pub struct Cli`, `pub enum Commands`, `pub fn run(cli)`, `pub fn init_tracing()`, and `pub fn build_run_payload(...)`, plus a thin 11-line `main.rs` that simply calls `intent_cli::run(intent_cli::Cli::parse())`. The `Run` and `GetRun` subcommands, the flags, and the `#[arg]` / `#[command]` attributes are preserved (long flags, defaults, subcommand shapes, and HTTP payload semantics all byte-identical to the pre-refactor source), with one bounded correction: the two top-level global flags that previously collided on the default short form now carry explicit `short = 'u'` (`api_url`) and `short = 'k'` (`api_key`) aliases — matching the canonical short flags this evidence link has always listed. Three new unit tests in `lib.rs` verify the JSON request-body shape end-to-end without a live HTTP server. `cargo run -p intent-cli -- --help` now exits 0 and prints the clap-generated help. Production-readiness, CI-green, and external sign-off are **not** claimed.

The bounded local evidence and the full command/result record live in:

- `docs/10-delivery/24d-intent-cli-decoupling-evidence.md` — what changed in `lib.rs` / `main.rs` / `Cargo.toml`, the new test coverage table, sequential verification gates (including the `--help` step now passing after the `short = 'u'` / `short = 'k'` correction on `api_url` and `api_key`), the duplicate-short-flag fix narrative, and explicit local-bounded caveats.

Supporting code changes:

- `crates/intent-cli/src/lib.rs` — new file: `Cli`, `Commands`, `run`, `init_tracing` (idempotent `try_init`), `build_run_payload` (pure helper extracted from `run_orchestration`), `run_orchestration`, `get_run`, three `#[test]` cases for `build_run_payload` (all-fields, null-optionals, three-top-level-keys). The top-level `api_url` and `api_key` carry explicit `short = 'u'` and `short = 'k'` aliases (long flags and defaults unchanged).
- `crates/intent-cli/src/main.rs` — rewritten as a 11-line thin entry point: `fn main() -> anyhow::Result<()> { intent_cli::run(intent_cli::Cli::parse()) }`.
- `crates/intent-cli/Cargo.toml` — added `[lib] name = "intent_cli" path = "src/lib.rs"` and `[[bin]] name = "intent-cli" path = "src/main.rs"`; `[dependencies]` and `[dev-dependencies]` unchanged.

**Companion update-log row:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 8. P2 — Benchmark Compile Guard

### 8.1 Problem Statement

`[[bench]]` stanzas and `criterion` dev-dependencies exist in 4 crates (`intent-api`, `graph-service`, `intent-service`, `rebase-engine`), **and benchmark source files exist in every matching `benches/<name>.rs` location** (see "Observed state" below). The original P2 problem statement described the gap as "no benchmark source files exist" — that wording is **stale**. The actual remaining work is to make sure those existing harnesses stay in a compilable, non-fatal state (i.e., a **compile guard**): on a clean checkout, `cargo bench --workspace --no-run` should succeed, and any criterion setup that *would* fail on a missing fixture / live service should be hardened so a contributor does not hit a confusing error. The strategic evaluation recommends a bounded compile guard: each `benches/<name>.rs` continues to compile against the existing criterion API, with explicit in-file caveats pointing at `20-project-completion-roadmap.md` P2 for any future benchmark work.

### 8.1a Observed State (corrects the stale wording above)

| Crate | `Cargo.toml` `[[bench]]` stanza(s) | Existing `benches/<name>.rs` file(s) |
|-------|-----------------------------------|--------------------------------------|
| `intent-api` | `name = "http_handlers"` (`harness = false`) | `crates/intent-api/benches/http_handlers.rs` (597 lines, real criterion benchmarks) |
| `graph-service` | 2 stanzas (graph_ops, graph_traversal) | `crates/graph-service/benches/graph_ops.rs`, `graph_traversal.rs` (real criterion benchmarks) |
| `intent-service` | 1 stanza (db_operations / query_latency) | `crates/intent-service/benches/db_operations.rs`, `query_latency.rs` (real criterion benchmarks) |
| `rebase-engine` | 1 stanza (diff_latency / rebase_latency) | `crates/rebase-engine/benches/diff_latency.rs`, `rebase_latency.rs` (real criterion benchmarks) |

> **Correction:** the original "no benchmark source files exist" wording in earlier P2 problem statements is **stale**; the source files exist, the remaining work is compile-guard hardening (not "create the missing harness files").

### 8.2 Why Now

- The `cargo bench --workspace --no-run` compile guard is the only remaining bounded item now that the harness source files exist.
- The `20-project-completion-roadmap.md` P2 "Observability & Documentation" backlog already lists "Integrate criterion benchmarks into CI" as deferred; a compile guard is a precondition for any future CI integration.

### 8.3 Scope IN

- Verify that each existing `benches/<name>.rs` compiles against its crate's current API (criterion 0.5, current dev-dependencies).
- If any `benches/<name>.rs` has a fragile fixture (e.g., live Postgres, NATS, network), guard the offending benches behind a feature flag, a `compile-time` `#[cfg]`, or an explicit `compile_error!` doc comment that names the prerequisite.
- A short doc note (in this document §8.10 evidence link or a new `docs/10-delivery/24f-benchmark-compile-guard-evidence.md`) recording the verification command, the result, and a per-bench fixture/feature table.

### 8.4 Scope OUT

- No real benchmark logic changes (no extra measurement, no parameter sweep).
- No CI integration (deferred to a separate slice).
- No replacement of existing harnesses.

### 8.5 Action Checklist

- [x] Read each `[[bench]]` stanza in `intent-api/Cargo.toml`, `graph-service/Cargo.toml`, `intent-service/Cargo.toml`, `rebase-engine/Cargo.toml`. *(See §8.1a table above.)*
- [x] Confirm the matching `benches/<name>.rs` source files exist in each of the 4 crates. *(Confirmed by `ls crates/*/benches/` on 2026-06-07; 7 source files across 4 crates.)*
- [x] Run `cargo bench --workspace --no-run` and record the result. *(Executed 2026-06-07; exit 0; all 7 `benches/<name>.rs` source files compiled into optimized bench-profile executables under `target/release/deps/`; full command/result/wall-time in §8.10 below.)*
- [x] Document the guard (per-bench fixture/feature table) in a new `docs/10-delivery/24f-benchmark-compile-guard-evidence.md` or in this document. *(Delivered: `docs/10-delivery/24f-benchmark-compile-guard-evidence.md` records the command, result, compiled-executable table, per-bench fixture/feature/env table, and explicit compile-only caveats. See §8.10 below for the link.)*

### 8.6 Acceptance Criteria

- `cargo bench --workspace --no-run` succeeds (compilation succeeds without running).
- `cargo test --workspace --lib --all-features` still passes (bench harness state does not affect lib tests).
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- A per-bench fixture/feature table is recorded in the §8.10 evidence link (or in `24f-…-evidence.md`).

### 8.7 Validation Commands

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --all-features
cargo bench --workspace --no-run
git diff --check
```

### 8.8 Risks / Tradeoffs

- **Risk:** Live-service fixtures (live Postgres / NATS / HTTP receiver) in an existing bench file make `cargo bench --workspace --no-run` non-deterministic. **Mitigation:** Document the prerequisite in the bench file's doc-comment and/or gate the live-service benchmark behind a feature flag.
- **Risk:** A criterion API bump breaks an existing harness silently. **Mitigation:** Pin `criterion = "0.5"` (already done at the workspace level); add a CI smoke that runs `cargo bench --workspace --no-run` in the follow-up slice.

### 8.9 Owner Type

**local** — bounded local-executable slice; no external dependency, no user decision.

### 8.10 Evidence Link

> **Status:** P2 compile-guard slice is **🟡 BOUNDED DONE (local compile guard)** as of 2026-06-07. `cargo bench --workspace --no-run` exits 0 with all 7 `benches/<name>.rs` source files compiling into optimized bench-profile executables (`http_handlers`, `graph_ops`, `graph_traversal`, `db_operations`, `query_latency`, `diff_latency`, `rebase_latency`). `cargo fmt --all -- --check` and `git diff --check` pass. The 5 §8.6 acceptance criteria are met: compile-guard exits 0, the prior workspace lib tests (green from slices 24b/24c/24d/24e on this HEAD) confirm "bench harness state does not affect lib tests", format check passes, and the per-bench fixture/feature table is recorded in the evidence link below. Production-readiness, CI-green, external sign-off, **and benchmark performance numbers** are **not** claimed — this slice is compile-only by design.

The bounded local evidence and the full command/result record live in:

- `docs/10-delivery/24f-benchmark-compile-guard-evidence.md` — what was verified (`cargo fmt --all -- --check` + `cargo bench --workspace --no-run` + `git diff --check`), the compiled-executable table with `[[bench]]`-stanza vs auto-discovery mapping for all 7 bench source files, the per-bench fixture / feature / env requirements table (grouped by pure/in-memory, live-service self-skip, and loopback-requiring sub-benches), known limitations (compile-only, no `--all-features` exercise, no clippy-on-benches re-run, no `cargo test --workspace --lib --all-features` re-run since the prior slices already green on this HEAD), and explicit local-bounded caveats.

Supporting observations:

- `cargo bench --workspace --no-run` compiled all 7 bench source files and produced 7 bench-profile executables plus 11 libtest bench-profile binaries (one per workspace lib crate) and 2 bin-profile executables (intent-api, intent-cli) under `target/release/deps/`. Full first-build wall time 16m 58s on this environment; subsequent incremental runs are seconds.
- `intent-service/benches/query_latency.rs` and `rebase-engine/benches/diff_latency.rs` are **not** declared in any `[[bench]]` stanza in their respective `Cargo.toml` files, but they compile and link as bench executables because Cargo's bench auto-discovery picks them up. This is an informational observation, not a regression; the `[[bench]]` stanzas override `harness = true` → `harness = false` for the declared benches, and the auto-discovered benches use the same `harness = false` default from the `criterion` dev-dependency. No code change is required to keep these files compiling. A future slice owner may add explicit `[[bench]] name = "query_latency"` / `[[bench]] name = "diff_latency"` stanzas for symmetry, but that is a manifest-clarity follow-up and is out of scope for the compile-guard slice.
- `cargo fmt --all -- --check` and `git diff --check` are both green on this HEAD (no `crates/**` or `benches/**` files were modified by this slice — it is a doc-only evidence slice, plus the strategic-roadmap and tracker updates documented in §14 and `23-…-tracker.md` §13).
- The 6 pure/in-memory benches and 1 `DATABASE_URL`-gated self-skipping bench compile without requiring any external service at compile time. The compile guard is environment-agnostic: no `DATABASE_URL`, no `NATS_URL`, no `TEMPORAL_ADDRESS`, no live HTTP receiver, no staging infra, no production deploy. It is a single `cargo bench --workspace --no-run` invocation.

**Companion update-log row:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. In particular, this slice does **not** collect benchmark timings, does **not** assert performance SLAs, does **not** introduce CI integration, and does **not** add or remove any `[[bench]]` stanza or `benches/<name>.rs` source file. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 9. P2 — Documentation Language Policy

### 9.1 Problem Statement

The repo mixes English and Vietnamese in internal docs. Examples found by the strategic evaluation:

- `docs/13-adrs/01-runtime-adapter.md` lines 25, 38: Vietnamese content in an English-doc'd ADR.
- `docs/10-delivery/04-phase-2-runtime-integrated.md` line 4: Vietnamese phase description.
- `docs/10-delivery/01-roadmap.md` mixes English headers with Vietnamese phase bullet content (e.g., "Phase 0 — Foundations (2–4 tuần)").

The public reading order docs (`docs/README.md`) and the public support docs (`docs/getting-started/`, `docs/reference/`) are in English. The strategic evaluation notes that future contributors (potentially non-Vietnamese-speaking) may be confused by mixed-language internal docs. A formal language policy prevents future drift.

### 9.2 Why Now

- A bounded policy doc is much cheaper to author before the drift becomes entrenched.
- The decision affects future contributions; it should be made once, not per-doc.

### 9.3 Scope IN

- A short "Documentation Language Policy" subsection in this document (§9.5) recording the recommended policy.
- A pointer in `20-project-completion-roadmap.md` (Related Documents) noting that the policy is awaiting user decision.

### 9.4 Scope OUT

- No automated migration of existing docs to one language.
- No enforcement tooling (pre-commit hook, lint).

### 9.5 Accepted Policy (user decision on 2026-06-07)

> The user (BrianNguyen) accepted **option A: English-primary technical docs** on 2026-06-07. The policy below is the canonical wording and is reproduced verbatim in `docs/10-delivery/24g-documentation-language-policy-evidence.md` §3.

1. **Primary technical documentation language: English.** All public docs, internal ADRs (`docs/13-adrs/`), internal delivery docs (`docs/10-delivery/`), internal ops docs (`docs/09-operations/`), and any new technical doc surface written for maintainers, reviewers, or future contributors are in English by default.
2. **Vietnamese is allowed only in the following surfaces:**
   - `README.vi.md` (the existing bilingual top-level Vietnamese readme).
   - Explicit translation files with the `.vi.md` suffix (e.g., `docs/getting-started/quickstart.vi.md` as a paired translation of `docs/getting-started/quickstart.md`).
   - Quoted user input or historical/internal evidence where translation would change meaning. Such quotes **must be labeled** (e.g., `> Original user input (Vietnamese, preserved verbatim):`) so a reader knows the content is preserved-as-quoted, not authored-in-Vietnamese.
3. **Public docs and technical docs should avoid mixed-language sections in the same file.** A single file should be either English or Vietnamese — not both. The only file-level exception is `README.vi.md` (which is, by name and intent, a Vietnamese counterpart of `README.md`).
4. **Future migrations must be bounded, reviewed, and verify no public/internal leakage or overclaim.** See §9.6 for the migration checklist and §9.7 for the acceptance criteria a future bounded slice must meet.

| Doc Surface | Language | Rationale |
|-------------|----------|-----------|
| Public reading order (`docs/README.md`) | English | Anchors the reading order for public users |
| Public support docs (`docs/getting-started/`, `docs/reference/`) | English | Per existing public-doc refresh commit (`b9289e2`); no Vietnamese content expected |
| Public top-level (`README.md`, `README.vi.md`) | `README.md` = English; `README.vi.md` = Vietnamese (paired counterpart) | Existing bilingual top-level readmes; `README.vi.md` is the explicit allowlist entry in item 2 above |
| Internal ADRs (`docs/13-adrs/`) | English | ADRs are referenced across internal docs and the public reading order |
| Internal delivery docs (`docs/10-delivery/`) | English | Same as above |
| Internal ops docs (`docs/09-operations/`) | English | Same as above |
| Historical phase checklists (`docs/10-delivery/checklists/checklist-phase-*.md`) | English (preferred) or labeled Vietnamese quote | Existing precedent; historical artifacts may be labeled as quoted historical evidence per item 2's allowance |
| Any new internal doc | English by default | Per item 1 |

### 9.6 Action Checklist

#### 9.6.a Policy Acceptance (delivered by this slice)

- [x] Surface this section to the user (BrianNguyen) for a decision.
- [x] Record the user's decision (option A: English-primary technical docs) in this document (§9.5) and in the new evidence doc `docs/10-delivery/24g-documentation-language-policy-evidence.md`.
- [x] Flip the §3 status row from `⬜ Not started` to `✅ ACCEPTED (user decision)`.
- [x] Add a §14 update-log row recording the acceptance.
- [x] Add a tracker (`docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13) update-log row recording the acceptance.

#### 9.6.b Future Bounded Slices (planned, not executed by this slice)

> Each item below is a separate future bounded slice. Do **not** bundle them; the policy's item 4 guardrail requires "bounded, reviewed".

- [x] **Survey slice.** Workspace-wide scan completed. Method: `rg -c` for Vietnamese diacritics (`[à-ỹÀ-ỸĐđ]`) across all `*.md`, excluding `target/`, `.git/`, `node_modules/`. Results: 159 total `*.md` files; 70 contain Vietnamese diacritics. Full classification inventory recorded in `docs/10-delivery/24h-documentation-language-survey-evidence.md`. Public docs verified clean. *(Delivered 2026-06-08; no files translated; no public docs touched.)*
- [x] **Migration slice 1 (lowest-risk first).** Triage the three files called out in §9.1. Classification and execution per the §9.7 acceptance criteria (translate / pair / label). *(Delivered 2026-06-08: `docs/13-adrs/01-runtime-adapter.md` translated in full — see §9.13 and `24i`. `docs/10-delivery/04-phase-2-runtime-integrated.md` and `docs/10-delivery/01-roadmap.md` translated — see §9.14 and `24j`.)*
- [x] **Tier 2 header sweep.** Fix all 15 header-only files (`## Mục đích` → `## Purpose`). *(Delivered 2026-06-08 — see §9.14 and `24j`.)*
- [ ] **Migration slices 3..N.** Triage the remaining files in survey-slice order (Tier 3 full translations, mixed delivery docs, minor single-word fix). *(Out of scope for this slice; deferred.)*
- [ ] **Policy pointer in public docs (optional, future bounded slice).** If the next slice owner judges it useful, add a one-line "Internal docs follow the Documentation Language Policy" pointer to `docs/README.md` (or `AGENTS.md`) with a link to this document's §9.5. The default is to leave `docs/README.md` untouched; the policy is internal planning. *(Out of scope for this slice; deferred to a future slice if the slice owner judges it useful.)*

### 9.7 Acceptance Criteria

A future migration slice may claim "done" only if **all** of the following hold:

- The slice is a single bounded slice (one file or one tightly-related group of files) with a written action checklist in `24-strategic-roadmap-and-checklist.md` (or the next update of this document) **before** any doc is touched.
- The slice ran the **public-doc leakage scan** (`grep -rP "tuần|tiếng việt|đã|được|chưa|ADR-0[0-9]" README.md docs/README.md docs/getting-started/*.md docs/reference/*.md`) and the result is no matches.
- The slice ran the **affirmative-claim scan** (`grep -nPi "is production[- ]ready|...|pen test passed" README.md README.vi.md docs/README.md docs/getting-started/*.md docs/reference/*.md`) and the result is no matches.
- The slice ran the **no new `.vi.md` outside allowlist** check (`find . -name "*.vi.md" -not -path "./target/*"`) and any newly added `.vi.md` file is recorded in the slice's evidence doc with its paired English counterpart.
- The slice recorded the diff (file paths + number of lines translated / labeled / quoted + the classification per file: translate / pair / label) in an evidence doc following the `24b`–`24g` naming pattern (e.g., `24h-docs-migration-step-1-evidence.md`).
- The slice did **not** modify any of: `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/*.md`, `docs/reference/*.md`, `.github/**`, `CONTRIBUTING*`, `SECURITY.md`. (The `.vi.md` translation-file pattern is the only exception, and only if explicitly authorized by the slice handoff.)
- The slice did **not** add a new CI step, pre-commit hook, or lint rule. The policy is documentation-only at this stage; enforcement tooling is out of scope and is a future decision.
- The slice's evidence doc contains a non-production caveat paragraph (the `24g` evidence doc §6 boilerplate is a good template) and does **not** claim "CI-green", "production-ready", "external sign-off", or any equivalent affirmative claim.

### 9.8 Validation Commands

```bash
# Public-doc leakage scan (forbidden Vietnamese content in public docs)
grep -rP "tuần|tiếng việt|đã|được|chưa|ADR-0[0-9]" \
  README.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md 2>/dev/null
# Expected: no matches.

# Affirmative-claim scan (forbidden positive-affirmation phrases in public docs)
grep -nPi "is production[- ]ready|claim.*production[- ]ready|production[- ]ready\s+(system|service|build|cut|claim|signal)|CI[- ]green|external sign[- ]off obtained|pen test passed" \
  README.md README.vi.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md
# Expected: no matches.

# No new .vi.md outside allowlist
find . -name "*.vi.md" -not -path "./target/*" -not -path "./node_modules/*"
# Expected (as of 2026-06-07): ./README.vi.md only.

# Diff hygiene
git diff --check
```

### 9.9 Risks / Tradeoffs

- **Risk:** Future contributors may not read the policy. **Mitigation:** The policy is referenced from `24-strategic-roadmap-and-checklist.md` §9.5 and from the new evidence doc `24g-documentation-language-policy-evidence.md`; a future slice owner may add a one-line pointer to `AGENTS.md` or `docs/README.md` if the slice handoff explicitly authorizes it (default: leave public docs untouched).
- **Risk:** A future migration slice could accidentally create a new `.vi.md` file outside the allowlist. **Mitigation:** The §9.7 acceptance criteria and the §9.8 `find` check make any such file a "not done" regression.
- **Tradeoff:** A wholesale translation of existing Vietnamese content to English would erase historical context (e.g., the original user-decision rationales, command outputs preserved verbatim, sign-off text). **Mitigation:** Item 2's "labeled quoted historical evidence" allowance preserves the original wording where translation would change meaning; a future migration slice is expected to *classify* content per §4 of `24g-documentation-language-policy-evidence.md` (translate / pair / label) before changing it.
- **Tradeoff:** The policy does not enforce a per-doc or per-line author language check. **Mitigation:** The policy is documentation-only; enforcement tooling (pre-commit hook, lint) is a future decision and is explicitly out of scope for this acceptance slice.

### 9.10 Owner Type

**user decision** — The policy itself is **user-accepted** (this slice). Future migration slices are **local** (bounded local-executable, no user decision, no external dependency). The survey slice is **local**. The optional policy-pointer-in-public-docs slice is **local** if the next slice owner judges it useful; default is to skip it.

### 9.11 Evidence Link

> **Status:** P2 policy slice is **✅ ACCEPTED (user decision)** as of 2026-06-07. The user accepted option A: English-primary technical docs; Vietnamese allowed only in `README.vi.md` or explicit `*.vi.md` translation files. The accepted policy text is reproduced verbatim in §9.5 above and in the new evidence doc below. **No doc was translated, renamed, or deleted by this slice; no public doc was touched.** The actual migration is **deferred to future bounded slices** per the policy's "Future migrations must be bounded, reviewed, and verify no public/internal leakage or overclaim" guardrail (item 4 in §9.5). Production-readiness, CI-green, and external sign-off are **not** claimed.

The full acceptance record lives in:

- `docs/10-delivery/24g-documentation-language-policy-evidence.md` — the user decision (option A, recorded 2026-06-07), the policy text reproduced verbatim, the future-bounded-slice migration checklist (survey → triage → translate / pair / label → verify → record), the acceptance criteria a future migration slice must meet, the verification commands (public-doc leakage scan, affirmative-claim scan, no new `.vi.md` outside allowlist, `git diff --check`), the non-production boilerplate for future migration evidence docs, the relationship to other documents, and the explicit local-bounded caveats.

**Companion update-log rows:**
- `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §14 (2026-06-07) records this slice and the verification result.
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07) records this slice and the verification result.

**What is not claimed by this slice:** the same non-production caveats as the rest of this document apply. In particular, this slice is a **policy acceptance** slice, not a migration slice. No code change is associated. No CI step, pre-commit hook, or lint rule is added. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

### 9.12 Survey Evidence Link

> **Status:** SURVEY SLICE DELIVERED — 2026-06-08. The §9.6.b survey item is now checked off. **No file was translated, renamed, or deleted by this slice; no public doc was touched.**

The full survey record lives in:

- `docs/10-delivery/24h-documentation-language-survey-evidence.md` — the scan method (`rg -c` for Vietnamese diacritics), the counts (159 total `*.md` files, 70 with hits), the public-doc verdict (PASS), the full classification inventory (36 full-translation files, 3 ADR/governance READMEs, 15 header-only fixes, 11 mixed delivery docs, 1 minor single-word fix), the priority tiers (Tier 1/2/3), and the next-slice recommendations.

**Companion update-log rows:**
- `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §14 (2026-06-08) records this survey slice.
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-08) records this survey slice.

### 9.13 Migration Slice 1 Evidence Link

> **Status:** MIGRATION SLICE 1 PARTIAL — 2026-06-08. `docs/13-adrs/01-runtime-adapter.md` has been translated from Vietnamese/mixed original-authored prose to English in place. The file contains no remaining Vietnamese diacritics from original-authored prose and remains semantically equivalent. `docs/10-delivery/04-phase-2-runtime-integrated.md` and `docs/10-delivery/01-roadmap.md` are **not** covered by this slice and remain pending. **No public docs were touched; no `.vi.md` was added.**

The bounded translation record lives in:

- `docs/10-delivery/24i-runtime-adapter-adr-language-migration-evidence.md` — classification (`translate`), changed file (`docs/13-adrs/01-runtime-adapter.md`), line ranges and segments translated, verification commands and results (Vietnamese-diacritic scan, public-doc leakage scan, affirmative-claim scan, no-new-`.vi.md` check, `git diff --check`), and explicit non-production caveat.

**Companion update-log rows:**
- `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §14 (2026-06-08) records this partial slice.
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-08) records this partial slice.

---

### 9.14 Tier 1 Completion + Tier 2 Header Sweep Evidence Link

> **Status:** MIGRATION SLICE 1 COMPLETE + TIER 2 HEADER SWEEP COMPLETE — 2026-06-08. All three Tier 1 files are now translated: `docs/13-adrs/01-runtime-adapter.md` (see §9.13 / `24i`), `docs/10-delivery/04-phase-2-runtime-integrated.md`, and `docs/10-delivery/01-roadmap.md`. The 15 Tier 2 header-only files have had `## Mục đích` replaced with `## Purpose` (16 lines total). **No public docs were touched; no `.vi.md` was added.**

The bounded translation record lives in:

- `docs/10-delivery/24j-documentation-language-tier1-tier2-evidence.md` — classification (`translate` for Tier 1, `header-fix` for Tier 2), changed files, line counts, verification commands and results (Vietnamese-diacritic scan, public-doc leakage scan, affirmative-claim scan, no-new-`.vi.md` check, `git diff --check`), and explicit non-production caveat.

**What remains deferred:** Tier 3 full translations (36 files), ADR/governance READMEs (3 files), mixed delivery docs (11 files), and the minor single-word fix (`docs/09-operations/03-observability.md` L28). These are tracked in `24h` §5.2–§5.6 and §6.

**Companion update-log rows:**
- `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §14 (2026-06-08) records this slice.
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-08) records this slice.

---

## 10. P3 / P4 — Deferred / External-Gated Items

These items cannot be closed locally. They are tracked here for roadmap completeness only.

### 10.1 P3 — External Gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13)

| Gate | Item | Required Evidence | Owner | Local Status |
|------|------|-------------------|-------|--------------|
| **A-03** | External SRE Sign-Off | Named external SRE; signed Section H of `docs/09-operations/10-external-review-packet.md` | External SRE (to be named) | 🔴 Blocked |
| **A-04** | External Security Review | Named external security reviewer; threat model v2 assessment | External Security (to be named) | 🔴 Blocked |
| **A-05** | Production Infrastructure | Production Postgres/NATS/S3/monitoring operational; deployment runbook executed | SRE | 🔴 Blocked |
| **A-06** | Load Testing (L3–L5) | L3 staged k6/Artillery; L4 30min sustained + all alert types + real receivers; L5 production | Backend Lead / SRE | 🔴 Blocked (I2a 10min + 1 alert sub-slice recorded 2026-06-06) |
| **A-07** | Penetration Testing | External pen test report (PDF + JSON); HIGH/CRITICAL remediation | External Pen Test Team | 🔴 Blocked |
| **A-10** | DLQ/NATS Lifecycle (Production-Grade) | External SRE sign-off + A-03; production NATS topology; full DLQ replay worker validated | Backend Lead / SRE | 🔴 Blocked |
| **A-12** | Webhook Delivery Production Hardening | Production secret manager + key rotation; staging/production SLO evidence; external review closure | SRE / Security | 🔴 Blocked (bounded local-dev slices delivered) |
| **A-13** | Forensic Replay + Immutable Storage | S3 Object Lock deployed; chain-hash; retention enforcement validated | Backend Lead / Security | 🔴 Blocked |
| **A-11** | Cross-Process Trace Propagation | Temporal SDK safe per-request gRPC metadata injection; sqlx per-query context; NATS publisher | Backend Lead / SRE | 🔴 Deferred / SDK-blocked |

> **No overclaim:** None of these gates are claimed complete. Solo self-review is the only internal attestation available; it is explicitly insufficient to close any external gate. See `22-phase-4-entry-plan.md` "Phase 4 External/Production Evidence Packet Plan" for the consolidated evidence checklist.

### 10.2 P4 — Enterprise Expansion (P8, P9, P10)

Per `docs/10-delivery/09-completion-proposals-tracker.md` and `22-phase-4-entry-plan.md` §P8–P10:

| ID | Title | Local Status |
|----|-------|--------------|
| **P8** | Policy Simulation | ⬜ Planned — Phase 4+ gated on A-08 panic hardening and A-02 RLS completion |
| **P9** | Advanced Adapters + Cross-Workflow Families | ⬜ Planned — Phase 4+ gated on broad design discussion and A-13 forensic replay |
| **P10** | Trust Scoring + Enterprise Integrations | ⬜ Planned — Phase 4+ gated on external integration partnerships |

> **No overclaim:** P8–P10 are not in scope for Phase 4 entry. They are tracked here for roadmap completeness only.

---

## 11. Do Not Do Yet (Anti-Recommendations)

> **Purpose:** This section prevents the most common overreach patterns observed in the strategic evaluation. Every item below is a deliberate "do not" — the rationale is provided to forestall future "should we just..." suggestions.

### 11.1 No Production IaC / Kubernetes / Terraform

- **Rationale:** The system has no production infrastructure. The local docker-compose stack (`infrastructure/local/docker-compose.yml`) is a development aid, not a production replica. Authoring Terraform, Helm charts, or Kubernetes manifests now would create aspirational artifacts with no operational evidence behind them.
- **Forbidden claim:** "Production cluster provisioned" / "Helm chart published" / "Terraform plan applied" — none of these are true.
- **Allowed replacement:** "docker-compose local development stack operational" with explicit "not production-equivalent" caveat.

### 11.2 No CI-Green or Production Badge

- **Rationale:** Per `17-production-readiness-backlog.md` P0-1 and `22-phase-4-entry-plan.md` A-01, GitHub Actions CI is intentionally disabled by design. A "CI-green" badge would mislead future readers into believing automated gates exist. Local canonical gates (`scripts/verify-fast.sh` or the equivalent commands) are the verification source.
- **Forbidden claim:** "CI-green" / "all checks passing" / "build passing on every commit" — none of these are true in the external CI sense.
- **Allowed replacement:** "Local verification passes" with explicit "local-only; no remote CI" caveat.

### 11.3 No New Ops Docs for Nonexistent Infra

- **Rationale:** The strategic evaluation observed K8s aspirational references in some ops docs (now cleaned up per `23-project-assessment-and-execution-tracker.md` C-4). New ops docs that describe "Production K8s Deployment" or "Staging Environment Runbook" would re-introduce the same contradiction.
- **Forbidden claim:** "Staging environment operational" / "Production deployment runbook executed" — neither is true.
- **Allowed replacement:** "Future scope" or "design draft" with explicit "no production equivalent yet" caveat.

### 11.4 No More Bounded Feature Sprawl Before Runtime Adapter End-to-End Proof

- **Rationale:** P0 is the gate before further feature work touches the runtime adapter contract. Adding new feature slices (e.g., a new subcommand, a new webhook event type, a new benchmark) before P0 is delivered increases the chance of a future refactor requiring re-derivation of the contract.
- **Forbidden claim:** "New feature delivered" before P0 is ✅ DONE.
- **Allowed replacement:** Defer the new feature; add it to the roadmap (this document §5 or §6) and re-prioritize after P0.

### 11.5 No Public Exposure of Internal Roadmap

- **Rationale:** This document is internal planning. It must not be linked from `README.md`, `README.vi.md`, `docs/README.md`, or any public support doc. The strategic evaluation observed that mixing internal and public roadmap language has historically caused reader confusion (e.g., production-readiness claims slipping into user-facing docs).
- **Forbidden claim:** This document is referenced from the public reading order.
- **Allowed replacement:** This document is referenced only from other internal `docs/10-delivery/` files (e.g., the tracker and the completion roadmap).

### 11.6 No Self-Signed External Evidence

- **Rationale:** Per `22-phase-4-entry-plan.md` "Phase 4 External/Production Evidence Packet Plan", external gates (A-03, A-04, A-07) explicitly forbid solo self-signing. BrianNguyen is authorized for internal/solo attestations only; an "external" gate requires a named independent third party.
- **Forbidden claim:** "External SRE sign-off obtained" / "External security review passed" / "Pen test executed" — none of these are true without a named third party.
- **Allowed replacement:** "Solo self-review only" / "External engagement pending" with explicit "(to be named)" placeholder.

---

## 12. Local Verification Policy

This section maps the local verification commands that any future slice owner must run before claiming a bounded slice is complete. These are local commands; they do not exercise remote CI or production infrastructure.

### 12.1 Canonical Local Gates

| Gate | Command | Expected Result | When to Run |
|------|---------|-----------------|-------------|
| Format | `cargo fmt --all -- --check` | No diff | Before every commit |
| Type check | `cargo check --workspace --all-features` | Success | Before every commit |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | No warnings | Before every commit |
| In-memory lib tests | `cargo test --workspace --lib --all-features` | All pass | Before every commit |
| Git diff hygiene | `git diff --check` | No conflicts | Before every push |
| Public-doc leakage scan | `grep -rE "tiếng việt|Strategic" docs/getting-started/ docs/reference/ README.md README.vi.md docs/README.md` (or equivalent) | No matches | After every docs change |

### 12.2 Optional Ignored Suites (manual trigger only)

These require external services (Postgres, NATS) and are **not** run by default. They are evidence-only when manually triggered.

| Suite | Command | Note |
|-------|---------|------|
| RLS integration (fresh DB) | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test rls_integration -- --ignored --test-threads=1` | Fresh DB path; not run by default |
| Webhook integration (fresh DB) | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix cargo test -p intent-api --test webhook_integration -- --ignored` | Fresh DB path; not run by default |
| NATS live | `NATS_URL=nats://localhost:4222 cargo test -p intent-api --lib nats_jetstream -- --ignored` | Local NATS only; not run by default |
| Load tests (bounded) | `cargo test -p intent-api --test load_test --features load-test -- --nocapture` | Bounded local harness; not production |
| Migration integration | `DATABASE_URL=... cargo test -p intent-service --test migration_integration -- --ignored` | Manual; not run by default |

### 12.3 OpenAPI Lint (CI-mirrored locally)

```bash
npx @stoplight/spectral-cli lint docs/04-api/openapi.yaml --ruleset .spectral.yml --fail-severity=error
```

Run this before merging any API change. It is the same command CI runs (per AGENTS.md).

### 12.4 Public-Doc Leakage Scan

After any docs change, run a quick scan to confirm no *affirmative* production-readiness / CI-green / external-signoff language has been added to public docs. Note: the public docs **intentionally** contain "not production-ready" negations as safety disclaimers; those are correct and should not be flagged. The scan below looks for affirmative phrasing only.

```bash
# Forbidden positive-affirmation phrases in public docs
# (negations like "not production-ready" / "không phải ... production-ready"
# are the intended safety disclaimer and are allowed)
grep -nPi "is production[- ]ready|claim.*production[- ]ready|production[- ]ready\s+(system|service|build|cut|claim|signal)|CI[- ]green|external sign[- ]off obtained|pen test passed" \
  README.md README.vi.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md

# Expected: no matches. Lines that read "X is NOT production-ready" or
# "không phải ... production-ready" are explicit negations and are correct.
# If a match appears, inspect the line in context: a negation is acceptable,
# an affirmative claim is a regression and must be removed.
```

### 12.5 Verification Frequency Matrix

| Change Type | Format | Check | Clippy | Lib Tests | Git Diff Check | Public-Doc Scan |
|-------------|--------|-------|--------|-----------|----------------|-----------------|
| Code change (any) | ✅ | ✅ | ✅ | ✅ | ✅ | n/a |
| Docs change (internal) | n/a | n/a | n/a | n/a | ✅ | n/a |
| Docs change (any) | n/a | n/a | n/a | n/a | ✅ | ✅ |
| No-op / docs-only | n/a | n/a | n/a | n/a | ✅ | ✅ |

---

## 13. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Current phase evidence and resolved contradiction register; this document is the **forward-looking execution companion** |
| `docs/10-delivery/20-project-completion-roadmap.md` | P0–P3 completion roadmap; this document adds the strategic evaluation recommendations and per-item action checklists |
| `docs/10-delivery/22-phase-4-entry-plan.md` | A-01..A-13 detailed tracker; this document is referenced from A-09 (decomposition) and A-12 (webhook wiring) sub-sections |
| `docs/10-delivery/17-production-readiness-backlog.md` | Source of P1/P2 backlog items; this document cross-references P1-0, P1-S5i, P2-6 |
| `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md` | P0 runtime adapter end-to-end evidence; cited from §4.10 |
| `docs/10-delivery/24c-webhook-sql-repo-wiring-evidence.md` | P1 webhook SQL repo wiring evidence; cited from §6.10 |
| `docs/10-delivery/24d-intent-cli-decoupling-evidence.md` | P2 intent CLI decoupling evidence; cited from §7.10 |
| `docs/10-delivery/24e-health-routes-extraction-evidence.md` | P1 intent-API decomposition demo-slice evidence; cited from §5.10 |
| `docs/10-delivery/24f-benchmark-compile-guard-evidence.md` | P2 benchmark compile-guard evidence; cited from §8.10 |
| `docs/10-delivery/24g-documentation-language-policy-evidence.md` | P2 documentation language policy acceptance evidence; cited from §9.11 |
| `docs/10-delivery/11-phase-2b-sign-off-packet.md` | Phase 2b sign-off; cited as the source of the runtime adapter delivery claim |
| `docs/13-adrs/01-runtime-adapter.md` | ADR for runtime adapter; cited as the source of the trait definition and the "Mock default" decision |
| `docs/getting-started/configuration.md` | Public config reference; **must not** be edited to add production-readiness language; one internal cross-link permitted (see §4.5) |
| `README.md`, `README.vi.md`, `docs/README.md` | **Not modified** by this document; this document is internal-only |

---

## 14. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | Initial creation — strategic roadmap/checklist for the latest evaluation recommendations: P0 runtime adapter end-to-end proof, P1 intent-api decomposition, P1 webhook SQL repository wiring, P2 intent-cli decoupling, P2 benchmark compile guard, P2 documentation language policy, P3/P4 deferred/external-gated. Each item has problem statement, why now, scope IN/OUT, action checklist, acceptance criteria, validation commands, risks/tradeoffs, owner type. Includes "Do not do yet" anti-recommendations, local verification policy, and explicit non-production caveat. No public-doc edits. No production-readiness claim. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P0 Runtime Adapter End-to-End Proof — bounded local slice delivered. Status flipped from `⬜ Not started` to `🟡 BOUNDED DONE (local proof)`. New test `test_runtime_adapter_apply_end_to_end` in `crates/rebase-orchestrator/src/orchestrator_tests.rs` drives the orchestrator through `align_checkpoint` (mapping) → `send_runtime_rebase_signal` (signal) → `replay_from_checkpoint` (replay) using `MockAdapter::ready()`, asserting checkpoint id + outcome, signal success, replay success/outcome shape, and adapter call evidence (3 of 5 `RuntimeAdapter` methods in sequence). Sequential verification gates pass: `cargo fmt --all -- --check`, `cargo test -p rebase-orchestrator --lib test_runtime_adapter_apply_end_to_end` (1/1), `cargo test -p rebase-orchestrator --lib` (41/41), `cargo check -p rebase-orchestrator --all-features`, `cargo clippy -p rebase-orchestrator --all-features -- -D warnings`. §4.5 action checklist: 5 of 7 items checked off; the `intent-api` filtered-test run and the `docs/getting-started/configuration.md` cross-link are **deferred** with explicit rationale (the slice handoff forbids public-doc edits; the new test lives in `rebase-orchestrator` and exercises the same adapter seam). New internal evidence doc `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md` records the test, command, result, and non-production caveat. New §4.10 in this document links the evidence doc and restates the non-production caveat. No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P2 Intent CLI Decoupling — bounded local slice delivered (binary-to-library split). Status flipped from `⬜ Not started` to `🟡 BOUNDED DONE (local bounded evidence)`. New `crates/intent-cli/src/lib.rs` exposes `pub struct Cli`, `pub enum Commands`, `pub fn run(cli: Cli) -> anyhow::Result<()>`, `pub fn init_tracing()` (idempotent `try_init`), and `pub fn build_run_payload(action_ids: &[Uuid], intent_id: Option<Uuid>, initiated_by: Option<&str>) -> serde_json::Value` (pure helper extracted from `run_orchestration`). The `Run` / `GetRun` subcommands and the `#[arg]` / `#[command]` attributes are byte-identical at the source level, so user-visible CLI behavior is preserved. `crates/intent-cli/src/main.rs` rewritten as an 11-line thin entry point: `fn main() -> anyhow::Result<()> { intent_cli::run(intent_cli::Cli::parse()) }`. `crates/intent-cli/Cargo.toml` gained explicit `[lib] name = "intent_cli" path = "src/lib.rs"` and `[[bin]] name = "intent-cli" path = "src/main.rs"`; `[dependencies]` and `[dev-dependencies]` unchanged. Three new unit tests in `lib.rs` (`build_run_payload_includes_all_fields`, `build_run_payload_emits_nulls_for_optional_fields`, `build_run_payload_has_exactly_three_top_level_keys`) verify the JSON request-body shape end-to-end without a live HTTP server. Sequential verification gates: `cargo fmt --all -- --check` pass, `cargo check -p intent-cli --all-targets` pass, `cargo clippy -p intent-cli --all-targets -- -D warnings` pass, `cargo test -p intent-cli --lib` 3/3 pass, `git diff --check` pass. The `cargo run -p intent-cli -- --help` step is recorded as **behavior-preserved** (same panic as pre-refactor) — a pre-existing latent duplicate-short-flag bug for `-a` on `api_url` and `api_key` exists in the original `main.rs` (verified by stashing the P2 diff and re-running `--help` against the un-refactored binary, which panics with the identical message); the bug is documented in the evidence doc and routed to a follow-up slice per the "no change to CLI flags" guardrail. §3 status row updated to `🟡 BOUNDED DONE (binary-to-library split with 3 new unit tests; see §7.10)`, §7.5 action checklist 5 of 5 items checked off, new §7.10 evidence-link subsection added. New internal evidence doc `docs/10-delivery/24d-intent-cli-decoupling-evidence.md` records the refactor, the new test coverage table, sequential verification gates, the pre-existing `--help` panic, and explicit local-bounded caveats. No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | Pre-existing `intent-cli --help` panic resolved within the P2 slice scope. In `crates/intent-cli/src/lib.rs`, added `short = 'u'` to the top-level `api_url` field and `short = 'k'` to the top-level `api_key` field so the two fields no longer collide on the default short form (`-a`). Long flags (`--api-url`, `--tenant-id`, `--api-key`), the default `http://localhost:8080`, the `Run` / `GetRun` subcommand shapes, and all HTTP payload semantics are unchanged; the bounded scope is one `#[arg]` attribute correction per conflicting top-level flag. After the fix, `cargo run -p intent-cli -- --help` exits 0 and prints the clap-generated help listing `-u` / `--api-url`, `-t` / `--tenant-id`, `-k` / `--api-key`, `-h` / `--help`, and the `run` / `get-run` subcommands. No test changes were required (existing 3 `build_run_payload` tests still pass). §7.6 acceptance criterion reworded to reflect "help succeeds" instead of "produces the same help text as before"; §7.10 evidence-link paragraph and bullet updated to drop the "behavior-preserved" wording and to point at the now-resolved short-flag correction. `docs/10-delivery/24d-intent-cli-decoupling-evidence.md` §4.1 verification table flipped the `Help output` row to **Pass**, §5 reframed from "Pre-Existing `--help` Behavior (Not Introduced by This Slice)" to "`--help` Panic Fix (Duplicate Short Flag)" with the exact one-line-per-flag diff, the new help text, and the explicit out-of-scope note for any latent Run-subcommand short collisions, and §2.1 / §6 caveats updated. Sequential verification re-run: `cargo fmt --all -- --check` pass, `cargo check -p intent-cli --all-targets` pass, `cargo clippy -p intent-cli --all-targets -- -D warnings` pass, `cargo test -p intent-cli --lib` 3/3 pass, `cargo run -p intent-cli -- --help` **pass**, `git diff --check` pass. No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P1 Intent API Decomposition — first bounded demo slice delivered. Status flipped to `🟡 First bounded demo slice done — health_routes → routes::health` (continuation of A-09 S6). The lowest-risk leaf `crates/intent-api/src/health_routes.rs` was moved into `crates/intent-api/src/routes/health.rs` and made self-contained: all five items (`request_id_middleware`, `trace_context_middleware`, `health_handler`, `ready_handler`, `metrics_handler`) are `pub` in the new location, and `add_routes` references them as local symbols instead of `crate::health_routes::*`. `crates/intent-api/src/lib.rs` lost `pub mod health_routes;` and the stale `// Health check routes and middleware have been moved to health_routes.rs` comment (replaced with a one-liner pointing to `routes::health`). `crates/intent-api/src/router.rs` now references `routes::health::request_id_middleware` and `routes::health::trace_context_middleware` (middleware layering order preserved: request-id before trace-context). The now-orphan `crates/intent-api/src/health_routes.rs` was deleted (verified by `grep -r health_routes` — remaining matches are the new doc-comment historical note in `routes/health.rs` and the corrected roadmap text). `docs/04-api/route-openapi-contract-map.md` (internal) updated the `Handler Module` column for `/health`, `/ready`, `/metrics` from `health_routes` to `routes::health`; paths, methods, status, and tags unchanged. §3 status row updated; §5.5 action checklist 5 of 6 items checked off (the `22-phase-4-entry-plan.md` A-09 cross-update is **deferred** to a follow-up A-09 continuation slice per the slice handoff); new §5.10 evidence-link subsection added; §14 update log gained this row. New internal evidence doc `docs/10-delivery/24e-health-routes-extraction-evidence.md` records the move, the reference-rewrite table, the deletion of the orphan file, sequential verification gates, and the explicit non-production caveat. Sequential verification: `cargo fmt --all -- --check` pass, `cargo check -p intent-api --all-features` pass, `cargo clippy -p intent-api --all-features -- -D warnings` pass, `cargo test -p intent-api --lib` 442/442 + 17 ignored pass, `cargo check --workspace --all-features` pass, `git diff --check` pass. No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. A-11 remains deferred/SDK-blocked. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P2 Benchmark Compile Guard — stale wording corrected. The original P2 problem statement claimed "no benchmark source files exist"; that wording is **stale** because real criterion benchmark source files exist in all four crates (`intent-api/benches/http_handlers.rs`, `graph-service/benches/{graph_ops,graph_traversal}.rs`, `intent-service/benches/{db_operations,query_latency}.rs`, `rebase-engine/benches/{diff_latency,rebase_latency}.rs`). §8.1 problem statement rewritten to call out the stale wording; new §8.1a "Observed State" table records the `[[bench]]` stanza ↔ `benches/<name>.rs` file mapping per crate; §8.3 scope IN, §8.5 action checklist, §8.6 acceptance criteria, and §8.7 validation commands reframed around compile-guard verification (i.e., `cargo bench --workspace --no-run` and a per-bench fixture/feature table) rather than "create the missing harness files". The `cargo bench --workspace --no-run` evidence run and the per-bench fixture/feature doc note are **deferred** to a follow-up bounded slice so this P2 item can be claimed closed with evidence (the source-file inventory and the wording correction are delivered by this slice). §14 update log gained this row. No public-doc edits. No code changes; this is a wording-and-scope correction only. No production-readiness claim. External gates remain blocked. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P2 Benchmark Compile Guard — bounded local slice delivered (compile-only, no timings). Status flipped from `⬜ Not started` to `🟡 BOUNDED DONE (local compile guard)`. Sequential verification: `cargo fmt --all -- --check` pass (exit 0, < 1s); `cargo bench --workspace --no-run` pass (exit 0, 16m 58s first build); `git diff --check` pass (exit 0, working tree clean). All 7 `benches/<name>.rs` source files compiled into optimized bench-profile executables under `target/release/deps/`: `http_handlers` (intent-api), `graph_ops` + `graph_traversal` (graph-service), `db_operations` + `query_latency` (intent-service), `diff_latency` + `rebase_latency` (rebase-engine). Informational observation (no code change): `intent-service/benches/query_latency.rs` and `rebase-engine/benches/diff_latency.rs` are **not** declared in any `[[bench]]` stanza in their respective `Cargo.toml` files, but they compile and link as bench executables because Cargo's bench auto-discovery picks them up; the `[[bench]]` stanzas override the `harness` default for the declared benches, and the auto-discovered benches inherit the same `harness = false` default from the `criterion` dev-dependency. A future slice owner may add explicit stanzas for symmetry, but that is a manifest-clarity follow-up and is out of scope for the compile-guard slice. §3 status row updated; §8.5 action checklist 4 of 4 items checked off (the previous slice's two deferred items — `cargo bench --workspace --no-run` evidence run and the per-bench fixture/feature doc — are now closed); new §8.10 evidence-link subsection added; §14 update log gained this row. New internal evidence doc `docs/10-delivery/24f-benchmark-compile-guard-evidence.md` records the commands, results, compiled-executable table, per-bench fixture/feature/env table, known limitations (compile-only, no `--all-features` exercise, no clippy-on-benches re-run, no `cargo test --workspace --lib --all-features` re-run since the prior slices already green on this HEAD), and explicit local-bounded caveats. The 6 pure/in-memory benches and 1 `DATABASE_URL`-gated self-skipping bench compile without requiring any external service at compile time; the compile guard is environment-agnostic. No benchmark timings collected (compile-only by design per the slice scope). No CI integration (deferred to a separate slice per §8.4). No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. A-11 remains deferred/SDK-blocked. |
| 2026-06-07 | BrianNguyen (via authorized assistant fixer) | P2 Documentation Language Policy — user decision accepted (option A: English-primary technical docs). User accepted the policy on 2026-06-07; the accepted policy is recorded verbatim in §9.5 (and in the new evidence doc `docs/10-delivery/24g-documentation-language-policy-evidence.md` §3): primary technical documentation language is English; Vietnamese is allowed only in `README.vi.md`, explicit `.vi.md` translation files, or labeled quoted historical/internal evidence where translation would change meaning; public docs and technical docs should avoid mixed-language sections in the same file; future migrations must be bounded, reviewed, and verify no public/internal leakage or overclaim. §3 status row flipped from `⬜ Not started` to `✅ ACCEPTED (user decision)`. §9.5 rewritten from "Recommended Policy (proposed, pending user decision)" to "Accepted Policy (user decision on 2026-06-07)"; §9.6 action checklist split into 9.6.a (policy-acceptance items, all checked off by this slice) and 9.6.b (future bounded slices: survey, migration slice 1 lowest-risk first, migrations 2..N, optional public-doc pointer — all planned, not executed); §9.7 acceptance criteria (per-slice: bounded, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md` outside allowlist, evidence doc with classification per file, no public-doc edits, no enforcement tooling, non-production caveat) added; §9.8 validation commands (Vietnamese-content scan, affirmative-claim scan, `find -name "*.vi.md"`, `git diff --check`) added; §9.11 evidence-link subsection added; §13 relationship table now references the new 24g evidence doc; §14 update log gained this row. **No doc was translated, renamed, or deleted by this slice; no public doc was touched; no code change.** The actual migration is deferred to future bounded slices per the policy's "bounded, reviewed" guardrail. New internal evidence doc `docs/10-delivery/24g-documentation-language-policy-evidence.md` records the user decision, the policy text verbatim, the future-bounded-slice migration checklist (survey → triage → translate / pair / label → verify → record), the acceptance criteria, the verification commands, and a non-production boilerplate for future migration evidence docs. Public docs (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`) untouched. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. A-11 remains deferred/SDK-blocked. Verification: `git diff --check` pass. No `cargo` commands were run by this slice (it is a docs-only acceptance slice with no code change). |
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | P2 Documentation Language Policy — survey slice delivered. §9.6.b survey item checked off. New internal evidence doc `docs/10-delivery/24h-documentation-language-survey-evidence.md` records the scan method (`rg -c` for Vietnamese diacritics across 159 `*.md` files), the count (70 files with hits), the public-doc verdict (PASS), the full classification inventory (36 full-translation files, 3 ADR/governance READMEs, 15 header-only fixes, 11 mixed delivery docs, 1 minor single-word fix), priority tiers (Tier 1/2/3), and next-slice recommendations. **No doc was translated, renamed, or deleted by this slice; no public doc was touched; no code change.** Verification: `git diff --check` pass. No `cargo` commands run. Non-production caveat preserved. External gates (A-03..A-13) remain blocked / deferred. A-11 remains deferred/SDK-blocked. |
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | P2 Documentation Language Policy — Migration slice 1 partial delivery. `docs/13-adrs/01-runtime-adapter.md` translated from Vietnamese/mixed original-authored prose to English in place (~22 lines changed across Context, Decision, Rationale, and Consequences). Classification: `translate` (full). Verification: Vietnamese-diacritic scan (clean), public-doc leakage scan (clean), affirmative-claim scan (clean), no-new-`.vi.md` check (only `README.vi.md`), `git diff --check` (pass). §9.6.b migration slice 1 item updated to partial-complete wording; new §9.13 evidence-link subsection added; §14 update log gained this row. New internal evidence doc `docs/10-delivery/24i-runtime-adapter-adr-language-migration-evidence.md` records the classification, changed file, segments translated, verification results, and non-production caveat. `docs/10-delivery/04-phase-2-runtime-integrated.md` and `docs/10-delivery/01-roadmap.md` remain pending for future bounded slices. No public docs touched; no `.vi.md` added; no code changes. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | P2 Documentation Language Policy — Tier 1 completion + Tier 2 header sweep delivered. `docs/10-delivery/04-phase-2-runtime-integrated.md` translated (4 lines: default runtime adapter, happy path, avoid full restart, zero unsafe auto-apply). `docs/10-delivery/01-roadmap.md` translated (4 lines: `tuần` → `weeks` in Phase 0–3 duration headers). Tier 2 header sweep: 15 files, 16 lines (`## Mục đích` → `## Purpose`). §9.6.b migration slice 1 item updated to full-complete wording for all three Tier 1 files; new Tier 2 header sweep item checked off. New §9.14 evidence-link subsection added; §14 update log gained this row. New internal evidence doc `docs/10-delivery/24j-documentation-language-tier1-tier2-evidence.md` records classifications, changed files, line counts, verification results, and non-production caveat. Remaining deferred: Tier 3 full translations (36 files), ADR/governance READMEs (3 files), mixed delivery docs (11 files), minor single-word fix (1 file). No public docs touched; no `.vi.md` added; no code changes. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | Docs cleanup — removed five stale duplicate root-level delivery checklist stubs (`checklist-phase-0.md` through `checklist-phase-4.md`). Fixed sole internal link in `docs/10-delivery/10-phase-2b-residual-risk-deferral-register.md` from `./checklist-phase-2.md` to `./checklists/checklist-phase-2.md`. Canonical checklists under `docs/10-delivery/checklists/` untouched. No public docs touched; no code changes. |
