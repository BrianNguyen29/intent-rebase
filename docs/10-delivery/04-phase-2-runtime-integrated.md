# Phase 2 — Runtime Integrated Rebase

## Scope
- runtime adapter mặc định: Temporal (ADR-01); adapter thay thế được phép theo ADR
- low/medium risk apply path
- checkpoint selection
- approval stale detection
- artifact quarantine

## KPIs
- rebase apply thành công trên happy path
- tránh full restart ở >= 40% test scenarios
- zero unsafe auto-apply ở critical scenarios
