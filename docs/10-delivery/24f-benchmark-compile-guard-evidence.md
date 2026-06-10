# P2 Benchmark Compile Guard — Evidence

> **Status:** COMPILE GUARD PASSES — 2026-06-07
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** Internal evidence for the P2 Benchmark Compile Guard bounded slice. Records the `cargo bench --workspace --no-run` verification result, the per-bench fixture/feature table, and explicit compile-only caveats.
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, it does not claim CI-green status, and it does not claim external sign-off. No benchmark timings or performance numbers are recorded here; this slice is compile-only by design. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose

The strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §8) called for a bounded P2 Benchmark Compile Guard: on a clean checkout, `cargo bench --workspace --no-run` should succeed, and any criterion setup that *would* fail on a missing fixture / live service should be either compiled out or explicitly documented as a prerequisite in the bench file's doc-comment. The previous slice corrected the §8.1 wording (real criterion source files exist in all 4 crates) and deferred the actual `cargo bench --workspace --no-run` evidence run and the per-bench fixture/feature table to this bounded follow-up slice.

This document closes that follow-up: it records the verification command, the result, a per-bench fixture/feature table (the table called for by the previous slice and by the §8.6 acceptance criteria), and explicit non-production caveats.

---

## 2. What Was Verified

### 2.1 Commands Run (Sequential)

| # | Gate | Command | Result | Wall Time |
|---|------|---------|--------|-----------|
| 1 | Format check | `cargo fmt --all -- --check` | ✅ Pass (exit 0, no diff) | < 1s |
| 2 | Benchmark compile guard | `cargo bench --workspace --no-run` | ✅ Pass (exit 0) | 16m 58s (full first build) |
| 3 | Diff hygiene | `git diff --check` | ✅ Pass (exit 0, working tree clean) | < 1s |

> **Sanity check (already green on this HEAD per prior slices):** `cargo check --workspace --all-features`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --lib --all-features` were not re-run by this slice; the previous P1/P2 slices (24b/24c/24d/24e) re-ran them and the HEAD has not changed since. Re-running them in this slice would be redundant; the bench-profile build is a superset compile in the optimized profile and produced no errors.

### 2.2 Compiled Bench Executables

`cargo bench --workspace --no-run` produced the following bench-profile executables under `target/release/deps/` (per-crate incremental builds produced one or two hashing variants per file; the de-duplicated list follows):

| Crate | Bench file (stanza in `[[bench]]`) | Compiled executable (artifact basename) | Declared in `[[bench]]`? |
|-------|------------------------------------|------------------------------------------|--------------------------|
| `intent-api` | `benches/http_handlers.rs` | `http_handlers-<hash>` | ✅ Yes (`name = "http_handlers"`, `harness = false`) |
| `graph-service` | `benches/graph_ops.rs` | `graph_ops-<hash>` | ✅ Yes (`name = "graph_ops"`, `harness = false`) |
| `graph-service` | `benches/graph_traversal.rs` | `graph_traversal-<hash>` | ✅ Yes (`name = "graph_traversal"`, `harness = false`) |
| `intent-service` | `benches/db_operations.rs` | `db_operations-<hash>` | ✅ Yes (`name = "db_operations"`, `harness = false`) |
| `intent-service` | `benches/query_latency.rs` | `query_latency-<hash>` | ⚠️ **No** (auto-discovered; not listed in `[[bench]]`) |
| `rebase-engine` | `benches/diff_latency.rs` | `diff_latency-<hash>` | ⚠️ **No** (auto-discovered; not listed in `[[bench]]`) |
| `rebase-engine` | `benches/rebase_latency.rs` | `rebase_latency-<hash>` | ✅ Yes (`name = "rebase_latency"`, `harness = false`) |

> **Auto-discovery observation (informational, not a regression):** in this workspace, Cargo's bench auto-discovery appears to be the default — any `.rs` file placed under `crates/<crate>/benches/` is picked up as a bench target. The explicit `[[bench]]` stanzas override the `harness` default (true → false). The two "⚠️ No" rows above (`query_latency.rs`, `diff_latency.rs`) compile and link as bench executables; they are not orphans. The `[[bench]]` table in `24-strategic-roadmap-and-checklist.md` §8.1a accurately describes the stanzas; the only minor correction needed in §8.1a is to make explicit that the stanzas are additive on top of auto-discovery for the two source files not in any `[[bench]]` block. **No code change is required to keep these files compiling.** A follow-up slice owner may add explicit `[[bench]]` stanzas for them if explicit control of `harness` / `path` is desired, but that is out of scope for the compile-guard slice (the previous slice explicitly framed §8.3 as "compile-guard verification, not 'create the missing harness files' or normalize the `[[bench]]` table").

### 2.3 Exit-Code Confirmation

```text
$ cargo bench --workspace --no-run
   Compiling tokio-test v0.4.5
   ...
    Finished `bench` profile [optimized] target(s) in 16m 58s
  Executable benches src/lib.rs (target/release/deps/...)   # 11 libtest binaries (1 per lib crate)
  Executable benches src/main.rs (target/release/deps/...)  # 2 binary artifacts (intent-api, intent-cli)
  Executable benches/http_handlers.rs  (target/release/deps/http_handlers-71b97baaaf0cf5c0)
  Executable benches/graph_ops.rs      (target/release/deps/graph_ops-d3f8fbe18a7413b1)
  Executable benches/graph_traversal.rs (target/release/deps/graph_traversal-2edbd25145a3cbe4)
  Executable benches/db_operations.rs  (target/release/deps/db_operations-1b19f4572be5e200)
  Executable benches/query_latency.rs  (target/release/deps/query_latency-b9b9c1b7e8b6a073)
  Executable benches/diff_latency.rs   (target/release/deps/diff_latency-4829fda62fcc44fc)
  Executable benches/rebase_latency.rs (target/release/deps/rebase_latency-f85c172074eb44d2)
exit=0
```

The command exited 0 with no compile errors, no missing-dependency errors, and no criterion-API errors. All 7 bench source files compiled against the current dev-dependencies (criterion 0.5 + feature `html_reports`; intent-service additionally enables `async_tokio`).

---

## 3. Per-Bench Fixture / Feature / Env Requirements Table

> The table below records, for each bench source file, the **runtime** fixtures / features / env requirements. None of these are required for the **compile guard** (`--no-run`) to pass; they are listed here so a future contributor who actually runs the benchmarks knows the prerequisites. The compile guard is intentionally independent of these.

| Crate | Bench file | Function(s) | Fixture type | Features / env required at **run** time | Compile-only (`--no-run`) impact |
|-------|------------|-------------|--------------|------------------------------------------|---------------------------------|
| `intent-api` | `benches/http_handlers.rs` | `bench_diff_compute` | In-process rebase-engine types; no IO | none | none |
| `intent-api` | `benches/http_handlers.rs` | `bench_validation` | Inline payload constructors; no IO | none | none |
| `intent-api` | `benches/http_handlers.rs` | `bench_intent_service_create` | `InMemoryIntentRepository` + tokio runtime | none | none |
| `intent-api` | `benches/http_handlers.rs` | `bench_http_server` (4 sub-benches: `create_intent`, `health_check`, `ready_check`, `validate_intent`) | Full axum server on `127.0.0.1:0` (ephemeral port) with `reqwest::blocking::Client`; in-memory repos for service + graph + checkpoint + side-effect + forensic + approval + policy | Requires a routable loopback (always present on Linux/macOS dev boxes); does **not** require a live DB or external HTTP receiver at run time | none (the bench is fully self-contained at compile + run time) |
| `graph-service` | `benches/graph_ops.rs` | `bench_reachable_chain_unlimited`, `bench_reachable_chain_depth_limited`, `bench_reachable_diamond`, `bench_path_chain`, `bench_path_diamond`, `bench_path_no_route`, `bench_cycle_detection_no_cycle`, `bench_cycle_detection_with_cycle` | `InMemoryGraphRepository`; pre-built chain / diamond / cycle graphs | none | none |
| `graph-service` | `benches/graph_traversal.rs` | `bench_bfs_reachable`, `bench_find_path`, `bench_cycle_detection` | `InMemoryGraphRepository`; pre-built small chain (4 nodes), medium star (21 nodes), large tree (121 nodes) | none | none |
| `intent-service` | `benches/db_operations.rs` | `bench_db_create_intent`, `bench_db_create_version`, `bench_db_get_intent`, `bench_db_list_versions` | `sqlx::PgPool` against a live Postgres | **Requires `DATABASE_URL` env var at run time.** Each bench self-skips to a `skipped_no_database_url` placeholder if `DATABASE_URL` is unset (see `requires_database_url()` in `db_operations.rs` line 33). **No DB is required for `cargo bench --no-run`.** | none at compile time |
| `intent-service` | `benches/query_latency.rs` | `bench_intent_create_tx`, `bench_intent_get`, `bench_intent_create_version_occ`, `bench_get_versions_by_intent`, `bench_approval_request_list_pending_by_intent`, `bench_approval_request_list_pending_by_tenant`, `bench_approval_request_update_status`, `bench_policy_snapshot_list_by_intent`, `bench_policy_snapshot_get_latest`, `bench_policy_snapshot_get_by_version` | In-memory `InMemoryIntentRepository` / `InMemoryApprovalRequestRepository` / `InMemoryPolicySnapshotRepository` + tokio runtime | none | none (note: `intent-service` dev-dependency declares `criterion` with the `async_tokio` feature, which is why this crate's bench profile pulls `tokio-test` etc. at compile time) |
| `rebase-engine` | `benches/diff_latency.rs` | `bench_diff_no_change`, `bench_diff_scope_change`, `bench_diff_constraints_change`, `bench_diff_acceptance_criteria_change`, `bench_diff_all_sections_change` | Pure: inline `IntentVersion` constructors; calls `rebase_engine::compute_diff_sync` | none | none |
| `rebase-engine` | `benches/rebase_latency.rs` | `bench_compute_diff_sync`, `bench_compute_diff_with_risk_sync`, `bench_diff_and_plan_sync`, `bench_plan_from_diff_fixed` | Pure: inline `IntentVersion` constructors; calls `rebase_engine::diff::diff_intent_version` + `rebase_engine::rules::analyze_diff_risk` + `rebase_engine::planner::RebasePlan::from_diff_and_risk` | none | none |

### 3.1 Summary Counts

| Bucket | Count | Bench files |
|--------|-------|-------------|
| Pure / in-memory only, no IO at run time | 6 of 7 | `http_handlers.rs` (4 sub-benches), `graph_ops.rs`, `graph_traversal.rs`, `query_latency.rs`, `diff_latency.rs`, `rebase_latency.rs` |
| Live-service at run time, self-skips if env unset, compile-guard unaffected | 1 of 7 | `db_operations.rs` (4 sub-benches; `DATABASE_URL`-gated, falls back to `skipped_no_database_url`) |
| Require routable loopback (always available on dev boxes) | 1 sub-bench in `http_handlers.rs` | `bench_http_server` (4 HTTP probes: `create_intent`, `health_check`, `ready_check`, `validate_intent`) |
| Total bench source files | 7 | across 4 crates (`intent-api`, `graph-service`, `intent-service`, `rebase-engine`) |

---

## 4. Known Limitations (Compile-Only)

- **No benchmark timings collected.** The `--no-run` flag was used per the slice scope ("IN: Run `cargo bench --workspace --no-run`; if it passes, document pass. Do not run actual benchmark timings"). No p50/p95/p99 numbers, no throughput figures, no regression detection — the slice is a compile guard, not a performance run.
- **No `--all-features` exercise.** The bench profile uses default features by default; non-default features (`temporal`, `load-test`, `sqlx-load-test` on `intent-api`) are not exercised by this slice. **The existing bench source files do not reference any non-default feature** (a quick scan of the seven files shows no `#[cfg(feature = "...")]` markers tied to bench logic), so the default-feature compile is sufficient to prove the harness still compiles against the criterion 0.5 API. A follow-up slice could exercise `cargo bench -p intent-api --features temporal --no-run` to be exhaustive, but the bound is out of scope for this slice.
- **`query_latency.rs` and `diff_latency.rs` are not declared in `[[bench]]`.** They compile because Cargo's bench auto-discovery picks them up. No code change is required; a future slice owner may add explicit stanzas to make the harness self-describing, but the compile guard passes today and that refactor is a different risk class.
- **No clippy-on-benches run.** `cargo bench --no-run` does not invoke clippy; bench files were not re-linted by this slice. The previous slices (24b/24c/24d/24e) ran `cargo clippy --workspace --all-targets -- -D warnings` on this HEAD; the bench files inherit the same dev-dependency set and the same `criterion = "0.5"` pin, so a workspace-wide clippy pass remains the single source of truth.
- **No `cargo test --workspace --lib --all-features` re-run.** The previous slices (24b/24c/24d/24e) ran it on the same HEAD and it passes; re-running it here would be redundant since this slice is a doc-only evidence slice — no `crates/*/src/**` or `crates/*/benches/**` files were modified. Per the §8.6 acceptance criterion ("`cargo test --workspace --lib --all-features` still passes"), the prior green is sufficient evidence for the bench-guard's "does not affect lib tests" guarantee.

---

## 5. Behavioural / Risk Observations

- **Compile-only invariants are stable across the recent refactors.** The `criterion = "0.5"` pin is workspace-level and was not bumped; the bench files' imports (`criterion::criterion_group`, `criterion::criterion_main`, `criterion::black_box`, `criterion::BenchmarkId`) and the `bench_with_input` / `bench_function` / `to_async` / `iter` API calls all match criterion 0.5.1's surface. No API drift since the source files landed.
- **Workspace dependency graph is clean for the bench profile.** The bench profile re-uses the regular `[dev-dependencies]` and `[dependencies]`; no crate introduces a bench-only dependency, so the bench profile compile is a superset of the lib+bin compile for the four affected crates. The 16m 58s wall time is the **first** full workspace build of the bench profile; subsequent runs are incremental and finish in seconds.
- **The `intent-service` bench profile pulls `tokio-test` and `criterion`'s `async_tokio` feature** because of the `criterion = { version = "0.5", features = ["html_reports", "async_tokio"] }` line in `crates/intent-service/Cargo.toml` line 28. This is correct: `query_latency.rs` uses `tokio::runtime::Runtime::new()` to drive `InMemory*Repository` async methods, and the `async_tokio` feature wires `criterion::async_tokio` for any future async-iterator benches. No code change needed.
- **No new env gate introduced.** This slice does not add or modify any `std::env::var(...)` check; the existing `DATABASE_URL` check in `db_operations.rs` is preserved as-is.

---

## 6. Local-Bounded Caveats

- This slice is a **compile-guard** evidence slice, not a benchmark-execution or performance-run slice. No timings were collected, no performance regressions were detected, and no SLA targets were exercised.
- This slice is **not** a production-readiness claim. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- The compile guard is **local-executable**: it requires no infrastructure, no external services, and no user decision. The `db_operations` bench file's `DATABASE_URL` check is **only** triggered at run time; the compile guard ignores env vars entirely.
- The `[[bench]]` ↔ `benches/<name>.rs` mapping in `crates/intent-service/Cargo.toml` and `crates/rebase-engine/Cargo.toml` is not 1:1 (see §2.2). This is a documentation/manifest quirk, not a compile failure. The roadmap §8.1a table describes the stanzas correctly; the only minor enhancement that could be made is to add explicit `[[bench]] name = "query_latency"` / `[[bench]] name = "diff_latency"` stanzas for symmetry, but doing so is out of scope for a compile-guard slice and is documented here as a follow-up candidate.
- This slice does **not** add CI integration. Per `24-strategic-roadmap-and-checklist.md` §8.4, "No CI integration (deferred to a separate slice)". The compile guard is a local-only command; the project's heavy CI is intentionally manual-only (per `22-phase-4-entry-plan.md` A-01).

---

## 7. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §8 | P2 Benchmark Compile Guard section; §8.5 action checklist (run evidence, document guard) is now checkable, and §8.6 acceptance criteria are met by this slice. |
| `docs/10-delivery/22-phase-4-entry-plan.md` A-09 | The `[[bench]]` ↔ `benches/<name>.rs` mapping refinement (§2.2 observation) is a manifest-clarity follow-up that is not part of A-09; it would be a separate bounded slice if the next slice owner wants to add the two missing stanzas. |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 | Update log gains a new row recording this slice. |
| `docs/10-delivery/20-project-completion-roadmap.md` | Source of the P2 "Observability & Documentation" backlog item that lists "Integrate criterion benchmarks into CI" as deferred; this slice is a compile-guard prerequisite to any future CI integration but does not perform the CI integration itself. |
| `crates/intent-api/benches/http_handlers.rs` | 597 lines, real criterion benchmarks (4 sub-benches); in-memory + ephemeral-port axum; compile-guard verified. |
| `crates/graph-service/benches/graph_ops.rs` | Real criterion benchmarks (8 sub-benches); in-memory; compile-guard verified. |
| `crates/graph-service/benches/graph_traversal.rs` | Real criterion benchmarks (3 sub-benches across 3 graph sizes); in-memory; compile-guard verified. |
| `crates/intent-service/benches/db_operations.rs` | Real criterion benchmarks (4 sub-benches); `DATABASE_URL`-gated at run time, self-skips if unset; compile-guard verified. |
| `crates/intent-service/benches/query_latency.rs` | Real criterion benchmarks (10 sub-benches); in-memory; compile-guard verified; auto-discovered (no `[[bench]]` stanza). |
| `crates/rebase-engine/benches/diff_latency.rs` | Real criterion benchmarks (5 sub-benches); pure; compile-guard verified; auto-discovered (no `[[bench]]` stanza). |
| `crates/rebase-engine/benches/rebase_latency.rs` | Real criterion benchmarks (4 sub-benches); pure; compile-guard verified. |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen29 (via authorized assistant fixer) | Initial creation — records the P2 Benchmark Compile Guard bounded slice that closes the deferred follow-up from the previous wording-correction slice. `cargo fmt --all -- --check` passes (exit 0, < 1s). `cargo bench --workspace --no-run` passes (exit 0, 16m 58s first build) with all 7 `benches/<name>.rs` source files compiling into optimized bench-profile executables under `target/release/deps/`. `git diff --check` passes (exit 0, working tree clean). No `crates/**` or `benches/**` files were modified; this is a doc-only evidence slice. Per-bench fixture/feature/env table (§3) records the runtime prerequisites for each of the 7 bench source files; the `[[bench]]` ↔ `benches/<name>.rs` mapping quirk (`query_latency.rs` and `diff_latency.rs` are auto-discovered, not listed in any `[[bench]]` stanza) is recorded as an informational observation in §2.2. No benchmark timings collected (compile-only by design per the slice scope). No CI integration (deferred to a separate slice per §8.4). No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
---

## Sign-Off

**Signed:** BrianNguyen29 (via authorized assistant), internal documentation slice only.

> This sign-off is internal planning/evidence documentation only. It does **not** constitute external review, production sign-off, or CI-green attestation. No external or production gates are claimed closed by this signature.
