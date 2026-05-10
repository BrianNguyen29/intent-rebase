# ADR-08 — Artifact Side-Effect Transaction Boundary

## Status

**Accepted** — Design decision recorded; implementation deferred to Phase 4+

## Context

When an artifact is ingested via `POST /v1/graph/artifacts`, the system may optionally record a side effect in the `side_effects` ledger. The question arises: should the side-effect ledger write be **transactionally wrapped** with the artifact ingest (atomic success/failure), or should it be **best-effort** (out-of-transaction, non-blocking)?

### Current State

- `ingest_artifact` RLS transaction is wired: `begin_with_tenant → ingest_artifact_with_tx → commit`
- Side-effect recording on artifact ingest is **out-of-transaction / best-effort** for the current bounded slice
- The `SideEffectRepository::record_side_effect_with_idempotency` is called after the RLS tx commits
- If side-effect recording fails, the artifact ingest still succeeds (fail-open on side-effect ledger)

### Design Decision

**Decision: Artifact side-effect recording remains out-of-transaction / best-effort for the current bounded slice.**

Rationale:
1. **Non-critical path**: Side-effect ledger is for compensation planning; artifact ingest is the primary operation
2. **Failure isolation**: Side-effect ledger failure should not cause artifact ingest to fail
3. **Idempotency exists**: The `record_side_effect_with_idempotency` method provides at-least-once semantics
4. **Compensation still possible**: If side-effect recording fails, compensation can still be triggered manually or via replay
5. **Consistent with event publishing**: Audit event publishing is already best-effort/fail-open (see Phase 2b sign-off)

### Consequences

**Positive:**
- Artifact ingest is not blocked by side-effect ledger issues
- System remains available even if side-effect ledger is degraded
- Simpler transaction semantics for the critical path

**Negative:**
- If side-effect recording fails, the ledger may be incomplete
- Compensation planning may miss some artifacts
- Requires manual replay or other remediation mechanisms for full accuracy

### Implementation Notes

For Phase 4+, if stricter guarantees are needed:

1. **Option A (Transactional)**: Wrap side-effect recording inside the same RLS transaction as artifact ingest
   - Requires: Single SQL transaction spanning `graph_artifacts` and `side_effects` tables
   - Tradeoff: Tighter consistency, but side-effect failure blocks ingest

2. **Option B (Best-effort with async reconciliation)**: Keep current approach but add async reconciliation job
   - Requires: Background worker to detect missing side effects and backfill
   - Tradeoff: Eventual consistency, no ingest blocking

3. **Option C (Idempotent retry with DLQ)**: Add side-effect recording to DLQ pipeline
   - Requires: DLQ worker implementation (Phase 4+)
   - Tradeoff: At-least-once delivery, DLQ monitoring

### Evidence

- Current implementation: `crates/intent-api/src/lib.rs` (ingest_artifact handler) — side effect recording is best-effort
- Idempotency: `crates/compensation-service/src/side_effect_service.rs` (atomic `record_side_effect_with_idempotency`)
- Design precedent: Phase 2b sign-off notes event publishing is best-effort/fail-open

### Related ADRs

- ADR-04 (Event Broker): Event publishing is best-effort
- ADR-07 (Approval Scope): Compensation actions use best-effort semantics

### Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-05-10 | (oracle) | Design-first recommendation — implementation deferred |

---

**Next Step**: If tighter guarantees are required, select Option A/B/C and implement in Phase 4+.
