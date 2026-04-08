# 01 — Audit Event Specification

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Security Team

---

## Mục đích

Canonical audit event schema cho phép:
- Full traceability of all state-changing operations
- Compliance evidence for regulatory requirements
- Forensic investigation support
- Anomaly detection và threat hunting

---

## Event Schema

```json
{
  "event_id": "uuid-v7",
  "event_type": "string (see Event Type Taxonomy)",
  "event_version": "v1",
  "tenant_id": "uuid",
  "timestamp": "ISO8601 with timezone",
  "actor": {
    "type": "user | service-account | system",
    "id": "uuid or service-name",
    "email": "actor@example.com (optional, for user type only)",
    "ip_address": "string (optional, if available)"
  },
  "target": {
    "type": "intent | artifact | approval | graph-node | policy-snapshot | other",
    "id": "uuid",
    "intent_version": "int (if applicable)"
  },
  "action": {
    "result": "success | failure | blocked",
    "detail": "string (human-readable summary)"
  },
  "metadata": {
    "trace_id": "string",
    "span_id": "string",
    "request_id": "string",
    "additional_context": {}
  },
  "integrity": {
    "hash": "SHA256 of event payload (excluding hash field)",
    "previous_event_hash": "string (hash chain link)"
  }
}
```

---

## Event Type Taxonomy

| Category | Event Types |
|----------|-------------|
| **Intent lifecycle** | `intent.created`, `intent.updated`, `intent.deleted`, `intent.version_created` |
| **Diff & rebase** | `rebase.detected`, `rebase.preview_generated`, `rebase.preview_viewed`, `rebase.applied`, `rebase.apply_blocked`, `rebase.rejected`, `rebase.rolled_back` |
| **Graph operations** | `graph.node_created`, `graph.node_updated`, `graph.edge_created`, `graph.edge_deleted`, `graph.traversed`, `graph.orphan_detected` |
| **Artifact lifecycle** | `artifact.created`, `artifact.invalidated`, `artifact.quarantined`, `artifact.released`, `artifact.deleted` |
| **Approval workflow** | `approval.requested`, `approval.granted`, `approval.revoked`, `approval.expired`, `approval.revalidated`, `approval.scope_changed` |
| **Policy snapshots** | `snapshot.created`, `snapshot.referenced`, `snapshot.verified` |
| **Security** | `auth.login`, `auth.logout`, `auth.token_issued`, `auth.token_revoked`, `auth.access_denied`, `auth.impersonation` |
| **System** | `system.startup`, `system.shutdown`, `system.config_changed`, `system.maintenance_started`, `system.maintenance_completed` |

### Current bounded implementation status (Phase 2b)

- External `POST /intents/{intent_id}/rebase-apply` currently emits bounded audit records for apply outcomes and blocked-manual-review outcomes.
- Approval queue endpoints currently emit bounded approval decision audit records for approve/reject actions.
- Actor attribution is currently best-effort with fallback values such as `external-api/unknown`, `external-api/approver`, and `external-api/rejector` when richer identity is unavailable.
- The canonical taxonomy above remains the target-state contract. Current Rust enum variants (`RebaseApplied`, `RebaseApplyBlocked`, `ApprovalGranted`, `ApprovalRevoked`) back the bounded implementation, and naming/serialization normalization to canonical event strings remains open.
- Append-only Postgres enforcement, hash-chain verification, and NATS/S3 downstream integration remain target-state requirements rather than fully enforced runtime guarantees in the current bounded slice.

---

## Integrity & Tamper Detection

### Hash Chain

Each event includes `integrity.previous_event_hash` forming a chain:

```
event_1.hash → event_2.previous_event_hash → event_3.previous_event_hash → ...
```

This enables detection of:
- Event deletion (gap in chain)
- Event modification (hash mismatch)
- Event insertion (future hash mismatch)

### Verification

```sql
-- Verify chain integrity for tenant
SELECT event_id, timestamp, hash, previous_event_hash
FROM audit_events
WHERE tenant_id = ?
ORDER BY timestamp ASC;

-- Detect tampering
-- If hash(prev) != current.previous_event_hash → tampered
```

---

## Storage & Retention

| Storage | Details |
|---------|---------|
| **Primary** | PostgreSQL `audit_events` table |
| **Stream** | NATS subject `audit.events.v1.>` (for real-time consumers) |
| **Cold storage** | S3: `audit-events/{tenant}/{year}/{month}/{day}/` |

### Retention Policy

- **Hot (Postgres):** 90 days
- **Cold (S3):** 7 years (configurable per tenant for compliance)
- **Deletion:** Hard delete only via automated retention job; no manual deletion

### Immutability Enforcement

```sql
-- Prevent UPDATE/DELETE on audit_events
REVOKE UPDATE, DELETE ON audit_events FROM app_service;

-- Append-only via trigger (optional additional protection)
CREATE OR REPLACE FUNCTION audit_no_modify()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'Audit events are immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_immutable
BEFORE UPDATE OR DELETE ON audit_events
FOR EACH ROW EXECUTE FUNCTION audit_no_modify();
```

---

## Query Patterns

### Common Queries

```sql
-- All events for a specific intent (all versions)
SELECT * FROM audit_events
WHERE target_type = 'intent'
  AND target_id = ?
ORDER BY timestamp DESC;

-- All rebase events for tenant in time range
SELECT * FROM audit_events
WHERE tenant_id = ?
  AND event_type LIKE 'rebase.%'
  AND timestamp BETWEEN ? AND ?
ORDER BY timestamp DESC;

-- Failed actions (for anomaly detection)
SELECT * FROM audit_events
WHERE tenant_id = ?
  AND action_result = 'failure'
  AND timestamp > NOW() - INTERVAL '24 hours';

-- Actor activity (for user behavior analysis)
SELECT actor_id, COUNT(*) as event_count
FROM audit_events
WHERE tenant_id = ?
  AND timestamp > NOW() - INTERVAL '7 days'
GROUP BY actor_id
ORDER BY event_count DESC;
```

---

## Event Export

### SIEM Integration

Audit events exported to SIEM via:
- **Webhook** — push to Splunk HEC, Elastic, etc.
- **S3 export** — SIEM polls or uses S3 connector

### CEF/LEEF Format

```json
{
  "format": "CEF",
  "version": "0.1",
  "device_vendor": "IntentRebaseEngine",
  "device_product": "IRE",
  "device_version": "1.0",
  "severity": "1-10",
  "name": "rebase.applied",
  "extensions": {
    "rt": "timestamp",
    "src": "actor.ip_address",
    "suid": "tenant_id",
    "msg": "Rebase applied to intent {target_id}"
  }
}
```

---

## Compliance Mapping

| Requirement | How Met |
|-------------|---------|
| Integrity | Hash chain + S3 immutable storage |
| Traceability | Full actor, action, target recorded |
| Tamper resistance | Append-only + trigger + chain verification |
| Retention | S3 lifecycle policy |
| Export | SIEM integration |

---

## Related Documents

- [02 — Provenance Specification](./02-provenance-spec.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [10 — Forensic Bundle](./10-forensic-bundle.md)
- [12 — Replay Compatibility](./12-replay-compatibility.md)
