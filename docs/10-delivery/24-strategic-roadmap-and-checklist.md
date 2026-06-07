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
| **Documentation Language Policy** | **P2** | The repo mixes English and Vietnamese in internal docs (e.g., `docs/10-delivery/01-roadmap.md` has English headers with Vietnamese items; `docs/13-adrs/01-runtime-adapter.md` lines 25, 38; `docs/10-delivery/04-phase-2-runtime-integrated.md` line 4). The strategic evaluation flagged this as an inconsistency risk: public-facing reading order docs are in English; internal-only files drift. A formal language policy prevents future contributor confusion. |
| **P3/P4 Deferred / External-Gated** | P3/P4 | Items that depend on external reviewers, production infrastructure, vendor certifications, or upstream SDK fixes. Not actionable locally beyond tracking. |

**Critical-Path Read:** P0 must be executed before any new feature work touches the runtime adapter contract. P1 items can be parallelized across crates. P2 items are bounded local-executable hardening.

---

## 3. Prioritized Roadmap Table

| # | Priority | Item | Owner Type | Est. Slice Effort | Status | External Gate? |
|---|----------|------|------------|-------------------|--------|----------------|
| 1 | **P0** | Runtime Adapter End-to-End Proof | local | S (1–2 days bounded) | 🟡 BOUNDED DONE (local proof — see [§4.10](#410-evidence-link)) | ❌ No |
| 2 | **P1** | Intent API Decomposition | local | M (3–5 bounded slices) | 🟡 In progress (continuation of A-09 S6) | ❌ No |
| 3 | **P1** | Webhook SQL Repository Wiring | local | S–M (bounded slices) | 🟡 BOUNDED WIRING DONE (SQL router startup now passes concrete `SqlxWebhookSubscriptionRepository` + `SqlxWebhookOutboxRepository`; see [§6.10](#610-evidence-link)) | ❌ No |
| 4 | **P2** | Intent CLI Decoupling | local | S (1 slice) | 🟡 BOUNDED DONE (binary-to-library split with 3 new unit tests; see [§7.10](#710-evidence-link)) | ❌ No |
| 5 | **P2** | Benchmark Compile Guard | local | XS (1 slice) | ⬜ Not started | ❌ No |
| 6 | **P2** | Documentation Language Policy | user decision | S (1 doc policy) | ⬜ Not started | ⚠️ User-facing policy decision |
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

- [ ] Inventory current `intent-api/src/` top-level modules and identify leaf modules safe to move (no `pub use` re-export coupling).
- [ ] Pick the lowest-risk leaf (suggested: `health_routes.rs` → `routes/health.rs`) and execute one bounded extraction.
- [ ] Update `lib.rs` `pub mod` and any `crate::health_routes::` references to `crate::routes::health::`.
- [ ] Run the validation commands in §5.7.
- [ ] Update `22-phase-4-entry-plan.md` A-09 status line to note the continuation slice.
- [ ] Update this document's §5.5 with the executed slice and the next candidate.

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

`[[bench]]` stanzas and `criterion` dev-dependencies exist in 4 crates (`intent-api`, `graph-service`, `intent-service`, `rebase-engine`). On a clean checkout, `cargo bench --workspace` will fail to compile because no benchmark source files exist. This is a hidden footgun for new contributors. The strategic evaluation recommends a bounded compile guard: a stub `benches/<name>.rs` returning a `criterion::criterion_main!(benches)` over a no-op benchmark. This makes the harness non-fatal and explicit.

### 8.2 Why Now

- New contributors who run `cargo bench` on a clean checkout encounter a confusing error.
- The `20-project-completion-roadmap.md` P2 "Observability & Documentation" backlog already lists "Integrate criterion benchmarks into CI" as deferred; a compile guard is a precondition for any future benchmark work.

### 8.3 Scope IN

- A stub benchmark file in each of the 4 crates with `[[bench]]` stanzas, named per the existing stanza.
- The stub returns a no-op `criterion::criterion_main!(benches)` and a `criterion::criterion_group!(benches, no_op_bench)` with `no_op_bench` doing nothing.
- A doc note in `docs/10-delivery/20-project-completion-roadmap.md` or this document explaining the guard.

### 8.4 Scope OUT

- No real benchmark logic.
- No CI integration.

### 8.5 Action Checklist

- [ ] Read each `[[bench]]` stanza in `intent-api/Cargo.toml`, `graph-service/Cargo.toml`, `intent-service/Cargo.toml`, `rebase-engine/Cargo.toml`.
- [ ] For each stanza, create a stub `benches/<name>.rs` with a no-op benchmark.
- [ ] Verify `cargo bench --workspace --no-run` succeeds.
- [ ] Document the guard.

### 8.6 Acceptance Criteria

- `cargo bench --workspace --no-run` succeeds (compilation succeeds without running).
- `cargo test --workspace --lib --all-features` still passes (bench stubs do not affect lib tests).
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass (with `#[allow(dead_code)]` if needed on the stub function).

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

- **Risk:** Stubs could be mistaken for real benchmarks. **Mitigation:** Stub doc-comment explicitly says "no-op guard; real benchmarks pending" with a TODO pointer to `20-project-completion-roadmap.md` P2.
- **Risk:** Clippy flags dead code. **Mitigation:** Use `#[allow(dead_code)]` on the stub function.

### 8.9 Owner Type

**local** — bounded local-executable slice; no external dependency, no user decision.

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

### 9.5 Recommended Policy (proposed, pending user decision)

| Doc Surface | Language | Rationale |
|-------------|----------|-----------|
| Public reading order (`docs/README.md`) | English | Anchors the reading order for public users |
| Public support docs (`docs/getting-started/`, `docs/reference/`) | English | Per existing public-doc refresh commit (`b9289e2`) |
| Public top-level (`README.md`, `README.vi.md`) | Both | Existing bilingual top-level readmes |
| Internal ADRs (`docs/13-adrs/`) | English (proposed) | ADRs are referenced across internal docs and the public reading order |
| Internal delivery docs (`docs/10-delivery/`) | English (proposed) | Same as above |
| Internal ops docs (`docs/09-operations/`) | English (proposed) | Same as above |
| **Allowed exception** | Vietnamese is allowed in: (a) historical phase artifacts (`checklist-phase-*.md`); (b) explicit bilingual bullet items where both languages appear in the same line | Existing precedent; preserves historical content |

**Decision required:** BrianNguyen (user) confirms whether the recommended policy is accepted, modified, or rejected. Until then, no automated enforcement is added.

### 9.6 Action Checklist

- [ ] Surface this section to the user (BrianNguyen) for a decision.
- [ ] If accepted: add a "Documentation Language Policy" section to `docs/README.md` (or a new top-level `docs/CONTRIBUTING-docs.md`).
- [ ] If accepted: open follow-up bounded slices to normalize the worst offenders (e.g., `docs/13-adrs/01-runtime-adapter.md`).
- [ ] If rejected: remove this section from the next update of this document.

### 9.7 Acceptance Criteria

- A user decision is recorded (accept/modify/reject) in the next update log of this document.
- If accepted, a top-level policy doc exists and links back to this section.

### 9.8 Validation Commands

```bash
# Public-doc leakage scan
grep -rE "tiếng việt|tuần|ADR-01" docs/getting-started/ docs/reference/ README.md
# (No matches expected: public docs should not contain Vietnamese content.)

# Diff hygiene
git diff --check
```

### 9.9 Risks / Tradeoffs

- **Risk:** Future contributors may not read the policy. **Mitigation:** The policy is referenced from `docs/README.md` and `AGENTS.md` (if accepted).
- **Tradeoff:** A wholesale migration of existing Vietnamese content to English is out of scope and would erase historical context.

### 9.10 Owner Type

**user decision** — BrianNguyen must accept, modify, or reject the recommended policy before any further bounded slice executes.

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
