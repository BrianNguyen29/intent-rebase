# Phase 2 — Runtime Integrated Rebase

## Scope
- chọn 1 adapter chính: Temporal hoặc LangGraph
- low/medium risk apply path
- checkpoint selection
- approval stale detection
- artifact quarantine

## KPIs
- rebase apply thành công trên happy path
- tránh full restart ở >= 40% test scenarios
- zero unsafe auto-apply ở critical scenarios
