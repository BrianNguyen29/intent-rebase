# ADR-06 — Rule Pack Versioning

**Status:** Proposed  
**Date:** 2026-04-03  
**Authors:** Intent Rebase Engine Team  
**Phase:** Phase 0–P1  

---

## Context

Intent Rebase Engine uses rule packs to:
- **Semantic diff** — determine if two intent versions are semantically different
- **Impact propagation** — determine which artifacts, approvals, and workflows are affected by an intent change
- **Rebase strategy** — determine which compensation actions to recommend

Rule packs contain:
- Diff thresholds (what constitutes a meaningful change)
- Propagation rules (how changes propagate through the dependency graph)
- Risk scoring (classify rebase risk as low/medium/high/critical)
- Approval requirements (which changes require human approval)

As IRE evolves, rule packs must also evolve — version control is essential for audit, rollback, and multi-tenant customization.

---

## Decision

**Rule packs are versioned as immutable snapshots stored in S3, with a registry in PostgreSQL.**

### Versioning Model

```
rule-pack/
  registry: PostgreSQL table (pack_id, version, status, created_at, created_by, s3_uri)
  versions: S3 bucket "rule-packs/{tenant_id}/{pack_id}/v{version}/rule-pack.json
```

### Pack Structure

```json
{
  "version": "v2.1.0",
  "pack_id": "default-diff-v1",
  "tenant_id": "system",
  "created_at": "2025-04-03T12:00:00Z",
  "created_by": "system",
  "status": "active",
  "rules": {
    "diff": {
      "threshold": 0.7,
      "exclude_fields": ["metadata.updated_at", "metadata.tags"],
      "include_fields": ["spec.target", "spec.constraints"]
    },
    "propagation": {
      "graph_traversal": "breadth_first",
      "max_depth": 10,
      "node_types": ["artifact", "approval", "workflow"]
    },
    "risk_scoring": {
      "critical": ["spec.target.delete", "spec.constraints.remove.protected"],
      "high": ["spec.target.modify", "spec.constraints.change"],
      "medium": ["spec.target.add"],
      "low": ["metadata.tags.add"]
    },
    "approval_requirements": {
      "critical": "MANUAL_APPROVAL_REQUIRED",
      "high": "AUTO_REVIEW_WITH_NOTIFICATION",
      "medium": "AUTO_APPROVE_WITH_LOG",
      "low": "AUTO_APPROVE"
    }
  },
  "metadata": {
    "description": "Default semantic diff and propagation rules for Phase 1",
    "compatible_with": ["Phase 0", "Phase 1"]
  }
}
```

### Status Lifecycle

| Status | Meaning |
|--------|---------|
| `draft` | Being edited, not active |
| `active` | Currently生效 |
| `deprecated` | No longer recommended, still referenced |
| `superseded` | Replaced by newer version |

### Version Selection

- **Tenant default** — `tenant.rule_pack_id` and `tenant.rule_pack_version` columns
- **Intent override** — intent can reference a specific pack version at creation time
- **Rebase reference** — rebase decisions store which pack version was used

---

## Consequences

### Positive
- Immutable versions enable audit trail and rollback
- S3 storage scales indefinitely, supports lifecycle policies
- PostgreSQL registry provides fast lookups and status management
- Multi-tenant pack customization via separate `tenant_id` namespaces

### Negative
- S3 versioning adds complexity vs simple database storage
- Pack migration when schema changes requires version coordination

### Neutral
- Phase 1: single `system` pack, no tenant customization
- Phase 2: multi-tenant packs with override support
- Phase 4: pack simulation and A/B testing

---

## Implementation Notes

- Create `rule_pack_registry` table in PostgreSQL
- Define S3 path scheme: `s3://ire-rule-packs/{tenant_id}/{pack_id}/v{version}/`
- Implement pack loader service: fetch from S3, cache in memory with TTL
- Pack validation on upload: JSON schema check + semantic consistency check

---

## Related ADRs

- [ADR-02](./02-data-plane.md) — S3 for blob storage
- [ADR-07](./07-approval-scope-canonicalization.md) — Approval requirements in rule packs

---

## References

- Semantic diff rules: `../03-spec/02-semantic-diff.md`
- Graph propagation: `../03-spec/03-dependency-graph.md`