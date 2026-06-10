# Low-Risk Remaining Tasks Evidence

> **Status:** INTERNAL EVIDENCE — local-verifiable tasks D2, G, F, E completed; D1 deferred/design-first
> **Date:** 2026-06-10
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** Evidence for the low-risk remaining tasks identified in `docs/10-delivery/25-remaining-todo-list.md` §2.D–G. No production claims. No public docs touched.

---

## D2 — graph.rs Doc-Comment TODO Wording

**Task:** Replace "TODO structure" with "placeholder structure" in `crates/intent-rebase-types/src/graph.rs:488` to avoid false-positive grep hits.

**Edit:**
- File: `crates/intent-rebase-types/src/graph.rs`
- Line 488 (now 488): `/// Replaces the Phase 1 baseline placeholder structure with graph-integrated classification.`
- Before: `/// Replaces the Phase 1 baseline TODO structure with graph-integrated classification.`

**Verification:**
- `cargo fmt --all -- --check` — pass (no output)
- `cargo check --workspace` — pass
- `git diff --check` — pass (no whitespace errors)

**Note:** This is a doc-comment historical reference only, not an active `TODO()` task. No functional change.

---

## G — Benchmark Manifest Symmetry for Auto-Discovered Benches

**Task:** Add explicit `[[bench]]` stanzas for `query_latency` and `diff_latency` to ensure manifest symmetry.

**Edits:**
1. `crates/intent-service/Cargo.toml` — added:
   ```toml
   [[bench]]
   name = "query_latency"
   harness = false
   ```
2. `crates/rebase-engine/Cargo.toml` — added:
   ```toml
   [[bench]]
   name = "diff_latency"
   harness = false
   ```

**Context:** `24f` observed that `intent-service/benches/query_latency.rs` and `rebase-engine/benches/diff_latency.rs` compiled via Cargo auto-discovery but lacked matching `[[bench]]` stanzas. The other 5 bench files in the workspace already had explicit stanzas.

**Verification:**
- `cargo check --benches -p intent-service -p rebase-engine` — pass (1m 15s)
- Full `cargo bench --workspace --no-run` was attempted but timed out in the local environment (compilation exceeded 10 minutes). The `check --benches` path is the bounded compile-guard equivalent and confirms manifest symmetry.
- `cargo fmt --all -- --check` — pass
- `git diff --check` — pass

---

## F — intent-cli Short Flag Audit

**Task:** Audit `#[arg(short)]` declarations in `crates/intent-cli/src/lib.rs` for latent collisions.

**Approach:** Run help commands and verify no `clap` panic.

**Results:**

| Command | Result | Notes |
|---------|--------|-------|
| `cargo run -p intent-cli -- --help` | ✅ Exit 0 | Top-level flags: `-u` (api_url), `-t` (tenant_id), `-k` (api_key) — no collision |
| `cargo run -p intent-cli -- run --help` | ❌ **PANIC** before fix | `-i` collision between `intent_id` and `initiated_by` |
| `cargo run -p intent-cli -- run --help` | ✅ Exit 0 after fix | `-a` (action_ids), `-i` (intent_id), `-b` (initiated_by) |
| `cargo run -p intent-cli -- get-run --help` | ✅ Exit 0 | `-r` (run_id) — no collision |

**Fix applied:**
- File: `crates/intent-cli/src/lib.rs`
- Change: `#[arg(short, long)]` → `#[arg(short = 'b', long)]` on `initiated_by` field inside `Commands::Run`
- Rationale: `-i` was already claimed by `intent_id`. `-b` is mnemonic for "initiated **b**y". No other field in the `Run` variant uses `-b`.

**Verification after fix:**
- `cargo fmt --all -- --check` — pass
- `cargo check --workspace` — pass
- All three help commands exit 0 with no panic.

**Historical context:** The top-level `-a` collision between `api_url` and `api_key` was fixed in `24d` (`short = 'u'` and `short = 'k'`). This audit caught the remaining latent collision in the `Run` subcommand.

---

## E — Stale Contradiction Cleanup

**Task:** Re-verify residual contradictions C-1, C-8, C-9, A-09 and apply concise updates.

**Actions taken:**

| ID | Action | Status |
|----|--------|--------|
| **C-1** | Re-read `17-production-readiness-backlog.md` P2-6 vs `22-phase-4-entry-plan.md` A-12. No new drift since 2026-05-20; A-12 Slice 5a/5b and Phases 1.1–2.3 were already recorded in the 2026-05-17 to 2026-05-18 update log. No edit required. | ✅ Verified, no change |
| **C-8** | Re-read `20-project-completion-roadmap.md` P2. Found stale wording: lines 84–85 claimed "no benchmark harnesses exist in the workspace yet (benchmark stubs are design-only)" and "no benchmarks exist yet". Updated to acknowledge real criterion source files exist in 4 crates while keeping CI integration deferred. | ✅ Wording polished |
| **C-9** | Re-read `10-external-review-packet.md` G-RLS-1 vs `22-phase-4-entry-plan.md` A-02 and `17-production-readiness-backlog.md` P1-S5i. No changes since 2026-05-20 RLS parity audit. No edit required. | ✅ Verified, no change |
| **A-09** | Added continuation note to `22-phase-4-entry-plan.md` A-09 status line tracking the `health_routes` → `routes::health` demo slice and next candidate. | ✅ Updated |

**Verification:**
- `cargo fmt --all -- --check` — pass (no doc changes affect code)
- `git diff --check` — pass

---

## D1 — Webhook Dispatcher TODO (Deferred / Design-First)

**Task:** `crates/intent-api/src/webhook_dispatcher.rs:79` — `TODO(Slice 4+): version info should be stored in the outbox record at creation time.`

**Rationale for deferral:**
1. This TODO touches the `WebhookOutboxRecord` schema (adding a `version` field at creation time).
2. Per repo-specific change constraints (AGENTS.md constraint #1): **Intent schema changes → must update ADR first** (`docs/13-adrs/`).
3. Per repo-specific change constraints (AGENTS.md constraint #5): **S3/S4 side-effect auto-compensation → requires explicit approval before implementation**. While this specific change is not S3/S4, webhook outbox schema changes are adjacent to the production delivery boundary and were explicitly classified as "risky/design-first" in the feasibility assessment.
4. The `version` field semantics (semantic version? API version? payload version?) are not yet defined in an ADR or design doc. Implementing without a design decision risks schema churn.
5. The existing outbox schema (migration 019 + 020 + 021) is already functional for local-dev bounded slices. Adding `version` is a nice-to-have, not a correctness fix.

**Decision:** Deferred to a future design-first slice. The TODO remains in code as a legitimate marker. No edit performed.

---

## Sequential Verification Summary

| Step | Command | Result |
|------|---------|--------|
| 1 | `cargo fmt --all -- --check` | ✅ Pass |
| 2 | `cargo check --workspace` | ✅ Pass |
| 3 | `cargo check --benches -p intent-service -p rebase-engine` | ✅ Pass |
| 4 | `cargo run -p intent-cli -- --help` | ✅ Exit 0 |
| 5 | `cargo run -p intent-cli -- run --help` | ✅ Exit 0 (after fix) |
| 6 | `cargo run -p intent-cli -- get-run --help` | ✅ Exit 0 |
| 7 | `git diff --check` | ✅ Pass |

---

## Files Changed

1. `crates/intent-rebase-types/src/graph.rs` — doc-comment wording (D2)
2. `crates/intent-service/Cargo.toml` — added `[[bench]] query_latency` stanza (G)
3. `crates/rebase-engine/Cargo.toml` — added `[[bench]] diff_latency` stanza (G)
4. `crates/intent-cli/src/lib.rs` — fixed `-i` collision via `short = 'b'` on `initiated_by` (F)
5. `docs/10-delivery/22-phase-4-entry-plan.md` — A-09 continuation note + update log (E)
6. `docs/10-delivery/20-project-completion-roadmap.md` — C-8 benchmark wording polish (E)
7. `docs/10-delivery/25-remaining-todo-list.md` — marked D2/G/F/E done, D1 deferred, added update log (E)
8. `docs/10-delivery/25d-low-risk-remaining-tasks-evidence.md` — this document (E)

---

## Non-Production Caveat

All changes are local-verifiable only. No production-readiness claim is made. No external gates are closed. No public docs were touched. D1 remains deferred pending design-first ADR and explicit approval.
---

## Sign-Off

**Signed:** BrianNguyen29 (via authorized assistant), internal documentation slice only.

> This sign-off is internal planning/evidence documentation only. It does **not** constitute external review, production sign-off, or CI-green attestation. No external or production gates are claimed closed by this signature.
