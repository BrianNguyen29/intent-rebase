# Authentication and Authorization

## Current Implementation Status

> **Bounded slice (Phase 2b):** JWT production guard and RLS session helper scaffold exist, but full JWT→SQL RLS end-to-end enforcement remains **pending implementation/testing**.

### JWT Production Guard (Bounded First Slice)

A production JWT guard is scaffolded via `INTENT_API_REQUIRE_JWT=true`:

- **Env var:** `INTENT_API_REQUIRE_JWT=true` activates strict JWT validation
- **When enabled:**
  - Fails startup if `JWT_SECRET` is not set
  - Fails startup if secret is < 32 bytes (HS256 minimum)
  - Fails startup if secret matches known weak patterns (`dev`, `secret`, `password`, etc.)
- **When disabled (default):** Dev fallback secret is used (backwards compatible)
- **Code location:** `crates/intent-api/src/auth.rs` — `AuthConfig::from_env()`

### RLS Session Context Helper (Scaffold)

RLS helper functions exist for safely setting PostgreSQL session tenant context:

- **Code location:** `crates/intent-api/src/auth.rs`
- **Functions:**
  - `rls_set_tenant_context_sql(tenant_id)` — generates `SET LOCAL app.current_tenant_id = '...'` SQL
  - `rls_reset_tenant_context_sql()` — generates `RESET app.current_tenant_id` SQL
  - `validate_tenant_id_for_rls(tenant_id)` — validates UUID is safe for RLS use
- **RLS policies:** Already enabled in `infrastructure/migrations/013_enable_rls_policies.sql`
- **Pending:** Full JWT→RLS context wiring in SQL query execution path

## Authentication
- User auth: OIDC/OAuth2
- Service auth: mTLS hoặc workload identity
- Connectors/webhooks: signed secrets + issuer validation

## Authorization model
Kết hợp:
- RBAC cho console actions
- ABAC theo tenant, workflow risk, domain, environment
- scope-based permissions cho APIs

## Permissions examples
- `intent.read`
- `intent.write`
- `rebase.preview`
- `rebase.apply.low_risk`
- `rebase.apply.high_risk`
- `approval.revalidate`
- `artifact.quarantine`
- `compensation.execute`
- `audit.export`

## Sensitive actions requiring step-up auth
- force apply high-risk rebase
- waive compensation
- export full forensic bundle
- cross-env operations
