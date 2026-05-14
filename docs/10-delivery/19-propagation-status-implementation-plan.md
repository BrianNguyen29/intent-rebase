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

1. **Explicit webhook subscriptions** — downstream systems register via a future `POST /webhooks/subscriptions` endpoint (Phase 4+ deferred). See P2-6d subscription CRUD API design in [Production Readiness Backlog](./17-production-readiness-backlog.md).
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

### Slice 3 — Webhook Delivery (Bounded Implemented — B3-B18)

> **Status:** Bounded non-production implementation delivered (B3-B18). Payload/header builders, async skeleton, env-gated dispatcher, retry loop, metrics, runbook, alert rule, RLS tests, docs sync, and dead_code cleanup are implemented. The following design decisions were originally proposed in the docs-only slice and have since been implemented as bounded code; remaining deferred items are explicitly called out.
>
> **Current bounded behavior:** Dispatch is `.await`ed synchronously within the apply handler post-commit. `tokio::spawn` fire-and-forget conversion remains deferred.
>
> **Deferred (still not implemented):** outbox pattern, transactional delivery boundary, background retry worker, `tokio::spawn` fire-and-forget lifecycle conversion, production readiness, HMAC signing/key rotation (P2-6c design in [Production Readiness Backlog](./17-production-readiness-backlog.md)), subscription CRUD API endpoints (P2-6d design in [Production Readiness Backlog](./17-production-readiness-backlog.md)), event streaming, cross-workflow lineage, per-attempt delivery log table.

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

- **Signature header (design-only, not implemented):** `X-Webhook-Signature: sha256=<hmac>` using a per-subscription secret. Key management and rotation are deferred. See P2-6c HMAC signing + key rotation design in [Production Readiness Backlog](./17-production-readiness-backlog.md).
- **Idempotency key:** `delivery_id` (UUID v4) passed in the `X-Idempotency-Key` header so downstream systems can deduplicate.
- **Bounded:** No payload compression, no chunked transfer encoding, no custom media types, no partial/delta payloads. Payload size is expected to be small (< 10 KB).

#### Sync vs Async Delivery Model

- **Proposed model:** Async fire-and-notify.
- **Behavior:** When a propagation signal is created (e.g., by rebase apply post-commit), spawn a local async task to deliver webhooks.
- **Sequential per intent:** All subscriptions for a given intent are delivered one-at-a-time to bound resource usage and avoid overwhelming a single downstream system.
- **Synchronous within handler:** The apply/signal handler `.await`s dispatch completion post-commit. Delivery outcomes are recorded before the handler returns. A `tracing::warn!` is emitted on dispatch failure. `tokio::spawn` fire-and-forget conversion remains deferred.
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

#### Acceptance Criteria (Bounded Implementation — B3-B18)

- [x] HTTP client (`reqwest`) configured with connect/request timeouts matching proposed constants.
- [x] Retry policy implements exponential backoff with full jitter (3 attempts max).
- [x] Payload schema matches proposed JSON structure and includes `delivery_id` + `attempt_number`.
- [x] `attempt_number` increments correctly across retries (1, 2, 3) — B10 regression test verifies this.
- [x] Delivery is async (does not block the signal creation handler) — env-gated dispatcher runs post-commit in `create_propagation_signals_after_apply`.
- [x] `propagation_records.status` transitions correctly per outcome table.
- [x] `delivery_attempt_count` and `last_delivery_attempt_at` are updated before every attempt.
- [x] Non-retryable errors (4xx) mark record as `failed` immediately without retries.
- [x] Retryable errors (5xx, timeout, network) retry up to max attempts.
- [x] Handler-level unit test verifies payload shape and header presence (G7 — `webhook_delivery_tests.rs`).
- [x] Route contract test verifies `POST /intents/{intent_id}/propagation-signals` remains reachable with no regression.
- [x] Metrics counters instrumented (`intent_api_webhook_deliveries_attempted_total`, `succeeded_total`, `failed_total`, `retry_exhausted_total`) — B11.
- [x] RB13 runbook documents webhook delivery failure diagnosis and rollback boundaries — B12.
- [x] Local Prometheus rule `WebhookDeliveryFailureRate` defined — B13.
- [x] Ignored live RLS tests for `webhook_subscriptions` tenant isolation — B14.
- [x] Env-gated dispatch integration tested via dispatcher-level tests — B16.
- [x] Full end-to-end apply integration test with `INTENT_API_WEBHOOK_DELIVERY=true` triggering live dispatch against wiremock — implemented in `rebase_apply_handler_tests.rs` via `create_propagation_signals_after_apply_with_resolver` test seam and wiremock.

#### Validation Gates (Slice 3 — Bounded Implemented)

| Gate | Check | Command |
|------|-------|---------|
| G1 — Compile | No warnings | `cargo check --workspace` |
| G2 — Format | No diff | `cargo fmt --all -- --check` |
| G3 — Lint | No clippy warnings | `cargo clippy --workspace --all-targets -- -D warnings` |
| G4 — Route wiring | All routes reachable | `cargo test -p intent-api --lib router_smoke_tests` |
| G5 — OpenAPI drift | Spec matches routes | `npx spectral lint docs/04-api/openapi.yaml` + drift guard test |
| G6 — Tenant isolation | RLS policies active | `cargo test --test rls_integration -- --ignored` |
| G7 — Handler unit test | Payload shape + headers | `cargo test --lib webhook_delivery_tests` (57 tests, including payload/header assertions) |
| G8 — Delivery simulation | Mock HTTP server verifies retry behavior | `cargo test --lib webhook_delivery_tests` (wiremock-based delivery simulation tests) |

> **Note:** G1–G8 reflect bounded implementation status. G7–G8 are implemented in `webhook_delivery_tests.rs`. Apply-level wiremock success/failure outcome coverage (200-success and 500-failure) is delivered in commit 5dcdd36 via `create_propagation_signals_after_apply_with_resolver` test seam in `rebase_apply_handler_tests.rs`. Full end-to-end production delivery with outbox, background worker, and real subscriptions remains deferred.

#### Implementation Readiness Checklist (Bounded Implementation Complete — B3-B18)

> **Status:** Bounded implementation is complete. R1–R7 were originally checked as pre-flight design decisions and have since been implemented as bounded code. R8 was a **BOUNDED GO** for the first non-production Slice 3 implementation slice; implementation is now delivered. See R8 Decision Note below for scope boundaries and deferred items.

| # | Item | Owner | Status |
|---|------|-------|--------|
| R1 | **Owner / Approval** — Named owner (individual or pair) assigned to Slice 3 implementation; design reviewed and approved by a second maintainer | Brian Nguyen (owner) / AI-oracle (reviewer) | ☑ |
| R2 | **Dependency Readiness** — Original decision recorded (see R2 Decision Note below). `reqwest` 0.12 with features `json`, `rustls-tls` only (no `blocking`) as crate-local regular dependency of `intent-api`; not promoted to workspace unless a second crate needs it. `wiremock` as crate-local `dev-dependency` of `intent-api`. Caveat: if delivery code moves away from `intent-api`, placement must be revisited. Bounded non-production implementation was subsequently delivered in B3-B18. | Brian Nguyen / AI-oracle | ☑ |
| R3 | **Schema & Trait Review** — Original decision recorded (see R3 Decision Note below). Migration 017 delivery columns are sufficient for Slice 3; no additive migration needed for `propagation_records`. `PropagationRecordRepository` trait gap identified (missing delivery attempt/outcome methods). B1 resolved as `webhook_subscriptions` table (migration 018). B2 resolved as trait methods `record_delivery_attempt` and `record_delivery_outcome`. Bounded non-production implementation was subsequently delivered in B3-B18. | Brian Nguyen / AI-oracle | ☑ |
| R4 | **RLS / Tenant Implications** — Original decision recorded (see R4 Decision Note below). `webhook_subscriptions` table follows existing P1 RLS pattern (`ENABLE RLS`, `FORCE RLS`, `tenant_isolation` policy). Dispatcher lookup is application-layer tenant-scoped with `tenant_id` on every query; RLS is defense-in-depth only. URL logging redaction policy documented. Bounded non-production implementation was subsequently delivered in B3-B18. | Brian Nguyen / AI-oracle | ☑ |
| R5 | **Retry Constants Acceptance** — Original decision recorded (see R5 Decision Note below). Timeout constants accepted: `WEBHOOK_CONNECT_TIMEOUT=5s`, `WEBHOOK_REQUEST_TIMEOUT=30s`, `WEBHOOK_MAX_TOTAL_DURATION=120s`. Retry/backoff policy accepted: exponential backoff with full jitter, base 2s, multiplier 2.0, max delay 30s, max 3 attempts. Error classification and 429 special-case behavior accepted. 120s ceiling edge case documented. Bounded non-production implementation was subsequently delivered in B3-B18. | Brian Nguyen / AI-oracle | ☑ |
| R6 | **Test Plan Mapping to G1–G8** — Original decision recorded (see R6 Decision Note below). G1–G8 mapped to checks with concrete commands or file locations. G7 unit test module and G8 wiremock integration test delivered with case lists. Delivery metrics counters added to test plan. Live Postgres RLS tests remain ignored/manual. Bounded non-production implementation was subsequently delivered in B3-B18. | Brian Nguyen / AI-oracle | ☑ |
| R7 | **Rollback / Non-Goals Acknowledgment** — Decision recorded (see R7 Decision Note below). Env gate `INTENT_API_WEBHOOK_DELIVERY` documented with default/conservative behavior and rollback/roll-forward procedure. `failure_reason` truncation/redaction policy accepted (max 500 chars, URL stripping, PII redaction). Failed-to-pending reset semantics: manual operator action only for Slice 3. Delivery task lifecycle: `.await`ed synchronously within the apply handler post-commit; `tokio::spawn` fire-and-forget remains a deferred aspiration. Non-goals restated. RB13 runbook and `WebhookDeliveryFailureRate` local alert rule delivered in B12-B13. No production readiness claim. | Brian Nguyen / AI-oracle | ☑ |
| R8 | **Go / No-Go Decision** — Explicit go/no-go gate convened before first commit; if any R1–R7 item is unresolved or any Pre-R8 Blocker (B1–B2) lacks a documented resolution path, decision must be **No-Go** with recorded reason and re-review date. **BOUNDED GO** authorizes starting the first non-production Slice 3 implementation slice only; production readiness, production deployment, and external signoff are explicitly excluded. | Brian Nguyen | ☑ |

#### R2 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** Dependency placement decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered `reqwest` as a crate-local regular dependency and `wiremock` as a crate-local dev-dependency in `intent-api`. R8 was a **BOUNDED GO** for the first non-production implementation slice.

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

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added the corresponding non-production `Cargo.toml` changes (`reqwest` regular dependency, `wiremock` dev-dependency).

#### R3 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** Schema and trait review decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered migration `018_create_webhook_subscriptions.sql`, `record_delivery_attempt` and `record_delivery_outcome` trait methods, and their SQL/in-memory implementations. R8 was a **BOUNDED GO** for the first non-production implementation slice.

**Existing schema findings (migration 017):**
- `propagation_records` already includes the delivery columns needed for Slice 3: `delivery_attempt_count`, `last_delivery_attempt_at`, `failure_reason`, and `failed_at`.
- The `PropagationRecord` domain type already carries these fields.
- **Conclusion:** No additive migration is required for `propagation_records` in Slice 3.

**Trait gap:**
- `PropagationRecordRepository` currently lacks methods to atomically record delivery attempts and outcomes.
- The existing `update_status` method does not cover incrementing `delivery_attempt_count`, setting `last_delivery_attempt_at`, or recording `failure_reason`.
- **Conclusion:** Delivery-specific methods were added to the trait in bounded implementation B3-B18 (see B2 resolution below).

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

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added migration `018_create_webhook_subscriptions.sql` and the corresponding non-production Rust trait and repository changes.

#### R4 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** RLS and tenant implications decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered migration 018 with RLS policies, tenant-scoped dispatcher queries, URL redaction in `sanitize_failure_reason`, and ignored live RLS tests for `webhook_subscriptions`. R8 was a **BOUNDED GO** for the first non-production implementation slice.

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

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added migration 018 with RLS, `sanitize_failure_reason` implementation, and ignored live RLS tests for `webhook_subscriptions`.

#### R5 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** Retry constants and error classification decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered timeout/retry constants, exponential backoff with full jitter, error classification, and 429 special-case handling in `webhook_delivery.rs`. R8 was a **BOUNDED GO** for the first non-production implementation slice.

**D5 — Timeout constants (accepted):**
- `WEBHOOK_CONNECT_TIMEOUT`: 5 seconds — TCP + TLS handshake establishment.
- `WEBHOOK_REQUEST_TIMEOUT`: 30 seconds — total per-delivery attempt (connect + send + wait-for-response).
- `WEBHOOK_MAX_TOTAL_DURATION`: 120 seconds — hard ceiling for all attempts including retries; abort remaining retries if exceeded.

**D6 — Retry / backoff policy (accepted):**
- Exponential backoff with full jitter.
- Base delay: 2 seconds; multiplier: 2.0; max delay: 30 seconds; max attempts: 3 total (initial + 2 retries).
- Jitter formula: `rand::random::<f64>() * delay` (full jitter).
- **Dependency caveat:** full jitter uses the crate-local regular dependency `rand` in `intent-api`, added during bounded implementation B3-B18.

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

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added the non-production retry/backoff Rust code, error classification logic, and `rand` crate-local dependency.

#### R6 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** Test plan mapping decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered `webhook_delivery_tests.rs` with G7 payload/header tests and G8 wiremock delivery simulation tests, plus metrics counters and local Prometheus alert rules. R8 was a **BOUNDED GO** for the first non-production implementation slice.

**G1–G8 mapping (accepted):**

| Gate | Check | Command / Location |
|------|-------|-------------------|
| G1 — Compile | No warnings | `cargo check --workspace` (existing CI) |
| G2 — Format | No diff | `cargo fmt --all -- --check` (existing CI) |
| G3 — Lint | No clippy warnings | `cargo clippy --workspace --all-targets -- -D warnings` (existing CI) |
| G4 — Route wiring | All routes reachable | `cargo test -p intent-api --lib router_smoke_tests` (existing) |
| G5 — OpenAPI drift | Spec matches routes | `npx spectral lint docs/04-api/openapi.yaml --ruleset .spectral.yml` (existing CI) |
| G6 — Tenant isolation | RLS policies active | Ignored/manual live Postgres: `cargo test -p intent-api --test rls_integration -- --ignored` (existing pattern) |
| G7 — Handler unit test | Payload shape + headers | `crates/intent-api/src/webhook_delivery_tests.rs` (57 tests covering payload shape, headers, retry behavior, metrics counters, env gate, and RLS isolation) |
| G8 — Delivery simulation | Mock HTTP server verifies retry behavior | `crates/intent-api/src/webhook_delivery_tests.rs` (wiremock-based delivery simulation tests) |

**D9 — G7 future unit test module (proposed):**
- File: `crates/intent-api/src/webhook_delivery_tests.rs`
- Registration: `#[cfg(test)] mod webhook_delivery_tests;` in `crates/intent-api/src/lib.rs`
- Proposed test cases:
  - Payload shape matches proposed JSON schema (event_type, intent_id, tenant_id, version, version_hash, previous_version, timestamp, delivery_id, attempt_number, subscription_id).
  - `Content-Type: application/json` header present.
  - `X-Idempotency-Key` header contains `delivery_id`.
  - `attempt_number` increments correctly across retries.
  - `attempt_number` is 1 on initial attempt and frozen on exhausted retries.
  - `X-Webhook-Signature` header is **absent** because HMAC signing is deferred.
  - `failure_reason` does not leak full URLs (R4 D3 cross-reference).

**D10 — G8 future wiremock integration test (proposed):**
- File: `crates/intent-api/tests/webhook_delivery_simulation.rs`
- Auto-discovered; no manual registration needed.
- Uses in-memory repository and `wiremock`; does **not** require `DATABASE_URL` or live Postgres.
- Proposed test cases:
  - Retry on HTTP 5xx then eventual success.
  - No retry on non-429 4xx; mark `failed` immediately.
  - Malformed URL fails without retry.
  - HTTP 429 with `Retry-After` header: wait, then success.
  - HTTP 429 without `Retry-After`: fallback to standard backoff, then success or failure.
  - Double 429 (retry also 429) marks `failed`.
  - Retry exhaustion after 3 attempts marks `failed`.
  - `WEBHOOK_MAX_TOTAL_DURATION` exceeded: record `timeout: max_total_duration_exceeded`, mark `failed`, exit gracefully (R5 D8 cross-reference).
  - Async fire-and-notify: caller response is not blocked by delivery.
  - Tenant isolation lookup prevents cross-tenant delivery (R4 D2 cross-reference).

**D11 — Future delivery observability metrics (proposed):**
- Counter names follow existing `intent_api_propagation_signals_*` convention:
  - `intent_api_webhook_delivery_attempted_total`
  - `intent_api_webhook_delivery_succeeded_total`
  - `intent_api_webhook_delivery_failed_total`
  - `intent_api_webhook_delivery_retry_exhausted_total`
- Histogram (e.g., delivery duration) is deferred to future observability work.
- Prometheus alerting rule authoring is not in R6 scope.

**D12 — Live Postgres RLS test note:**
- Existing `rls_integration` tests remain ignored/manual by default (`-- --ignored`).
- Future `webhook_subscriptions` RLS tenant-isolation tests are a post-migration test-plan item, not R6 implementation.
- Wiremock integration tests (G8) are normal `cargo test` tests and do not require live Postgres.

**Explicit non-goals:**
- No test files are written in this docs-only slice.
- No live Postgres run is required for R6.
- No Prometheus rule authoring.
- No per-attempt delivery log testing (deferred).
- No load/stress testing.
- No cross-workflow lineage testing.

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added `webhook_delivery_tests.rs` (57 tests), metrics counters, local Prometheus alert rules, and apply-path wiremock integration tests.

#### R7 Decision Note (Original Decision — Delivered in B3-B18)

> **Status:** Rollback, non-goals, and operational policy decision recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently delivered env gate `INTENT_API_WEBHOOK_DELIVERY`, `sanitize_failure_reason`, RB13 runbook, `WebhookDeliveryFailureRate` local alert rule, and observability docs updates. R8 was a **BOUNDED GO** for the first non-production implementation slice.

**D13 — Env gate rollback / roll-forward:**
- Gate name: `INTENT_API_WEBHOOK_DELIVERY` (boolean).
- Checked at dispatcher spawn time only.
- **Default behavior:**
  - Local dev: `true` (enable dispatch for testing).
  - Production: `false` (disable dispatch until SRE sign-off).
  - Unset or invalid: conservative `false`.
- **Rollback (disable dispatch):** set `INTENT_API_WEBHOOK_DELIVERY=false`, restart. Propagation signal creation (`POST /intents/{intent_id}/propagation-signals`) and query (`GET /intents/{intent_id}/propagation-status`) remain enabled; only webhook POST dispatch is disabled. `attempted_total` counter stops increasing. No migration rollback, code revert, or data loss.
- **Roll-forward (re-enable dispatch):** set `INTENT_API_WEBHOOK_DELIVERY=true`, restart. Next rebase apply resumes delivery for new signals.

**D14 — `failure_reason` truncation / redaction policy (accepted):**
- Max length: 500 characters; append truncation marker (`... [truncated]`) if exceeded.
- URL stripping: remove path, query parameters, fragments, and embedded credentials.
- Body snippet: max 100 characters from downstream response body; do not log full bodies.
- PII redaction: detect and mask common patterns (email addresses, IP addresses, JWT tokens).
- **Ownership:** R7 consolidates R4 D3 (URL logging redaction) and R5 D7 (error classification redaction).
- **Implementation:** `sanitize_failure_reason` was delivered in bounded implementation B3-B18.

**D15 — Failed-to-pending reset semantics (accepted):**
- Slice 3: `failed` records reset to `pending` only via explicit operator action (manual re-signal or direct SQL update).
- Future dispatcher skips `failed` records; it does not automatically retry them.
- Automatic reset on intent version bump is **Phase 4+** scope, not Slice 3.
- This is a future design requirement documented now; no implementation is written.

**D16 — Delivery task lifecycle (accepted):**
- Current bounded behavior: `.await`ed synchronously within the apply handler post-commit. Delivery completes before the handler returns.
- Deferred aspiration: `tokio::spawn` fire-and-forget conversion (not implemented; remains Phase 4+ scope). See P2-6b background delivery worker lifecycle design in [Production Readiness Backlog](./17-production-readiness-backlog.md).
- Process restart: in-flight deliveries are lost; no in-flight recovery.
- Shutdown: no `CancellationToken` or graceful shutdown for Slice 3.
- Timeout enforcement: 120s max total duration per R5 D8.
- Outcome recording: best-effort via repository methods.
- Concurrency: sequential per intent; separate intents are independent with no shared backoff state.

**D17 — Non-goals restatement (accepted):**
- No outbox or transactional boundary spanning DB write + HTTP delivery.
- No distributed transactions.
- No delivery guarantees (best-effort only).
- No background retry worker (in-process sequential retries only).
- No production-readiness claim.
- No HMAC signing or key rotation (deferred). See P2-6c design in [Production Readiness Backlog](./17-production-readiness-backlog.md).
- No subscription CRUD API endpoints (deferred). See P2-6d design in [Production Readiness Backlog](./17-production-readiness-backlog.md).
- No event streaming / NATS integration (Slice 4).
- No cross-workflow lineage (Slice 5).
- No per-attempt delivery log table (deferred).
- No dead-letter topic or queue (deferred).
- No consumer-managed subscriptions (deferred).
- No multi-region replication (deferred).
- No SLA or latency guarantee (deferred).

**D18 — Runbook and observability (delivered in B12-B13):**
- **RB13 — Webhook Delivery Failures** delivered in `docs/09-operations/05-runbooks.md` (B12).
- **Local Prometheus rule `WebhookDeliveryFailureRate`** delivered in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` (B13).
- Observability docs updated in `docs/09-operations/03-observability.md` and `docs/09-operations/04-sre-and-slos.md` (B15).

**N1–N6 resolution summary:**
| # | Refinement | Resolved In | Decision |
|---|------------|-------------|----------|
| N1 | Workspace dependency placement | R2 | `reqwest` and `wiremock` crate-local in `intent-api` |
| N2 | Delivery observability metrics | R6 D11 | Four counters defined; histogram deferred |
| N3 | Failed-to-pending reset semantics | R7 D15 | Manual operator action only for Slice 3; automatic reset is Phase 4+ |
| N4 | Delivery task lifecycle | R7 D16 | `.await`ed synchronously within handler post-commit; `tokio::spawn` fire-and-forget remains deferred |
| N5 | `failure_reason` truncation / redaction | R7 D14 | Max 500 chars, URL stripping, body snippet 100 chars, PII redaction |
| N6 | Feature flag / env rollback gate | R7 D13 | `INTENT_API_WEBHOOK_DELIVERY` boolean; conservative default |

**Historical note:** These decisions were recorded in the original docs-only slice. Bounded implementation B3-B18 subsequently added `sanitize_failure_reason`, RB13 runbook, local Prometheus alert rules, and observability docs updates.

#### R8 Decision Note (Bounded Implementation Complete — B3-B18)

> **Status:** Bounded Go decision was recorded in the docs-only slice. Subsequent commits B3-B18 implemented the bounded non-production Slice 3 code. The R8 scope boundaries and non-goals listed in D22 remain in force — no production readiness claim is made.

**D19 — Bounded Go verdict:**
- Decision: **BOUNDED GO** for the first non-production Slice 3 implementation slice only.
- Implementation completed in commits B3-B18 (payload builders, async skeleton, env-gated dispatcher, retry loop with incrementing `attempt_number`, metrics counters, RB13 runbook, `WebhookDeliveryFailureRate` local alert rule, webhook_subscriptions RLS test/helpers, docs sync, dead_code cleanup).
- This does **not** authorize production deployment, production readiness claims, or external signoff.
- Production readiness, production deployment, and external signoff are explicitly excluded.

**D20 — Prerequisites resolved:**
- B1: migration `018_create_webhook_subscriptions.sql` delivered (webhook URL / subscription storage with RLS).
- B2: trait methods `record_delivery_attempt` and `record_delivery_outcome` delivered on `PropagationRecordRepository`.

**D21 — R1–R7 acceptance and N1–N6 resolution:**
- R1–R7 were checked and accepted in the docs-only slice.
- B1 and B2 are now implemented.
- N1–N6 are resolved per R2 (dependency placement), R6 (test plan / metrics), and R7 (rollback, non-goals, env gate).

**D22 — Scope boundaries and non-goals preserved:**
- Env gate `INTENT_API_WEBHOOK_DELIVERY` defaults `false` outside local/dev; local-only testing may set `true`.
- Bounded Go does **not** authorize: outbox pattern, transactional delivery, delivery guarantees, background retry workers, HMAC signing, subscription CRUD API endpoints, event streaming, cross-workflow lineage, per-attempt delivery log table, dead-letter queue, production readiness, or external receiver production config.
- Owner Brian Nguyen signs off on the bounded scope and non-goals listed above.

#### Pre-R8 Blockers / Open Decisions

> **Status:** B1–B2 were blockers for the first implementation slice and are now **resolved** in commits B3-B18. Implementation has been delivered as bounded non-production code.

**Blockers (resolved in B3-B18):**

| # | Blocker | Resolution |
|---|---|---------|
| B1 | **No webhook URL / subscription storage existed** — Migration `018_create_webhook_subscriptions.sql` delivered with `tenant_id` RLS policy. `SqlxWebhookSubscriptionResolver` and `InMemoryWebhookSubscriptionResolver` implementations exist. | Resolved — dispatcher queries `webhook_subscriptions` by `(tenant_id, intent_id)`. |
| B2 | **`PropagationRecordRepository` lacked delivery-outcome update methods** — Trait methods `record_delivery_attempt` and `record_delivery_outcome` delivered with in-memory and SQLx implementations. | Resolved — `InMemoryPropagationRecordRepository` and `SqlxPropagationRecordRepository` both implement delivery attempt/outcome recording. |

**Non-blocking Readiness Refinements (documented, do not block design approval):**

| # | Refinement | Recommendation |
|---|------------|----------------|
| N1 | **Workspace dependency placement** — Decide which crate owns the `reqwest` dependency and whether the mock library is placed in workspace `dev-dependencies` or crate-local. | Document in crate README or module doc before first PR. |
| N2 | **Delivery observability metrics** — Define counters/gauges for webhook delivery (e.g., `intent_api_webhook_delivery_attempted_total`, `intent_api_webhook_delivery_succeeded_total`, `intent_api_webhook_delivery_failed_total`, `intent_api_webhook_delivery_retry_exhausted_total`). | Add to test plan (R6) and metrics registry doc; follow existing `intent_api_propagation_signals_*` naming convention. |
| N3 | **Failed-to-pending reset semantics** — Specify when and how a `failed` record transitions back to `pending` (e.g., manual operator reset only, or automatic on next intent version change). | Default recommendation: manual reset only for Slice 3; automatic re-signal on new version is Phase 4+ scope. |
| N4 | **Delivery task lifecycle** — Document spawn behavior (`tokio::spawn`), cancellation on shutdown, and what happens if the delivery task panics or is dropped. | Default recommendation: fire-and-forget with `tracing::error!` on panic; no restart logic for Slice 3. |
| N5 | **`failure_reason` truncation / redaction** — Agree max length (e.g., 500 chars) and whether to redact URLs, tokens, or PII from downstream response bodies before persisting. | Default recommendation: truncate to 500 chars and redact any URL query parameters. |
| N6 | **Feature flag / env rollback gate** — Choose an explicit env var or compile-time feature flag name to enable/disable dispatch without code change. | Default recommendation: env-gated at dispatcher spawn point; default disabled (false) until explicitly enabled. |

> **Bounded Go criteria met:** R1–R7 are checked and accepted; Pre-R8 Blockers B1–B2 have documented resolution paths; owner Brian Nguyen signs off on bounded scope and non-goals. This is a **BOUNDED GO** for the first non-production Slice 3 implementation slice only. Production readiness, production deployment, and external signoff are explicitly excluded.
> **No-Go criteria (still apply to any scope creep):** Any R1–R7 item is unresolved, any Pre-R8 Blocker (B1–B2) lacks a resolution path, or scope creep is introduced (e.g., outbox pattern, background worker, delivery guarantees, production deployment, production readiness claims, external signoff).
> **Re-review:** If future review downgrades to No-Go, re-review no sooner than one week after blockers are addressed.

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
