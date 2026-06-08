# ADR-01 — Runtime Adapter Selection

**Status:** Accepted
**Date:** 2026-04-03
**Authors:** Intent Rebase Engine Team
**Phase:** Phase 0–1

---

## Context

Intent Rebase Engine (IRE) operates as a control layer over agent runtime execution platforms. It requires the ability to:

- Track intent versions and execution checkpoints
- Detect and react to intent changes
- Send rebase signals to the runtime to adjust/pause/resume workflows
- Map checkpoint ↔ intent version to support replay

Common runtime platforms include Temporal, Prefect, Airflow, or custom event-loop runtimes.

---

## Decision

**Select MockAdapter as the default runtime adapter, with TemporalAdapter available through explicit opt-in when compiled with the `temporal` feature.**

### Rationale

1. **Bounded selection** — Phase 2b bounded scope: MockAdapter is the default to keep dev/test workflows independent of a live Temporal cluster.
2. **Explicit Temporal opt-in** — TemporalAdapter is only activated when `INTENT_API_RUNTIME_ADAPTER=temporal` and compiled with the `temporal` feature.
3. **Fail-clear on misconfiguration** — A Temporal request without the feature/config must fail visibly (not silently fall back to the mock).
4. **No production readiness claim** — Trace propagation (W3C traceparent/tracestate) is not supported with the current SDK.

### Adapter Architecture

```
Intent Rebase Engine
  └── Runtime Adapter Interface (trait RuntimeAdapter)
        ├── MockAdapter          ← default (dev/testing only)
        ├── TemporalAdapter      ← opt-in via INTENT_API_RUNTIME_ADAPTER=temporal
        ├── PreflightAdapter    ← future
        └── CustomEventLoopAdapter ← future
```

### Configuration

```bash
# Default: MockAdapter (dev/testing only)
INTENT_API_RUNTIME_ADAPTER=mock

# Opt-in for Temporal (requires temporal feature compiled in + config)
INTENT_API_RUNTIME_ADAPTER=temporal
TEMPORAL_ADDRESS=http://localhost:7233
TEMPORAL_NAMESPACE=default
TEMPORAL_TASK_QUEUE=intent-rebase
```

---

## Consequences

### Positive
- Dev/test workflows are independent of a live Temporal cluster
- Clear failure mode when Temporal is requested but not available/configured
- Temporal adapter is ready when the bounded scope expands

### Negative
- Temporal Cloud or self-hosted cluster remains an operational dependency upon opt-in
- Trace propagation is not supported in the Phase 2b bounded scope

### Neutral
- The adapter trait abstracts runtime-specific logic; protocol-level changes affect only the adapter
- Phase 0–2a: define trait and mock/internal wiring; Phase 2b bounded: explicit env-gated Temporal path
- Other runtimes deferred to Phase 4+

---

## Implementation Notes

- Define `RuntimeAdapter` trait in `src/runtime/adapter.rs`
- Implement `MockAdapter` for dev/testing (default)
- Implement `TemporalAdapter` using Temporal Rust client (feature-gated)
- Adapter handles: `get_checkpoints()`, `send_rebase_signal(...)`, `map_intent_to_checkpoint(...)`, `replay_from_checkpoint(...)`
- Current bounded replay semantics use cooperative workflow signaling with checkpoint metadata; native Temporal reset remains deferred until checkpoints carry Temporal run/event correlation
- Phase 0–2a: define trait and mock/internal wiring; Phase 2b: implement Temporal adapter external path in batches
- `select_runtime_adapter()` helper provides env-gated adapter selection with clear error messages

---

## Related ADRs

- [ADR-02](./02-data-plane.md) — Data plane storage decisions
- [ADR-03](./03-external-api.md) — How external systems interact with IRE
- [ADR-04](./04-event-broker.md) — Event streaming infrastructure

---

## References

- Temporal Rust SDK: https://github.com/temporal-rs/temporal-rs
- Temporal Signals: https://docs.temporal.io/features/signals
