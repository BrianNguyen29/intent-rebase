# 08 — Tenant Isolation

**Status:** Proposed  
**Phase:** Phase 3  
**Owner:** Platform Team

---

## Mục đích

Define and verify tenant isolation guarantees to ensure:
- **No cross-tenant data access** — intentional or accidental
- **No cross-tenant data leakage** — in logs, metrics, or exports
- **No cross-tenant interference** — resource consumption

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

### Layer 4: Rule Pack Registry Isolation (P3-S3 bounded slice)

```rust
// Tenant-scoped rule pack repository trait
pub trait TenantRulePackRepository: Send + Sync {
    async fn list_packs(&self, tenant_id: Uuid, status: Option<RulePackStatus>) -> Result<Vec<RulePack>, RulePackRegistryError>;
    async fn get_pack(&self, tenant_id: Uuid, version: &RulePackVersion) -> Result<RulePack, RulePackRegistryError>;
    async fn get_active_pack(&self, tenant_id: Uuid) -> Result<RulePack, RulePackRegistryError>;
    async fn create_pack(&self, tenant_id: Uuid, pack: RulePack) -> Result<RulePack, RulePackRegistryError>;
    async fn update_pack_status(&self, tenant_id: Uuid, version: &RulePackVersion, status: RulePackStatus) -> Result<RulePack, RulePackRegistryError>;
    async fn list_versions(&self, tenant_id: Uuid) -> Result<Vec<RulePackVersion>, RulePackRegistryError>;
}

// In-memory implementation for testing
pub struct InMemoryTenantRulePackRepository {
    packs: RwLock<HashMap<Uuid, HashMap<RulePackVersion, RulePack>>>,
}

// Isolation: all methods require tenant_id; cross-tenant access returns NotFound
```

**P3-S3 bounded slice delivered:**
- `crates/rebase-engine/src/rule_pack_registry.rs` — registry primitives (trait + InMemory impl)
- `crates/rebase-engine/src/rule_pack.rs` — `RulePackVersion` now derives `Hash`
- 8 tenant isolation tests passing in `cargo test -p rebase-engine --all-features`

**Out of scope for this slice:**
- Full upload/management API (Phase 4+)
- S3/object storage integration
- Rule evaluation engine rewiring

### Layer 6: Tenant Service (P3-S5 bounded slice)

```rust
// Tenant repository trait
#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn create(&self, tenant: Tenant) -> Result<Tenant, IntentRebaseError>;
    async fn get(&self, tenant_id: Uuid) -> Result<Tenant, IntentRebaseError>;
    async fn get_by_slug(&self, slug: &str) -> Result<Tenant, IntentRebaseError>;
    async fn list_by_status(&self, status: TenantStatus) -> Result<Vec<Tenant>, IntentRebaseError>;
    async fn update_status(&self, tenant_id: Uuid, new_status: TenantStatus) -> Result<Tenant, IntentRebaseError>;
    async fn list_all(&self, limit: Option<usize>) -> Result<Vec<Tenant>, IntentRebaseError>;
}

// Tenant model with status lifecycle
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: TenantStatus,
    pub region: TenantRegion,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**P3-S5 bounded slice delivered:**
- `crates/tenant-service/src/lib.rs` — service scaffold with re-exports
- `crates/tenant-service/src/tenant.rs` — `Tenant` model, `TenantStatus` enum, `TenantRegion` enum
- `crates/tenant-service/src/tenant_repo.rs` — `TenantRepository` trait + `InMemoryTenantRepository` implementation
- `crates/intent-rebase-types/src/error.rs` — `TenantNotFound` and `TenantNotFoundBySlug` error variants
- Tests: `cargo test -p tenant-service --all-features` (15 tests pass)

**Out of scope for this slice:**
- SQL persistence (`SqlxTenantRepository` — future phase)
- Public API endpoints for tenant CRUD (future phase)
- Residency enforcement/routing (future phase)
- Offboarding deletion orchestration (future phase)
- Quota enforcement (future phase)

### Layer 5: NATS Isolation

```
# NATS subjects are tenant-scoped
audit.events.v1.{tenant_id}.>
rebase.signals.{tenant_id}.>

# Consumer group limited to tenant
consumer: {tenant_id}-audit-consumer
filter subject: audit.events.v1.{tenant_id}.>
```

### Layer 7: Audit Query API Isolation (P3-S4 bounded slice)

```rust
// Tenant-scoped audit repository trait
#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Get a single audit event by ID (tenant-scoped).
    /// Returns Err(ArtifactNotFound) if event doesn't exist or belongs to a different tenant.
    async fn get_audit_event(
        &self,
        event_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<AuditEvent, IntentRebaseError>;

    /// List audit events by tenant (ordered by occurred_at descending)
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<AuditEvent>, IntentRebaseError>;
}

// API endpoints enforce tenant scoping
// GET /audit/events?tenant_id=xxx&limit=yyy - list events for tenant
// GET /audit/events/{event_id}?tenant_id=xxx - get single event (404 if wrong tenant)
```

**P3-S4 bounded slice delivered:**
- `crates/intent-rebase-types/src/audit_repo.rs` — `get_audit_event` method added to trait
- `crates/intent-api/src/lib.rs` — `GET /audit/events` and `GET /audit/events/{event_id}` endpoints
- `crates/intent-api/src/lib.rs` — Cross-tenant isolation tests verifying:
  - Tenant A's events are not visible in Tenant B's queries
  - `GET /audit/events/{event_id}` returns 404 for wrong tenant
  - `GET /audit/events` returns only tenant's own events
- Tests: `cargo test -p intent-api --all-features` (cross-tenant audit tests pass)

**Out of scope for this slice:**
- S3 cold storage and archival (Phase 4+)
- Cross-service audit streaming via NATS consumers
- Audit event retention/lifecycle policies

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