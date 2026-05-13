# Propagation Status Implementation Plan

> **Status:** Slices 1–2 bounded implemented locally — migration 017 applied, `PropagationRecord` domain type, in-memory and SQL repositories wired, query handler reads from repo when available and falls back to stub when `None`, signal ingestion endpoint wired. No production-ready claim.
> **Scope:** Concrete bounded plan for evolving `GET /intents/{intent_id}/propagation-status` from stub to real downstream tracking  
> **Related:** [ADR-12](../13-adrs/12-workflow-migration-rebase.md), [Agent Safety Roadmap](./18-agent-safety-rebase-roadmap.md), [REST API Design](../04-api/01-rest-api.md)

---

## Current State

**Slice 1 bounded implemented locally:**
- Migration `017_create_propagation_records.sql` created with RLS policy on `tenant_id`
- `PropagationRecord` domain type (`intent-rebase-types`) with `PropagationStatus` enum (`pending`, `acknowledged`, `failed`)
- `PropagationRecordRepository` trait + `InMemoryPropagationRecordRepository` (`intent-service`)
- `AppState` carries `propagation_record_repo: Option<Arc<dyn PropagationRecordRepository>>`
- Query handler reads from repository when `Some`, falls back to stub (empty `downstream_systems`, zeroed summary) when `None`
- Router signatures updated across all variants (`build_router`, SQL, JWT)

**Stub fallback preserved:** When `propagation_record_repo` is `None`, the endpoint returns the same bounded stub shape as before — empty `downstream_systems`, zeroed `propagation_summary`, and `unsupported_items` listing deferred integrations.

**Deferred:** Webhook delivery, event streaming acknowledgment, cross-workflow lineage, and real-time monitoring remain Phase 4+ scope.

---

## Downstream Tracking Source

Real propagation status requires a **registry of downstream systems** that consume intent changes. Proposed tracking sources (in priority order):

1. **Explicit webhook subscriptions** — downstream systems register via a future `POST /webhooks/subscriptions` endpoint (Phase 4+ deferred)
2. **Event stream consumers** — NATS JetStream consumer metadata (consumer name, last delivered sequence, ack wait)
3. **Graph lineage edges** — cross-workflow lineage (N2) records which workflows consume artifacts from this intent
4. **Runtime adapter heartbeats** — bounded optional: adapter reports last-seen intent version during health checks

**Bounded initial scope:** Start with (1) explicit subscriptions only. Event stream and lineage integration are follow-on slices.

---

## Event Acknowledgment Model

### Acknowledgment States

| State | Meaning | Transition Triggers |
|-------|---------|---------------------|
| `pending` | Change signaled but not yet acknowledged | Initial state when a new intent version is published |
| `acknowledged` | Downstream system confirmed receipt | Webhook delivery success (2xx) or explicit consumer ack |
| `failed` | Downstream system rejected or delivery failed | Webhook non-2xx, consumer nack, or delivery timeout |
| `stale` | Downstream system's last seen version is behind current | Periodic reconciliation detects version drift |

### State Machine

```
           +-----------+
           |  pending  |
           +----+------+
                |
     +----------+----------+
     |                     |
     v                     v
+----+------+       +-----+-----+
|acknowledged|       |   failed   |
+------------+       +-----+-----+
     |                     |
     |     +---------------+
     |     |
     v     v
+----+-----+---+
|    stale     |  (re-enter pending on next version)
+--------------+
```

**Key rule:** `stale` is a derived state, not persisted. It is computed at query time by comparing `last_seen_version` with the intent's current head version.

---

## Persistence Schema Direction

### Option A: Dedicated `propagation_status` Table (Recommended)

```sql
CREATE TABLE propagation_records (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    intent_id UUID NOT NULL REFERENCES intents(id),
    downstream_system_id TEXT NOT NULL,
    -- acknowledgment state
    status TEXT NOT NULL CHECK (status IN ('pending', 'acknowledged', 'failed')),
    last_seen_version INT NOT NULL DEFAULT 0,
    signaled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    failure_reason TEXT,
    -- delivery metadata
    delivery_attempt_count INT NOT NULL DEFAULT 0,
    last_delivery_attempt_at TIMESTAMPTZ,
    -- optimistic locking
    lock_version INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, intent_id, downstream_system_id)
);

CREATE INDEX idx_propagation_records_intent
    ON propagation_records(tenant_id, intent_id, updated_at DESC);
CREATE INDEX idx_propagation_records_system
    ON propagation_records(tenant_id, downstream_system_id, status);
```

**RLS:** Row-level security policy on `tenant_id` (follows existing P1 RLS pattern).

### Option B: Event-Sourced Log

Append-only log of propagation events (`signaled`, `acknowledged`, `failed`, `retried`). Current state is a fold of the log.

**Deferred:** Option B is more robust for audit but requires event sourcing infrastructure. Start with Option A; migrate to Option B if audit requirements demand it.

---

## Failure Semantics

### Delivery Failures

| Scenario | Behavior | Retry Policy |
|----------|----------|--------------|
| Webhook timeout (no response) | Mark `failed`, queue retry | 3 attempts with exponential backoff (1s, 5s, 25s) |
| Webhook non-2xx | Mark `failed`, record status code | No auto-retry for 4xx; retry 5xx per policy |
| Consumer nack (NATS) | Mark `failed`, re-deliver per JetStream policy | Delegated to JetStream max-deliver |
| Downstream system permanently gone | Mark `failed`, alert operator | Manual intervention required |

### Query-Time Failures

- **Missing downstream system registry:** Returns empty list (same as current stub) with `unsupported_items` noting registry is not configured
- **DB unavailable:** Fail-open with 503 and `retryable: true` error (does not crash the handler)
- **Tenant mismatch:** 401 Unauthorized (existing behavior preserved)

---

## Staged Implementation Slices

### Slice 1 — Downstream System Registry (Bounded — Locally Implemented)

**Implemented:**
- Migration `017_create_propagation_records.sql` with `tenant_id` RLS policy
- `PropagationRecord` domain type and `PropagationStatus` enum
- `PropagationRecordRepository` trait + `InMemoryPropagationRecordRepository`
- `AppState` integration with optional repo (backward-compatible stub fallback when `None`)
- Query handler reads from repo when available; returns stub when unavailable
- Router signatures updated across all build variants (in-memory, SQL, SQL+JWT)

**Deferred (remains unimplemented):**
- `POST /webhooks/subscriptions` — register a downstream system for an intent pattern
- `DELETE /webhooks/subscriptions/{subscription_id}` — deregister
- SQL-backed repository (deferred to Slice 2)

**Acceptance criteria:**
- [x] Migration 017 created with RLS policy
- [x] Domain type, repository trait, and in-memory impl exist
- [x] Query handler uses repo when `Some`, stub fallback when `None`
- [x] Router signatures updated and all tests pass
- [x] OpenAPI descriptive text updated
- [x] Subscription CRUD endpoints remain deferred

### Slice 2 — Propagation Status Persistence (Bounded — Locally Implemented)

**Implemented:**
- `SqlxPropagationRecordRepository` with create/list/update against `propagation_records` table
- SQL `AppState` wiring in `main.rs` — both JWT and non-JWT SQL router paths use `SqlxPropagationRecordRepository`
- `POST /intents/{intent_id}/propagation-signals` — bounded signal ingestion endpoint (no webhook delivery, no event streaming)
- Automatic propagation signal creation triggered by rebase apply (post-commit, only for `Proceed` outcomes)
  - Uses existing propagation records as de facto downstream registry
  - Best-effort: `tracing::warn!` on failure, never fails apply response
  - In-memory/no-repo paths remain unaffected (backward compatible)
- Observability metrics for propagation signal creation:
  - `intent_api_propagation_signals_attempted_total` — counter incremented when apply trigger runs
  - `intent_api_propagation_signals_succeeded_total` — counter incremented per successful record update
  - `intent_api_propagation_signals_failed_total` — counter incremented per failed record update or list error
  - `intent_api_propagation_signals_no_downstream_total` — counter incremented when no downstream records exist for the intent
- Runbook RB12 for propagation signal failures: alerting guidance, diagnosis, manual re-signal workflow
  - See [docs/09-operations/05-runbooks.md](../09-operations/05-runbooks.md#RB12-Propagation-Signal-Creation-Failures)
- Query handler reads from SQL repo when `propagation_record_repo` is `Some`; falls back to stub when `None`
- Ignored live RLS tests for `propagation_records`: tenant-isolated insert/list/update and tenant mismatch fail-closed
- OpenAPI updated with `/intents/{intent_id}/propagation-signals` endpoint and schemas

**Deferred:**
- Automatic trigger on intent version creation (rejected per oracle recommendation)
- Automatic trigger on policy snapshot impact report (rejected per oracle recommendation)
- Webhook delivery, event streaming, cross-workflow lineage remain Phase 4+

**Acceptance criteria:**
- [x] `GET /intents/{intent_id}/propagation-status` returns registered systems with correct status when repo available
- [x] `propagation_summary` counts match persisted records
- [x] Tenant isolation enforced via RLS (migration 017 + live tests)
- [x] Route contract tests pass
- [x] Signal ingestion endpoint wired and reachable
- [x] Rebase apply triggers propagation signal update for Proceed outcomes
- [x] Blocked/NoOp apply paths do not create signals
- [x] Missing repo does not fail apply response
- [x] Observability metrics instrumented (attempted, succeeded, failed, no-downstream)
- [x] Runbook RB12 documents alerting guidance and manual re-signal workflow
- [x] Local Prometheus rule `PropagationSignalFailureRate` defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` (local dev scaffolding; production requires SRE sign-off)

### Slice 3 — Webhook Delivery (Design Refinement — Not Implemented)

> **Status:** Design-only. No code implementation started. Webhook delivery remains deferred to future Phase 4+ work. The following are concrete design decisions proposed for when implementation begins; they are not live code or production commitments.

#### HTTP Client Choice

- **Proposed:** `reqwest` (async) with the `rustls-tls` feature enabled.
- **Rationale:** Already idiomatic in the Rust ecosystem; supports connection pooling, configurable timeouts, and middleware (e.g., `reqwest-middleware` for retry/logging). The project already uses `reqwest` indirectly via dependencies, so adding it directly is low-friction.
- **Bounded:** No custom TLS stack, no HTTP/3, no connection pinning, no client certificate auth. mTLS and custom CA bundles are future scope.

#### Timeout Constants

| Constant | Proposed Value | Rationale |
|----------|---------------|-----------|
| `WEBHOOK_CONNECT_TIMEOUT` | 5 seconds | Time to establish TCP + TLS handshake |
| `WEBHOOK_REQUEST_TIMEOUT` | 30 seconds | Total time per delivery attempt (includes connect + send + wait-for-response) |
| `WEBHOOK_MAX_TOTAL_DURATION` | 120 seconds | Hard ceiling for all attempts including retries; abort remaining retries if exceeded |

> These values are proposed defaults for local/non-production use. Production tuning (e.g., lower request timeout for fast receivers) is future SRE work.

#### Retry / Backoff Policy

- **Proposed:** Exponential backoff with full jitter.
- **Base delay:** 2 seconds.
- **Multiplier:** 2.0.
- **Max delay:** 30 seconds.
- **Max attempts:** 3 total (initial attempt + 2 retries).
- **Retryable conditions:** HTTP 5xx, connect timeout, request timeout, DNS failure, connection refused, connection reset.
- **Non-retryable conditions:** HTTP 4xx (except 429), malformed URL, TLS certificate failure, unresolvable host.
- **429 Too Many Requests:** Retry once after `Retry-After` header value (capped at 60 seconds). If no `Retry-After` header, fall back to standard backoff slot. If the retry also returns 429, mark `failed`.
- **Jitter:** Full jitter (`rand::random::<f64>() * delay`) to prevent thundering herd across downstream systems.

> **Bounded scope:** Retries are in-process sequential against a local async task. No external retry queue, no background worker, no distributed scheduling, no dead-letter topic for exhausted attempts.

#### Payload Schema

Proposed JSON payload posted to each subscription URL with `Content-Type: application/json`:

```json
{
  "event_type": "intent_changed",
  "intent_id": "uuid",
  "tenant_id": "uuid",
  "version": 42,
  "version_hash": "sha256:abc123...",
  "previous_version": 41,
  "timestamp": "2026-05-13T12:00:00Z",
  "delivery_id": "uuid",
  "attempt_number": 1,
  "subscription_id": "uuid"
}
```

- **Signature header (design-only, not implemented):** `X-Webhook-Signature: sha256=<hmac>` using a per-subscription secret. Key management and rotation are deferred.
- **Idempotency key:** `delivery_id` (UUID v4) passed in the `X-Idempotency-Key` header so downstream systems can deduplicate.
- **Bounded:** No payload compression, no chunked transfer encoding, no custom media types, no partial/delta payloads. Payload size is expected to be small (< 10 KB).

#### Sync vs Async Delivery Model

- **Proposed model:** Async fire-and-notify.
- **Behavior:** When a propagation signal is created (e.g., by rebase apply post-commit), spawn a local async task to deliver webhooks.
- **Sequential per intent:** All subscriptions for a given intent are delivered one-at-a-time to bound resource usage and avoid overwhelming a single downstream system.
- **Not awaited by caller:** The apply/signal handler returns immediately; delivery outcomes are recorded asynchronously. A `tracing::warn!` is emitted on spawn failure, but the caller response is never blocked.
- **Bounded:** No delivery guarantees (at-least-once is best-effort). No outbox pattern, no transactional boundary spanning DB write + HTTP delivery, no saga compensation for delivery failures.
- **Error recording:** On task completion (success or failure), update `propagation_records.status`, `delivery_attempt_count`, and `last_delivery_attempt_at`.

#### Audit / Delivery-Attempt Semantics

- **Before each HTTP request:** Increment `delivery_attempt_count` and set `last_delivery_attempt_at = NOW()`.
- **On 2xx response:** Set `status = 'acknowledged'` and `acknowledged_at = NOW()`.
- **On retryable failure (5xx, timeout, network error):**
  - If attempts remain: keep `status = 'pending'`.
  - If max attempts exhausted: set `status = 'failed'`, `failed_at = NOW()`, `failure_reason = "<category>: <detail>"`.
- **On non-retryable failure (4xx except 429, malformed URL, TLS failure):** Set `status = 'failed'`, `failed_at = NOW()`, `failure_reason = "<status>: <body_snippet>"`.
- **Per-attempt log (deferred):** A separate `propagation_delivery_attempts` table may be introduced in a future slice to capture per-attempt detail (HTTP method, URL, status code, response body snippet, duration_ms). For Slice 3, audit is inline on `propagation_records` only.

#### Error Behavior

| Scenario | Behavior | Recorded State |
|----------|----------|----------------|
| DNS resolution fails | Retry per policy | `pending` (if retries remain) → `failed` |
| TCP/TLS timeout | Retry per policy | `pending` (if retries remain) → `failed` |
| HTTP 2xx | Success | `acknowledged` |
| HTTP 4xx (non-429) | No retry, mark failed immediately | `failed` |
| HTTP 429 | Retry once with backoff | `pending` (if retry remains) → `failed` |
| HTTP 5xx | Retry per policy | `pending` (if retries remain) → `failed` |
| Subscription URL missing or invalid | No retry, mark failed | `failed` |
| DB unavailable during outcome recording | Best-effort `tracing::warn!`; delivery outcome may be lost | Inconsistent (known bounded limitation) |

> **Known bounded limitation:** Because there is no outbox/transactional boundary, a crash between HTTP delivery and DB update can leave the delivery state inconsistent (delivered but not recorded, or recorded but not delivered). This is accepted for Slice 3 and can be addressed later with an outbox or idempotent re-delivery log.

#### Acceptance Criteria (Design-Level — Not Implemented)

- [ ] HTTP client (`reqwest`) configured with connect/request timeouts matching proposed constants.
- [ ] Retry policy implements exponential backoff with full jitter (3 attempts max).
- [ ] Payload schema matches proposed JSON structure and includes `delivery_id` + `attempt_number`.
- [ ] Delivery is async (does not block the signal creation handler).
- [ ] `propagation_records.status` transitions correctly per outcome table.
- [ ] `delivery_attempt_count` and `last_delivery_attempt_at` are updated before every attempt.
- [ ] Non-retryable errors (4xx) mark record as `failed` immediately without retries.
- [ ] Retryable errors (5xx, timeout, network) retry up to max attempts.
- [ ] Handler-level unit test verifies payload shape and header presence (proposed gate G7).
- [ ] Route contract test verifies `POST /intents/{intent_id}/propagation-signals` remains reachable with no regression.

#### Validation Gates (Slice 3 — Proposed for Future Implementation)

| Gate | Check | Command |
|------|-------|---------|
| G1 — Compile | No warnings | `cargo check --workspace` |
| G2 — Format | No diff | `cargo fmt --all -- --check` |
| G3 — Lint | No clippy warnings | `cargo clippy --workspace --all-targets -- -D warnings` |
| G4 — Route wiring | All routes reachable | `cargo test -p intent-api --lib router_smoke_tests` |
| G5 — OpenAPI drift | Spec matches routes | `npx spectral lint docs/04-api/openapi.yaml` + drift guard test |
| G6 — Tenant isolation | RLS policies active | `cargo test --test rls_integration -- --ignored` |
| G7 — Handler unit test | Payload shape + headers | New unit test in `intent-api` handler module |
| G8 — Delivery simulation | Mock HTTP server verifies retry behavior | Integration test with `wiremock` or `mockito` (proposed) |

> **Note:** G7–G8 are proposed gates for when implementation begins. They are not runnable today because Slice 3 is design-only.

#### Implementation Readiness Checklist (Pre-Implementation — Not Started)

> **Status:** Pre-flight checklist. Implementation has **not** started. R1–R5 are checked to record owner assignment / design review completion, dependency placement, schema/trait review, RLS/tenant implications, and retry constants acceptance decisions only; this is **not** an implementation Go. R6–R8 remain unchecked and require explicit approval before any code is written.

| # | Item | Owner | Status |
|---|------|-------|--------|
| R1 | **Owner / Approval** — Named owner (individual or pair) assigned to Slice 3 implementation; design reviewed and approved by a second maintainer | Brian Nguyen (owner) / AI-oracle (reviewer) | ☑ |
| R2 | **Dependency Readiness** — Decision recorded (see R2 Decision Note below). `reqwest` 0.12 with features `json`, `rustls-tls` only (no `blocking`) as crate-local regular dependency of `intent-api`; not promoted to workspace unless a second crate needs it. `wiremock` as crate-local `dev-dependency` of `intent-api`; verify latest compatible version at implementation time. Caveat: if delivery code moves away from `intent-api`, placement must be revisited. No `Cargo.toml` changes made in this docs-only slice. | Brian Nguyen / AI-oracle | ☑ |
| R3 | **Schema & Trait Review** — Decision recorded (see R3 Decision Note below). Migration 017 delivery columns are sufficient for Slice 3; no additive migration needed for `propagation_records`. `PropagationRecordRepository` trait gap identified (missing delivery attempt/outcome methods). B1 resolved as future `webhook_subscriptions` table (migration 018). B2 resolved as future trait methods `record_delivery_attempt` and `record_delivery_outcome`. No migration or Rust files were modified in this docs-only slice. | Brian Nguyen / AI-oracle | ☑ |
| R4 | **RLS / Tenant Implications** — Decision recorded (see R4 Decision Note below). Future `webhook_subscriptions` table follows existing P1 RLS pattern (`ENABLE RLS`, `FORCE RLS`, `tenant_isolation` policy). Dispatcher lookup is application-layer tenant-scoped with `tenant_id` on every query; RLS is defense-in-depth only. URL logging redaction policy documented. No migration, Rust, or test files were modified in this docs-only slice. | Brian Nguyen / AI-oracle | ☑ |
| R5 | **Retry Constants Acceptance** — Decision recorded (see R5 Decision Note below). Timeout constants accepted: `WEBHOOK_CONNECT_TIMEOUT=5s`, `WEBHOOK_REQUEST_TIMEOUT=30s`, `WEBHOOK_MAX_TOTAL_DURATION=120s`. Retry/backoff policy accepted: exponential backoff with full jitter, base 2s, multiplier 2.0, max delay 30s, max 3 attempts. Error classification and 429 special-case behavior accepted. 120s ceiling edge case documented. No Cargo, Rust, or test files were modified in this docs-only slice. | Brian Nguyen / AI-oracle | ☑ |
| R6 | **Test Plan Mapping to G1–G8** — Each validation gate has a corresponding test or verification step assigned: G1-G3 via CI, G4 via route smoke tests, G5 via Spectral + drift guard, G6 via ignored RLS tests, G7 via handler unit test, G8 via mock-server integration test; delivery observability metrics (attempted, succeeded, failed, retry_exhausted) added to test plan and metrics registry | TBD | ☐ |
| R7 | **Rollback / Non-Goals Acknowledgment** — Team acknowledges Slice 3 non-goals: no outbox, no distributed transactions, no delivery guarantees, no background retry worker, no production-readiness claim; rollback plan documented including explicit feature-flag/env gate name (e.g., `INTENT_API_WEBHOOK_DELIVERY=true`) to disable dispatch without code change; failed-to-pending reset semantics and delivery task lifecycle (spawn, cancel, timeout, panic) documented; `failure_reason` truncation/redaction policy agreed (max length, PII redaction) | TBD | ☐ |
| R8 | **Go / No-Go Decision** — Explicit go/no-go gate convened before first commit; if any R1–R7 item is unresolved or any Pre-R8 Blocker (B1–B2) lacks a documented resolution path, decision must be **No-Go** with recorded reason and re-review date | TBD | ☐ |

#### R2 Decision Note (Docs-Only — No Cargo Changes)

> **Status:** Dependency placement decision recorded. No `Cargo.toml` edits were made. R8 remains No-Go.

**`reqwest` placement:**
- Crate: `intent-api` (regular dependency, not workspace-level).
- Version: `0.12` (verify latest compatible patch at implementation time).
- Features: `json`, `rustls-tls` only. Do **not** enable `blocking`.
- Rationale: `reqwest` is already present in `intent-api` dev-dependencies; `intent-service` has no HTTP client need today. Keeping it crate-local avoids unnecessary workspace promotion. Promote to workspace only if a second crate needs HTTP client capabilities.
- Caveat: if the delivery dispatcher moves out of `intent-api` (e.g., into `intent-service` or a new crate), dependency placement must be revisited.

**Mock HTTP library placement:**
- Crate: `intent-api` (crate-local `dev-dependency`).
- Choice: `wiremock` (verify latest compatible version at implementation time).
- Rationale: `wiremock` provides declarative HTTP mocking suitable for async Rust integration tests; `mockito` was considered but `wiremock` is preferred for tokio-based test suites in this repo.

**No Cargo changes:** These decisions are recorded for the future implementation phase. No `Cargo.toml` was modified in this docs-only update.

#### R3 Decision Note (Docs-Only — No Migration or Rust Changes)

> **Status:** Schema and trait review decision recorded. No migration DDL or Rust files were modified. R8 remains No-Go.

**Existing schema findings (migration 017):**
- `propagation_records` already includes the delivery columns needed for Slice 3: `delivery_attempt_count`, `last_delivery_attempt_at`, `failure_reason`, and `failed_at`.
- The `PropagationRecord` domain type already carries these fields.
- **Conclusion:** No additive migration is required for `propagation_records` in Slice 3.

**Trait gap:**
- `PropagationRecordRepository` currently lacks methods to atomically record delivery attempts and outcomes.
- The existing `update_status` method does not cover incrementing `delivery_attempt_count`, setting `last_delivery_attempt_at`, or recording `failure_reason`.
- **Conclusion:** Future implementation must add delivery-specific methods to the trait (see B2 resolution below).

**B1 resolution — `webhook_subscriptions` table:**
- Decision: introduce a separate `webhook_subscriptions` table in a future migration (proposed name: `018_create_webhook_subscriptions.sql`). Do **not** inline the webhook URL onto `propagation_records`.
- Proposed minimal columns:
  - `id UUID PRIMARY KEY`
  - `tenant_id UUID NOT NULL`
  - `intent_id UUID NOT NULL`
  - `subscription_id UUID NOT NULL`
  - `webhook_url TEXT NOT NULL`
  - `downstream_system_id TEXT`
  - `created_at TIMESTAMPTZ`
  - `updated_at TIMESTAMPTZ`
- RLS policy on `tenant_id` following the migration 017 pattern.
- The dispatcher queries by `(tenant_id, intent_id)` to obtain target URLs.
- **Deferred to future scope:** secret/HMAC keys, custom headers, enabled/disabled flag, subscription CRUD API endpoints, per-attempt delivery log table.

**B2 resolution — repository trait methods:**
- Decision: extend the existing `PropagationRecordRepository` trait (do not create a new trait).
- Proposed future async methods:
  - `record_delivery_attempt(id, tenant_id) -> Result<PropagationRecord, IntentRebaseError>` — atomically increments `delivery_attempt_count`, sets `last_delivery_attempt_at = NOW()`, and increments `lock_version`.
  - `record_delivery_outcome(id, tenant_id, status: PropagationStatus, failure_reason: Option<String>) -> Result<PropagationRecord, IntentRebaseError>` — atomically updates `status`, `acknowledged_at`/`failed_at`, and `failure_reason`, and increments `lock_version`.
- SQL implementation should use optimistic locking (`lock_version`) consistently with existing repository patterns.

**No migration or Rust changes:** These decisions are recorded for the future implementation phase. No `.sql` migration file and no `.rs` source file was modified in this docs-only update.

#### R4 Decision Note (Docs-Only — No Migration, Rust, or Test Changes)

> **Status:** RLS and tenant implications decision recorded. No migration, Rust, or test files were modified. R8 remains No-Go.

**D1 — RLS policy for future `webhook_subscriptions` table:**
- Decision: apply the existing P1 RLS pattern exactly.
- Proposed future migration DDL:
  - `ALTER TABLE webhook_subscriptions ENABLE ROW LEVEL SECURITY;`
  - `ALTER TABLE webhook_subscriptions FORCE ROW LEVEL SECURITY;`
  - `CREATE POLICY tenant_isolation ON webhook_subscriptions FOR ALL USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());`
- Rationale: follows migration 017 RLS pattern; no new policy semantics.

**D2 — Dispatcher lookup scope:**
- Decision: every dispatcher query and repository update must include an application-layer `tenant_id` filter.
- Proposed future SQL patterns:
  - `SELECT * FROM webhook_subscriptions WHERE tenant_id = $1 AND intent_id = $2;`
  - `record_delivery_attempt(id, tenant_id)` and `record_delivery_outcome(id, tenant_id, status, failure_reason)` both require `tenant_id` as an explicit parameter.
- `tenant_id` is sourced from persisted intent context at dispatcher spawn time, not from the HTTP request/JWT.
- RLS is defense-in-depth only; it must not substitute for application-layer `tenant_id` scoping.
- **Caution:** PostgreSQL `current_tenant_id()` session context may not automatically propagate into spawned async delivery tasks, so application-layer scoping is the primary control.

**D3 — URL logging and redaction policy:**
- Decision: never log full webhook URLs at `warn` or `error` levels.
- Allowed: log `downstream_system_id` and, if needed, a sanitized URL in the form `scheme://host[:port]` only.
- Must strip: path, query parameters, fragments, and any embedded credentials.
- Response bodies from downstream systems must not be logged.
- `failure_reason` truncation and redaction are covered in R7 (deferred to R7 decision).

**D4 — Future G6 RLS test extension:**
- Decision: when `webhook_subscriptions` is created, extend the existing ignored/live-Postgres RLS test suite with tenant-isolation cases for the new table.
- Pattern: tenant-scoped insert/list should succeed for matching tenant, fail-closed for mismatched tenant, following the same test structure as existing `rls_integration` tests.
- **Scope:** this is a future test-plan item (R6/G6), not implemented in R4.

**Missing-JWT / backward-compatible note:**
- The bounded non-production paths (in-memory, no JWT) remain fail-open and backward-compatible.
- Tenant-scoped DB queries use `tenant_id` from the persisted intent context, not from JWT claims.
- This design does **not** claim production safety for missing-JWT paths.

**Explicit cautions / deferred scope:**
- Log aggregator tenancy is unknown; centralized logging may see cross-tenant webhook delivery traces if not filtered by deployment.
- Secrets/HMAC key storage for webhook signing remains deferred (not in Slice 3).

**No migration, Rust, or test changes:** These decisions are recorded for the future implementation phase. No `.sql` migration, `.rs` source, or test file was modified in this docs-only update.

#### R5 Decision Note (Docs-Only — No Cargo, Rust, or Test Changes)

> **Status:** Retry constants and error classification decision recorded. No `Cargo.toml`, Rust, or test files were modified. R8 remains No-Go.

**D5 — Timeout constants (accepted):**
- `WEBHOOK_CONNECT_TIMEOUT`: 5 seconds — TCP + TLS handshake establishment.
- `WEBHOOK_REQUEST_TIMEOUT`: 30 seconds — total per-delivery attempt (connect + send + wait-for-response).
- `WEBHOOK_MAX_TOTAL_DURATION`: 120 seconds — hard ceiling for all attempts including retries; abort remaining retries if exceeded.

**D6 — Retry / backoff policy (accepted):**
- Exponential backoff with full jitter.
- Base delay: 2 seconds; multiplier: 2.0; max delay: 30 seconds; max attempts: 3 total (initial + 2 retries).
- Jitter formula: `rand::random::<f64>() * delay` (full jitter).
- **Dependency caveat:** full jitter requires a future crate-local regular dependency `rand = "0.8"` in `intent-api`. No `Cargo.toml` change is made now; placement is crate-local unless a second crate needs random generation.

**D7 — Error classification (accepted):**
- **Retryable:** HTTP 5xx, connect timeout, request timeout, DNS failure, connection refused, connection reset.
- **Non-retryable:** HTTP 4xx (except 429), malformed URL, TLS certificate failure, unresolvable host.
- **429 special case:** retry once using `Retry-After` header when present. Parse delta-seconds or HTTP-date if implemented; cap wait at 60 seconds. Fall back to standard backoff slot if header is missing or invalid. Mark `failed` after bounded retry exhaustion.
- **Failure reason redaction:** per R4 D3, `failure_reason` must never contain full URLs, query parameters, credentials, or response bodies. R7 N5 owns truncation/PII detail.

**D8 — 120s ceiling edge case:**
- Normal 3-attempt worst-case duration without 429 is ~102 seconds (5s connect + 30s request per attempt, plus backoff delays).
- A 429 with 60s `Retry-After` can push total duration to the 120s ceiling exactly.
- If `WEBHOOK_MAX_TOTAL_DURATION` is exceeded, the future dispatcher should record `failure_reason = "timeout: max_total_duration_exceeded"`, mark the record as `failed`, and exit gracefully without attempting further retries.

**Explicit non-goals:**
- No `rand` Cargo change is made in this docs-only slice.
- No runtime tuning, adaptive timeouts, or SRE-calibrated production values.
- No circuit breaker or per-host backoff state.
- No `failure_reason` truncation/redaction implementation (deferred to R7).

**No Cargo, Rust, or test changes:** These decisions are recorded for the future implementation phase. No `Cargo.toml`, `.rs` source, or test file was modified in this docs-only update.

#### Pre-R8 Blockers / Open Decisions

> **Status:** Blocking and non-blocking open items identified by independent design review. Must be resolved before R8 Go. Implementation has **not** started.

**Blockers (must resolve before Go):**

| # | Blocker | Impact if Unresolved |
|---|---------|---------------------|
| B1 | **No webhook URL / subscription storage exists** — There is no table, entity, or repository for storing downstream webhook URLs and their mapping to `subscription_id`. The design assumes a subscription registry but does not specify where URLs live or how they are queried at delivery time. | Delivery cannot target any URL; Slice 3 is unimplementable without a subscription source. |
| B2 | **`PropagationRecordRepository` lacks delivery-outcome update methods** — The trait does not define methods to atomically update `delivery_attempt_count`, `last_delivery_attempt_at`, `failure_reason`, and status based on delivery outcome. | Delivery attempt recording and state transitions cannot be implemented against the existing repository contract. |

**Proposed Resolution Paths (Docs-Only — Not Implemented):**

> **Note:** These are proposed directions for resolving B1–B2. They do not constitute implementation or blocker closure. R8 remains No-Go until these paths are reviewed and accepted.

**B1 — Webhook URL / Subscription Storage**
Preferred path: introduce a minimal `webhook_subscriptions` table (or equivalent entity) in a future migration with proposed columns `id`, `tenant_id`, `intent_id`, `subscription_id`, `webhook_url`, `created_at`, `updated_at`, plus a `tenant_id` RLS policy following existing P1 patterns. The dispatcher queries this table by `(tenant_id, intent_id)` to obtain target URLs at delivery time. No migration is created now; this is a design note for future implementation.

**B2 — Repository Trait Extension**
Preferred path: extend `PropagationRecordRepository` with two proposed async methods:
- `record_delivery_attempt(tenant_id, intent_id, downstream_system_id)` — atomically increments `delivery_attempt_count` and sets `last_delivery_attempt_at`.
- `record_delivery_outcome(tenant_id, intent_id, downstream_system_id, status, failure_reason)` — atomically sets `status`, `acknowledged_at`/`failed_at`, and `failure_reason`.

No trait code is written now; these are proposed signatures for future implementation.

**Non-blocking Readiness Refinements (should document before Go, do not block design approval):**

| # | Refinement | Recommendation |
|---|------------|----------------|
| N1 | **Workspace dependency placement** — Decide which crate owns the `reqwest` dependency and whether the mock library is placed in workspace `dev-dependencies` or crate-local. | Document in crate README or module doc before first PR. |
| N2 | **Delivery observability metrics** — Define counters/gauges for webhook delivery (e.g., `intent_api_webhook_delivery_attempted_total`, `intent_api_webhook_delivery_succeeded_total`, `intent_api_webhook_delivery_failed_total`, `intent_api_webhook_delivery_retry_exhausted_total`). | Add to test plan (R6) and metrics registry doc; follow existing `intent_api_propagation_signals_*` naming convention. |
| N3 | **Failed-to-pending reset semantics** — Specify when and how a `failed` record transitions back to `pending` (e.g., manual operator reset only, or automatic on next intent version change). | Default recommendation: manual reset only for Slice 3; automatic re-signal on new version is Phase 4+ scope. |
| N4 | **Delivery task lifecycle** — Document spawn behavior (`tokio::spawn`), cancellation on shutdown, and what happens if the delivery task panics or is dropped. | Default recommendation: fire-and-forget with `tracing::error!` on panic; no restart logic for Slice 3. |
| N5 | **`failure_reason` truncation / redaction** — Agree max length (e.g., 500 chars) and whether to redact URLs, tokens, or PII from downstream response bodies before persisting. | Default recommendation: truncate to 500 chars and redact any URL query parameters. |
| N6 | **Feature flag / env rollback gate** — Choose an explicit env var or compile-time feature flag name to enable/disable dispatch without code change. | Default recommendation: env-gated at dispatcher spawn point; default disabled (false) until explicitly enabled. |

> **Go criteria:** R1–R7 are checked and accepted; Pre-R8 Blockers B1–B2 have a documented resolution path; owner signs off on bounded scope and non-goals.
> **No-Go criteria:** Any R1–R7 item is unresolved, any Pre-R8 Blocker (B1–B2) lacks a resolution path, or scope creep is introduced (e.g., outbox pattern, background worker, delivery guarantees).
> **Re-review:** If No-Go, re-review no sooner than one week after blockers are addressed.

### Slice 4 — Event Stream Integration (Deferred)

**Scope:**
- NATS JetStream consumer metadata integration
- Map consumer sequences to `last_seen_version`
- Treat consumer ack as `acknowledged`

**Deferred reason:** Requires NATS consumer lifecycle stabilization (Phase 4+). Slice 1–3 do not depend on this.

### Slice 5 — Cross-Workflow Lineage Integration (Deferred)

**Scope:**
- Graph lineage edges (N2) as implicit downstream systems
- No explicit subscription required for lineage-derived consumers

**Deferred reason:** Requires cross-workflow lineage model (N2). Slice 1–3 do not depend on this.

---

## Validation Gates

| Gate | Check | Command |
|------|-------|---------|
| G1 — Compile | No warnings | `cargo check --workspace` |
| G2 — Format | No diff | `cargo fmt --all -- --check` |
| G3 — Lint | No clippy warnings | `cargo clippy --workspace --all-targets -- -D warnings` |
| G4 — Route wiring | All routes reachable | `cargo test -p intent-api --lib router_smoke_tests` |
| G5 — OpenAPI drift | Spec matches routes | `npx spectral lint docs/04-api/openapi.yaml` + drift guard test |
| G6 — Tenant isolation | RLS policies active | `cargo test --test rls_integration -- --ignored` |
| G7 — Contract shape | Response matches design | Handler-level unit test asserting response schema |

---

## Non-Goals (Explicitly Out of Scope)

- **No real-time push** — polling-only for query; delivery is best-effort webhook, not guaranteed real-time
- **No distributed transaction** — webhook delivery and DB update are not atomic; delivery attempt is recorded, then HTTP is fired, then status is updated on callback
- **No retry queue** — retries are in-process sequential; a background retry worker is future scope
- **No consumer-managed subscriptions** — only service-to-service registration; self-service developer portal is future scope
- **No multi-region replication** — propagation records are single-region; global replication is future scope
- **No SLA guarantee** — delivery is best-effort; no latency or availability SLA is claimed
- **No production-ready claim** until all G1–G7 pass and external sign-off is obtained

---

## Related Docs

- [ADR-12](../13-adrs/12-workflow-migration-rebase.md): Workflow Migration / Rebase Pillar — Phase 4 Design
- [Agent Safety Rebase Roadmap](./18-agent-safety-rebase-roadmap.md): Phase 4 timeline and dependencies
- [REST API Design](../04-api/01-rest-api.md): Propagation-status contract shape and status values
- [Novelty Roadmap](./13-novelty-roadmap.md): N1 IRPaaS propagation context
