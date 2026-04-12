# SRE and SLOs

## SLO Definitions (Phase 3 Batch 2 - P2-S2)

**P2-S2 delivered:** SLO definitions, alerting rules, error-budget dashboard, metrics infrastructure, observability stack.

> **Note:** P2-S2 is a bounded slice covering items 2-1, 2-2, 2-3. Items 2-4, 2-5, 2-6 remain open.

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

**Status: Delivered (P2-S2 bounded slice)**

- adapter failures
- queue backlogs
- stuck compensations
- approval stale not triggering
- audit append failures

## Observability Infrastructure (P2-S2 Delivered)

### Local Development Stack

```bash
# Start observability stack
docker compose --profile observability up -d

# Access points:
# - Prometheus: http://localhost:9090
# - Grafana: http://localhost:3000 (admin/admin)
# - Alertmanager: http://localhost:9093
```

### Metrics Endpoint

Intent API exposes Prometheus metrics at `GET /metrics`.

**Key metrics:**
- `intent_api_version_created_total{status="success|error"}`
- `intent_api_rebase_preview_total{status="success|error"}`
- `intent_api_rebase_apply_total{status="success|error"}`
- `intent_api_audit_append_total{status="success|error"}`
- `intent_api_diff_duration_seconds_bucket`
- `intent_api_rebase_preview_duration_seconds_bucket`
- `intent_api_rebase_apply_duration_seconds_bucket`
- `intent_api_approval_wait_duration_seconds_bucket`
- `intent_api_error_budget_remaining{slo="..."}`
- `compensation_action_executed_total{status,strategy,feasibility}`

### Alerting Rules

Prometheus alerting rules defined in `infrastructure/local/prometheus/rules.yml`:

**Critical alerts:**
- `IntentVersionCreationSuccessRateCritical` (< 99.5%)
- `RebasePreviewAvailabilityCritical` (< 99.0%)
- `RebaseApplyAvailabilityCritical` (< 98.0%)
- `AuditAppendSuccessRateCritical` (< 99.5%)
- `DiffComputeLatencyCritical` (> 4s)
- `RebasePreviewLatencyCritical` (> 20s)
- `RebaseApplyLatencyCritical` (> 120s)
- `ApprovalWaitLatencyCritical` (> 60 min)
- `ErrorBudgetExhausted` (< 20% remaining)
- `ErrorBudgetDepleted` (0% remaining)

**Warning alerts:**
- `IntentVersionCreationSuccessRateWarning` (< 99.9%)
- `RebasePreviewAvailabilityWarning` (< 99.5%)
- `RebaseApplyAvailabilityWarning` (< 99.0%)
- `AuditAppendSuccessRateWarning` (< 99.9%)
- `DiffComputeLatencyWarning` (> 2s)
- `RebasePreviewLatencyWarning` (> 10s)
- `RebaseApplyLatencyWarning` (> 60s)
- `ApprovalWaitLatencyWarning` (> 30 min)
- `CompensationExecutionFailureRateWarning` (> 1%)

## Phase 3 Provisional Targets

These targets are Batch 0 planning inputs only.

- They are **not yet SRE-approved**.
- They should be confirmed or adjusted before Batch 2 alerting/dashboards are treated as exit evidence.

### Candidate service-level targets

- 99.9% successful intent version creation ✅ (delivered)
- 99.5% rebase preview availability ✅ (delivered)
- 99.0% rebase apply path availability ✅ (delivered)
- 99.9% audit append success ✅ (delivered)
- 99.0% compensation plan generation success once Batch 1 exists
- 99.0% forensic bundle generation success once Batch 3 exists

### Candidate latency targets

- p95 diff compute < 2s for structured changes ✅ (delivered)
- p95 rebase preview < 10s for medium graph size ✅ (delivered)
- p95 rebase apply < 60s for low/medium risk ✅ (delivered)
- p95 approval wait alert threshold: 30 minutes ✅ (delivered)
- p95 compensation execution target: define after Batch 1 basic flow exists
- p95 forensic bundle generation target: define after Batch 3 implementation data exists

### Batch 2 observability prep notes

- compensation and forensic targets should remain provisional until real implementations and benchmark baselines exist
- queue backlog and consumer lag alerts depend on production NATS/JetStream topology
- artifact quarantine failure alerts depend on a real artifact storage boundary
- forensic export/download alerts depend on Batch 3 API and storage implementation
