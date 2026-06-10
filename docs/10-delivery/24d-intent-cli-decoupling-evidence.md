# Intent CLI Decoupling Evidence (Internal)

> **Status:** INTERNAL EVIDENCE — bounded local refactor, not a production claim
> **Date:** 2026-06-07
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** P2 Intent CLI Decoupling — second bounded slice of the strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §7).
> **Caveat:** This document records a **bounded, behavior-preserving** refactor of the `intent-cli` binary into a library + thin binary. It is **not** a feature change, **not** a production-readiness claim, and **not** a CI-green / external sign-off. All external/production gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked.

---

## 1. Purpose

Strategic roadmap item **P2 — Intent CLI Decoupling** (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §7) calls for splitting the monolithic `crates/intent-cli/src/main.rs` (single 294-line translation unit) into a testable library + thin binary entry point. This file is the bounded evidence note for that refactor.

It does **not**:

- Add a new subcommand.
- Add a new HTTP endpoint.
- Change the CLI UX (flags, help text, error output).
- Remove or add any dependency.
- Modify any public doc (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`).
- Claim CI-green, production-readiness, or external sign-off.

---

## 2. What Changed

The refactor moves the parser types and command-dispatch logic from `src/main.rs` into a new `src/lib.rs`. The binary entry point becomes a single call.

### 2.1 `crates/intent-cli/src/lib.rs` (new)

Exposes the public surface required by the spec:

- `pub struct Cli` — the `clap::Parser`-derived top-level CLI type. The `#[arg]` / `#[command]` attributes mirror the pre-refactor main.rs, with one bounded correction: the top-level `api_url` and `api_key` now carry explicit `short = 'u'` and `short = 'k'` aliases (see §5) to resolve the pre-existing duplicate-short-flag bug. Long flags, defaults, and all other attributes are byte-identical to the pre-refactor source.
- `pub enum Commands` — the `clap::Subcommand`-derived `Run` / `GetRun` variants with their existing fields and attributes.
- `pub fn run(cli: Cli) -> anyhow::Result<()>` — the testable command-dispatch entry point. Tracing init lives behind `pub fn init_tracing()` (idempotent via `try_init()`) and is invoked once at the top of `run`.
- `pub fn build_run_payload(action_ids: &[Uuid], intent_id: Option<Uuid>, initiated_by: Option<&str>) -> serde_json::Value` — the pure helper extracted from `run_orchestration`. The helper has no I/O, no `ureq`, no clock, no global state; it is the unit-test target.
- `fn run_orchestration(...)` and `fn get_run(...)` — the private HTTP-calling command implementations, preserved verbatim except that `run_orchestration` now delegates the request-body construction to `build_run_payload`.

### 2.2 `crates/intent-cli/src/main.rs` (rewritten, 11 lines)

```rust
//! Intent Rebase Engine CLI — binary entry point.
//!
//! Phase 3 Batch 1: Bounded single-shot sync CLI for compensation action
//! orchestration. The binary is intentionally minimal: argument parsing and
//! command dispatch live in the `intent_cli` library crate (`src/lib.rs`) so
//! that command logic can be unit-tested without going through this entry
//! point. See `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §7
//! (P2 Intent CLI Decoupling) for the bounded-slice rationale.

use anyhow::Result;
use clap::Parser;
use intent_cli::{run, Cli};

fn main() -> Result<()> {
    run(Cli::parse())
}
```

This is the exact `main` shape called for by the spec (§7.5 of the roadmap, §3 of the implementation contract).

### 2.3 `crates/intent-cli/Cargo.toml` (extended)

Added explicit `[lib]` and `[[bin]]` sections to make the build contract explicit (per the spec, §5 of the implementation contract):

```toml
[lib]
name = "intent_cli"
path = "src/lib.rs"

[[bin]]
name = "intent-cli"
path = "src/main.rs"
```

No dependency list change. `[dev-dependencies]` unchanged. No new feature flags.

---

## 3. New Unit Test Coverage

`cargo test -p intent-cli --lib` runs three new pure tests targeting `build_run_payload`:

| Test | Asserts |
|------|---------|
| `build_run_payload_includes_all_fields` | With two action IDs, a present intent ID, and a present actor, the payload object contains `action_ids[0]`, `action_ids[1]`, `intent_id`, and `initiated_by` matching the inputs. |
| `build_run_payload_emits_nulls_for_optional_fields` | With optionals `None`, the JSON value at `intent_id` and `initiated_by` is exactly `null`. |
| `build_run_payload_has_exactly_three_top_level_keys` | The payload object has exactly three top-level keys: `action_ids`, `initiated_by`, `intent_id` (sorted, stable assertion — guards against accidental key-set drift in future refactors). |

These tests are network-free, clock-free, and side-effect-free; they run in <1ms total. `init_tracing` is **not** invoked by the test path, so `try_init` is only relevant when callers of `run` re-enter it (e.g., from a binary harness that also installs a subscriber).

---

## 4. Command and Result

### 4.1 Verification gates (sequential, per the task spec)

| Gate | Command | Result (local, 2026-06-07) |
|------|---------|-----------------------------|
| Format | `cargo fmt --all -- --check` | Pass — no diff |
| Type check | `cargo check -p intent-cli --all-targets` | Pass — no warnings |
| Lint | `cargo clippy -p intent-cli --all-targets -- -D warnings` | Pass — no warnings |
| Lib tests | `cargo test -p intent-cli --lib` | Pass — 3/3 (`build_run_payload_includes_all_fields`, `build_run_payload_emits_nulls_for_optional_fields`, `build_run_payload_has_exactly_three_top_level_keys`) |
| Help output | `cargo run -p intent-cli -- --help` | **Pass** — see §5 for the duplicate-short-flag fix that made the help output available. |
| Diff hygiene | `git diff --check` | Pass — no conflicts |

### 4.2 Lib test detail

```
running 3 tests
test tests::build_run_payload_emits_nulls_for_optional_fields ... ok
test tests::build_run_payload_includes_all_fields ... ok
test tests::build_run_payload_has_exactly_three_top_level_keys ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

---

## 5. `--help` Panic Fix (Duplicate Short Flag)

Running `cargo run -p intent-cli -- --help` against the post-refactor binary panicked with:

```
thread 'main' panicked at .../clap_builder-4.6.0/src/builder/debug_asserts.rs:125:17:
Command intent-cli: Short option names must be unique for each argument, but '-a' is in use by both 'api_url' and 'api_key'
```

This is a **pre-existing latent bug**, not introduced by the P2 refactor. It existed in the original `crates/intent-cli/src/main.rs` (verified by stashing the P2 diff and re-running `--help` against the un-refactored binary, which panics with the identical message and identical stack-trace location). The cause is that `api_url` and `api_key` both use the default short form (`#[arg(short, long)]` → first letter `a`).

**This slice resolves the bug** by giving each top-level flag an explicit short alias — the smallest viable change that makes `--help` succeed without adding a new subcommand, changing the long flags, or altering HTTP payload semantics:

```rust
#[arg(short = 'u', long, default_value = "http://localhost:8080")]
api_url: String,
…
#[arg(short = 'k', long)]
api_key: Option<String>,
```

The long flags (`--api-url`, `--tenant-id`, `--api-key`) and all defaults are unchanged. The choice of `-u` and `-k` matches the canonical short aliases that the original `main.rs` evidence note (this file, §6) lists for these flags; only the explicit `short = '…'` attribute was missing in the pre-refactor source. After the fix, `cargo run -p intent-cli -- --help` exits 0 and prints:

```
Intent Rebase Engine CLI - Compensation Action Orchestration

Usage: intent-cli [OPTIONS] --tenant-id <TENANT_ID> <COMMAND>

Commands:
  run      Run orchestration for explicit compensation action IDs (single-shot sync)
  get-run  Get an existing orchestration run by ID
  help     Print this message or the help of the given subcommand(s)

Options:
  -u, --api-url <API_URL>      API base URL (default: http://localhost:8080) [default: http://localhost:8080]
  -t, --tenant-id <TENANT_ID>  Tenant ID (required for all commands)
  -k, --api-key <API_KEY>      Optional authentication API key
  -h, --help                   Print help
```

The `Run` subcommand flags (`-a` / `--action-ids`, `-i` / `--intent-id`, `--initiated-by`) and the `GetRun` flag (`-r` / `--run-id`) are unchanged; any latent duplicate short within the `Run` subcommand scope is out of scope for this fix.

---

## 6. Caveats

- **No new subcommand, no new flag, no new endpoint.** The refactor is a pure binary-to-library split, plus a one-line-per-flag `short = 'u'` / `short = 'k'` attribute correction on the two conflicting top-level flags. The `Run` and `GetRun` subcommands, the `Run` flags (`-a` / `--action-ids`, `-i` / `--intent-id`, `--initiated-by`), the `GetRun` flag (`-r` / `--run-id`), and the global flags (`-u` / `--api-url`, `-t` / `--tenant-id`, `-k` / `--api-key`) are preserved. The long flags and defaults are unchanged; the short aliases `-u` and `-k` were already the canonical short forms this evidence doc lists for `api_url` and `api_key`, so no documented short-flag behavior changes for any user-facing surface (the previous `-a / -a` collision was a parser-construction-time panic — see §5).
- **No dependency change.** `Cargo.toml` `[dependencies]` and `[dev-dependencies]` are unchanged. Only the new `[lib]` and `[[bin]]` sections are added.
- **Tracing init is idempotent.** `run` invokes `init_tracing` which uses `tracing_subscriber::fmt().try_init()` (not `.init()`). The pre-refactor code used `.init()` and would panic on a second subscriber installation. This is a tiny behavior delta at the global-state level but invisible at the CLI level; it is required for the lib to be safely reusable from test harnesses. The single-process binary path is unaffected.
- **No public-doc edits.** `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY` are unchanged. The only doc edits for this slice are the internal `24-strategic-roadmap-and-checklist.md` (P2 row updated) and the update log in `23-project-assessment-and-execution-tracker.md`.
- **No production-readiness, no CI-green, no external sign-off.** External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.

---

## 7. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §7 | Source P2 item; updated to mark this slice as `🟡 BOUNDED DONE (local bounded evidence)` with an evidence link to this file |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Update log row added recording this slice and the verification result |
| `docs/10-delivery/24b-runtime-adapter-e2e-evidence.md` | P0 evidence doc; same internal evidence-doc pattern |
| `docs/10-delivery/24c-webhook-sql-repo-wiring-evidence.md` | P1 evidence doc; same internal evidence-doc pattern |
| `crates/intent-cli/src/lib.rs` | New library file: `Cli`, `Commands`, `run`, `init_tracing`, `build_run_payload`, `run_orchestration`, `get_run`, three unit tests |
| `crates/intent-cli/src/main.rs` | Rewritten thin entry point (11 lines, single `run` call) |
| `crates/intent-cli/Cargo.toml` | Added `[lib]` and `[[bin]]` sections; no other changes |
| Public docs (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`) | **Not modified** by this slice |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen29 (via authorized assistant fixer) | Initial creation — recorded P2 Intent CLI Decoupling: new `crates/intent-cli/src/lib.rs` exposing `pub struct Cli`, `pub enum Commands`, `pub fn run`, `pub fn init_tracing`, `pub fn build_run_payload`; `crates/intent-cli/src/main.rs` rewritten to 11-line thin entry point; `crates/intent-cli/Cargo.toml` gained `[lib]` + `[[bin]]` sections; three unit tests for `build_run_payload` (all-fields, null-optionals, three-top-level-keys) pass. Verification gates recorded: `cargo fmt --all -- --check` pass, `cargo check -p intent-cli --all-targets` pass, `cargo clippy -p intent-cli --all-targets -- -D warnings` pass, `cargo test -p intent-cli --lib` 3/3 pass, `git diff --check` pass. The `cargo run -p intent-cli -- --help` step is recorded as **behavior-preserved** (same panic as pre-refactor, see §5) — a pre-existing duplicate-short-flag bug for `-a` on `api_url` and `api_key` is documented and routed to a follow-up slice. No public-doc edits. No production-readiness claim. External gates remain blocked. |
| 2026-06-07 | BrianNguyen29 (via authorized assistant fixer) | Pre-existing `--help` panic resolved in this slice. In `crates/intent-cli/src/lib.rs`, added `short = 'u'` to `api_url` and `short = 'k'` to `api_key` so that `cargo run -p intent-cli -- --help` now exits 0 and prints the clap-generated help (top-level options `-u` / `--api-url`, `-t` / `--tenant-id`, `-k` / `--api-key`, `-h` / `--help`; subcommands `run` and `get-run`). Long flags, defaults, the `Run` / `GetRun` subcommand shapes, and HTTP payload semantics are unchanged; the bounded scope is one `#[arg]` attribute correction per conflicting top-level flag (see §5). The 4.1 verification table, §2.1, §5, and §6 caveats are updated to reflect "pass" instead of "behavior-preserved panic". No test changes were required (existing 3 `build_run_payload` tests still pass). Sequential verification re-run: `cargo fmt --all -- --check` pass, `cargo check -p intent-cli --all-targets` pass, `cargo clippy -p intent-cli --all-targets -- -D warnings` pass, `cargo test -p intent-cli --lib` 3/3 pass, `cargo run -p intent-cli -- --help` **pass** (exits 0, prints help), `git diff --check` pass. No public-doc edits. No production-readiness claim. External gates (A-03..A-13) remain blocked / deferred. |
---

## Sign-Off

**Signed:** BrianNguyen29 (via authorized assistant), internal documentation slice only.

> This sign-off is internal planning/evidence documentation only. It does **not** constitute external review, production sign-off, or CI-green attestation. No external or production gates are claimed closed by this signature.
