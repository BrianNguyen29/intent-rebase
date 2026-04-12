# SRE and SLOs

## P2-S1 Metrics Evidence (Phase 3 Batch 2 Slice 1 — Bounded)

**P2-S1 delivered:** Real metrics instrumentation on existing `/metrics` endpoint via `metrics-exporter-prometheus`. Full alerting/dashboard/OTel propagation/runbooks are P2-S2+ scope.

**Instrumented candidate metrics (real code paths, verified by cargo check/test):**
- `intent_rebase.intent.create.total` / `intent_rebase.intent.create.errors` — create_intent handler
- `intent_rebase.version.create.total` / `intent_rebase.version.create.errors` — create_version handler
- `intent_rebase.rebase.preview.total` / `intent_rebase.rebase.preview.errors` / `intent_rebase.rebase.preview.duration_seconds` — rebase_preview handler
- `intent_rebase.rebase.apply.total` / `intent_rebase.rebase.apply.errors` / `intent_rebase.rebase.apply.duration_seconds` — rebase_apply handler
- `intent_rebase.compensation.actions.total` / `intent_rebase.compensation.actions.errors` — approve/waive/execute/reapprove handlers
- `intent_rebase.compensation.execute.total` / `intent_rebase.compensation.execute.duration_seconds` / `intent_rebase.compensation.execute.success` / `intent_rebase.compensation.execute.failure` — execute_compensation_action handler
- `intent_rebase.compensation.planned.total` / `intent_rebase.compensation.planned.by_feasibility` — plan_compensation_actions in compensation-service

**Exposed via:** `GET /metrics` on intent-api (text/plain; version=0.0.4 Prometheus format)

**SLO applicability:** These metrics directly measure P2 candidate SLO paths (rebase preview availability, rebase apply path availability, intent creation success rate). Histogram buckets available for p50/p95/p99 latency derivation via standard Prometheus `histogram_quantile()`.

---

## Example SLOs
- 99.9% successful intent version creation
- 99.5% rebase preview availability
- 99.0% rebase apply path availability
- p95 diff compute < 2s for structured changes
- p95 rebase preview < 10s for medium graph size
- 99.9% audit append success

## Error budgets
- separate budgets for preview vs apply path
- critical path incidents consume budget faster

## On-call considerations
- adapter failures
- queue backlogs
- stuck compensations
- approval stale not triggering
- audit append failures

## Phase 3 provisional targets

These targets are Batch 0 planning inputs only.

- They are **not yet SRE-approved**.
- They should be confirmed or adjusted before Batch 2 alerting/dashboards are treated as exit evidence.

### Candidate service-level targets

- 99.9% successful intent version creation
- 99.5% rebase preview availability
- 99.0% rebase apply path availability
- 99.9% audit append success
- 99.0% compensation plan generation success once Batch 1 exists
- 99.0% forensic bundle generation success once Batch 3 exists

### Candidate latency targets

- p95 diff compute < 2s for structured changes
- p95 rebase preview < 10s for medium graph size
- p95 rebase apply < 60s for low/medium risk
- p95 approval wait alert threshold: 30 minutes
- p95 compensation execution target: define after Batch 1 basic flow exists
- p95 forensic bundle generation target: define after Batch 3 implementation data exists

### Batch 2 observability prep notes

- compensation and forensic targets should remain provisional until real implementations and benchmark baselines exist
- queue backlog and consumer lag alerts depend on production NATS/JetStream topology
- artifact quarantine failure alerts depend on a real artifact storage boundary
- forensic export/download alerts depend on Batch 3 API and storage implementation
