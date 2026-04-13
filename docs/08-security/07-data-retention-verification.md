# 07 — Data Retention & Deletion Verification (Bounded Slice)

**Status:** Phase 3 — P6-S1 Bounded Slice Delivered  
**Phase:** Phase 3 Batch 4b (Security Hardening)  
**Owner:** Security Team  
**Last Updated:** April 2026

---

## Mục đích

This document describes the **bounded retention verification slice** delivered in Phase 3 P6-S1. It provides:

1. **Retention period types** — local Rust types for specifying and querying retention periods per data category
2. **Verification helpers** — local logic to check if a given timestamp is within/outside retention
3. **Deletion request tracking types** — local types for tracking deletion requests and their status
4. **S3 lifecycle configuration template** — a local configuration artifact (not live cloud enforcement)

**Scope is bounded to local verification types and configuration artifacts. Actual S3 deletion enforcement requires out-of-band cloud tooling.**

---

## Delivered Artifacts

### Code: `crates/intent-rebase-types/src/retention_verification.rs`

```
RetentionPeriod              — per-category retention spec (hot/cold/total days)
standard_retention::*        — preset retention periods per data category
RetentionVerificationResult — verification check result
DeletionRequest              — deletion request tracking
DeletionRequestStatus       — status enum (Pending/Processing/Completed/Failed)
DeletionTargetType          — target enum (User/Tenant/Intent/Artifact)
S3LifecycleConfig           — S3 lifecycle configuration template
S3LifecycleRule             — individual lifecycle rule
S3StorageTransition          — storage class transition
```

### Standard Retention Periods

| Data Type | Hot (Postgres) | Cold (S3) | Total Retention |
|-----------|---------------|-----------|-----------------|
| Audit events | 90 days | 7 years (2555 days) | 7 years |
| Policy snapshots | 90 days | 10 years (3650 days) | 10 years |
| Provenance records | 90 days | 10 years (3650 days) | 10 years |
| Forensic bundles | 90 days | 7 years (2555 days) | 7 years |
| Rule pack history | 90 days | 5 years (1825 days) | 5 years |

---

## What Is Verified (This Slice)

### ✅ Retention Period Specification & Query

The `RetentionPeriod` type and `RetentionVerificationResult::verify()` provide local logic to determine:
- Whether a given timestamp is within hot storage retention
- Whether a timestamp is within total retention period
- Days until hot storage cutoff
- Days until total retention cutoff

This enables **local verification** that data lifecycle policies are being observed.

### ✅ Deletion Request Tracking Types

`DeletionRequest`, `DeletionRequestStatus`, and `DeletionTargetType` provide a local type system for tracking deletion requests through their lifecycle:
- `Pending` → `Processing` → `Completed` / `Failed`

### ✅ S3 Lifecycle Configuration Template

`S3LifecycleConfig::governance_bucket_config()` produces a local configuration artifact for governance data buckets including:
- Transition rules to GLACIER after 90 days
- Expiration rules per data category (7 years for audit events, 10 years for policy snapshots, etc.)

---

## What Is NOT Verified (Future Phase Scope)

### ❌ Live S3 Enforcement

The `S3LifecycleConfig` is a **local configuration template**. Actual S3 enforcement requires:
- AWS IAM policies restricting bucket access
- S3 Object Lock configuration via AWS console/API (`aws s3api put-object-lock-configuration`)
- AWS Config rules for compliance monitoring
- CloudTrail for audit logging of S3 operations

### ❌ Actual Deletion Execution

Deletion requests tracked via `DeletionRequest` are **local type tracking only**. Actual deletion execution requires:
- Production DB deletion procedures (soft delete → hard delete)
- S3 object deletion or quarantine move
- Backup rotation handling
- Verification sampling
- Deletion certificate issuance

### ❌ Backup Rotation Verification

Backups are handled by backup rotation procedures outside this codebase. Verification that backups comply with retention policies requires:
- Backup metadata retention checks
- Backup deletion verification logs

### ❌ GDPR/Compliance Deletion Workflow Automation

The types provide scaffolding but the full GDPR Right to Deletion workflow (request verification, multi-system deletion coordination, certificate issuance) remains Phase 4+.

---

## Usage Example

```rust
use intent_rebase_types::retention_verification::{
    RetentionVerificationResult, DeletionRequest, DeletionRequestStatus, DeletionTargetType,
    standard_retention,
};

// Check if an audit event is within retention
let audit_rp = standard_retention::audit_events();
let event_timestamp = Utc::now() - chrono::Duration::days(30);
let result = RetentionVerificationResult::verify(&audit_rp, event_timestamp);
assert!(result.is_within_retention);

// Create a deletion request
let request = DeletionRequest {
    id: Uuid::new_v4(),
    target_type: DeletionTargetType::User,
    target_id: user_id,
    tenant_id,
    reason: "GDPR data deletion request".to_string(),
    authorized_by: "privacy@example.com".to_string(),
    requested_at: Utc::now(),
    status: DeletionRequestStatus::Pending,
    completed_at: None,
    notes: None,
};
```

---

## Test Results

```
cargo test -p intent-rebase-types --all-features -- retention
running 11 tests
test retention_verification::tests::test_retention_period_new ... ok
test retention_verification::tests::test_retention_period_within_hot ... ok
test retention_verification::tests::test_retention_period_outside_hot ... ok
test retention_verification::tests::test_retention_period_within_total ... ok
test retention_verification::tests::test_retention_period_outside_total ... ok
test retention_verification::tests::test_retention_verification_result_within ... ok
test retention_verification::tests::test_retention_verification_result_outside ... ok
test retention_verification::tests::test_s3_lifecycle_config_governance_bucket ... ok
test retention_verification::tests::test_deletion_request_status_transitions ... ok
test retention_verification::tests::test_deletion_request_status_can_transition ... ok
test retention_verification::tests::test_s3_lifecycle_config_has_rule_pack_history ... ok

test result: ok. 11 passed; 0 failed
```

---

## Related Documents

- [05 — Immutable Retention & Tamper Resistance](../14-governance/05-immutable-retention-tamper-resistance.md) — longer-term retention strategy including S3 Object Lock and hash chains
- [09 — Data Handling & Redaction](../14-governance/09-data-handling-redaction.md) — PII handling, deletion workflow, and privacy principles
- [Phase 3 Checklist](./checklists/checklist-phase-3.md) — Section 7 (Security Hardening) item for data retention
- [Completion Proposals Tracker](./09-completion-proposals-tracker.md) — P6-S1 status update

---

## Phase 4+ Forward Looking Items

The following are documented as future phase requirements but are NOT in scope for this slice:

| Item | Description | Phase |
|------|-------------|-------|
| S3 Object Lock enforcement | Enable Object Lock on all governance buckets | Phase 4+ |
| Live S3 lifecycle policy enforcement | AWS Config rules monitoring S3 lifecycle compliance | Phase 4+ |
| Actual deletion execution API | API endpoint to trigger and track multi-system deletion | Phase 4+ |
| Deletion verification sampling | Automated verification that deleted data is actually gone | Phase 4+ |
| GDPR deletion certificate | Automated certificate issuance post-deletion | Phase 4+ |
| Backup retention verification | Backup metadata checks against retention policy | Phase 4+ |
