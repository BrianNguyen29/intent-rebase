# 05 — Immutable Retention & Tamper Resistance

**Status:** Proposed  
**Phase:** Phase 2+  
**Owner:** Security Team

---

## Mục đích

Ensure that audit data, policy snapshots, and provenance records are:
- **Immutable**: Cannot be modified or deleted
- **Tamper-evident**: Any modification attempt is detectable
- **Retained**: Stored for required retention period
- **Recoverable**: Can be restored for investigation

---

## Immutability Strategy

### Layer 1: PostgreSQL Constraints

```sql
-- Prevent UPDATE/DELETE on critical tables
REVOKE UPDATE, DELETE ON audit_events FROM app_service;
REVOKE UPDATE, DELETE ON policy_snapshot FROM app_service;
REVOKE UPDATE, DELETE ON artifacts FROM app_service;

-- Trigger-based enforcement
CREATE OR REPLACE FUNCTION prevent_audit_modify()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'Audit events are immutable - UPDATE/DELETE not allowed';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_immutable
BEFORE UPDATE OR DELETE ON audit_events
FOR EACH ROW EXECUTE FUNCTION prevent_audit_modify();
```

### Layer 2: Application-Level Enforcement

```rust
// All audit writes go through audit service
impl AuditService {
    async fn append_event(&self, event: AuditEvent) -> Result<()> {
        // No update/delete methods exposed
        // Only append_event() available
    }
}
```

### Layer 3: S3 Object Lock

```bash
# All S3 buckets for governance data use Object Lock
aws s3api create-bucket \
  --bucket ire-policy-snapshots \
  --object-lock-enabled-for-bucket \
  --region us-east-1

# Default retention for all objects
aws s3api put-object-lock-configuration \
  --bucket ire-policy-snapshots \
  --object-lock-configuration '{"ObjectLockMode":"GOVERNANCE","ObjectLockEnabled":"Enabled"}'
```

### Layer 4: Hash Chain Verification

```python
class HashChainVerifier:
    def verify_chain(self, events: list[AuditEvent]) -> VerificationResult:
        for i, event in enumerate(events[1:], 1):
            expected_prev = events[i-1].integrity.hash
            if event.integrity.previous_event_hash != expected_prev:
                return VerificationResult(
                    valid=False,
                    reason=f"Chain broken at event {event.event_id}",
                    broken_event_id=event.event_id
                )
        return VerificationResult(valid=True)
```

---

## Retention Policies

| Data Type | Hot (Postgres) | Cold (S3) | Total Retention |
|-----------|---------------|-----------|-----------------|
| Audit events | 90 days | 7 years | 7 years |
| Policy snapshots | 90 days | 10 years | 10 years |
| Provenance records | 2 years | 10 years | 10 years |
| Forensic bundles | 1 year | 7 years | 7 years |
| Rule pack history | 90 days | 5 years | 5 years |

### S3 Lifecycle Configuration

```json
{
  "Rules": [
    {
      "ID": "Move-to-glacier-after-90-days",
      "Prefix": "audit-events/",
      "Status": "Enabled",
      "Transitions": [
        {"Days": 90, "StorageClass": "GLACIER"}
      ]
    },
    {
      "ID": "Delete-after-retention",
      "Prefix": "audit-events/",
      "Status": "Enabled",
      "Expiration": {"Days": 2555}
    }
  ]
}
```

---

## Tamper Detection

### Real-Time Detection

```python
class TamperDetector:
    def check_event(self, event: AuditEvent):
        # Verify hash matches payload
        if not self.verify_hash(event):
            self.alert("Tamper detected: hash mismatch", event)
        
        # Verify previous hash chain link
        if not self.verify_chain_link(event):
            self.alert("Tamper detected: chain broken", event)
        
        # Check for sequence gaps
        if self.has_sequence_gap(event):
            self.alert("Tamper detected: sequence gap", event)
```

### Periodic Audit

```bash
# Daily integrity check job
#!/bin/bash
# Run via cron or scheduler

for tenant in $(list_tenants); do
  echo "Verifying chain for tenant $tenant..."
  psql -c "SELECT verify_audit_chain('$tenant')"
  
  if [ $? -ne 0 ]; then
    echo "CHAIN BROKEN for $tenant" | mail -s "ALERT: Audit Chain Broken" security@example.com
  fi
done
```

---

## Recovery & Backup

### Backup Strategy

| Component | Method | Frequency | RTO |
|-----------|-------|----------|-----|
| PostgreSQL audit tables | Continuous replication to standby | Real-time | < 1 minute |
| S3 governance data | Cross-region replication | Real-time | < 15 minutes |
| Hash chain state | Daily hash checkpoint recorded in separate system | Daily | < 24 hours |

### Restoration Procedure

```
1. Detect data loss or corruption
2. Identify last known-good state (hash chain intact)
3. Restore from cross-region replica
4. Verify restored data integrity
5. Resume audit logging from last-good state
6. Document incident in incident tracking system
```

---

## Alerts & Monitoring

| Alert | Condition | Severity |
|-------|-----------|----------|
| Chain verification failure | Hash chain broken | Critical |
| Sequence gap detected | Missing events in sequence | Critical |
| S3 Object Lock disabled | Bucket lock removed | Critical |
| Hot storage near capacity | > 80% of retention period used | Warning |
| Cold storage restore initiated | Glacier retrieval started | Info |

---

## Compliance Mapping

| Requirement | Implementation |
|-------------|----------------|
| Data integrity | Hash chain + S3 Object Lock |
| Tamper detection | Real-time monitoring + daily audit |
| Retention | S3 lifecycle policies |
| Recoverability | Cross-region replication + daily backups |

---

## Related Documents

- [01 — Audit Event Specification](./01-audit-event-spec.md)
- [03 — Policy Snapshot Specification](./03-policy-snapshot-spec.md)
- [10 — Forensic Bundle](./10-forensic-bundle.md)