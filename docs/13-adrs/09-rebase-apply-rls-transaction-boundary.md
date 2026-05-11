# ADR-09 — Rebase Apply RLS Transaction Boundary

## Status

**Accepted** — Design decision recorded; implementation deferred to Phase 4 D1–D7

## Context

The `rebase_apply` handler orchestrates an external rebase request through `RebaseOrchestrator::apply_rebase`. The current orchestration has three relevant seams:

1. **Checkpoint alignment** — reads checkpoint records for a workflow and tenant, then selects the best checkpoint for alignment.
2. **Graph state updates** — mutates graph-node state for affected artifacts, approvals, and side effects.
3. **Runtime signal dispatch** — sends a post-rebase signal to the runtime adapter and may request replay from an aligned checkpoint.

The current bounded RLS work does **not** provide a full primary-path RLS transaction boundary for all three seams. Prior slices only cover:

- a post-hoc graph RLS verification/update helper for proceeded outcomes, and
- the `BlockedManualReview` approval create/cancel path inside an RLS transaction.

That leaves the proceeded path with checkpoint reads and graph mutations outside the primary RLS transaction boundary, while runtime signaling remains external I/O.

This ADR records the design decision for the Phase 4 `rebase_apply` RLS transaction boundary. It does not implement the boundary and does not claim production readiness.

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

The post-hoc RLS graph helper is only a best-effort second pass. It cannot guarantee rollback because the non-RLS mutation may already have committed before the RLS check runs. Phase 4 implementation should make tx-aware graph update methods the primary mutation path for RLS requests.

The existing `update_node_state_with_tx` seam is the foundation. Phase 4 should add tx-aware orchestration helpers and remove the post-hoc helper once the primary path is covered and verified.

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

| ID | Description | Priority |
|----|-------------|----------|
| D1 | Add tx-aware checkpoint read method, e.g. `list_by_workflow_with_tx` | P2 |
| D2 | Add tx-aware graph updater method, e.g. `update_node_state_if_affected_with_tx` | P1 |
| D3 | Add `RebaseOrchestrator::update_graph_state_with_tx` or equivalent decomposed tx-aware method | P1 |
| D4 | Add RLS caller-side orchestration in `rebase_apply`: read-only tx → write tx → post-commit signal | P1 |
| D5 | Remove the superseded post-hoc `rls_graph_update` helper after the primary path is verified | P1 |
| D6 | Add/extend live RLS integration coverage for same-tenant success and cross-tenant rejection | P1 |
| D7 | Preserve and test non-RLS fallback behavior when no RLS pool/claims exist | P1 |

---

## Verification Plan

1. **Unit tests:** Cover RLS proceeded path, non-RLS fallback, and failure behavior before signal dispatch.
2. **Live RLS integration tests:** Add RLC coverage showing same-tenant graph update succeeds inside the RLS path and cross-tenant graph mutation is rejected.
3. **Rollback check:** Force a graph update failure inside the write transaction and verify no partial graph mutation commits.
4. **Signal sequencing check:** Verify runtime signal dispatch happens only after the write transaction commits.
5. **Canonical local gates:** Run `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and targeted tests for the changed crates.

---

## Consequences

**Positive:**

- Graph mutations become tenant-isolated at the database layer on the primary path.
- Checkpoint reads gain database-enforced defense-in-depth.
- Runtime signaling remains clearly best-effort and out-of-transaction.
- The post-hoc graph RLS helper can be removed after the primary path is verified.
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
| Regression from decomposing orchestrator flow | Keep the existing non-RLS fallback and add targeted tests before removing the post-hoc helper |
| Long-running write transaction over many affected nodes | Keep updates bounded; monitor and test affected-item counts |
| Read/write skew between checkpoint read and graph write | Keep read tx short; validate critical identifiers during write tx if needed |
| Runtime signal failure after commit | Treat as degraded runtime condition; rely on retry/replay/operator remediation rather than DB rollback |

---

## Non-Goals

- Not claiming production readiness.
- Not moving runtime signal dispatch into a database transaction.
- Not redesigning ADR-08 artifact side-effect transaction semantics.
- Not removing the non-RLS fallback path in this ADR.
- Not implementing the D1–D7 plan as part of this documentation change.

---

## Evidence

- `crates/intent-api/src/rebase_apply_handlers.rs` — current handler-level RLS seams and post-hoc graph helper.
- `crates/rebase-orchestrator/src/lib.rs` — checkpoint, graph update, and runtime signal orchestration.
- `crates/rebase-orchestrator/src/checkpoint_aligner.rs` — checkpoint read/alignment flow.
- `crates/rebase-orchestrator/src/graph_updater.rs` — graph update helper flow.
- `crates/graph-service/src/lib.rs` — existing tx-aware graph update seam.

---

## Phase 3 Risk Acceptance

For non-production Phase 3 close-out, the residual risk of the non-RLS `rebase_apply` primary path is **accepted** with the following mitigations:

1. **Post-hoc RLS graph check** — The `AutoProceeded` and `AutoProceededWithNotification` paths use a post-hoc RLS transaction check/update after the non-RLS graph mutation succeeds. This is **detection-only** (not prevention): if the RLS check fails, a warning is logged but the mutation is not rolled back.
2. **BlockedManualReview path is RLS-wrapped** — The approval create/cancel path for `BlockedManualReview` outcomes runs inside a full RLS transaction (`begin_with_tenant → create_approval_request_with_tx → cancel_*_with_tx → commit`).
3. **Tenant mismatch rejection** — Handler-level JWT tenant guard rejects cross-tenant requests before any graph mutation begins (fail-closed).
4. **RLC-14 tenant isolation test** — A dedicated tenant mismatch rejection test exists in `crates/intent-api/src/rebase_apply_handler_tests.rs`.

**Residual risk:** A compromised or misconfigured non-RLS graph mutation on the `AutoProceeded` path could write cross-tenant data before the post-hoc check detects it. The probability is low (requires both JWT bypass AND graph mutation bypass), but the impact is high (cross-tenant data leakage).

**Decision:** Accept this residual risk for non-production Phase 3. Full primary-path RLS wrapping (D1–D7) remains Phase 4 scope and must be completed before any production deployment claim.

## Related ADRs

- ADR-02 (Data Plane): Tenant isolation and RLS baseline.
- ADR-04 (Event Broker): Best-effort external/event dispatch precedent.
- ADR-08 (Artifact Side-Effect Transaction Boundary): Related transaction-boundary reasoning; side-effect semantics remain separate.

---

## Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-05-11 | (oracle) | Design-first recommendation — all three seams resolved; implementation deferred to Phase 4 |
| 2026-05-11 | Backend Lead | Self-acceptance — design approved per oracle recommendation; D1–D7 remain Phase 4 scope |

---

**Next Step**: Implement D1–D7 as a bounded Phase 4 RLS slice.
