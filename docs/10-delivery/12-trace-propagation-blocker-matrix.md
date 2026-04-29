# 12 — Cross-Process Trace Propagation Blocker Matrix

**Status:** Proposed (Investigation Complete — Implementation Gated)
**Phase:** Phase 3+
**Owner:** SRE / Backend Lead

---

## Purpose

This document catalogs the blockers preventing full cross-process distributed trace propagation across Intent Rebase Engine service boundaries. Each blocker row documents the root cause, affected components, and explicit unblock conditions required before implementation can proceed.

> **Note:** In-process trace propagation (within a single service) is implemented in Phase 3 Batch 2 Slices 2, 8, and 9. Cross-process propagation requires additional infrastructure decisions documented here.

---

## Blocker Matrix

### B-01: Temporal SDK Per-Request Metadata Injection

| Field | Value |
|-------|-------|
| **Blocker ID** | B-01 |
| **Affected Components** | `runtime-adapter`, `rebase-orchestrator` |
| **Trace Path** | intent-api → Temporal workflow execution |
| **Current State** | Phase 2b bounded Temporal adapter tracing delivers local span correlation around Temporal gRPC calls (P2-S8). W3C trace-context is NOT injected into Temporal workflow metadata. |
| **Root Cause** | `temporalio-client` 0.2.0 uses a shared `Arc<RwLock>` for client-level options. There is no per-request gRPC metadata injection mechanism to propagate trace context into workflow executions. |
| **Unblock Conditions** | 1. Upgrade to `temporalio-client` version that supports per-request metadata injection **OR** 2. Implement a trace-context-encoding approach using workflow payload (e.g., encode traceparent as workflow argument) **OR** 3. Accept cross-process trace gap for Temporal path and document as known limitation |
| **Priority** | Medium |
| **Risk If Delayed** | Temporal-bound workflows appear as disconnected trace segments. Root cause analysis across Temporal boundaries requires manual correlation using workflow ID. |
| **Status** | 🔴 BLOCKED |

#### Technical Details

```
temporalio-client 0.2.0 architecture:
┌─────────────────────────────────────────────────────────┐
│ Client (Arc<RwLock<ClientOptions>>)                      │
│   └── Cannot inject per-request metadata                 │
│       into workflow execution requests                  │
└─────────────────────────────────────────────────────────┘

Current behavior:
 intent-api (traceparent: 00-abc-123-01)
   └── start_workflow(workflow_id=xyz)
       └── Temporal sees: no trace context
           └── Workflow executes in isolated trace context
```

#### Workaround Options

**Option A — Workflow Payload Encoding (Recommended for Phase 3)**

Encode W3C trace context as workflow input:

```rust,ignore
#[workflow]
async fn rebase_workflow(input: RebaseWorkflowInput) -> RebaseWorkflowResult {
    // Extract trace context from input if present
    let traceparent = input.trace_context.as_ref()
        .map(|ctx| ctx.traceparent.clone());

    // Propagate to child activities
    ...
}
```

**Option B — Temporal Audit Event Correlation**

Use `workflow_id` as the correlation key in audit events, enabling post-hoc correlation without native trace propagation.

---

### B-02: sqlx Per-Query Context Propagation

| Field | Value |
|-------|-------|
| **Blocker ID** | B-02 |
| **Affected Components** | `intent-service`, `graph-service`, `forensic-service` |
| **Trace Path** | intent-api → PostgreSQL queries |
| **Current State** | Phase 2b bounded sqlx repository tracing delivers local span correlation around high-value transactions (P2-S9). Trace context is NOT propagated into SQL query tags/logs. |
| **Root Cause** | `sqlx` 0.8 does not support per-query context propagation. `QueryBuilder` and `sqlx::query()` do not accept trace context. There is no mechanism to inject `trace_id` into SQL query comments or logging tags. |
| **Unblock Conditions** | 1. `sqlx` adds per-query context/tag support **OR** 2. Implement application-level query tagging using `SET LOCAL trace_id = '...'` (requires PostgreSQL extension) **OR** 3. Accept that database traces are correlated via parent span only (current behavior) |
| **Priority** | Low |
| **Risk If Delayed** | SQL query spans show up under parent service span but without explicit `trace_id` tag. Correlation requires following parent relationship rather than direct ID lookup. |
| **Status** | 🟡 LOW PRIORITY — Acceptable limitation with current design |

#### Technical Details

```
sqlx current behavior:
 intent-api span (traceparent: 00-abc-123-01)
   └── "SELECT * FROM intents" (no trace_id tag)
       └── sqlx executor executes without context

Desired behavior (future):
 intent-api span (traceparent: 00-abc-123-01)
   └── "/* trace_id=abc123 */ SELECT * FROM intents"
       └── PostgreSQL log includes trace_id for correlation
```

---

### B-03: NATS Publisher Trace Header Injection

| Field | Value |
|-------|-------|
| **Blocker ID** | B-03 |
| **Affected Components** | `intent-api` (NATS event publisher) |
| **Trace Path** | intent-api → NATS → Event consumers |
| **Current State** | Phase 2b bounded slice: `NatsEventPublisher` implemented with W3C trace-context header injection (`traceparent`). Uses async-nats 0.47 with core publish (fire-and-forget). Fails open on connection/publish errors. Bounded timeouts: 2s connect, 1s publish, one retry with backoff. Bounded `JetStreamInitializer` delivered and wired fail-safe when `NATS_URL` exists (creates `audit_events` stream for `audit.events.v1.>`; no DLQ subject). **Consumer lifecycle and DLQ/retry worker remain Phase 3 scope.** |
| **Root Cause** | Previously blocked on async-nats vs nats 0.33 decision and core publish vs JetStream decision. Decision made: async-nats + core publish. `NatsEventPublisher` implemented in `crates/intent-api/src/nats_event_publisher.rs`. |
| **Unblock Conditions** | ✅ DECISION MADE: async-nats (tokio-based) + core publish (fire-and-forget) **AND** ✅ `NatsEventPublisher` implemented with W3C trace-context header injection **AND** ✅ bounded JetStream stream creation **AND** ⬜ Phase 3: consumer subscription lifecycle and DLQ/retry worker |
| **Priority** | High |
| **Risk If Delayed** | Events published to NATS have trace context via traceparent header. Downstream consumers (Phase 3) can extract trace context. End-to-end trace propagation is partially enabled at publish boundary. |
| **Status** | 🟡 PARTIALLY RESOLVED — bounded stream init delivered; consumer lifecycle/DLQ worker remain Phase 3 |

#### NATS Implementation Decisions (RESOLVED)

| Decision Point | Resolution | Notes |
|---------------|------------|-------|
| **NATS Client Library** | `async-nats` 0.47 | Replaces deprecated `nats = "0.33"`. Tokio-native, actively maintained. |
| **Publish Mode** | Core publish (fire-and-forget) | Bounded publisher remains core publish. Bounded JetStream stream initialization exists for `audit_events`; consumer lifecycle and DLQ/retry worker remain Phase 3 productionization scope. |
| **Trace Header Injection** | ✅ Implemented | W3C traceparent header injected when trace context is available. Uses `trace_id` + `span_id` from `TraceContext`. |
| **Fail-Open Behavior** | ✅ Implemented | Connection/publish errors return `PublishResult::Skipped`. Server startup not blocked by NATS unavailability. |
| **Bounded Timeouts** | ✅ Implemented | Connect: 2s, Publish: 1s, One retry with 100ms base/500ms max backoff. |

#### S3/Slice B Relationship

> **Important:** This blocker is directly related to **Slice B** of the Phase 3 work. The async-nats vs nats 0.33 decision has been made (`async-nats` 0.47), bounded core publishing is implemented, and a bounded JetStream stream initializer/consumer adapter seam now exists. Remaining work is productionizing the consumer lifecycle and DLQ/retry worker.

---

### B-04: NATS Consumer Trace Context Extraction

| Field | Value |
|-------|-------|
| **Blocker ID** | B-04 |
| **Affected Components** | `intent-service` (checkpoint creator consumer), `compensation-service` (future) |
| **Trace Path** | NATS → intent-service consumer |
| **Current State** | Phase 2b bounded in-memory consumer abstraction proves the event→action path. `NatsPullConsumerAdapter` exists with native `traceparent` extraction from NATS message headers and ignored live tests. Consumer subscription lifecycle and background worker runtime are not implemented/productionized. **Phase 3 scope.** |
| **Root Cause** | NATS consumer implementation (real subscriptions) is Phase 3. B-03 publisher with header injection is now partially resolved, but consumer infrastructure remains Phase 3. |
| **Unblock Conditions** | ✅ Bounded NATS consumer adapter with header extraction **AND** ✅ bounded JetStream stream creation **AND** ⬜ wire consumer into startup/background lifecycle **AND** ⬜ implement DLQ/retry worker |
| **Priority** | High |
| **Risk If Delayed** | Consumer spans appear as root spans without parent trace context. Trace lineage is broken at the consume boundary. |
| **Status** | 🔴 BLOCKED — Phase 3 scope (JetStream consumers/DLQ) |

---

### B-05: Cross-Service HTTP Header Forwarding

| Field | Value |
|-------|-------|
| **Affected Components** | All HTTP endpoints (intent-api, graph-service, forensic-service) |
| **Trace Path** | External client → intent-api → downstream services |
| **Current State** | Phase 3 Batch 2 Slice 2 implements W3C trace-context extraction from inbound requests and adds traceparent/tracestate response headers. |
| **Root Cause** | No blocking issue. W3C trace-context extraction and response header injection are implemented. However, forwarding trace context to downstream HTTP calls is not yet implemented. |
| **Unblock Conditions** | 1. Implement trace context propagation in HTTP client (outbound requests to graph-service, forensic-service, etc.) **AND** 2. Add trace context to OpenTelemetry span attributes |
| **Priority** | Medium |
| **Risk If Delayed** | Downstream service calls appear as separate trace segments without parent context. End-to-end request traces are incomplete. |
| **Status** | 🟡 IN PROGRESS — Implementation straightforward once B-03 decisions are made |

---

## Blocker Dependency Graph

```
B-03: NATS Publisher ✅ RESOLVED (bounded core publisher delivered)
   │
   ├──► B-04: NATS Consumer 🔴 Phase 3 scope (JetStream consumers/DLQ)
   │
   └──► Cross-process trace propagation (partial — publisher side)

B-01: Temporal SDK (per-request metadata)
   │
   └──► Temporal workflow trace injection

B-02: sqlx (per-query context) — Low priority, acceptable limitation

B-05: HTTP header forwarding — In progress
```

---

## Slice B Relationship

**Slice B** refers to the bounded NATS publisher prerequisites work within P2 (Phase 3 Batch 2 — Observability + SRE). **Slice B (bounded core NATS publisher + JetStream initializer) is now DELIVERED:**

- ✅ Decision made: async-nats (tokio-based) + core publish (fire-and-forget)
- ✅ `NatsEventPublisher` implemented in `crates/intent-api/src/nats_event_publisher.rs`
- ✅ W3C trace-context header injection (`traceparent`)
- ✅ Fail-open behavior on connection/publish errors
- ✅ Bounded timeouts (2s connect, 1s publish, one retry with backoff)
- ✅ Wired into startup lifecycle via `NATS_URL` env var
- ✅ `JetStreamInitializer` delivered and wired fail-safe (creates `audit_events` stream with subject `audit.events.v1.>` when `NATS_URL` exists)
- ✅ `NatsPullConsumerAdapter` with native `traceparent` extraction exists (live tests ignored)
- ⬜ Phase 3 scope remains: consumer subscription lifecycle, background worker runtime, NATS consumers, DLQ/retry worker

---

## Unblock Action Items

| Blocker | Action | Owner | Status |
|---------|--------|-------|--------|
| B-01 | Evaluate temporalio-client upgrade path or document workflow payload encoding approach | Backend Lead | ⬜ Pending |
| B-02 | Document as acceptable limitation (low priority) | SRE | ⬜ Pending |
| B-03 | ✅ DECISION MADE: async-nats 0.47 + core publish | Backend Lead | ✅ RESOLVED |
| B-03 | ✅ Implement NatsEventPublisher with W3C trace header injection | Backend Lead | ✅ RESOLVED |
| B-04 | Phase 3: Implement NATS consumer with header extraction + JetStream streams | Backend Lead | 🔴 Phase 3 scope |
| B-05 | Implement HTTP client trace header forwarding | SRE | 🟡 In Progress |

---

## Related Documents

- [09 — Completion Proposals Tracker](./09-completion-proposals-tracker.md) (P2 progress)
- [P2 Progress Notes](./09-completion-proposals-tracker.md#p2--phase-3-batch-2--observability--sre) (cross-process trace deferred note)
- [Phase 3 Batch 2 Slice 2](./09-completion-proposals-tracker.md#cross-process-trace-propagation) (bounded OTEL propagation delivered)
- [event_publisher.rs](../../crates/intent-rebase-types/src/event_publisher.rs) (Phase 2b bounded publisher abstraction)
- [nats_event_publisher.rs](../../crates/intent-api/src/nats_event_publisher.rs) (Phase 2b bounded NATS publisher implementation)
- [Cargo.toml](../../Cargo.toml) (async-nats = "0.47" dependency — replaces deprecated nats = "0.33")
