# 03 — Policy Snapshot Specification

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Compliance Team

---

## Current Implementation Status

> **⚠️ Bounded Phase 2b Implementation**
> 
> This document describes the **target design** and **future desired state**.
> The current Phase 2b implementation is **bounded** and includes only:
> - PostgreSQL persistence layer for `policy_snapshot` table
> - In-memory and SQLx repository implementations
> - Basic CRUD operations (create, read, list)
> 
> **NOT YET IMPLEMENTED:**
> - S3-backed immutable blob storage (current `snapshot_uri` is a `memory://` placeholder)
> - Revalidation API and approval invalidation logic
> - Re-approval workflow and queueing
> - S3 Object Lock for tamper-evidence
> - Integrity verification against S3 blob

---

## Purpose

Policy snapshots create point-in-time records of:
- Approval policy in effect when an intent was approved
- Rule pack version active at time of approval
- Scope boundaries that applied to the approval

This enables:
- **Auditability**: what policy was in effect when approval was granted
- **Revalidation**: re-approvals use correct policy version *(future — not yet implemented)*
- **Compliance**: evidence that approval was based on authorized policy *(future — S3 tamper-evidence not yet available)*

---

## Snapshot Model

```
PolicySnapshot
  ├── intent_id (FK)
  ├── intent_version (INT)
  ├── rule_pack_version (STRING)
  ├── scope_definition (JSON)
  │     ├── scope_type (full | partial | none)
  │     ├── affected_resources (Array<ResourceID>)
  │     ├── required_approvers (Array<ApproverID>)
  │     └── min_approvals (INT)
  ├── scope_hash (SHA256 of scope_definition)
  ├── snapshot_uri (URI — currently memory:// placeholder; S3 URI future)
  ├── created_at (TIMESTAMPTZ)
  └── canonicalized_at (TIMESTAMPTZ — set to creation time; full S3 blob canonicalization future)
```

> **Note on snapshot_uri:** The current implementation stores `memory://policy-snapshots/{intent_id}/v{version}` as a placeholder URI. S3-backed immutable storage is planned for a future phase.
>
> **Note on scope_hash:** The `scope_hash` field is computed using canonical JSON serialization (SHA256 of deterministically-ordered JSON), ensuring semantically equivalent scope definitions with different key/array ordering produce identical hashes. See `compute_scope_hash()` in `crates/intent-rebase-types/src/policy_snapshot.rs`.

### Snapshot Content (S3 Blob) — Future Target

> **NOT YET IMPLEMENTED**: The following describes the target S3 blob format for future implementation. The current Phase 2b implementation stores scope data in PostgreSQL only.

```json
{
  "snapshot_id": "uuid",
  "snapshot_version": "v1",
  "intent_id": "uuid",
  "intent_version": 3,
  "rule_pack": {
    "pack_id": "default-diff-v1",
    "version": "v1.2.0",
    "uri": "s3://ire-rule-packs/system/default-diff-v1/v1.2.0/rule-pack.json"
  },
  "approval_scope": {
    "type": "partial",
    "affected_resources": [
      {"type": "artifact", "id": "artifact-123"},
      {"type": "workflow", "id": "workflow-456"}
    ],
    "required_approvers": [
      {"type": "role", "id": "security-reviewer"}
    ],
    "min_approvals": 1
  },
  "metadata": {
    "created_by": "system",
    "created_at": "2025-04-03T12:00:00Z",
    "intent_summary": "Update customer data retention policy"
  },
  "integrity": {
    "hash": "sha256:abc123...",
    "previous_snapshot_hash": "sha256:def456..."
  }
}
```

---

## Snapshot Lifecycle

### Creation Triggers

| Trigger | When | Status |
|---------|------|--------|
| Intent approval | New approval granted | ❌ Future (snapshot creation must be triggered by caller) |
| Intent update | New policy snapshot created for new version | ❌ Future (snapshot creation must be triggered by caller) |
| Re-approval | New snapshot for revalidated approval | ❌ Future (re-approval workflow not implemented) |
| Rule pack update | Existing intent snapshots remain valid (time-bound) | ❌ Future |

### Snapshot Selection for Revalidation — Future

> **NOT YET IMPLEMENTED**: The following describes the target revalidation logic. The current Phase 2b implementation does not include approval invalidation or re-approval workflow.

```
On intent change:
1. Compute new scope
2. Compute scope_hash(new_scope)
3. Compare with scope_hash from approval snapshot
4. If different → approval invalidated, re-approval required
5. If same → approval remains valid (policy unchanged)
```

---

## Immutability Guarantees

> **NOT YET IMPLEMENTED**: The following describes the target S3 Object Lock and integrity verification design. Current Phase 2b implementation does not include S3 storage or tamper-evident blob storage.

### S3 Object Lock — Future Target

```bash
# Enable S3 Object Lock (must be done at bucket creation)
aws s3api create-bucket \
  --bucket ire-policy-snapshots \
  --object-lock-enabled-for-bucket

# Upload with retention
aws s3api put-object \
  --bucket ire-policy-snapshots \
  --key "{tenant}/{snapshot_id}/snapshot.json" \
  --body snapshot.json \
  --object-lock-mode GOVERNANCE \
  --object-lock-retain-until-date "2030-12-31T00:00:00Z"
```

### Integrity Verification — Future Target

```python
# Verify snapshot has not been modified
def verify_snapshot(snapshot_uri, expected_hash):
    actual_hash = compute_hash(fetch_from_s3(snapshot_uri))
    return actual_hash == expected_hash
```

**Current State**: PostgreSQL `scope_hash` column provides hash-based change detection at the database record level, but does not provide tamper-evident S3 blob storage.

---

## Storage Schema (PostgreSQL)

```sql
CREATE TABLE policy_snapshot (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  intent_id UUID NOT NULL REFERENCES intents(id),
  intent_version INT NOT NULL,
  rule_pack_version TEXT NOT NULL,
  scope_type TEXT NOT NULL CHECK (scope_type IN ('full', 'partial', 'none')),
  affected_resources JSONB NOT NULL,
  required_approvers JSONB NOT NULL,
  min_approvals INT NOT NULL DEFAULT 1,
  scope_hash TEXT NOT NULL,
  snapshot_uri TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  canonicalized_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  
  UNIQUE(intent_id, intent_version)
);

CREATE INDEX idx_policy_snapshot_intent_version 
  ON policy_snapshot(tenant_id, intent_id, intent_version DESC);
CREATE INDEX idx_policy_snapshot_hash 
  ON policy_snapshot(tenant_id, scope_hash);
```

---

## API

> **Phase 2b Bounded Read-Only REST API Surface**: The following endpoints are implemented as of Phase 2b.

```yaml
GET /policy-snapshots/{snapshot_id}:
  description: Get a single policy snapshot by ID
  query: tenant_id (required)
  response: PolicySnapshot
  status: ✅ Implemented

GET /policy-snapshots/intent/{intent_id}/latest:
  description: Get latest policy snapshot for an intent
  query: tenant_id (required)
  response: PolicySnapshot
  status: ✅ Implemented

GET /policy-snapshots/intent/{intent_id}/versions/{version}:
  description: Get policy snapshot for specific intent version
  query: tenant_id (required)
  response: PolicySnapshot
  status: ✅ Implemented

GET /policy-snapshots/intent/{intent_id}:
  description: List all policy snapshots for an intent
  query: tenant_id (required)
  response: { policy_snapshots: [PolicySnapshot], total: int }
  status: ✅ Implemented

GET /api/v1/approvals/{id}/snapshot:
  description: Get policy snapshot linked to an approval (future — approval-linked snapshot endpoint not yet implemented)
  response: PolicySnapshot
  status: ❌ Future (no approval-linked snapshot endpoint exists)

POST /api/v1/policy-snapshots/verify:
  description: Verify snapshot integrity against S3 blob
  body: { snapshot_id: uuid }
  response: { valid: boolean, verification_details: {...} }
  status: ❌ Future (S3 storage not implemented)
```

**Internal Repository API** (unchanged — used by REST handlers):
- `create_snapshot(PolicySnapshot) -> PolicySnapshot`
- `get_snapshot(UUID) -> PolicySnapshot`
- `get_latest_by_intent(intent_id, tenant_id) -> Option<PolicySnapshot>`
- `get_by_intent_version(intent_id, version, tenant_id) -> Option<PolicySnapshot>`
- `list_by_intent(intent_id, tenant_id) -> Vec<PolicySnapshot>`

---

## Related Documents

- [04 — Approval Scope & Revalidation](./04-approval-revalidation.md)
- [07 — Authorization Matrix](./07-authz-matrix.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)