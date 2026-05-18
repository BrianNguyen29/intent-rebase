# Tenant Onboarding Runbook

> **Note:** The `06-` prefix in this filename is a legacy sequence number. It overlaps with `06-slo-dashboard.md` in the same directory; the numbering is non-semantic and does not indicate ordering or dependency.

**Phase:** Phase 3 P3-S5 (scaffold/baseline only)  
**Status:** IN PROGRESS — full automation and API endpoints are future phase scope  
**Owner:** Platform Team

---

## Purpose

This runbook documents the tenant onboarding procedure for the Intent Rebase Engine platform. It provides step-by-step instructions for provisioning a new tenant from a standing start.

> **P3-S5 Scope Note:** This document describes the manual/administrative procedure for onboarding a tenant. Full API-driven automation, S3 bucket provisioning, NATS account creation, and RBAC setup are future phase scope. The `tenant-service` crate delivered in P3-S5 provides the Tenant model and repository scaffold only.

---

## Prerequisites

Before onboarding a tenant, ensure the following are available:

- [ ] PostgreSQL database with `tenants` table (migration pending — future phase)
- [ ] Access to the Intent Rebase Engine admin tooling
- [ ] Tenant details: name, slug, target region

---

## Onboarding Procedure

### Step 1: Create Tenant Record

Create a new tenant record via the tenant service repository:

```rust
use tenant_service::{Tenant, TenantRegion, TenantStatus, TenantRepository, InMemoryTenantRepository};

let repo = InMemoryTenantRepository::new();
let tenant = Tenant::new(
    "Acme Corp".to_string(),
    "acme-corp".to_string(),
    TenantRegion::UsEast1,
);
let created = repo.create(tenant).await?;
println!("Created tenant: {}", created.id);
```

> **P3-S5 Note:** This uses the in-memory repository. SQL-backed persistence is future phase.

**Expected output:** A new `Tenant` record with:
- `status`: `TenantStatus::Provisioning`
- `slug`: `"acme-corp"` (unique)
- `region`: `"us_east_1"`

---

### Step 2: Activate Tenant

After the tenant record is created and all infrastructure is provisioned (future phase):

```rust
repo.update_status(tenant_id, TenantStatus::Active).await?;
```

**Valid status transitions:**
- `Provisioning` → `Active` (activation after provisioning complete)
- `Active` → `Suspended` (temporary suspension)
- `Suspended` → `Active` (reactivation)
- `Active` → `Offboarding` (initiate offboarding)
- `Offboarding` → `Offboarded` (after data deletion complete)

---

### Step 3: Verify Tenant Status

```rust
let tenant = repo.get(tenant_id).await?;
assert!(tenant.is_active());
assert!(tenant.allows_read());
assert!(tenant.allows_write());
```

---

## Tenant Status Lifecycle

| Status | Allows Read | Allows Write | Notes |
|--------|-------------|--------------|-------|
| `Provisioning` | No | No | Tenant being provisioned |
| `Active` | Yes | Yes | Fully operational |
| `Suspended` | Yes | No | Can be reactivated |
| `Offboarding` | No | No | Data deletion in progress |
| `Offboarded` | No | No | Archived, billing records remain |

---

## Out of Scope (Future Phase)

The following are **NOT** implemented in P3-S5 and remain as future phase work:

- [ ] SQL-backed `TenantRepository` (`SqlxTenantRepository`)
- [ ] Public API endpoints for tenant CRUD operations
- [ ] S3 bucket/prefix creation for new tenants
- [ ] NATS service account and consumer group provisioning
- [ ] Initial RBAC role assignment
- [ ] API key generation for tenant
- [ ] Monitoring dashboard setup
- [ ] Billing tracking integration
- [ ] Offboarding deletion orchestration

---

## Rollback

If onboarding fails at any step:

1. Verify tenant status: `repo.get(tenant_id)`
2. If in `Provisioning` status, delete is not yet supported (future phase)
3. If in `Active` status, suspend the tenant: `repo.update_status(id, TenantStatus::Suspended)`

---

## Verification Commands

```bash
# Check tenant-service compiles
cargo check -p tenant-service

# Run tenant-service tests
cargo test -p tenant-service --all-features
```

---

## Related Documents

- [08 — Tenant Isolation](../14-governance/08-tenant-isolation.md)
- [05 — Immutable Retention & Tamper Resistance](../14-governance/05-immutable-retention-tamper-resistance.md)
- [06 — Threat Model v2](../14-governance/06-threat-model-v2.md)
