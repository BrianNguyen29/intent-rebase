# 10 — Forensic Bundle

**Status:** Proposed
**Phase:** Phase 3
**Owner:** Security Team

---

## Phase 3 Batch 3b — Bounded Forensic Verification + Export Slice (DELIVERED)

**Bounded scope:** This slice delivers:
1. Forensic verification API that validates parameters and computes coverage estimates WITHOUT generating actual bundles
2. Forensic archive export API that generates in-memory archives with scaffolded data for download

Both APIs are bounded: no actual bundle generation, storage, or replay.

### Delivered (Phase 3 Batch 3b)

#### Verification API
- `ForensicVerificationService` trait and `InMemoryForensicVerificationService` implementation
- Request/Response types: `ForensicVerificationRequest`, `ForensicVerificationResponse`
- Coverage types: `IntentVersionCoverage`, `ArtifactCoverage`, `AuditEventCoverage`, `PolicySnapshotCoverage`
- API endpoint: `POST /forensic/verify` (bounded request-driven verification)
- Integration in `intent-api` with tests

#### Export API
- `ForensicArchiveGenerator` trait and `InMemoryForensicArchiveGenerator` implementation
- Request/Response types: `ForensicExportRequest`, `ForensicExportResponse`
- Archive entry types: `IntentVersion`, `Artifact`, `AuditEvent`, `PolicySnapshot`, `BundleManifest`
- API endpoint: `POST /forensic/export` (bounded in-memory archive generation)
- Archive contains scaffolded/fictional entries representing what a real bundle would contain
- Integration in `intent-api` with tests

### NOT in This Slice

- Actual bundle generation (data collection from intent service, graph service, audit repository) — future phase
- Bundle storage (S3 or any persistence) — future phase
- Bundle retrieval (downloading stored bundles) — future phase
- Bundle replay (reproducing state from a bundle) — future phase
- Hash chain integrity verification (requires generated bundle) — future phase
- Async job orchestration for bundle generation — future phase

### Verification Status Semantics

| Status | Meaning |
|--------|---------|
| `ready` | All referenced entities exist and are within time range |
| `incomplete` | Some entities are missing or time range has gaps |
| `not_supported` | Verification mode not implemented |

### Export Status Semantics

| Status | Meaning |
|--------|---------|
| `generated` | Archive was successfully generated in-memory |
| `failed` | Archive generation failed |

### Request Example

```json
POST /forensic/verify
{
  "tenant_id": "uuid",
  "intent_id": "uuid",
  "time_range": {
    "start": "2025-01-01T00:00:00Z",
    "end": "2025-01-31T23:59:59Z"
  },
  "purpose": "incident_investigation",
  "include_artifacts": true,
  "include_audit_events": true,
  "include_policy_snapshots": true
}
```

### Response Example

```json
{
  "verification_id": "uuid",
  "verified_at": "2025-04-13T12:00:00Z",
  "status": "ready",
  "status_reason": "All referenced entities exist and are within time range",
  "tenant_id": "uuid",
  "intent_id": "uuid",
  "time_range": { "start": "...", "end": "..." },
  "purpose": "incident_investigation",
  "intent_version_coverage": {
    "intent_exists": true,
    "intent_id": "uuid",
    "version_count": 5,
    "earliest_version": "...",
    "latest_version": "...",
    "has_artifact_traceability": true
  },
  "artifact_coverage": {
    "artifact_count": 10,
    "artifacts_with_provenance": 8,
    "coverage_complete": false
  },
  "audit_event_coverage": {
    "event_count": 100,
    "time_range_complete": true,
    "first_event": "...",
    "last_event": "..."
  },
  "policy_snapshot_coverage": {
    "snapshot_count": 3,
    "coverage_complete": true
  },
  "estimated_bundle_item_count": 118
}
```

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
  },
  "retention": {
    "policy": "cold",
    "expires_at": null,
    "retention_set_at": "2025-04-03T12:00:00Z",
    "retention_set_by": "system"
  }
}
```

> **Truthful retention scope — model-level evidence only.** The `retention` field records the intended retention policy and expiry metadata. Actual S3 lifecycle enforcement (GLACIER after 30d, DEEP_ARCHIVE after 3650d), background deletion jobs, and automatic expiry are NOT implemented. These are future phase scope.

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

### Bounded Replay Capability (P4 Bounded Slice)

This implementation provides a **bounded replay verification surface**:

```
1. Load bundle manifest
2. Verify bundle integrity against recorded hashes
3. Generate reconstruction report (per-section verification results)
4. Provide human-readable verification summary
```

**What This IS:**
- Verification: Confirm that a bundle's recorded integrity hashes match the content
- Reconstruction report: Summary of what the bundle contains and how it validates
- Audit trail: Provides evidence of bundle completeness for investigators
- Read-only and isolated: No production data is modified

**What This IS NOT (Phase 4 Scope):**
- **Not runtime replay**: Does NOT reconstruct system state or replay events in a live system
- **Not mutation**: Does NOT modify any production data or state
- **Not S3 storage**: Does NOT handle bundle storage or retrieval from cloud
- **Not export**: Does NOT provide download or export functionality
- **Not full replay**: Does NOT execute events or reconstruct graph state

### Full Replay Environment (Phase 4)

Full forensic replay requires runtime adapter integration:

```
1. Load bundle manifest
2. Verify bundle integrity
3. Reconstruct intent versions
4. Reconstruct graph state (in isolated environment)
5. Replay audit events (in order)
6. Validate end state matches original
```

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