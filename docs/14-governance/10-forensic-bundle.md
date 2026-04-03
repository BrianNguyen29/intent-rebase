# 10 — Forensic Bundle

**Status:** Proposed  
**Phase:** Phase 3  
**Owner:** Security Team

---

## Mục đích

Forensic bundles provide complete, tamper-evident snapshots of system state for:
- Incident investigation
- Compliance audits
- Legal proceedings
- Post-mortem analysis

---

## Bundle Structure

### Directory Structure

```
forensic-bundle-{bundle_id}/
├── manifest.json
├── intent/
│   ├── intent-{id}-v1.json
│   ├── intent-{id}-v2.json
│   └── intent-{id}-v3.json
├── provenance/
│   ├── artifact-{id}.json
│   └── artifact-{id}.provenance-chain.json
├── approvals/
│   ├── approval-{id}.json
│   └── approval-{id}-snapshot.json
├── graph/
│   └── graph-state-{timestamp}.json
├── audit-events/
│   ├── audit-events-{date-range-start}.jsonl
│   └── audit-events-{date-range-end}.jsonl
├── policy-snapshots/
│   ├── policy-snapshot-{id}.json
│   └── rule-pack-v{version}.json
└── integrity/
    └── hash-chain.json
```

### Manifest

```json
{
  "bundle_id": "uuid",
  "bundle_version": "v1",
  "created_at": "2025-04-03T12:00:00Z",
  "created_by": "system",
  "tenant_id": "uuid",
  "time_range": {
    "start": "2025-03-01T00:00:00Z",
    "end": "2025-04-03T12:00:00Z"
  },
  "purpose": "incident-investigation | compliance-audit | legal",
  "contents": {
    "intent_versions": 12,
    "artifacts": 45,
    "approvals": 8,
    "audit_events": 12500,
    "policy_snapshots": 12
  },
  "integrity": {
    "manifest_hash": "sha256:abc123...",
    "chain_verified": true,
    "verification_timestamp": "2025-04-03T12:00:00Z"
  }
}
```

---

## Integrity Verification

### Hash Chain for Bundle

```python
class ForensicBundleVerifier:
    def verify(self, bundle: ForensicBundle) -> VerificationResult:
        # 1. Verify manifest hash matches computed hash
        manifest_hash = compute_hash(bundle.manifest)
        if manifest_hash != bundle.integrity.manifest_hash:
            return VerificationResult(False, "Manifest hash mismatch")
        
        # 2. Verify each contained file matches recorded hash
        for file in bundle.files:
            recorded_hash = bundle.get_hash(file)
            actual_hash = compute_hash(file)
            if actual_hash != recorded_hash:
                return VerificationResult(False, f"File hash mismatch: {file.path}")
        
        # 3. Verify cross-references (intent versions exist, artifacts exist)
        for intent_ref in bundle.intent_references:
            if not bundle.contains_intent(intent_ref):
                return VerificationResult(False, f"Referenced intent not in bundle: {intent_ref}")
        
        return VerificationResult(True, "Bundle integrity verified")
```

---

## Bundle Generation

### Generation API

```yaml
POST /api/v1/forensic/bundle:
  description: Generate forensic bundle
  body:
    {
      "time_range": {
        "start": "ISO8601",
        "end": "ISO8601"
      },
      "intent_ids": ["uuid", ...],  # optional, filter by intents
      "purpose": "incident-investigation | compliance-audit | legal",
      "include_artifacts": true,
      "include_audit_events": true
    }
  response:
    {
      "bundle_id": "uuid",
      "status": "generating",
      "estimated_completion": "ISO8601"
    }

GET /api/v1/forensic/bundle/{bundle_id}:
  response:
    {
      "bundle_id": "uuid",
      "status": "ready | failed | generating",
      "download_uri": "s3://...",
      "expires_at": "ISO8601"
    }
```

### Generation Process

```
1. Validate request (authorization, parameters)
2. Create bundle directory in S3
3. Export intent versions to JSON
4. Export artifacts metadata and provenance
5. Export approval records and snapshots
6. Export graph state
7. Export audit events (JSONL format)
8. Export policy snapshots
9. Compute all file hashes
10. Generate manifest.json
11. Generate hash-chain.json
12. Verify bundle integrity
13. Store verification result in manifest
14. Notify requestor (webhook/event)
```

---

## Bundle Storage & Retention

| Phase | Storage | Retention |
|-------|---------|-----------|
| Hot | S3 Standard | 30 days |
| Cold | S3 Glacier | 7 years |
| Archived | S3 Glacier Deep Archive | 10 years |

### S3 Lifecycle

```json
{
  "Rules": [
    {
      "ID": "Move-to-glacier-after-30-days",
      "Status": "Enabled",
      "Prefix": "forensic-bundles/",
      "Transitions": [
        {"Days": 30, "StorageClass": "GLACIER"},
        {"Days": 3650, "StorageClass": "DEEP_ARCHIVE"}
      ]
    }
  ]
}
```

---

## Access Control

| Role | Permissions |
|------|-------------|
| `security-reviewer` | Create bundle, read own tenant's bundles |
| `tenant:auditor` | Read bundles for own tenant |
| `system:security` | Create/read bundles for any tenant |
| `system:admin` | Full access including delete |

---

## Bundle Replay

### Replay Capability

Forensic bundles can be replayed to reproduce system state:

```
1. Load bundle manifest
2. Verify bundle integrity
3. Reconstruct intent versions
4. Reconstruct graph state (in isolated environment)
5. Replay audit events (in order)
6. Validate end state matches original
```

### Replay Environment

- Replay must occur in isolated, non-production environment
- Replay is read-only (no production data modified)
- Replay activity is itself audited

---

## Related Documents

- [01 — Audit Event Specification](./01-audit-event-spec.md)
- [02 — Provenance Specification](./02-provenance-spec.md)
- [03 — Policy Snapshot Specification](./03-policy-snapshot-spec.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [11 — Incident Freeze](./11-incident-freeze.md)