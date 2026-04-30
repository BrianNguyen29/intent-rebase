# ADR-01 — Runtime Adapter Selection

**Status:** Accepted
**Date:** 2026-04-03
**Authors:** Intent Rebase Engine Team
**Phase:** Phase 0–1

---

## Context

Intent Rebase Engine (IRE) hoạt động như một control layer trên các agent runtime execution platforms. Nó cần khả năng:

- Theo dõi intent versions và execution checkpoints
- Phát hiện và phản ứng với intent changes
- Gửi rebase signals tới runtime để adjust/pause/resume workflows
- Mapping checkpoint ↔ intent version để support replay

Các runtime platforms phổ biến bao gồm Temporal, Prefect, Airflow, hoặc custom event-loop runtimes.

---

## Decision

**Chọn MockAdapter làm default runtime adapter, với TemporalAdapter available qua explicit opt-in khi compiled với `temporal` feature.**

### Rationale

1. **Bounded selection** — Phase 2b bounded scope: MockAdapter là default để giữ dev/test workflow không phụ thuộc vào live Temporal cluster.
2. **Explicit Temporal opt-in** — TemporalAdapter chỉ được activate khi `INTENT_API_RUNTIME_ADAPTER=temporal` và compiled với `temporal` feature.
3. **Fail-clear on misconfiguration** — Temporal request without feature/config phải fail visibly (not silent mock fallback).
4. **No production readiness claim** — Trace propagation (W3C traceparent/tracestate) không được support với SDK hiện tại.

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
- Dev/test workflow không phụ thuộc vào live Temporal cluster
- Clear failure mode khi Temporal requested nhưng không available/configured
- Temporal adapter sẵn sàng khi bounded scope mở rộng

### Negative
- Temporal Cloud hoặc self-hosted cluster vẫn là operational dependency khi opt-in
- Trace propagation không support trong Phase 2b bounded scope

### Neutral
- Adapter trait abstract hóa runtime-specific logic; protocol-level changes affect only adapter
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
