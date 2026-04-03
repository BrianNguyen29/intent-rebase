# 02 — Provenance Specification

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Security Team

---

## Mục đích

Provenance tracking provides a complete chain of custody for artifacts produced under Intent Rebase Engine. It answers:
- **Which intent version** produced this artifact?
- **What was the state of the system** when it was produced?
- **Is this artifact still valid** or has it been invalidated?

---

## Provenance Model

```
Artifact
  └── produced_from_intent_version
        └── intent (with version history)
              └── created_by (actor)
              └── rule_pack_version
  └── checkpoint_id (runtime checkpoint)
  └── produced_at (timestamp)
  └── verified (boolean)
```

### Provenance Chain

Each artifact carries a provenance chain:

```json
{
  "artifact_id": "uuid",
  "provenance_chain": [
    {
      "step": 1,
      "type": "intent",
      "id": "intent-uuid",
      "version": 3,
      "created_by": "user@example.com",
      "created_at": "2025-04-03T10:00:00Z"
    },
    {
      "step": 2,
      "type": "checkpoint",
      "id": "checkpoint-uuid",
      "runtime_workflow_id": "temporal-workflow-id",
      "created_at": "2025-04-03T10:01:00Z"
    },
    {
      "step": 3,
      "type": "artifact",
      "id": "artifact-uuid",
      "produced_at": "2025-04-03T10:01:30Z",
      "hash": "sha256:abc123..."
    }
  ],
  "metadata": {
    "rule_pack_version": "v1.2.0",
    "risk_level": "medium",
    "approval_status": "approved"
  }
}
```

---

## Artifact Registry Schema

```sql
CREATE TABLE artifacts (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  artifact_type TEXT NOT NULL,
  name TEXT NOT NULL,
  uri TEXT NOT NULL,  -- S3 URI
  hash TEXT NOT NULL,  -- SHA256 of content
  size_bytes BIGINT NOT NULL,
  
  -- Provenance
  produced_from_intent_id UUID REFERENCES intents(id),
  produced_from_intent_version INT NOT NULL,
  produced_from_checkpoint_id UUID,
  produced_at TIMESTAMPTZ NOT NULL,
  produced_by TEXT NOT NULL,  -- actor ID
  
  -- Validity
  status TEXT NOT NULL DEFAULT 'active',  -- active, invalidated, quarantined, deleted
  invalidated_at TIMESTAMPTZ,
  invalidated_reason TEXT,
  quarantine_uri TEXT,  -- S3 URI if quarantined
  
  -- Verification
  verified BOOLEAN DEFAULT FALSE,
  verified_at TIMESTAMPTZ,
  verification_hash TEXT,  -- Hash used for verification
  
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_artifacts_intent ON artifacts(tenant_id, produced_from_intent_id, produced_from_intent_version);
CREATE INDEX idx_artifacts_status ON artifacts(tenant_id, status);
```

---

## Verification

### Content Verification

```bash
# Verify artifact integrity
sha256sum artifact-file  # compare to hash stored in DB

# Verify provenance chain integrity
SELECT * FROM artifacts 
WHERE id = ? 
  AND hash = ?;
```

### Chain Verification

```json
{
  "verification": {
    "artifact_id": "uuid",
    "artifact_hash_valid": true,
    "checkpoint_exists": true,
    "intent_exists": true,
    "intent_version_matches": true,
    "chain_complete": true,
    "verified_at": "2025-04-03T12:00:00Z"
  }
}
```

---

## Invalidation & Quarantine

When an intent changes and artifacts are invalidated:

```
1. Update artifact.status = 'quarantined'
2. Move S3 object to quarantine prefix
3. Store quarantine_uri
4. Log provenance.invalidated audit event
5. Publish artifact.invalidated event to NATS
```

### Quarantine S3 Path

```
Original:  s3://ire-artifacts/{tenant}/{intent_id}/v{version}/{artifact_id}
Quarantine: s3://ire-artifacts/{tenant}/{intent_id}/v{version}/quarantine/{artifact_id}
```

---

## Provenance API

```yaml
GET /api/v1/artifacts/{id}/provenance:
  response:
    {
      "artifact_id": "uuid",
      "provenance_chain": [...],
      "verification": {...},
      "current_status": "active | quarantined | deleted"
    }

GET /api/v1/artifacts/{id}/provenance/verify:
  response:
    {
      "valid": true,
      "verification_details": {...}
    }
```

---

## Compliance & Audit

| Requirement | Implementation |
|-------------|----------------|
| Chain of custody | Full provenance_chain stored with artifact |
| Immutable | Quarantine path, no delete until retention expires |
| Verifiable | Hash chain + verification API |
| Exportable | Provenance included in forensic bundle |

---

## Related Documents

- [01 — Audit Event Specification](./01-audit-event-spec.md)
- [03 — Policy Snapshot Specification](./03-policy-snapshot-spec.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [10 — Forensic Bundle](./10-forensic-bundle.md)