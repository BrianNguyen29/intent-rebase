# Phase 2 — Runtime Integrated Rebase

## Scope
- default runtime adapter: Temporal (ADR-01); alternative adapters permitted per ADR
- low/medium risk apply path
- checkpoint selection
- approval stale detection
- artifact quarantine

## KPIs
- rebase apply succeeds on happy path
- avoid full restart in >= 40% test scenarios
- zero unsafe auto-apply in critical scenarios
