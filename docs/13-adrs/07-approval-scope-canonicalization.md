# ADR-07 — Approval Scope & Policy Snapshot Canonicalization

**Status:** Accepted — Partially implemented
**Date:** 2026-04-03
**Authors:** Intent Rebase Engine Team
**Phase:** Phase 1

---

## Current Implementation Status

> **⚠️ Bounded Phase 2b Implementation**
>
> This ADR describes the **target design** for approval scope canonicalization.
> The current Phase 2b implementation is **bounded** and includes:
> - PostgreSQL `policy_snapshot` table with scope fields ✅
> - Repository layer (in-memory + SQLx) for basic CRUD ✅
> - `scope_hash` computation for change detection ✅
> - `GET /approval-requests/{id}/revalidate` — read-only scope comparison ✅
> - `POST /approval-requests/{id}/expire` — manual expiry transition ✅
> - `POST /approval-requests/trigger-reapproval` — bounded re-approval trigger ✅
>
> **NOT YET IMPLEMENTED:**
> - S3-backed immutable snapshot blobs (current `snapshot_uri` is `memory://` placeholder)
> - Full approval invalidation orchestration
> - Re-approval workflow queueing and notification delivery
> - Risk-based invalidation rules (critical/high/medium/low)
>
> See: [03-policy-snapshot-spec.md](../14-governance/03-policy-snapshot-spec.md) for current implementation boundaries.

---

## Context

When an intent changes, IRE must determine:
1. **Which approvals are invalidated** — approvals granted under the old intent *(future)*
2. **Which scope is affected** — the boundary of what must be re-approved *(future)*
3. **How to canonicalize the approval state** — creating a point-in-time snapshot of the approval policy *(partial — DB persistence implemented; S3 canonicalization future)*

This is critical for:
- **Compliance** — ensuring that changes to intent cannot bypass approval requirements *(future)*
- **Audit** — showing that approval was based on a specific policy version *(partial — DB record; S3 tamper-evidence future)*
- **Rebase correctness** — knowing exactly what must be re-approved after an intent change *(future)*

Key concepts:
- **Approval scope** — the set of resources/actions that require approval (defined by rule pack)
- **Approval policy** — the rules governing approval (who can approve, what conditions apply)
- **Policy snapshot** — an immutable record of the approval policy at a specific point in time *(partial — DB persistence only; S3 immutable blob future)*

---

## Decision

**Store approval scope definitions in PostgreSQL with policy snapshots as immutable S3-backed records.** *(Target design — current implementation is PostgreSQL-only; S3 is future)*

### Approval Scope Model — Current + Future Target

```
approval_scope: (future — not yet modeled as separate table)
  - id: UUID
  - intent_id: FK
  - intent_version: int
  - scope_type: enum (full, partial, none)
  - affected_resources: JSON array of resource IDs
  - required_approvers: JSON array of approver roles/IDs
  - min_approvals: int
  - created_at: timestamp

policy_snapshot: (current — PostgreSQL with memory:// URI placeholder)
  - id: UUID
  - intent_id: FK
  - intent_version: int
  - rule_pack_version: string
  - scope_hash: SHA256(scope definition)
  - snapshot_uri: URI (currently memory:// placeholder; S3 URI future)
  - created_at: timestamp
  - canonicalized_at: timestamp (placeholder — true canonicalization future)
```

> **Note**: The `approval_scope` table described above is the target design. In the current Phase 2b implementation, scope data is embedded within the `policy_snapshot.scope_definition` JSONB column.

### Snapshot Canonicalization Process

```
1. Intent update received                                                    ❌ Future
2. Compute new diff (vs previous version)                                  ❌ Future
3. Evaluate rule pack → determine affected approval scope                   ❌ Future
4. Create policy_snapshot:
   4a. Hash current scope definition → scope_hash                           ✅ Implemented (canonical JSON SHA256)
   4b. Serialize scope + rule_pack version → JSON                         ❌ Future (S3 not implemented)
   4c. Upload to S3 → snapshot_uri                                          ❌ Future (S3 not implemented)
   4d. Insert record into policy_snapshot table                            ✅ Implemented (CRUD only)
5. Invalidate existing approvals whose snapshot_hash != new scope_hash    ❌ Future
6. Queue re-approval workflow for invalidated scopes                      ❌ Future
```

> **Current State (Phase 2b)**: Only the repository layer (CRUD) and `scope_hash` helper are implemented. Steps 1–3 (triggering, diff computation, rule pack evaluation) and steps 4b–4c (S3 serialization/upload) are not yet implemented. Snapshot creation must be triggered explicitly by the caller.

### Approval Revalidation Rules — Future Target

> **NOT YET IMPLEMENTED**: The following risk-based invalidation rules are planned for future implementation. Current Phase 2b does not include approval invalidation or re-approval workflow.

| Change Type | Invalidation Behavior |
|-------------|----------------------|
| `critical` risk | Invalidate all approvals in scope; full re-approval required |
| `high` risk | Invalidate approvals on directly affected resources; partial re-approval |
| `medium` risk | Log change; auto-notify approvers; no invalidation unless explicit |
| `low` risk | No approval impact |

### Scope Boundaries

Approval scope is computed from the dependency graph:
- Starting from changed intent nodes
- Traverse graph to find downstream `approval` nodes
- Boundary = all paths from intent to approval node are included
- Cycles are detected and handled (see `03-spec/03-dependency-graph.md`)

---

## Consequences

### Positive
- Immutable policy snapshots provide strong audit trail *(future — S3 tamper-evidence not yet implemented)*
- Scope hash enables efficient comparison: two versions have identical scope iff `scope_hash` matches ✅
- S3-backed snapshots are independently verifiable and tamper-evident *(future)*

### Negative
- S3 snapshot creation adds latency to intent update path *(future — not yet applicable)*
- Hash collisions theoretically possible *(mitigated by storing full scope alongside hash)* ✅

### Neutral
- Phase 1: synchronous snapshot creation; async in Phase 2 *(future)*
- Phase 1: single approver required; multi-approver threshold in Phase 2 *(future)*
- **Phase 2b**: PostgreSQL persistence only; S3, canonicalization, revalidation, re-approval workflow are future phases

---

## Implementation Notes

### Data Model (PostgreSQL)

> **Current Phase 2b Schema**: Only `policy_snapshot` table exists. `approval_scope` table is future. Scope data is embedded in `scope_definition` JSONB column.

```sql
CREATE TABLE policy_snapshot (
  id UUID PRIMARY KEY,
  intent_id UUID NOT NULL REFERENCES intents(id),
  intent_version INT NOT NULL,
  rule_pack_version TEXT NOT NULL,
  scope_type TEXT NOT NULL CHECK (scope_type IN ('full', 'partial', 'none')),
  affected_resources JSONB NOT NULL DEFAULT '[]',
  required_approvers JSONB NOT NULL DEFAULT '[]',
  min_approvals INT NOT NULL DEFAULT 1,
  scope_hash TEXT NOT NULL,
  snapshot_uri TEXT NOT NULL,  -- Currently memory:// placeholder; S3 URI future
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  canonicalized_at TIMESTAMPTZ NOT NULL DEFAULT NOW()  -- Placeholder; true canonicalization future

  UNIQUE(intent_id, intent_version)
);

CREATE INDEX idx_policy_snapshot_intent_version ON policy_snapshot(tenant_id, intent_id, intent_version DESC);
CREATE INDEX idx_policy_snapshot_hash ON policy_snapshot(tenant_id, scope_hash);  -- For future revalidation
```

> **Future**: `approval_scope` table (separate from `policy_snapshot`) is planned but not yet implemented.

### S3 Snapshot Format — Future Target

> **NOT YET IMPLEMENTED**: The following describes the target S3 blob format. Current Phase 2b does not include S3 storage; scope data is stored in PostgreSQL only.

```json
{
  "snapshot_id": "uuid",
  "intent_id": "uuid",
  "intent_version": 3,
  "rule_pack_version": "v1.2.0",
  "scope": {
    "type": "partial",
    "affected_resources": ["artifact-123", "workflow-456"],
    "required_approvers": ["role:security-reviewer"],
    "min_approvals": 1
  },
  "canonicalized_at": "2025-04-03T12:00:00Z"
}
```

---

## Related ADRs

- [ADR-02](./02-data-plane.md) — Storage architecture
- [ADR-06](./06-rule-pack-versioning.md) — Rule pack versioning
- [Governance Pack](../14-governance/04-approval-revalidation.md) — Full approval revalidation specification

---

## References

- Dependency graph: `../03-spec/03-dependency-graph.md`
- Approval workflows: `../07-frontend/03-operator-workflows.md`
