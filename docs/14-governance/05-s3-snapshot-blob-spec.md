# 05 — S3/MinIO Snapshot Blob Specification

**Status:** Proposed (Future — Not Implemented)  
**Phase:** Phase 3+  
**Owner:** Backend Lead / Platform  

---

## Purpose

This document specifies the S3/MinIO-backed immutable blob storage contract for policy snapshots. It describes the object key structure, JSON payload format, upload/retrieval operations, retention/lifecycle policy, and migration path from the current `memory://` placeholder URI scheme.

> **⚠️ Production Readiness Warning**
>
> This specification describes **target design for future implementation**. The current Phase 2b implementation uses `memory://policy-snapshots/{intent_id}/v{version}` as a placeholder URI stored in PostgreSQL. S3-backed immutable blob storage is **NOT YET IMPLEMENTED**. Do not claim production readiness for S3 snapshot storage until this specification is implemented and verified.

---

## Storage Backend

| Environment | Endpoint | Bucket | Protocol |
|-------------|----------|--------|----------|
| Local Dev | `localhost:9000` | `ire-policy-snapshots` | S3-compatible (MinIO) |
| Staging/Production | Configured via `AWS_ENDPOINT_URL` env var | `ire-policy-snapshots` | S3-compatible |

### MinIO Console (Local Dev Only)

- **URL:** http://localhost:9001
- **Credentials:** `minioadmin` / `minioadmin`

### Bucket Requirements

```bash
# Create bucket (local dev — MinIO)
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/ire-policy-snapshots --ignore-existing

# Note: Object Lock is NOT enabled in Phase 3 bounded slice.
# Object Lock and 100-year retention are Phase 4+ scope.
# mc ilm set GOVERNANCE 100y local/ire-policy-snapshots
```

> **⚠️ Phase 3 Scope Limitation:** Object Lock is NOT enabled or enforced in the Phase 3 bounded slice. The upload sequence below omits Object Lock headers. Object Lock, GOVERNANCE/COMPLIANCE retention modes, and chain-hash verification are Phase 4+ scope and must NOT be claimed as implemented in Phase 3.

---

## Object Key Structure

### Key Format

```
{tenant_id}/{intent_id}/v{intent_version}/snapshot.json
```

### Key Components

| Component | Format | Example |
|-----------|--------|---------|
| `tenant_id` | UUID (lowercase, no dashes) | `550e8400e29b41d4a716446655440000` |
| `intent_id` | UUID (lowercase, no dashes) | `9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3` |
| `intent_version` | Integer | `3` |
| Filename | Fixed | `snapshot.json` |

### Example Keys

```
550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v1/snapshot.json
550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v2/snapshot.json
550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v3/snapshot.json
```

---

## Object JSON Format

### Snapshot Blob Schema

```json
{
  "$schema": "https://intent-rebase.example.com/schemas/policy-snapshot/v1.json",
  "snapshot_id": "uuid",
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
    "intent_summary": "Update customer data retention policy",
    "tenant_id": "uuid"
  },
  "integrity": {
    "algorithm": "SHA256",
    "content_hash": "abc123def456...",
    "previous_snapshot_hash": "sha256:def456..."
  }
}
```

### Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `snapshot_id` | UUID | Yes | Unique identifier for this snapshot |
| `intent_id` | UUID | Yes | Reference to the parent intent |
| `intent_version` | Integer | Yes | Version of the intent this snapshot captures |
| `rule_pack` | Object | Yes | Rule pack information at snapshot time |
| `rule_pack.pack_id` | String | Yes | Identifier of the rule pack |
| `rule_pack.version` | String | Yes | Version string of the rule pack |
| `rule_pack.uri` | String | Yes | S3 URI to the rule pack JSON |
| `approval_scope` | Object | Yes | Scope boundaries at approval time |
| `approval_scope.type` | Enum | Yes | `full`, `partial`, or `none` |
| `approval_scope.affected_resources` | Array | Yes | List of affected resource IDs |
| `approval_scope.required_approvers` | Array | Yes | List of required approver IDs/roles |
| `approval_scope.min_approvals` | Integer | Yes | Minimum approvals required |
| `metadata` | Object | Yes | Snapshot creation metadata |
| `metadata.created_by` | String | Yes | Principal that created the snapshot |
| `metadata.created_at` | ISO8601 | Yes | Timestamp of snapshot creation |
| `metadata.intent_summary` | String | No | Human-readable intent summary |
| `metadata.tenant_id` | UUID | Yes | Tenant identifier |
| `integrity` | Object | Yes | Integrity verification data |
| `integrity.algorithm` | String | Yes | Hash algorithm (always `SHA256`) |
| `integrity.content_hash` | String | Yes | SHA256 hash of the content |
| `integrity.previous_snapshot_hash` | String | No | Hash of previous snapshot (for chain verification) |

---

## Upload Contract

### Preconditions

1. Snapshot JSON must be serialized with **canonical JSON ordering** (deterministic key order)
2. Content hash must be computed **before** upload using canonical JSON bytes
3. Caller must have `s3:PutObject` permission on the bucket

> **⚠️ Phase 3 Scope:** Object Lock headers are NOT included in Phase 3 bounded upload. Object Lock enforcement and chain-hash linkage are Phase 4+ scope.

### Upload Sequence

```
1. Serialize snapshot to canonical JSON string
2. Compute SHA256 hash of JSON bytes → content_hash (using sha2::Sha256)
3. Upload to S3 with:
   - Content-Type: application/json
   - checksum_sha256: <content_hash>
4. Store snapshot_uri in PostgreSQL policy_snapshot record
```

> **Note:** Chain-hash linkage (previous_snapshot_hash) is documented for future implementation but NOT implemented in Phase 3.

### Upload Example (AWS CLI)

```bash
# Compute hash
CONTENT_HASH=$(sha256sum snapshot.json | cut -d' ' -f1)

# Upload (Phase 3 bounded — no Object Lock)
aws s3api put-object \
  --bucket ire-policy-snapshots \
  --key "550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v3/snapshot.json" \
  --body snapshot.json \
  --content-type application/json \
  --content-sha256 "${CONTENT_HASH}"
```

### Upload Example (Rust/aws-sdk-s3)

```rust,ignore
use sha2::{Digest, Sha256};

let content = canonical_json_snapshot()?;
let mut hasher = Sha256::new();
hasher.update(content.as_bytes());
let content_hash = format!("{:x}", hasher.finalize());

// Note: Object Lock is NOT enforced in Phase 3 bounded slice.
// Governance mode and retention are future scope (Phase 4+).
let put_output = s3_client.put_object()
    .bucket("ire-policy-snapshots")
    .key(key)
    .body(content.into())
    .content_type("application/json")
    .checksum_sha256(&content_hash)
    .send()
    .await?;
```

---

## Retrieval Contract

### Retrieval Sequence

```
1. Construct object key from snapshot_uri
2. GET object from S3
3. Verify Content-Length matches expected size (optional)
4. Verify x-amz-content-sha256 header matches stored content_hash (optional but recommended)
5. Deserialize JSON (with canonical ordering preserved)
6. Return snapshot blob
```

### Retrieval with Integrity Verification

```rust,ignore
use sha2::{Digest, Sha256};

async fn get_verified_snapshot(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    snapshot_uri: &str,
    expected_hash: &str,
) -> Result<PolicySnapshotBlob, SnapshotError> {
    // Fetch from S3
    let get_output = s3_client.get_object()
        .bucket(bucket)
        .key(snapshot_uri.replace("s3://ire-policy-snapshots/", ""))
        .send()
        .await?;

    // Collect body bytes
    let body = get_output.body.collect().await?;
    let body_bytes = body.to_vec();

    // Verify content hash using sha2 (aws-sdk-s3 does not expose content_sha256 on GetObjectOutput)
    let mut hasher = Sha256::new();
    hasher.update(&body_bytes);
    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != expected_hash {
        return Err(SnapshotError::IntegrityMismatch {
            expected: expected_hash.to_string(),
            actual: actual_hash.to_string(),
        });
    }

    // Deserialize
    let blob: PolicySnapshotBlob = serde_json::from_slice(&body_bytes)?;

    Ok(blob)
}
```

---

## Retention and Lifecycle

### Retention Policy

| Phase | Retention Period | Object Lock Mode | Rationale |
|-------|------------------|------------------|-----------|
| GOVERNANCE | 100 years | GOVERNANCE | Prevents accidental deletion; requires special privilege to delete |
| COMPLIANCE | 7 years | COMPLIANCE | Legal/regulatory requirement; cannot be bypassed |

### Lifecycle Configuration

```json
{
  "Rules": [
    {
      "ID": "ire-policy-snapshot-retention",
      "Status": "Enabled",
      "Filter": {"Prefix": ""},
      "Expiration": {"Date": "2036-04-03T00:00:00Z"},
      "Transitions": [
        {
          "Date": "2033-04-03T00:00:00Z",
          "StorageClass": "GLACIER"
        }
      ]
    }
  ]
}
```

### Migration: memory:// → S3

#### Phase 1: Dual-Write (Backfill)

During migration, both `memory://` URI (in PostgreSQL) and S3 blob must be written:

1. For new snapshots: write to both PostgreSQL and S3
2. For existing snapshots: backfill S3 from PostgreSQL scope data
3. After backfill: update `snapshot_uri` column from `memory://` to `s3://`

#### Phase 2: S3-Only

After migration verification:

1. Remove dual-write path
2. Validate all existing snapshots have valid S3 objects
3. Update application code to reject `memory://` URIs

#### Backfill Script (Pseudocode)

```python,ignore
def backfill_snapshot(snapshot: PolicySnapshot):
    # Reconstruct blob from PostgreSQL fields
    blob = PolicySnapshotBlob(
        snapshot_id=snapshot.id,
        intent_id=snapshot.intent_id,
        intent_version=snapshot.intent_version,
        rule_pack={...},  # From snapshot fields
        approval_scope=snapshot.scope_definition,
        metadata={...},
        integrity={...}
    )

    # Upload to S3
    key = build_key(snapshot.tenant_id, snapshot.intent_id, snapshot.intent_version)
    s3_client.put_object(key, blob)

    # Update URI in PostgreSQL
    snapshot.snapshot_uri = f"s3://ire-policy-snapshots/{key}"
    db.commit()
```

---

## Migration from memory:// URI Scheme

### URI Format Transition

| Phase | URI Format | Example |
|-------|-------------|---------|
| Current (Phase 2b) | `memory://policy-snapshots/{intent_id}/v{version}` | `memory://policy-snapshots/9f4b2e5a.../v3` |
| Target (Phase 3+) | `s3://ire-policy-snapshots/{tenant_id}/{intent_id}/v{version}/snapshot.json` | `s3://ire-policy-snapshots/550e8400.../9f4b2e5a.../v3/snapshot.json` |

### Migration Steps

1. **Schema migration**: Add `content_hash` column to `policy_snapshot` table
2. **Backfill S3**: For each `memory://` URI, create S3 object and update URI
3. **Verification**: Verify all S3 objects are accessible and hash-valid
4. **Code update**: Remove `memory://` URI handling from snapshot repository
5. **Validation**: Remove `memory://` URIs from test fixtures

---

## Integrity Verification

### Verification Levels

| Level | What is Verified | When to Use | Phase 3 Status |
|-------|------------------|-------------|----------------|
| **Hash-only** | content_hash matches computed hash | Quick verification | ✅ Implemented |
| **Chain** | Hash chain intact (previous_snapshot_hash linkage) | Full audit verification | ❌ Phase 4+ |
| **Object Lock** | GOVERNANCE/COMPLIANCE mode active | Compliance verification | ❌ Phase 4+ |

### Verification Procedure

```rust,ignore
async fn verify_snapshot_chain(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    tenant_id: Uuid,
    intent_id: Uuid,
    versions: &[i32],
) -> Result<ChainVerificationResult, SnapshotError> {
    let mut previous_hash: Option<String> = None;
    let mut results = Vec::new();

    for &version in versions.iter() {
        let key = build_key(tenant_id, intent_id, version);
        let blob = fetch_snapshot(s3_client, bucket, &key).await?;

        // Verify content hash
        let computed = Sha256::digest(blob.canonical_json());
        if computed != blob.integrity.content_hash {
            return Err(SnapshotError::ContentHashMismatch { version });
        }

        // Verify chain linkage
        if let Some(prev) = previous_hash {
            if blob.integrity.previous_snapshot_hash != prev {
                return Err(SnapshotError::ChainBroken { version });
            }
        }

        results.push(VersionVerification { version, valid: true });
        previous_hash = Some(blob.integrity.content_hash);
    }

    Ok(ChainVerificationResult { versions: results })
}
```

---

## Current Implementation Status

| Capability | Status | Notes |
|------------|--------|-------|
| `memory://` URI placeholder | ✅ Implemented | Phase 2b — used for development/testing only |
| PostgreSQL `policy_snapshot` table | ✅ Implemented | Phase 2b — stores scope data |
| `scope_hash` computation | ✅ Implemented | Phase 2b — SHA256 of canonical JSON |
| S3 upload/retrieval seam | ✅ Implemented | Phase 3 — write-only S3 path with memory fallback |
| S3 key/URI/schema derivation | ✅ Implemented | Deterministic key generation in `S3SnapshotStorage` |
| `memory://` fallback | ✅ Implemented | `InMemorySnapshotStorage` for tests/dev |
| Object Lock GOVERNANCE | ❌ Phase 4+ | NOT implemented — deferred to Phase 4 |
| 100-year retention | ❌ Phase 4+ | NOT implemented — deferred to Phase 4 |
| Chain-hash linkage | ❌ Phase 4+ | NOT implemented — deferred to Phase 4 |
| S3 lifecycle rules | ❌ Phase 4+ | NOT implemented — deferred to Phase 4 |
| `memory://` → S3 migration | ❌ Future | Not implemented — backfill procedure needed |

---

## Related Documents

- [03 — Policy Snapshot Specification](./03-policy-snapshot-spec.md) (Phase 2b bounded implementation)
- [07 — Approval Scope Canonicalization](../13-adrs/07-approval-scope-canonicalization.md) (ADR for snapshot design)
- [04 — Approval & Revalidation](./04-approval-revalidation.md) (Approval invalidation workflow)
- [Docker Compose Infrastructure](../infrastructure/local/docker-compose.yml) (MinIO service configuration)
