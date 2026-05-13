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
- [ ] OpenAPI descriptive text updated (this doc update)
- [ ] Subscription CRUD endpoints remain deferred

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

### Slice 3 — Webhook Delivery (Bounded)

**Scope:**
- Async webhook dispatcher (bounded: sequential delivery, no queue)
- On `signaled`, POST to each subscription URL with intent change payload
- Update `propagation_records` status based on delivery outcome
- Delivery attempt count and failure reason recorded

**Acceptance criteria:**
- [ ] Webhook delivery triggers on intent version change
- [ ] Status transitions from `pending` → `acknowledged` or `failed`
- [ ] Retry policy applied per failure semantics table
- [ ] Delivery audit trail exists (attempt_count, last_delivery_attempt_at)

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
