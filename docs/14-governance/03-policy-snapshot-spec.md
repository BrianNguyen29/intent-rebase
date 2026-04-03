# 03 — Policy Snapshot Specification

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Compliance Team

---

## Mục đích

Policy snapshots create point-in-time, immutable records of:
- Approval policy in effect when an intent was approved
- Rule pack version active at time of approval
- Scope boundaries that applied to the approval

This ensures:
- **Auditability**: what policy was in effect when approval was granted
- **Revalidation**: re-approvals use correct policy version
- **Compliance**: evidence that approval was based on authorized policy

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
  ├── snapshot_uri (S3 URI — immutable blob)
  ├── created_at (TIMESTAMPTZ)
  └── canonicalized_at (TIMESTAMPTZ)
```

### Snapshot Content (S3 Blob)

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

| Trigger | When |
|---------|------|
| Intent approval | New approval granted |
| Intent update | New policy snapshot created for new version |
| Re-approval | New snapshot for revalidated approval |
| Rule pack update | Existing intent snapshots remain valid (time-bound) |

### Snapshot Selection for Revalidation

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

### S3 Object Lock

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

### Integrity Verification

```python
# Verify snapshot has not been modified
def verify_snapshot(snapshot_uri, expected_hash):
    actual_hash = compute_hash(fetch_from_s3(snapshot_uri))
    return actual_hash == expected_hash
```

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

```yaml
GET /api/v1/intents/{id}/policy-snapshots:
  description: List all policy snapshots for an intent
  response:
    items: [PolicySnapshot]

GET /api/v1/intents/{id}/policy-snapshots/{version}:
  description: Get policy snapshot for specific intent version
  response: PolicySnapshot

GET /api/v1/approvals/{id}/snapshot:
  description: Get policy snapshot that was basis for approval
  response: PolicySnapshot

POST /api/v1/policy-snapshots/verify:
  description: Verify snapshot integrity
  body: { snapshot_id: uuid }
  response: { valid: boolean, verification_details: {...} }
```

---

## Related Documents

- [04 — Approval Scope & Revalidation](./04-approval-revalidation.md)
- [07 — Authorization Matrix](./07-authz-matrix.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)