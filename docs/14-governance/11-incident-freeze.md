# 11 — Incident Freeze

**Status:** Proposed  
**Phase:** Phase 2+  
**Owner:** Security Team

---

## Mục đích

During incident investigation, data freeze procedures ensure:
- **Evidence preservation** — critical data is not modified or deleted
- **Chain of custody** — investigation actions are logged and auditable
- **Controlled modification** — changes to frozen data require explicit authorization

---

## When to Invoke Incident Freeze

| Scenario | Freeze Scope | Duration |
|----------|-------------|----------|
| Active security incident | Full tenant data | Duration of investigation |
| Data integrity issue | Affected data only | Until issue resolved |
| Legal hold | Specified data | Per legal requirements |
| Compliance audit | Relevant data | Until audit complete |

---

## Freeze Types

### Full Tenant Freeze

```
All data modifications blocked for tenant:
- No new intents created
- No intent updates
- No artifact operations
- No approval changes
- No graph modifications
- Audit events continue (append-only)
```

### Partial Freeze

```
Specific resources frozen:
- Intent {id} and all downstream
- Artifacts {id1, id2, ...}
- Approval {id}
```

### Immutable Freeze

```
Data locked from any modification or deletion:
- No updates, no deletes
- Additional writes may be blocked
- Used for legal/compliance holds
```

---

## Freeze Invocation

### API

```yaml
POST /api/v1/incident/freeze:
  description: Invoke incident data freeze
  body:
    {
      "scope": "full-tenant | partial | immutable",
      "tenant_id": "uuid",
      "resource_ids": ["uuid", ...],  # for partial freeze
      "reason": "string",
      "duration_hours": int,  # 0 = until manually released
      "authorized_by": "uuid"
    }
  response:
    {
      "freeze_id": "uuid",
      "status": "active",
      "started_at": "ISO8601",
      "estimated_release": "ISO8601"
    }

DELETE /api/v1/incident/freeze/{freeze_id}:
  description: Release incident data freeze
  body:
    {
      "reason": "string",
      "authorized_by": "uuid"
    }
  response:
    {
      "freeze_id": "uuid",
      "status": "released",
      "released_at": "ISO8601"
    }
```

---

## Freeze Enforcement

### Database-Level Enforcement

```sql
CREATE TABLE incident_freeze (
  id UUID PRIMARY KEY,
  tenant_id UUID NOT NULL,
  scope_type TEXT NOT NULL,  -- 'full-tenant', 'partial', 'immutable'
  resource_ids JSONB,  -- null for full-tenant
  reason TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ,  -- null = manual release only
  authorized_by UUID NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'  -- 'active', 'released'
);

-- Trigger to enforce freeze
CREATE OR REPLACE FUNCTION check_freeze_before_write()
RETURNS TRIGGER AS $$
DECLARE
  freeze_record RECORD;
BEGIN
  SELECT * INTO freeze_record FROM incident_freeze
  WHERE tenant_id = NEW.tenant_id
    AND status = 'active'
    AND (expires_at IS NULL OR expires_at > NOW())
  ORDER BY started_at DESC LIMIT 1;
  
  IF freeze_record IS NOT NULL THEN
    IF freeze_record.scope_type = 'full-tenant' THEN
      RAISE EXCEPTION 'Data freeze active for tenant %', NEW.tenant_id;
    ELSIF freeze_record.scope_type = 'partial' THEN
      IF NEW.id = ANY(freeze_record.resource_ids) THEN
        RAISE EXCEPTION 'Data freeze active for resource %', NEW.id;
      END IF;
    END IF;
  END IF;
  
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER freeze_enforcement_intents
BEFORE INSERT OR UPDATE ON intents
FOR EACH ROW EXECUTE FUNCTION check_freeze_before_write();
```

---

## Operations During Freeze

### Allowed Operations

| Operation | Status | Reason |
|-----------|--------|--------|
| Read data (investigators) | ✓ | Investigation access |
| Audit event append | ✓ | Cannot block audit trail |
| New audit events | ✓ | Evidence collection |
| Bundle generation | ✓ | Forensic support |
| Freeze release | ✓ | Authorized security personnel only |

### Blocked Operations

| Operation | Status | Reason |
|-----------|--------|--------|
| Intent create/update/delete | ✗ | Evidence preservation |
| Artifact modification | ✗ | Evidence preservation |
| Approval changes | ✗ | Evidence preservation |
| Graph modifications | ✗ | Evidence preservation |
| Data deletion | ✗ | Evidence preservation |

---

## Freeze Documentation

### Audit Trail

Every freeze operation is logged:

```json
{
  "event_type": "incident.freeze.invoked",
  "freeze_id": "uuid",
  "tenant_id": "uuid",
  "scope_type": "full-tenant",
  "reason": "Security incident investigation - credential compromise",
  "authorized_by": "user-uuid",
  "started_at": "2025-04-03T10:00:00Z"
}

{
  "event_type": "incident.freeze.released",
  "freeze_id": "uuid",
  "reason": "Investigation complete",
  "released_by": "user-uuid",
  "released_at": "2025-04-03T18:00:00Z"
}
```

---

## Freeze Release

### Release Criteria

1. Investigation complete or
2. Data integrity verified or
3. Legal/compliance hold satisfied or
4. Authorized personnel approves release

### Post-Release Actions

```
1. Re-enable normal operations
2. Clear freeze flags
3. Document freeze duration and actions taken
4. Review any attempted blocked operations during freeze
5. Update incident report with freeze timeline
```

---

## Related Documents

- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [10 — Forensic Bundle](./10-forensic-bundle.md)
- [12 — Replay Compatibility](./12-replay-compatibility.md)