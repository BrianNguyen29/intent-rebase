# Authentication and Authorization

## Current Implementation Status

> **Bounded slice:** RLS transaction wrapping for `create_graph_node` is delivered. The `jwt_auth_async` middleware validates `tenant_id` (rejects nil UUID) before allowing authenticated requests. `RlsTenantContext` helper is available for transaction-scoped RLS context. `RlsAwarePool` provides RLS-aware transaction support. `create_graph_node` handler now wraps node creation in RLS-set transactions with tenant mismatch rejection. Full repository transaction wrapping (automatic wiring for all SQL paths) remains **pending**.

### JWT Production Guard (Bounded — Wiring Delivered)

A production JWT guard is wired via `INTENT_API_REQUIRE_JWT=true`:

- **Env var:** `INTENT_API_REQUIRE_JWT=true` activates strict JWT validation
- **When enabled:**
  - Fails startup if `JWT_SECRET` is not set
  - Fails startup if secret is < 32 bytes (HS256 minimum)
  - Fails startup if secret matches known weak patterns (`dev`, `secret`, `password`, etc.)
- **When disabled (default):** Dev fallback secret is used (backwards compatible)
- **Code location:** `crates/intent-api/src/auth.rs` — `AuthConfig::from_env()`

### JWT Tenant ID Validation for RLS (Phase 3 P3-S5 Bounded — Delivered)

When JWT authentication is enabled (`INTENT_API_REQUIRE_JWT=true`), the `jwt_auth_async` middleware now validates the `tenant_id` claim:

- **Validation:** Rejects nil UUID (`00000000-0000-0000-0000-000000000000`) which is reserved as sentinel/default
- **Error response:** Returns 401 Unauthorized if tenant_id is nil or invalid UUID
- **Purpose:** Ensures invalid tenant claims cannot bypass RLS policies
- **Code location:** `crates/intent-api/src/lib.rs` — `jwt_auth_async`

### RLS Session Context Helper (Phase 3 P3-S5 Bounded — Delivered)

`RlsTenantContext` struct provides transaction-scoped RLS tenant context:

- **Code location:** `crates/intent-rebase-types/src/rls.rs` (moved from `intent-api` for sharing)
- **Struct:** `RlsTenantContext` with validated tenant UUID
- **Methods:**
  - `RlsTenantContext::new(tenant_id)` — creates from validated UUID, rejects nil UUID
  - `tenant_id()` — returns the validated tenant UUID
  - `set_rls_context(&self, tx)` — executes `SET LOCAL app.current_tenant_id = '...'` in transaction
  - `reset_rls_context(&self, tx)` — executes `RESET app.current_tenant_id` in transaction
- **SQL helpers (moved to `intent-rebase-types`):**
  - `rls_set_tenant_context_sql(tenant_id)` — generates `SET LOCAL app.current_tenant_id = '...'` SQL
  - `rls_reset_tenant_context_sql()` — generates `RESET app.current_tenant_id` SQL
  - `validate_tenant_id_for_rls(tenant_id)` — validates UUID is safe for RLS use
- **RLS policies:** Already enabled in `infrastructure/migrations/013_enable_rls_policies.sql`

### RLS Tenant Claims Extractor (Phase 3 P3-S5 Bounded — Delivered)

`RlsTenantClaims` extracts validated JWT tenant claims for use in handlers:

- **Code location:** `crates/intent-api/src/auth.rs`
- **Usage:** Extract via axum `Extension<RlsTenantClaims>` in handler arguments
- **Validation:** Parses `tenant_id` from JWT claims, rejects nil/invalid UUIDs
- **Error types:** `RlsTenantClaimsExtractionError` (401), `TenantMismatchError` (403)

### RLS-Aware Pool (Phase 3 P3-S5 Bounded — Delivered)

`RlsAwarePool` wraps `sqlx::PgPool` to provide RLS-aware transaction support:

- **Code location:** `crates/graph-service/src/lib.rs`
- **Method:** `begin_with_tenant(tenant_id)` — starts transaction and sets RLS context
- **Returns:** `sqlx::Transaction` with `SET LOCAL app.current_tenant_id` already executed
- **Validation:** Rejects nil tenant_id before beginning transaction

### create_graph_node RLS Wrapping (Phase 3 P3-S5 Bounded — Delivered)

The `POST /v1/graph/nodes` endpoint now supports RLS-wrapped node creation:

- **When `rls_pool` is configured in `AppState`:**
  1. Extracts `RlsTenantClaims` from JWT via `Extension<RlsTenantClaims>`
  2. Validates `request.tenant_id == JWT tenant_id` (tenant mismatch rejection)
  3. Begins RLS-aware transaction via `RlsAwarePool::begin_with_tenant`
  4. Calls `SqlxGraphRepository::create_node_with_tx` within the transaction
  5. Commits the transaction
- **When `rls_pool` is `None`:** Falls back to existing non-RLS `add_node` path
- **Error responses:**
  - 401 Unauthorized: JWT invalid/missing or tenant claim extraction failed
  - 403 Forbidden: Request tenant_id does not match JWT tenant_id
  - 500 Internal: Transaction begin/commit failures

### Pending Items

- Full repository transaction wrapping (automatic `RlsTenantContext` wiring in all SQL query paths)
- RLS wrapping for other handlers (`create_graph_edge`, etc.)
- NATS tenant isolation
- Production certification

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
