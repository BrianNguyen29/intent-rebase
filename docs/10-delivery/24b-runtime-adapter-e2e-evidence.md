# Runtime Adapter End-to-End Evidence (Internal)

> **Status:** INTERNAL EVIDENCE — bounded local proof, not a production claim
> **Date:** 2026-06-07
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** P0 Runtime Adapter End-to-End Proof — first bounded slice of the strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §4).
> **Caveat:** This document records a **local, in-memory** proof that uses `MockAdapter`. It is **not** a Temporal end-to-end test, **not** a production-readiness claim, and **not** a CI-green / external sign-off. All external/production gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked.

---

## 1. Purpose

Strategic roadmap item **P0 — Runtime Adapter End-to-End Proof** (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §4) calls for a single local-verifiable test that exercises an apply cycle through the `RuntimeAdapter` trait boundary and an internal evidence note recording the test, command, and result. This file is that evidence note.

It does **not**:

- Add a Temporal-backed test (Temporal server is out of scope per §4.4).
- Add a new dependency.
- Modify any public doc (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`).
- Claim CI-green, production-readiness, or external sign-off.

---

## 2. What Path Is Proven

The new test `test_runtime_adapter_apply_end_to_end` in `crates/rebase-orchestrator/src/orchestrator_tests.rs` drives a single apply cycle through the runtime adapter boundary and asserts observable outcomes for each of the three stages called out in §4.3 of the roadmap:

| Stage | Trait Method (on `Arc<dyn RuntimeAdapter>`) | Orchestrator Call | Observable Outcome Asserted |
|-------|---------------------------------------------|-------------------|-----------------------------|
| 1. Checkpoint alignment | `is_adapter_ready` (gate before signal) — alignment itself uses `CheckpointAligner` over `intent_service::CheckpointRepository` (not a trait method) | `RebaseOrchestrator::align_checkpoint` | `AlignedCheckpoint` with `checkpoint_id == Some(seeded_id)`, `outcome == ClosestMatch`, non-empty `rationale` |
| 2. Signal delivery | `send_rebase_signal` | `RebaseOrchestrator::send_runtime_rebase_signal` | `RuntimeExecutionResult::signal_sent == true`, non-empty `status_message` |
| 3. Replay | `replay_from_checkpoint` | `RebaseOrchestrator::send_runtime_rebase_signal` (same call as stage 2) | `status == Succeeded`, `replay_attempted == true`, `replay_completed == true`, `status_message` contains `"Signal sent and replay completed"` |

A single call to `send_runtime_rebase_signal` exercises two of the three trait methods in the table above (`send_rebase_signal` then `replay_from_checkpoint`), gated by the third (`is_adapter_ready`). The checkpoint-alignment stage uses the orchestrator's own `CheckpointAligner` seam, which is the planner-selection → real-record mapping the roadmap §4.1 calls out. Together the three stages cover the full P0 path that the strategic evaluation flagged as having no single end-to-end proof.

**Adapter call evidence:** The test also asserts that `MockAdapter::is_adapter_ready()` still returns `AdapterStatus::Ready` after the apply cycle and that `RebaseOrchestrator::is_runtime_ready()` returns `true`, proving the call path consistently uses the same `Arc<dyn RuntimeAdapter>` seam.

**Trait methods exercised in this single test (3 of 5):** `is_adapter_ready`, `send_rebase_signal`, `replay_from_checkpoint`. The remaining two (`get_checkpoints`, `map_intent_to_checkpoint`) are exercised by their own targeted tests in `crates/runtime-adapter/src/lib.rs` (mock-only) and are out of scope for the orchestrator-driven apply path.

---

## 3. Test Code Reference

`crates/rebase-orchestrator/src/orchestrator_tests.rs` — new test at the end of the file:

```rust
#[tokio::test]
async fn test_runtime_adapter_apply_end_to_end() { /* see source for full body */ }
```

**Setup:**

- `MockAdapter::ready()` — all three success flags (`is_ready`, `signal_success`, `replay_success`) default to `true` (see `crates/runtime-adapter/src/lib.rs::MockAdapter::new`).
- `MockCheckpointRepo` — in-memory checkpoint repo following the pattern in the existing `test_runtime_execution_success` test.
- `MockGraphRepo` — in-memory graph repo.
- One real `Checkpoint` seeded for the test's `intent_id` / `workflow_id` / `tenant_id` triple.

**Plan shape:** `DecisionClass::B`, `RiskTier::Low`, `phase1_baseline` deferred fields. The planner's `CheckpointSelection` is `ready: false` (Phase 1 baseline), so the orchestrator's `CheckpointAligner` falls into the best-effort path and selects the seeded checkpoint, returning `CheckpointAlignmentOutcome::ClosestMatch`. This is the same shape used by `test_plan_and_apply` and `test_runtime_execution_success`.

**Drive path:**

1. `orchestrator.align_checkpoint(intent_id, tenant_id, workflow_id, &plan)` → asserts `checkpoint_id == Some(seeded_id)`, `outcome == ClosestMatch`, `!rationale.is_empty()`.
2. `orchestrator.send_runtime_rebase_signal(intent_id, tenant_id, workflow_id, &aligned)` → asserts the three observable outcomes above.
3. `orchestrator.is_runtime_ready()` and `mock_adapter.is_adapter_ready()` → assert `Ready` / `true`.

---

## 4. Command and Result

**Command:**

```bash
cargo test -p rebase-orchestrator --lib test_runtime_adapter_apply_end_to_end
```

**Result (local, 2026-06-07):**

```
running 1 test
test orchestrator_tests::test_runtime_adapter_apply_end_to_end ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out; finished in 0.00s
```

**Full orchestrator test suite (regression check):**

```bash
cargo test -p rebase-orchestrator --lib
```

```
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

(40 pre-existing tests + 1 new P0 test, all pass.)

**Sequential verification gates (per task specification):**

| Gate | Command | Result |
|------|---------|--------|
| Format | `cargo fmt --all -- --check` | Pass — no diff |
| Type check | `cargo check -p rebase-orchestrator --all-features` | Pass — no warnings |
| Lint | `cargo clippy -p rebase-orchestrator --all-features -- -D warnings` | Pass — no warnings |
| Targeted test | `cargo test -p rebase-orchestrator --lib test_runtime_adapter_apply_end_to_end` | Pass — 1/1 |
| Diff hygiene | `git diff --check` | Pass — no conflicts |

---

## 5. Caveats

- **`MockAdapter` only.** This is a `MockAdapter`-driven proof. The `TemporalAdapter` is feature-gated (`runtime-adapter` `temporal` feature) and was not exercised; the roadmap §4.4 explicitly excludes Temporal-server requirements from this slice.
- **No external service.** No Postgres, NATS, or S3 is touched. The in-memory checkpoint repo (`MockCheckpointRepo`) is sufficient for the alignment step.
- **No graph mutations exercised.** The P0 path is alignment + signal + replay; the bounded graph state mutations from `apply_rebase` step 2 are out of scope for `send_runtime_rebase_signal`. The graph state update path is covered by `test_graph_state_update` and `test_audit_summary_with_graph_updates` (both pre-existing).
- **Not a Temporal reset.** This is the cooperative signal-based replay seam, not a native Temporal reset. See the doc-comment on `RebaseOrchestrator::replay` (`crates/rebase-orchestrator/src/lib.rs`).
- **No production-readiness, no CI-green, no external sign-off.** External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- **Public docs untouched.** `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY` are unchanged. The only doc edits for this slice are the internal `24-strategic-roadmap-and-checklist.md` (P0 row updated) and the update log in `23-project-assessment-and-execution-tracker.md`.

---

## 6. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §4 | Source P0 item; updated to mark this slice as `🟡 BOUNDED DONE (local proof)` with an evidence link to this file |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Update log row added recording this slice and the verification result |
| `docs/10-delivery/11-phase-2b-sign-off-packet.md` | Phase 2b sign-off; this slice extends the runtime-adapter delivery claim with a focused end-to-end proof |
| `docs/13-adrs/01-runtime-adapter.md` | ADR for the runtime adapter trait; this proof covers the `Mock default` decision with a runnable seam |
| `crates/runtime-adapter/src/lib.rs` | Trait and `MockAdapter` definitions; `is_ready`, `signal_success`, `replay_success` flags are the source of the success path used here |
| `crates/rebase-orchestrator/src/lib.rs` | `RebaseOrchestrator::send_runtime_rebase_signal` (function under test) and `RebaseOrchestrator::align_checkpoint` (alignment seam) |
| `crates/rebase-orchestrator/src/orchestrator_tests.rs` | New test at the end of the file: `test_runtime_adapter_apply_end_to_end` |
| Public docs (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`) | **Not modified** by this slice |

---

## 7. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen29 (via authorized assistant fixer) | Initial creation — recorded P0 Runtime Adapter End-to-End Proof: test code reference, command/result, trait-method coverage table, sequential verification gates, and explicit non-production caveat. Companion to the new `test_runtime_adapter_apply_end_to_end` test in `crates/rebase-orchestrator/src/orchestrator_tests.rs` and the P0 row update in `24-strategic-roadmap-and-checklist.md` §4. No public-doc edits. No production-readiness claim. External gates remain blocked. |
---

## Sign-Off

**Signed:** BrianNguyen29 (via authorized assistant), internal documentation slice only.

> This sign-off is internal planning/evidence documentation only. It does **not** constitute external review, production sign-off, or CI-green attestation. No external or production gates are claimed closed by this signature.
