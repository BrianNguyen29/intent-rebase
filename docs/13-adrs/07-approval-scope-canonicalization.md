# ADR-07 — Approval Scope & Policy Snapshot Canonicalization

**Status:** Proposed  
**Date:** 2026-04-03  
**Authors:** Intent Rebase Engine Team  
**Phase:** Phase 1  

---

## Context

When an intent changes, IRE must determine:
1. **Which approvals are invalidated** — approvals granted under the old intent
2. **Which scope is affected** — the boundary of what must be re-approved
3. **How to canonicalize the approval state** — creating a point-in-time snapshot of the approval policy

This is critical for:
- **Compliance** — ensuring that changes to intent cannot bypass approval requirements
- **Audit** — showing that approval was based on a specific policy version
- **Rebase correctness** — knowing exactly what must be re-approved after an intent change

Key concepts:
- **Approval scope** — the set of resources/actions that require approval (defined by rule pack)
- **Approval policy** — the rules governing approval (who can approve, what conditions apply)
- **Policy snapshot** — an immutable record of the approval policy at a specific point in time

---

## Decision

**Store approval scope definitions in PostgreSQL with policy snapshots as immutable S3-backed records.**

### Approval Scope Model

```
approval_scope:
  - id: UUID
  - intent_id: FK
  - intent_version: int
  - scope_type: enum (full, partial, none)
  - affected_resources: JSON array of resource IDs
  - required_approvers: JSON array of approver roles/IDs
  - min_approvals: int
  - created_at: timestamp

policy_snapshot:
  - id: UUID
  - intent_id: FK
  - intent_version: int
  - rule_pack_version: string
  - scope_hash: SHA256(scope definition)
  - snapshot_uri: S3 URI (immutable JSON blob)
  - created_at: timestamp
```

### Snapshot Canonicalization Process

```
1. Intent update received
2. Compute new diff (vs previous version)
3. Evaluate rule pack → determine affected approval scope
4. Create policy_snapshot:
   - Hash current scope definition → scope_hash
   - Serialize scope + rule_pack version → JSON
   - Upload to S3 → snapshot_uri
   - Insert record into policy_snapshot table
5. Invalidate existing approvals whose snapshot_hash != new scope_hash
6. Queue re-approval workflow for invalidated scopes
```

### Approval Revalidation Rules

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
- Immutable policy snapshots provide strong audit trail
- Scope hash enables efficient comparison: two versions have identical scope iff `scope_hash` matches
- S3-backed snapshots are independently verifiable and tamper-evident

### Negative
- S3 snapshot creation adds latency to intent update path (mitigate with async background job)
- Hash collisions theoretically possible (mitigate by storing full scope alongside hash)

### Neutral
- Phase 1: synchronous snapshot creation; async in Phase 2
- Phase 1: single approver required; multi-approver threshold in Phase 2

---

## Implementation Notes

### Data Model (PostgreSQL)

```sql
CREATE TABLE approval_scope (
  id UUID PRIMARY KEY,
  intent_id UUID NOT NULL REFERENCES intents(id),
  intent_version INT NOT NULL,
  scope_type TEXT NOT NULL CHECK (scope_type IN ('full', 'partial', 'none')),
  affected_resources JSONB NOT NULL,
  required_approvers JSONB NOT NULL,
  min_approvals INT NOT NULL DEFAULT 1,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE policy_snapshot (
  id UUID PRIMARY KEY,
  intent_id UUID NOT NULL REFERENCES intents(id),
  intent_version INT NOT NULL,
  rule_pack_version TEXT NOT NULL,
  scope_hash TEXT NOT NULL,
  snapshot_uri TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_policy_snapshot_intent_version ON policy_snapshot(intent_id, intent_version);
CREATE INDEX idx_approval_scope_intent ON approval_scope(intent_id, intent_version);
```

### S3 Snapshot Format

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