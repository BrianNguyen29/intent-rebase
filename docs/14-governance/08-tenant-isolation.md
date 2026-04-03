# 08 — Tenant Isolation

**Status:** Proposed  
**Phase:** Phase 3  
**Owner:** Platform Team

---

## Mục đích

Define and verify tenant isolation guarantees to ensure:
- **No cross-tenant data access** — intentional or accidental
- **No cross-tenant data leakage** — in logs, metrics, or exports
- **No cross-tenant interference** — resource consumption隔离

---

## Isolation Layers

### Layer 1: Database Isolation

```sql
-- All tenant-scoped tables include tenant_id as first column
CREATE TABLE intents (
  tenant_id UUID NOT NULL,
  id UUID NOT NULL,
  ...
  PRIMARY KEY (tenant_id, id)
);

-- Enforce tenant_id in all queries
CREATE POLICY tenant_isolation ON intents
  USING (tenant_id = current_tenant_id());

-- Row-level security enabled
ALTER TABLE intents ENABLE ROW LEVEL SECURITY;
```

### Layer 2: API Isolation

```rust
// All API handlers extract tenant_id from JWT claims
async fn get_intent(
    claims: JwtClaims,
    path: Path<IntentPath>,
) -> Result<Intent> {
    // tenant_id from JWT, not from request path
    let tenant_id = claims.tenant_id;
    
    // Query includes tenant_id filter
    let intent = db.query(
        "SELECT * FROM intents WHERE tenant_id = $1 AND id = $2",
        tenant_id, path.id
    ).await?;
    
    Ok(intent)
}
```

### Layer 3: Storage Isolation (S3)

```
# S3 bucket structure enforces tenant boundary
ire-artifacts/
  {tenant_id_1}/
    intents/
    artifacts/
    forensic-bundles/
  {tenant_id_2}/
    intents/
    artifacts/
    forensic-bundles/

# S3 bucket policy denies cross-tenant access
{
  "Effect": "Deny",
  "Principal": "*",
  "Action": "s3:*",
  "Resource": [
    "arn:aws:s3:::ire-artifacts/${jwt.tenant_id}/*"
  ],
  "Condition": {
    "StringNotEquals": {
      "s3:ResourceAccount": "${jwt.tenant_id}"
    }
  }
}
```

### Layer 4: NATS Isolation

```
# NATS subjects are tenant-scoped
audit.events.v1.{tenant_id}.>
rebase.signals.{tenant_id}.>

# Consumer group limited to tenant
consumer: {tenant_id}-audit-consumer
filter subject: audit.events.v1.{tenant_id}.>
```

---

## Verification Tests

### Cross-Tenant Access Tests

```rust
#[test]
async fn test_cross_tenant_intent_access_blocked() {
    let tenant_a = create_tenant("TenantA");
    let tenant_b = create_tenant("TenantB");
    
    let intent = create_intent(tenant_a, "secret-intent");
    
    // Tenant B attempts to access Tenant A's intent
    let response = tenant_b_api.get(&format!("/intents/{}", intent.id)).await;
    
    // Must be rejected
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
async fn test_cross_tenant_audit_blocked() {
    let tenant_a = create_tenant("TenantA");
    let tenant_b = create_tenant("TenantB");
    
    let _event = create_audit_event(tenant_a, "secret-action");
    
    // Tenant B queries audit for Tenant A's events
    let response = tenant_b_api.get("/audit/events").await;
    
    // Must return only Tenant B's events
    let events: Vec<AuditEvent> = response.json().await;
    assert!(events.iter().all(|e| e.tenant_id != tenant_a.id));
}
```

### Data Leakage Tests

```rust
#[test]
async fn test_logs_contain_no_cross_tenant_data() {
    // Perform operations across multiple tenants
    for tenant in all_tenants() {
        perform_operations(tenant);
    }
    
    // Collect all log output
    let logs = capture_logs();
    
    // Verify no tenant data appears in other tenant's logs
    for (tenant_a, tenant_b) in all_tenant_pairs() {
        let tenant_a_data = get_tenant_data(tenant_a);
        for log_line in logs.iter() {
            // No tenant A data in tenant B's log context
            assert!(!contains_cross_tenant_data(log_line, tenant_a_data));
        }
    }
}
```

---

## Resource Quotas

### Per-Tenant Limits

| Resource | Limit | Scope |
|----------|-------|-------|
| Intents | 10,000 per tenant | Hard limit |
| Artifacts | 100,000 per tenant | Hard limit |
| Audit events/day | 1,000,000 per tenant | Soft limit |
| Storage | 1 TB per tenant | Hard limit |
| API requests/min | 10,000 per tenant | Soft limit |
| Concurrent rebase operations | 10 per tenant | Soft limit |

### Quota Enforcement

```rust
async fn enforce_quota(tenant_id: &Uuid, resource: &ResourceType) -> Result<()> {
    let usage = get_current_usage(tenant_id, resource).await?;
    let limit = get_quota_limit(tenant_id, resource).await?;
    
    if usage >= limit {
        return Err(QuotaExceeded {
            tenant_id: *tenant_id,
            resource: resource.clone(),
            usage,
            limit,
        });
    }
    
    Ok(())
}
```

---

## Tenant Onboarding/Offboarding

### Onboarding Procedure

```
1. Create tenant record in PostgreSQL
2. Create tenant-scoped S3 buckets/prefixes
3. Create NATS service accounts and consumer groups
4. Create initial RBAC roles
5. Create tenant API keys
6. Set up monitoring dashboards
7. Set up billing tracking
```

### Offboarding Procedure (Data Deletion)

```
1. Lock tenant account (prevent new operations)
2. Wait for in-flight operations to complete
3. Export forensic bundle (for compliance)
4. Delete all artifacts from S3
5. Delete all audit events (after retention check)
6. Delete all intent records
7. Delete tenant configuration
8. Archive tenant record (for billing/history)
9. Revoke all API keys
10. Confirm deletion with signed confirmation
```

---

## Related Documents

- [07 — Authorization Matrix](./07-authz-matrix.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [06 — Threat Model v2](./06-threat-model-v2.md)