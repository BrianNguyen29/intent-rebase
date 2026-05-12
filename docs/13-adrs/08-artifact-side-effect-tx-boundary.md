# ADR-08 — Artifact Side-Effect Transaction Boundary

## Status

**Accepted — Option A bounded implemented** — ADR-08 Option A (transactional side-effect recording inside RLS tx) is implemented for the SQL/RLS path with fail-closed semantics. Non-RLS fallback remains best-effort/post-ingest.

## Context

When an artifact is ingested via `POST /v1/graph/artifacts`, the system may optionally record a side effect in the `side_effects` ledger. The question arises: should the side-effect ledger write be **transactionally wrapped** with the artifact ingest (atomic success/failure), or should it be **best-effort** (out-of-transaction, non-blocking)?

### Current State

- `ingest_artifact` RLS transaction is wired: `begin_with_tenant → ingest_artifact_with_tx → [side-effect recording with_tx] → commit`
- **RLS SQL path (Option A implemented):** Side-effect recording happens inside the same RLS transaction before commit. If side-effect recording fails, the artifact ingest is rolled back (fail-closed).
- **Non-RLS fallback:** Side-effect recording remains **out-of-transaction / best-effort** (post-ingest). If recording fails, the artifact ingest still succeeds (fail-open). This preserves backward compatibility when no SQL side-effect repo exists.
- The `SideEffectRepository` now exposes `as_sqlx_repo()` and `SqlxSideEffectRepository` provides `create_with_tx` / `get_or_create_idempotent_with_tx` for caller-owned transaction participation.

### Design Decision

**Decision: ADR-08 Option A (transactional) is implemented for the SQL/RLS path; non-RLS fallback remains best-effort.**

Rationale for Option A in RLS path:
1. **Atomic consistency**: Artifact ingest and side-effect ledger are either both committed or both rolled back
2. **Fail-closed semantics**: Side-effect write failure aborts the artifact ingest, preventing orphan artifacts without ledger records
3. **Idempotency preserved**: The `get_or_create_idempotent_with_tx` method provides at-least-once semantics within the tx
4. **RLS-native**: Both operations share the same `SET LOCAL app.current_tenant_id` transaction context

Rationale for keeping non-RLS fallback best-effort:
1. **Backward compatibility**: In-memory or non-SQL side-effect repos cannot participate in SQL transactions
2. **Bounded scope**: Fail-closed semantics are only possible when both graph and side-effect repos are SQL-backed
3. **Consistent with event publishing**: Audit event publishing remains best-effort/fail-open (see Phase 2b sign-off)

### Consequences

**Positive (Option A in RLS path):**
- Artifact ingest and side-effect ledger are atomically consistent in the SQL/RLS path
- No orphan artifacts without corresponding ledger records
- RLS tenant context covers both operations in a single transaction

**Positive (non-RLS fallback preserved):**
- Artifact ingest is not blocked by side-effect ledger issues when using in-memory repos
- System remains available even if side-effect ledger is degraded in non-SQL configurations

**Negative:**
- Side-effect write failure now blocks artifact ingest in the SQL/RLS path (intentional fail-closed)
- Non-RLS path still has best-effort semantics; ledger may be incomplete in fallback scenarios
- Compensation planning may miss some artifacts in non-RLS fallback paths

### Implementation Notes

**Implemented (Option A — bounded):**

- `SideEffectRepository::as_sqlx_repo()` exposes the underlying `SqlxSideEffectRepository` for RLS-aware operations
- `SqlxSideEffectRepository::create_with_tx()` and `get_or_create_idempotent_with_tx()` accept a caller-owned `sqlx::Transaction` for in-tx recording
- `SideEffectService::repo()` accessor allows handlers to reach the SQL repo directly
- `ingest_artifact` JWT/RLS path: after `ingest_artifact_with_tx` succeeds, side effects are recorded via `create_with_tx` / `get_or_create_idempotent_with_tx` inside the same tx before commit
- Fail-closed: if in-tx side-effect recording fails, the handler returns an error before `tx.commit()`, causing the tx to roll back and the artifact ingest to be aborted
- Non-RLS fallback (no `rls_pool`, no JWT claims, or no SQL side-effect repo): side-effect recording remains post-ingest best-effort via `SideEffectService`

**Not implemented (out of scope):**
- Option B (async reconciliation / background worker)
- Option C (DLQ pipeline integration)
- S3 Object Lock, production-ready immutable storage, or external gates

### Evidence

- RLS SQL path implementation: `crates/intent-api/src/ingest_handlers.rs` — `ingest_artifact_with_tx` + `create_with_tx` / `get_or_create_idempotent_with_tx` inside same RLS tx before commit
- Side effect repo transaction helpers: `crates/compensation-service/src/side_effect_repo.rs` — `create_with_tx`, `get_or_create_idempotent_with_tx`, `as_sqlx_repo`
- Service repo accessor: `crates/compensation-service/src/side_effect_service.rs` — `repo()` method
- Idempotency: `crates/compensation-service/src/side_effect_service.rs` (atomic `record_side_effect_with_idempotency`)
- Design precedent: Phase 2b sign-off notes event publishing is best-effort/fail-open (non-RLS fallback)

### Related ADRs

- ADR-04 (Event Broker): Event publishing is best-effort
- ADR-07 (Approval Scope): Compensation actions use best-effort semantics

### Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-05-10 | (oracle) | Design-first recommendation — implementation deferred |
| 2026-05-12 | (fixer) | Option A bounded implemented — transactional side-effect recording in RLS SQL path with fail-closed semantics; non-RLS fallback preserved |

---

**Next Step**: Option A is bounded implemented. Full production readiness (DLQ, async reconciliation, external gates) remains Phase 4+ scope.
