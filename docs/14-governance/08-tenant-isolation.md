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

**Bounded consumer-side guard delivered (first slice):**
- `crates/intent-api/src/nats_jetstream/consumer.rs` — `NatsPullConsumerAdapter` with optional `tenant_scope: Option<Uuid>`
- `with_tenant_scope` builder method added; no breaking changes to call sites
- Cross-tenant events are rejected **before** domain side effects (`process_one`)
- Rejected events are acked to prevent infinite redelivery
- Unit tests cover: matching tenant, mismatched tenant, unscoped behavior
- **Preserves current shared-consumer behavior when `tenant_scope` is `None`**

**Bounded delivered / manual local-dev evidence:**
- Live NATS integration tests for tenant isolation (4 `#[ignore]` tests in `crates/intent-api/src/nats_jetstream/tests_live_integration.rs`)
  - `live_jetstream_tenant_scope_matching_tenant_consumes`
  - `live_jetstream_tenant_scope_mismatched_tenant_rejects_and_acks`
  - `live_jetstream_tenant_scope_unscoped_consumes_all`
  - `live_jetstream_tenant_scope_missing_tenant_rejects`
  - Run: `cargo test -p intent-api --lib -- nats_jetstream::tests_live_integration::live_jetstream_tenant_scope --ignored -- --test-threads=1`
  - These tests validate `with_tenant_scope` through the real JetStream `process_one` path but do NOT claim production topology, ACLs, or certification

**Out of scope / remains pending:**
- Per-tenant JetStream streams — 🟡 MIGRATION PATH DOCUMENTED (see below); server-side rollout pending and requires shared-stream replacement, not augmentation
- NATS user/subject ACLs — 🔴 PENDING
- Production NATS topology changes — 🔴 PENDING (blocked on A-03..A-05 external gates)
- External security/SRE sign-off for NATS isolation — 🔴 PENDING

---

## Per-Tenant Stream Migration Path

> **Status:** RUNBOOK DOCUMENTED — no code changes; not executed; not production-ready

This section documents the path from the current **shared-stream architecture** to per-tenant JetStream streams. It is a planning and local-prep runbook only. No server-side rollout has been performed, and no production claim is made.

### Current Architecture

- **Shared stream:** `audit_events` with subject filter `audit.events.v1.>`
- **Publisher subjects:** `audit.events.v1.<tenant_id>.<event_type>` (see `crates/intent-api/src/nats_event_publisher.rs`)
- **Consumer guard:** `NatsPullConsumerAdapter::tenant_scope` rejects cross-tenant events before domain side effects (`process_one`)
- **Stream initializer:** `JetStreamInitializer` creates the shared stream idempotently (see `crates/intent-api/src/nats_jetstream/stream.rs`)

The shared stream stores all audit events for all tenants. The consumer-side guard provides bounded tenant isolation at the application layer, but it does **not** provide server-side subject isolation.

### Subject-Filter Overlap / Duplication Risk

JetStream stores a copy of every message in **each stream whose subject filter matches**. This means:

- Shared stream filter: `audit.events.v1.>` — matches **all** tenant-scoped events
- Per-tenant stream filter: `audit.events.v1.{tenant_id}.>` — matches **one** tenant's events

If a per-tenant stream is added **alongside** the shared stream, every message will be stored in **both** streams. This is:

- **Duplicate storage** with no retention benefit
- **NOT additive isolation** — the shared stream still contains all tenant data
- **A migration hazard** if operators assume per-tenant streams can be created incrementally without removing the shared stream

> **Rule:** Per-tenant streams must **replace** or **narrow** the shared stream, not augment it.

### Staged Migration Sequence

#### Stage 1 — Local-Executable Prep (no running server changes)

1. **Inventory:** List all active tenants and their `tenant_id` UUIDs.
2. **Subject mapping:** For each tenant, define the per-tenant subject filter: `audit.events.v1.{tenant_id}.>`.
3. **Stream config templates:** Prepare JetStream `Config` JSON files (or `nats stream add` commands) for each tenant stream.
4. **Local validation:** Spin up a local NATS container (e.g., `docker compose -f infrastructure/local/docker-compose.yml up nats`) and validate configs with `nats-box`.
5. **Consumer audit:** Identify all consumer groups that bind to `audit_events` and plan their migration to per-tenant streams.

#### Stage 2 — Server-Side Rollout (requires coordination)

Choose **one** of the following strategies:

**Strategy A — Narrow then Add:**
1. Narrow the shared `audit_events` stream filter to exclude tenant-scoped subjects (e.g., remove `audit.events.v1.>` and replace with non-tenant subjects only, if any exist).
2. Create per-tenant streams with `audit.events.v1.{tenant}.>`.
3. Migrate consumers.

**Strategy B — Replace (recommended if no non-tenant subjects exist):**
1. Create per-tenant streams with replica configs matching the shared stream.
2. Start new consumer groups on per-tenant streams (dual-consume window).
3. Verify message continuity on per-tenant streams.
4. Stop and delete old consumer groups bound to `audit_events`.
5. Delete the shared `audit_events` stream once confirmed empty / no active consumers.

> **Caveat:** Both strategies require a coordinated rollout. There is no zero-downtime path without a dual-consume window and careful sequencing.

#### Stage 3 — Consumer Migration

1. Update consumer definitions to bind to the tenant-specific stream name (e.g., `audit_events_{tenant_id}`).
2. Preserve `tenant_scope` as defense-in-depth: even with per-tenant streams, the consumer guard should still reject events where the embedded tenant claim does not match the expected tenant.
3. Validate with local live integration tests: `cargo test -p intent-api --lib -- nats_jetstream::tests_live_integration --ignored -- --test-threads=1`

#### Stage 4 — Shared Stream Removal & Verification

1. Confirm no active consumers reference `audit_events`.
2. Check stream info for pending messages; drain or ack as needed.
3. Delete `audit_events` shared stream.
4. Verify per-tenant stream info shows expected message counts.

### External Blockers (cannot be closed locally)

| Blocker | Why It Blocks Migration | Owner |
|---------|------------------------|-------|
| NATS ACL design for per-tenant service accounts | Per-tenant streams without ACLs do not prevent a compromised service from subscribing to another tenant's stream | Security / Platform |
| External SRE sign-off for topology change | Stream deletion and consumer migration affect durability guarantees and observability baselines | SRE |
| Staging environment with real multi-tenant load | Migration must be validated under load to prove no message loss or ordering violations | SRE / Backend Lead |
| External security review | Per-tenant stream topology is part of the tenant isolation evidence packet | Security |

### Illustrative Local-Dev Commands (not executed evidence)

The commands below are illustrative examples for local development with `nats-box`. They are **not** evidence of a completed migration.

```bash
# 1. Inspect the current shared stream
 docker run --rm --network local_default natsio/nats-box:latest \
   nats stream info audit_events -s nats://nats:4222

# 2. List current consumers on the shared stream
 docker run --rm --network local_default natsio/nats-box:latest \
   nats consumer ls audit_events -s nats://nats:4222

# 3. Create a per-tenant stream (illustrative — DO NOT run alongside shared stream)
 docker run --rm --network local_default natsio/nats-box:latest \
   nats stream add audit_events_tenant_a \
     --subjects="audit.events.v1.550e8400-e29b-41d4-a716-446655440000.>" \
     --storage=file \
     --retention=limits \
     -s nats://nats:4222

# 4. Verify per-tenant stream info
 docker run --rm --network local_default natsio/nats-box:latest \
   nats stream info audit_events_tenant_a -s nats://nats:4222

# 5. Create a tenant-scoped consumer on the per-tenant stream
 docker run --rm --network local_default natsio/nats-box:latest \
   nats consumer add audit_events_tenant_a tenant-a-audit-consumer \
     --filter="audit.events.v1.550e8400-e29b-41d4-a716-446655440000.>" \
     --pull \
     --ack=explicit \
     --max-deliver=3 \
     -s nats://nats:4222
```

> **Warning:** Running the per-tenant stream creation (step 3) while the shared `audit_events` stream still has filter `audit.events.v1.>` will cause **duplicate storage** of every matching message. Only execute after the shared stream filter has been narrowed or the shared stream has been removed.

### What Is Preserved During Migration

- `NatsPullConsumerAdapter::tenant_scope` remains active as a defense-in-depth layer.
- Existing live integration tests (`tests_live_integration.rs`) continue to validate consumer-side behavior; they do not claim server-side isolation.
- The `NatsEventPublisher` tenant-scoped subject pattern (`audit.events.v1.<tenant_id>.<event_type>`) does not need to change.

### Related Documents

- `docs/08-security/02-authn-authz.md` — NATS tenant isolation pending items
- `docs/10-delivery/22-phase-4-entry-plan.md` — A-02 (RLS/NATS completion) and A-10 (DLQ/NATS lifecycle)
- `crates/intent-api/src/nats_jetstream/stream.rs` — shared stream initializer
- `crates/intent-api/src/nats_event_publisher.rs` — tenant-scoped subject publishing

---

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