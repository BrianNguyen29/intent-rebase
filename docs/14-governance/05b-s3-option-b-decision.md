# 05b — S3 Option B: Current Implementation & Object Lock Deferral

**Status:** `DOCUMENTED — Design Decision Record`
**Phase:** Phase 3 — Governance Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** April 2026

---

## Purpose

This document records the **S3 Option B decision** — the current approach for S3-backed policy snapshot storage — and explicitly documents what is **not** implemented (Object Lock, 100-year retention, chain-hash linkage). This is a clarification document that separates the current bounded implementation from future Phase 4+ compliance requirements.

> **⚠️ Evidence Strength Disclaimer**
>
> This document describes the **current design decision and implementation state**. It does not represent a production-ready S3 storage system. Object Lock, GOVERNANCE/COMPLIANCE retention modes, and chain-hash verification are **Phase 4+ scope** and are explicitly deferred.

---

## S3 Storage Options Considered

### Option A: S3 with Object Lock (GOVERNANCE/COMPLIANCE) + Chain-Hash

**Description:** Full S3 implementation with:
- Object Lock enabled on bucket (GOVERNANCE or COMPLIANCE mode)
- 100-year retention period
- Chain-hash linkage between consecutive snapshots
- Hash chain verification on retrieval

**Status:** ❌ **NOT IMPLEMENTED — Deferred to Phase 4**

**Rationale for Deferral:**
- Object Lock requires bucket-level configuration that affects all objects
- GOVERNANCE mode allows privileged deletion (requires IAM policies)
- COMPLIANCE mode is irreversible (no deletion even by root)
- Chain-hash linkage requires versioning and additional metadata
- Requires external SRE/security review before production deployment

---

### Option B: S3 without Object Lock (Current Implementation)

**Description:** S3-backed storage with:
- Standard S3 durability (99.999999999% for Standard storage)
- Content hash verification (SHA256)
- Deterministic key structure
- **No** Object Lock, **no** GOVERNANCE/COMPLIANCE retention
- **No** chain-hash linkage
- **No** versioning

**Status:** ✅ **CURRENT CHOICE — Phase 3 Bounded Implementation**

**Rationale for Selection:**
- Provides basic content integrity via SHA256 hash verification
- Standard S3 durability is sufficient for non-compliance data
- Simpler to implement and test in Phase 3
- Can be migrated to Option A in Phase 4
- Backup/restore procedures can protect against accidental deletion

**Limitations:**
- Objects can be overwritten or deleted (no WORM protection)
- No tamper-evidence beyond content hash
- No chain-of-custody verification
- Not suitable for regulatory compliance requiring data immutability

---

## S3 Option B — Current Implementation

### Architecture

```
Intent Rebase Engine
       │
       ▼
┌──────────────────┐
│  S3SnapshotStorage │
│  (aws-sdk-s3)    │
└────────┬─────────┘
         │ PutObject / GetObject
         ▼
┌──────────────────┐
│  MinIO / S3      │
│  Bucket:         │
│  ire-policy-snapshots │
└──────────────────┘
```

### Key Design Elements

| Element | Implementation | Phase 3 Status |
|---------|---------------|-----------------|
| **Storage Backend** | MinIO (local dev), S3-compatible (staging/prod) | ✅ Implemented |
| **Object Key Structure** | `{tenant_id}/{intent_id}/v{intent_version}/snapshot.json` | ✅ Implemented |
| **Content Hash** | SHA256 of canonical JSON | ✅ Implemented |
| **Integrity Verification** | Verify content_hash on retrieval | ✅ Implemented |
| **Upload Path** | Write to S3, store URI in PostgreSQL | ✅ Implemented |
| **Retrieval Path** | Fetch from S3, verify hash, deserialize | ✅ Implemented |
| **Object Lock** | ❌ NOT implemented | Phase 4+ |
| **Versioning** | ❌ NOT implemented | Phase 4+ |
| **Chain-Hash Linkage** | ❌ NOT implemented | Phase 4+ |
| **GOVERNANCE Retention** | ❌ NOT implemented | Phase 4+ |
| **COMPLIANCE Retention** | ❌ NOT implemented | Phase 4+ |

### Object Key Examples

```
ire-policy-snapshots/550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v1/snapshot.json
ire-policy-snapshots/550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v2/snapshot.json
ire-policy-snapshots/550e8400e29b41d4a716446655440000/9f4b2e5a8c3d4b1e9a2c8d7f6e5b4a3/v3/snapshot.json
```

### S3 Client Configuration (Phase 3)

```rust,ignore
// S3SnapshotStorage — Phase 3 Bounded Implementation
// Object Lock: NOT enabled
// Chain-hash: NOT implemented
// Versioning: NOT enabled

use aws_sdk_s3::Client;

pub struct S3SnapshotStorage {
    client: Client,
    bucket: String,
}

impl S3SnapshotStorage {
    pub async fn new(bucket: &str) -> Result<Self, SnapshotError> {
        let config = aws_config::load_defaults(
            aws_config::BehaviorVersion::latest()
        ).await;

        let client = Client::new(&config);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
        })
    }

    pub async fn upload(
        &self,
        tenant_id: &Uuid,
        intent_id: &Uuid,
        version: i32,
        blob: &PolicySnapshotBlob,
    ) -> Result<String, SnapshotError> {
        // 1. Serialize to canonical JSON
        let content = serde_json::to_string(blob)
            .map_err(|e| SnapshotError::SerializationError(e.to_string()))?;

        // 2. Compute SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        // 3. Build object key
        let key = format!(
            "{}/{}/v{}/snapshot.json",
            tenant_id.simple(),
            intent_id.simple(),
            version
        );

        // 4. Upload to S3 (NO Object Lock headers)
        let put_output = self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(content.into())
            .content_type("application/json")
            .content_sha256(&content_hash)  // Checksum only, not Object Lock
            .send()
            .await
            .map_err(|e| SnapshotError::UploadFailed(e.to_string()))?;

        // 5. Return S3 URI
        Ok(format!("s3://{}/{}", self.bucket, key))
    }

    pub async fn download(
        &self,
        snapshot_uri: &str,
        expected_hash: &str,
    ) -> Result<PolicySnapshotBlob, SnapshotError> {
        // 1. Extract key from URI
        let key = snapshot_uri
            .strip_prefix(&format!("s3://{}/", self.bucket))
            .ok_or_else(|| SnapshotError::InvalidUri(snapshot_uri.to_string()))?;

        // 2. Fetch from S3
        let get_output = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| SnapshotError::DownloadFailed(e.to_string()))?;

        // 3. Collect body bytes
        let body = get_output.body.collect().await?;
        let body_bytes = body.to_vec();

        // 4. Verify content hash (NOT Object Lock — just checksum)
        let mut hasher = Sha256::new();
        hasher.update(&body_bytes);
        let actual_hash = format!("{:x}", hasher.finalize());

        if actual_hash != expected_hash {
            return Err(SnapshotError::IntegrityMismatch {
                expected: expected_hash.to_string(),
                actual: actual_hash,
            });
        }

        // 5. Deserialize
        let blob = serde_json::from_slice(&body_bytes)
            .map_err(|e| SnapshotError::DeserializationError(e.to_string()))?;

        Ok(blob)
    }
}
```

---

## What Object Lock Provides (Phase 4+)

### GOVERNANCE Mode (Phase 4+ Scope)

GOVERNANCE mode prevents accidental deletion but allows privileged users to remove Object Lock:

```bash
# Enable GOVERNANCE mode Object Lock on bucket
aws s3api create-bucket \
  --bucket ire-policy-snapshots \
  --object-lock-enabled-for-bucket \
  --region us-east-1

# Set default retention (GOVERNANCE, 100 years)
aws s3api put-object-lock-configuration \
  --bucket ire-policy-snapshots \
  --object-lock-configuration '{
    "ObjectLockMode": "GOVERNANCE",
    "ObjectLockEnabled": "Enabled",
    "Rule": {
      "DefaultRetention": {
        "Mode": "GOVERNANCE",
        "Years": 100
      }
    }
  }'

# After GOVERNANCE enabled, objects cannot be deleted for 100 years
# unless the caller has s3:BypassGovernanceRetention permission
```

### COMPLIANCE Mode (Phase 4+ Scope)

COMPLIANCE mode is irreversible — even the root user cannot delete until retention expires:

```bash
# Set COMPLIANCE retention (irreversible until retention period expires)
aws s3api put-object-retention \
  --bucket ire-policy-snapshots \
  --key "tenant_id/intent_id/v1/snapshot.json" \
  --retention '{
    "Mode": "COMPLIANCE",
    "RetainUntilDate": "2126-04-29T00:00:00Z"
  }'

# WARNING: COMPLIANCE mode cannot be removed until RetainUntilDate passes
# This is permanent until the retention period expires
```

### Why Both Are Deferred

| Concern | GOVERNANCE | COMPLIANCE |
|---------|-----------|------------|
| Accidental deletion prevention | ✅ Yes | ✅ Yes |
| Privileged deletion (root/IAM) | ⚠️ Can bypass | ❌ Cannot bypass |
| Reversibility | ✅ Yes (with permission) | ❌ No (until retention expires) |
| Operational complexity | Medium | High |
| External audit required | Recommended | Required |
| Phase 3 suitability | Too complex | Not suitable |

---

## Chain-Hash Linkage (Phase 4+ Scope)

### What Chain-Hash Provides

Chain-hash linkage creates a cryptographically verifiable sequence of snapshots:

```
snapshot_v1.json
  └── content_hash: sha256(...)
      └── integrity.previous_snapshot_hash: null (genesis)

snapshot_v2.json
  └── content_hash: sha256(...)
      └── integrity.previous_snapshot_hash: sha256(snapshot_v1 content)

snapshot_v3.json
  └── content_hash: sha256(...)
      └── integrity.previous_snapshot_hash: sha256(snapshot_v2 content)
```

### Verification Procedure (Phase 4+)

```rust,ignore
async fn verify_snapshot_chain(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    tenant_id: Uuid,
    intent_id: Uuid,
    versions: &[i32],
) -> Result<ChainVerificationResult, SnapshotError> {
    let mut previous_hash: Option<String> = None;

    for &version in versions.iter() {
        // Fetch snapshot
        let blob = fetch_snapshot(s3_client, bucket, tenant_id, intent_id, version).await?;

        // Verify content hash
        let computed = Sha256::digest(serde_json::to_vec(&blob)?);
        if format!("{:x}", computed) != blob.integrity.content_hash {
            return Err(SnapshotError::ContentHashMismatch { version });
        }

        // Verify chain linkage
        if let Some(prev) = previous_hash {
            if blob.integrity.previous_snapshot_hash != Some(prev.clone()) {
                return Err(SnapshotError::ChainBroken { version });
            }
        }

        previous_hash = Some(blob.integrity.content_hash);
    }

    Ok(ChainVerificationResult { valid: true })
}
```

### Why Chain-Hash Is Deferred

- Requires S3 versioning to be enabled (additional storage cost)
- Requires additional metadata field (`previous_snapshot_hash`) in blob
- Verification procedure is complex and requires careful implementation
- Immutability is better served by Object Lock + backup strategy
- Not required for Phase 3 compliance scope

---

## Migration Path: Option B → Option A

### Migration Prerequisites

Before migrating from Option B to Option A:

1. ✅ Phase 3 S3 implementation is stable and tested
2. ✅ Backup/restore procedures are documented and validated
3. ✅ External SRE/security review confirms Object Lock requirement
4. ✅ Retention policy is approved by legal/compliance
5. ✅ Migration procedure is tested in staging environment

### Migration Steps (Phase 4+)

```
Phase 1: Dual-Write with Object Lock
─────────────────────────────────────
1. Enable Object Lock on bucket (GOVERNANCE mode initially)
2. Update S3SnapshotStorage to include Object Lock headers on new uploads
3. Existing objects without Object Lock continue to work (read-only)
4. Monitor: Verify new uploads have Object Lock applied

Phase 2: Backfill Existing Objects
───────────────────────────────────
1. For each existing snapshot in PostgreSQL:
   a. Fetch current blob from S3
   b. Re-upload with Object Lock headers
   c. Update metadata in PostgreSQL if needed
2. This is a long-running operation for large datasets

Phase 3: Enable COMPLIANCE Mode
────────────────────────────────
1. After backfill is complete and verified
2. Switch from GOVERNANCE to COMPLIANCE mode
3. WARNING: COMPLIANCE is irreversible — ensure legal/compliance approval

Phase 4: Chain-Hash Linkage (Optional)
───────────────────────────────────────
1. If chain-hash linkage is required:
   a. Enable S3 versioning
   b. Update blob schema to include previous_snapshot_hash
   c. Implement chain verification procedure
   d. Backfill previous_snapshot_hash for existing snapshots
```

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `S3 storage is immutable` | `S3 storage uses Standard durability; Object Lock is Phase 4+ scope` |
| `S3 storage is tamper-evident` | `Content hash is verified on retrieval; chain-hash linkage is Phase 4+ scope` |
| `Data is protected for 100 years` | `Target retention is 100 years; Object Lock implementation is Phase 4+` |
| `Option A is implemented` | `Option B (current) is implemented; Option A (Object Lock) is Phase 4+ deferred` |
| `S3 storage meets compliance requirements` | `S3 Standard storage provides 99.999999999% durability; compliance-grade retention is Phase 4+ scope` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/14-governance/05-s3-snapshot-blob-spec.md` | Full S3 specification including Object Lock and chain-hash (Phase 4+ sections) |
| `docs/14-governance/05-immutable-retention-tamper-resistance.md` | Immutable retention strategy (Object Lock is one layer) |
| `docs/09-operations/07-backup-restore.md` | Backup procedures complement S3 storage (backup provides deletion protection in Phase 3) |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (fixer) | Initial creation — S3 Option B decision recorded; Object Lock/chain-hash/retention explicitly deferred to Phase 4+; migration path documented |
