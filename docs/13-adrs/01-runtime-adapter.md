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

**Chọn Temporal làm default primary runtime adapter.**

### Rationale

1. **Workflow-as-code model** — Temporal's paradigm phù hợp với intent-based execution, nơi workflow state chứa cả execution context và intent metadata.
2. **Activity-level checkpointing** — Temporal tracks activity completion, hỗ trợ fine-grained rebase mapping.
3. **Signals and continue-as-new** — Temporal hỗ trợ signaling mechanism cho phép IRE gửi rebase directives mà không cần workflow disruption lớn.
4. **Strong consistency guarantees** — Temporal provides external consistency, giảm race conditions khi mapping rebase decisions.
5. **Ecosystem maturity** — Temporal SDK có Rust client (`temporal-rs`), hỗ trợ long-running workflows với built-in retry, timeout, và heartbeating.

### Adapter Architecture

```
Intent Rebase Engine
  └── Runtime Adapter Interface (trait RuntimeAdapter)
        ├── TemporalAdapter     ← default
        ├── PreflightAdapter    ← future
        └── CustomEventLoopAdapter ← future
```

---

## Consequences

### Positive
- Temporal's activity history cung cấp sẵn checkpoint trail cho rebase mapping
- Signals API cho phép non-disruptive rebase directives
- Strong consistency giảm complexity trong rebase planning

### Negative
- Temporal là dependency bắt buộc; migration sang runtime khác yêu cầu adapter rewrite
- Temporal Cloud hoặc self-hosted cluster là operational dependency
- Temporal's activity retry semantics cần được mapped vào IRE's compensation model

### Neutral
- Adapter trait abstract hóa runtime-specific logic; protocol-level changes affect only adapter
- Phase 1 chỉ implement Temporal adapter; other runtimes deferred to Phase 4

---

## Implementation Notes

- Define `RuntimeAdapter` trait in `src/runtime/adapter.rs`
- Implement `TemporalAdapter` using Temporal Rust client
- Adapter handles: `get_checkpoints()`, `send_rebase_signal(...)`, `map_intent_to_checkpoint(...)`, `replay_from_checkpoint(...)`
- Current bounded replay semantics use cooperative workflow signaling with checkpoint metadata; native Temporal reset remains deferred until checkpoints carry Temporal run/event correlation
- Phase 0–2a: define trait and mock/internal wiring; Phase 2b: implement Temporal adapter external path in batches

---

## Related ADRs

- [ADR-02](./02-data-plane.md) — Data plane storage decisions
- [ADR-03](./03-external-api.md) — How external systems interact with IRE
- [ADR-04](./04-event-broker.md) — Event streaming infrastructure

---

## References

- Temporal Rust SDK: https://github.com/temporal-rs/temporal-rs
- Temporal Signals: https://docs.temporal.io/features/signals
