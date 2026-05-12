# ADR-09 — Rebase Apply RLS Transaction Boundary

## Status

**Accepted** — Design decision recorded; bounded D1–D7 implemented at commit `d98c7dc`

## Context

The `rebase_apply` handler orchestrates an external rebase request through `RebaseOrchestrator::apply_rebase`. The current orchestration has three relevant seams:

1. **Checkpoint alignment** — reads checkpoint records for a workflow and tenant, then selects the best checkpoint for alignment.
2. **Graph state updates** — mutates graph-node state for affected artifacts, approvals, and side effects.
3. **Runtime signal dispatch** — sends a post-rebase signal to the runtime adapter and may request replay from an aligned checkpoint.

The bounded D1–D7 slice (commit `d98c7dc`) implements a primary-path RLS transaction boundary for checkpoint reads and graph mutations, while keeping runtime signaling post-commit and out-of-transaction by design. Prior to this slice, coverage was limited to:

- a post-hoc graph RLS verification/update helper for proceeded outcomes (now removed — superseded by the primary path), and
- the `BlockedManualReview` approval create/cancel path inside an RLS transaction.

The current state after D1–D7:

- **Checkpoint reads** (D1) are wrapped in a short read-only RLS transaction for defense-in-depth.
- **Graph updates** (D2/D3/D4) run inside the primary read-write RLS transaction path as the authoritative mutation path.
- **Runtime signal dispatch** (D4) remains post-commit and out-of-transaction by design.
- **The post-hoc helper** (D5) has been removed after the primary path was verified.
- **RLS integration tests** (D6) and **non-RLS fallback tests** (D7) are in place.
- The **non-RLS fallback path** remains preserved for local development and existing non-RLS tests/configurations.

This ADR records the design decision and its bounded implementation. It does not claim production readiness.

---

## Decision

Adopt a caller-side, tenant-scoped transaction boundary for `rebase_apply`:

1. **Checkpoint reads** should move into a short read-only RLS transaction for defense-in-depth.
2. **Graph updates** must move into the primary RLS transaction path as the authoritative mutation path.
3. **Runtime signal dispatch** must remain post-commit and out-of-transaction.
4. **The API handler/caller owns transaction sequencing**; the orchestrator exposes decomposed `_with_tx` operations instead of managing RLS internals directly.
5. **The non-RLS fallback path remains** for local development and existing non-RLS tests/configurations.

The intended sequence is:

```text
1. If RLS context is unavailable: keep existing non-RLS fallback path.
2. If RLS context is available:
   a. Open tenant-scoped read-only tx.
   b. Read checkpoint/alignment data.
   c. Commit/close read-only tx.
   d. Open tenant-scoped read-write tx.
   e. Apply graph state updates through tx-aware graph repository methods.
   f. Commit read-write tx.
   g. Dispatch runtime signal post-commit, out-of-transaction.
```

---

## Seam Decisions

### 1. Checkpoint Reads

**Decision:** Move checkpoint reads into a read-only RLS transaction for defense-in-depth. Priority: **P2**.

Checkpoint reads are read-only and already carry explicit tenant filtering. However, wrapping them in a tenant-scoped read-only RLS transaction adds a database-enforced guard against missing filters, incorrect tenant propagation, or future query changes.

The implementation should add a tx-aware checkpoint repository method, such as `list_by_workflow_with_tx`, and use it only when an RLS transaction is available. The non-RLS path remains unchanged.

### 2. Graph State Updates

**Decision:** Move graph updates into the primary RLS transaction path. Priority: **P1**.

The post-hoc RLS graph helper was a best-effort second pass that could not guarantee rollback. It has been superseded by the primary RLS transaction path (D2/D3/D4). The `update_node_state_with_tx` seam is now exercised through `RebaseOrchestrator::update_graph_state_with_tx` inside the caller-side RLS write transaction, and the post-hoc helper has been removed (D5).

### 3. Runtime Signal Dispatch

**Decision:** Keep runtime signal dispatch post-commit and out-of-transaction. Priority: **P1** as a boundary rule.

Runtime signaling is external I/O. It must not run inside a database transaction because the runtime signal can succeed while the database commit fails, or fail while the database commit succeeds. Keeping it post-commit preserves database correctness and matches the existing best-effort runtime/eventing pattern.

Signal failure after commit is a degraded runtime condition, not a database rollback condition.

### 4. Orchestrator Decomposition

**Decision:** Use caller-side orchestration rather than making the orchestrator own RLS internals.

The API handler already has the RLS pool and tenant claims context. It should own transaction setup and sequencing. The orchestrator should expose decomposed methods that can accept a transaction for database work and keep runtime dispatch separate.

### 5. Backward Compatibility

**Decision:** Preserve the non-RLS fallback path.

When `rls_pool` or tenant claims are absent, the existing non-RLS behavior remains available for local development, existing tests, and non-production configurations. Tenant mismatch checks remain fail-closed where claims are present.

---

## Alternatives Considered

### Alternative A — Keep post-hoc RLS graph verification

Rejected. A post-hoc check cannot roll back a prior committed non-RLS mutation without compensating transactions. It is useful as a transition aid, not as the final isolation boundary.

### Alternative B — Put checkpoint reads and graph updates in one long transaction

Rejected for now. A single long transaction increases duration and couples read alignment to graph mutation. A short read-only transaction followed by a write transaction is simpler and lower risk for the Phase 4 slice.

### Alternative C — Put runtime signal dispatch inside the database transaction

Rejected. External I/O inside a database transaction creates distributed consistency failure modes and can block/rollback unrelated database work.

### Alternative D — Make the orchestrator own `rls_pool` and transaction lifecycle

Rejected. That couples domain orchestration to API/RLS infrastructure and makes fallback/error handling less explicit. Caller-side orchestration keeps the boundary visible.

---

## Implementation Plan

| ID | Description | Priority | Status |
|----|-------------|----------|--------|
| D1 | Add tx-aware checkpoint read method, e.g. `list_by_workflow_with_tx` | P2 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D2 | Add tx-aware graph updater method, e.g. `update_node_state_if_affected_with_tx` | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D3 | Add `RebaseOrchestrator::update_graph_state_with_tx` or equivalent decomposed tx-aware method | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D4 | Add RLS caller-side orchestration in `rebase_apply`: read-only tx → write tx → post-commit signal | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D5 | Remove the superseded post-hoc graph helper after the primary path is verified | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D6 | Add/extend live RLS integration coverage for same-tenant success and cross-tenant rejection | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |
| D7 | Preserve and test non-RLS fallback behavior when no RLS pool/claims exist | P1 | ✅ BOUNDED IMPLEMENTED (`d98c7dc`) |

---

## Verification Plan

1. **Unit tests:** Cover RLS proceeded path, non-RLS fallback, and failure behavior before signal dispatch.
2. **Live RLS integration tests:** RLC-14 coverage shows same-tenant graph update succeeds inside the RLS path and cross-tenant graph mutation is rejected.
3. **Rollback check:** Force a graph update failure inside the write transaction and verify no partial graph mutation commits.
4. **Signal sequencing check:** Verify runtime signal dispatch happens only after the write transaction commits.
5. **Canonical local gates:** `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and targeted tests for the changed crates passed at commit `d98c7dc`.

---

## Consequences

**Positive:**

- Graph mutations become tenant-isolated at the database layer on the primary path.
- Checkpoint reads gain database-enforced defense-in-depth.
- Runtime signaling remains clearly best-effort and out-of-transaction.
- The post-hoc graph RLS helper has been removed after the primary path was verified (D5).
- Non-RLS compatibility remains intact.

**Negative:**

- `_with_tx` methods expand repository/orchestrator API surface.
- RLS and non-RLS paths must both be tested until fallback is retired.
- The read/write split leaves a small read/write skew window; implementation should keep the read phase short and validate assumptions in the write phase where needed.
- Runtime signal failure after commit remains possible by design.

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Regression from decomposing orchestrator flow | Keep the existing non-RLS fallback and add targeted tests; post-hoc helper removed after primary path verified |
| Long-running write transaction over many affected nodes | Keep updates bounded; monitor and test affected-item counts |
| Read/write skew between checkpoint read and graph write | Keep read tx short; validate critical identifiers during write tx if needed |
| Runtime signal failure after commit | Treat as degraded runtime condition; rely on retry/replay/operator remediation rather than DB rollback |

---

## Non-Goals

- Not claiming production readiness.
- Not moving runtime signal dispatch into a database transaction.
- Not redesigning ADR-08 artifact side-effect transaction semantics.
- Not removing the non-RLS fallback path in this ADR.
- Not claiming the D1–D7 bounded slice is production-ready.

---

## Evidence

- `crates/intent-api/src/rebase_apply_handlers.rs` — current handler-level RLS seams (post-hoc helper removed at `d98c7dc`).
- `crates/rebase-orchestrator/src/lib.rs` — checkpoint, graph update, and runtime signal orchestration.
- `crates/rebase-orchestrator/src/checkpoint_aligner.rs` — checkpoint read/alignment flow.
- `crates/rebase-orchestrator/src/graph_updater.rs` — graph update helper flow.
- `crates/graph-service/src/lib.rs` — existing tx-aware graph update seam.

---

## Phase 3 Risk Acceptance (Pre-D1–D7)

> **Historical note:** The following describes the risk posture *before* commit `d98c7dc`. After D1–D7 bounded implementation, the primary-path RLS transaction boundary for checkpoint reads and graph mutations is in place. The remaining blockers are external gates (SRE sign-off, security review, load/pen testing), ADR-08 artifact side-effect transaction semantics, forensic application-level RLS transactions, and the intentional non-RLS fallback path — not the `rebase_apply` RLS boundary itself.

For non-production Phase 3 close-out (pre-`d98c7dc`), the residual risk of the non-RLS `rebase_apply` primary path was **accepted** with the following mitigations:

1. **Post-hoc RLS graph check** (superseded by D1–D7) — The `AutoProceeded` and `AutoProceededWithNotification` paths previously used a post-hoc RLS transaction check/update after the non-RLS graph mutation succeeded. This was **detection-only** (not prevention). The post-hoc helper has been removed (D5) and replaced by the primary RLS transaction path.
2. **BlockedManualReview path is RLS-wrapped** — The approval create/cancel path for `BlockedManualReview` outcomes runs inside a full RLS transaction (`begin_with_tenant → create_approval_request_with_tx → cancel_*_with_tx → commit`).
3. **Tenant mismatch rejection** — Handler-level JWT tenant guard rejects cross-tenant requests before any graph mutation begins (fail-closed).
4. **RLC-14 tenant isolation test** — A dedicated tenant mismatch rejection test exists in `crates/intent-api/tests/rls_integration.rs`.

**Residual risk (post-D1–D7):** The bounded D1–D7 slice implements the primary RLS transaction boundary but does not eliminate all risk. Remaining explicit blockers before any production deployment claim:

- External SRE sign-off, security review, load testing (L3–L5), and penetration testing.
- ADR-08 artifact side-effect transaction semantics remain a separate boundary.
- Forensic application-level RLS transaction wrapping remains pending.
- Runtime signal dispatch remains post-commit and best-effort by design.
- The non-RLS fallback path remains available for local development.

**Decision:** The D1–D7 bounded slice is accepted as non-production feature completion. Production readiness requires closing the external gates and blockers listed above.

## Related ADRs

- ADR-02 (Data Plane): Tenant isolation and RLS baseline.
- ADR-04 (Event Broker): Best-effort external/event dispatch precedent.
- ADR-08 (Artifact Side-Effect Transaction Boundary): Related transaction-boundary reasoning; side-effect semantics remain separate.

---

## Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-05-11 | (oracle) | Design-first recommendation — all three seams resolved; implementation deferred to Phase 4 |
| 2026-05-11 | Backend Lead | Self-acceptance — design approved per oracle recommendation; D1–D7 were accepted for bounded implementation |
| 2026-05-12 | Backend Lead | Bounded D1–D7 implemented at commit `d98c7dc` — primary RLS path for checkpoint reads and graph mutations; post-hoc helper removed; non-RLS fallback preserved; runtime signal remains post-commit |

---

**Next Step**: Close external gates (SRE sign-off, security review, load/pen testing) and complete ADR-08 artifact side-effect boundary + forensic application-level RLS transactions before any production readiness claim.
