# ADR-02 — Data Plane Architecture

**Status:** Accepted — Partially implemented
**Date:** 2026-04-03
**Authors:** Intent Rebase Engine Team
**Phase:** Phase 0–1

---

## Implementation Status

### Implemented
- PostgreSQL schema for intents, artifacts, dependency_graph, audit_events, approval_scope, policy_snapshots
- S3-compatible artifact storage path scheme: `artifacts/{tenant}/{intent_id}/v{version}/`
- Intent CRUD with version management via sqlx repository
- Audit event append-only persistence via Postgres INSERT

### Remaining Gaps
- S3 forensic bundle storage lifecycle (Phase 3 Batch 3b uses local repository; S3 persistence is Phase 4 scope)
- Cross-region replication for DR not configured
- Rule pack S3 storage not yet implemented (Phase 2/3)

---

## Context

IRE's data plane handles:

- **Intent storage** — versioned intent documents with metadata
- **Artifact registry** — outputs produced under specific intent versions
- **Graph state** — dependency graph nodes and edges
- **Audit log** — immutable append-only event stream
- **Checkpoint mapping** — runtime checkpoint ↔ intent version alignment

Storage requirements:
- Strong consistency for intent CRUD and graph mutations (OLTP)
- High write throughput for audit events (event append)
- Efficient version-range queries for diff and rebase computation
- S3-compatible blob storage for large artifacts and forensic bundles

---

## Decision

**Primary store: PostgreSQL (OLTP) + S3-compatible object store (artifacts/blobs).**

### PostgreSQL — OLTP and Relational State

| Schema Area | Purpose |
|-------------|---------|
| `intents` | Versioned intent documents |
| `artifacts` | Artifact registry with provenance |
| `dependency_graph` | Nodes, edges, propagation state |
| `audit_events` | Immutable append-only event log |
| `approval_scope` | Approval boundaries and policies |
| `policy_snapshots` | Point-in-time policy snapshots for revalidation |

**Why PostgreSQL:**
- ACID transactions ensure graph mutations are consistent
- JSONB columns support flexible intent schema evolution
- Strong consistency critical for rebase correctness
- Rich query capability for compliance and debugging

### S3-Compatible Store — Blob and Artifact Storage

| Bucket/Path | Contents |
|-------------|----------|
| `artifacts/{tenant}/{intent_id}/{version}/` | Produced outputs |
| `forensic-bundles/{tenant}/{event_id}/` | Forensic replay bundles |
| `rule-packs/{tenant}/{pack_id}/{version}/` | Versioned rule packs |
| `snapshots/{tenant}/policy/{snapshot_id}/` | Policy snapshots |

**Why S3:**
- Cost-effective for large blobs
- Built-in versioning
- Retention policies via lifecycle rules
- Accessible to compute nodes and audit exporters

---

## Consequences

### Positive
- PostgreSQL provides strong consistency for intent CRUD and graph state — critical for rebase correctness
- S3 provides cost-effective, durable blob storage with built-in versioning
- Separation of hot (Postgres) vs cold (S3) data optimizes cost/performance
- Both technologies well-understood, reduce operational complexity

### Negative
- Dual storage systems introduce consistency management overhead
- S3 event-driven invalidation requires additional coordination
- Cross-region replication for DR requires careful configuration

### Neutral
- Phase 1 baseline: single-region, single Postgres instance, single S3 bucket
- Multi-region/tenant isolation hardening in Phase 3

---

## Implementation Notes

### Phase 0
- Define DDL migrations for core schemas
- Design artifact storage path scheme: `s3://artifacts/{tenant_id}/{intent_id}/v{version}/{artifact_hash}`
- Establish S3 bucket naming conventions and lifecycle policies

### Phase 1
- Implement intent CRUD with version management
- Implement artifact registry with provenance tags
- Audit events appended via Postgres `INSERT` (no UPDATE/DELETE)

### Phase 2
- Integrate runtime checkpoint mapping
- Implement artifact quarantine path (move to `quarantine/` prefix on invalidation)

### Phase 3
- Forensic bundle export to S3
- Cross-region replica consideration

---

## Related ADRs

- [ADR-01](./01-runtime-adapter.md) — Runtime adapter selection
- [ADR-04](./04-event-broker.md) — Event streaming infrastructure
- [ADR-05](./05-observability-baseline.md) — Observability data storage
- [ADR-07](./07-approval-scope-canonicalization.md) — Approval scope storage

---

## References

- PostgreSQL JSONB: https://www.postgresql.org/docs/current/datatype-json.html
- S3 object versioning: https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-versioning.html
