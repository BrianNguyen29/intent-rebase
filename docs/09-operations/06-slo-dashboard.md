# SLO Dashboard — Grafana Scaffold

> **Status (Batch 2 Slice 1 + Slice 5 + Slice 7):** Dashboard scaffold (Slice 1). Slice 5 adds an error-budget tracking row (preview + apply 1h burn-rate stat panels) backed by the metrics emitted in Slice 3 (intent_api_rebase_preview_requests_total, intent_api_rebase_apply_requests_total with status label). Slice 7 adds 6h and 3d burn-rate panels and multi-window burn-rate alerting rules.
> Panel queries reference metric names that require instrumentation to exist before they can return data.
> Alerting rules and distributed tracing are out of scope for this slice.

---

## Dashboard Overview

**Dashboard name:** Intent Rebase — SLO Overview  
**Data source:** Prometheus (once instrumented)  
**Refresh:** 30s (live) / 5m (historical)  
**Time range default:** 7d  
**Owner:** SRE / Platform  
**Notes:** All panels are scaffolded with provisional queries. They require actual metrics to be emitted before rendering real data.

---

## Panel Layout

### Row 1 — Availability SLOs

#### Panel 1: Intent Version Creation Success Rate
- **Metric:** `intent_api_intent_version_created_total` (counter)
- **Query:** `sum(rate(intent_api_intent_version_created_total{status="success"}[5m])) / sum(rate(intent_api_intent_version_created_total[5m])) * 100`
- **Type:** Stat / gauge
- **Thresholds:** 99.9% green / 99.0% yellow / 95.0% red
- **Notes:** Aggregates across all tenants. Requires instrumentation to emit `status` label.

#### Panel 2: Rebase Preview Availability
- **Metric:** `intent_api_rebase_preview_requests_total`
- **Query:** `sum(rate(intent_api_rebase_preview_requests_total{status="success"}[5m])) / sum(rate(intent_api_rebase_preview_requests_total[5m])) * 100`
- **Type:** Stat / gauge
- **Thresholds:** 99.5% green / 99.0% yellow / 95.0% red
- **Notes:** HTTP endpoint availability at the intent-api layer.

#### Panel 3: Rebase Apply Path Availability
- **Metric:** `intent_api_rebase_apply_requests_total`
- **Query:** `sum(rate(intent_api_rebase_apply_requests_total{status="success"}[5m])) / sum(rate(intent_api_rebase_apply_requests_total[5m])) * 100`
- **Type:** Stat / gauge
- **Thresholds:** 99.0% green / 98.5% yellow / 95.0% red
- **Notes:** Covers apply endpoint + runtime adapter chain. Not yet instrumented.

#### Panel 4: Audit Append Success
- **Metric:** `intent_api_audit_append_total`
- **Query:** `sum(rate(intent_api_audit_append_total{status="success"}[5m])) / sum(rate(intent_api_audit_append_total[5m])) * 100`
- **Type:** Stat / gauge
- **Thresholds:** 99.9% green / 99.5% yellow / 99.0% red
- **Notes:** Audit event persistence. Requires audit service instrumentation.

---

### Row 2 — Latency SLOs

#### Panel 5: Diff Compute Latency (p95)
- **Metric:** `intent_api_diff_compute_duration_seconds`
- **Query:** `histogram_quantile(0.95, sum(rate(intent_api_diff_compute_duration_seconds_bucket[5m])) by (le))`
- **Type:** Time series + stat
- **Thresholds:** < 2s green / 2–5s yellow / > 5s red
- **Notes:** Intent diff calculation for structured changes.

#### Panel 6: Rebase Preview Latency (p95, medium graph)
- **Metric:** `intent_api_rebase_preview_duration_seconds`
- **Query:** `histogram_quantile(0.95, sum(rate(intent_api_rebase_preview_duration_seconds_bucket[5m])) by (le, graph_size))`
- **Type:** Time series
- **Filter:** `graph_size=~"medium|large"`
- **Thresholds:** < 10s green / 10–30s yellow / > 30s red
- **Notes:** Requires `graph_size` label on histogram buckets.

#### Panel 7: Rebase Apply Latency (p95, low/medium risk)
- **Metric:** `intent_api_rebase_apply_duration_seconds`
- **Query:** `histogram_quantile(0.95, sum(rate(intent_api_rebase_apply_duration_seconds_bucket[5m])) by (le, risk_class))`
- **Type:** Time series
- **Filter:** `risk_class=~"low|medium"`
- **Thresholds:** < 60s green / 60–120s yellow / > 120s red
- **Notes:** Not yet instrumented. Risk classification label required.

#### Panel 8: Approval Wait Time (p95)
- **Metric:** `intent_api_approval_wait_duration_seconds`
- **Query:** `histogram_quantile(0.95, sum(rate(intent_api_approval_wait_duration_seconds_bucket[5m])) by (le))`
- **Type:** Time series
- **Thresholds:** < 30min green / 30–60min yellow / > 60min red
- **Notes:** Stale approval detection. Not yet instrumented.

---

### Row 3 — Compensation Engine Health

#### Panel 9: Compensation Action Status Breakdown
- **Metric:** `intent_api_compensation_action_total`
- **Query:** `sum by (status) (rate(intent_api_compensation_action_total[5m]))`
- **Type:** Pie chart / stacked bar
- **Statuses:** pending, approved, waived, executed, failed, dlq
- **Notes:** Requires compensation action service instrumentation.

#### Panel 10: Compensation Execution Success Rate
- **Metric:** `intent_api_compensation_execution_total`
- **Query:** `sum(rate(intent_api_compensation_execution_total{status="success"}[5m])) / sum(rate(intent_api_compensation_execution_total[5m])) * 100`
- **Type:** Stat / gauge
- **Thresholds:** 99.0% green / 95.0% yellow / 90.0% red
- **Notes:** Success = acknowledged, not necessarily reversed. Depends on Batch 1 execution.

#### Panel 11: DLQ Candidate Count
- **Metric:** `intent_api_compensation_dlq_candidate_count`
- **Query:** `sum(intent_api_compensation_dlq_candidate_count)`
- **Type:** Stat / alert threshold
- **Thresholds:** 0 green / 1–5 yellow / > 5 red
- **Notes:** Derived DLQ condition: Failed + exhausted budget OR non-retryable error. Not yet exposed as metric.

---

### Row 4 — Side Effect Ledger Health

#### Panel 12: Side Effect Capture Rate
- **Metric:** `intent_api_side_effect_captured_total`
- **Query:** `sum(rate(intent_api_side_effect_captured_total[5m]))`
- **Type:** Time series
- **Notes:** Measures how many side effects are being captured per interval.

#### Panel 13: Side Effect Capture Errors
- **Metric:** `intent_api_side_effect_capture_errors_total`
- **Query:** `sum(rate(intent_api_side_effect_capture_errors_total[5m]))`
- **Type:** Time series
- **Thresholds:** Alert if rate > 0
- **Notes:** Indicates artifact-ingest failures to record side effects. Depends on Batch 1 capture-on-write.

---

### Row 5 — Queue / Adapter Health (Reference — Not Yet Instrumented)

#### Panel 14: Queue Backlog Depth
- **Metric:** `nats_consumer_backlog_depth` (external)
- **Query:** TBD — depends on NATS/JetStream topology
- **Notes:** Not yet instrumented. Queue backlog alerts depend on production NATS topology.

#### Panel 15: Runtime Adapter Failure Rate
- **Metric:** `intent_api_runtime_adapter_errors_total`
- **Query:** `sum(rate(intent_api_runtime_adapter_errors_total[5m]))`
- **Type:** Time series
- **Notes:** Adapter failures consume error budget at 5x rate per on-call considerations.

#### Panel 16: Artifact Quarantine Failure Rate
- **Metric:** `intent_api_artifact_quarantine_failures_total`
- **Query:** TBD
- **Notes:** Not yet instrumented. Depends on real artifact storage boundary (Batch 2+ scope).

---

### Row 6 — Error Budget Tracking (Batch 2 Slice 5 + Slice 7)

These panels consume the metrics emitted by Batch 2 Slice 3 to display burn rate for the preview and apply paths across multiple time windows.

#### Panel 17: Preview Path Error Budget Burn (1h)
- **Metric:** `intent_api_rebase_preview_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_preview_requests_total{status!="success"}[1h])) / sum(rate(intent_api_rebase_preview_requests_total[1h]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.2% (0.002) green / 0.2–0.6% yellow / > 0.6% (0.006) red
- **Notes:** Tracks 1-hour burn rate against the preview path error budget (0.1% of 43,200 min = 43.2 min/month). Aligned with `PreviewPathBurnRate1h` alert in `intent_api_alerts.yml`.

#### Panel 18: Apply Path Error Budget Burn (1h)
- **Metric:** `intent_api_rebase_apply_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_apply_requests_total{status!="success"}[1h])) / sum(rate(intent_api_rebase_apply_requests_total[1h]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.4% (0.004) green / 0.4–1.2% yellow / > 1.2% (0.012) red
- **Notes:** Tracks 1-hour burn rate against the apply path error budget (0.5% of 43,200 min = 216 min/month). Aligned with `ApplyPathBurnRate1h` alert in `intent_api_alerts.yml`.

#### Panel 19: Preview Path Error Budget Burn (6h)
- **Metric:** `intent_api_rebase_preview_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_preview_requests_total{status!="success"}[6h])) / sum(rate(intent_api_rebase_preview_requests_total[6h]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.2% (0.002) green / 0.2–0.6% yellow / > 0.6% (0.006) red
- **Notes:** Tracks 6-hour burn rate for sustained error elevation detection. Aligned with `PreviewPathBurnRate6h` alert in `intent_api_alerts.yml`.

#### Panel 20: Apply Path Error Budget Burn (6h)
- **Metric:** `intent_api_rebase_apply_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_apply_requests_total{status!="success"}[6h])) / sum(rate(intent_api_rebase_apply_requests_total[6h]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.4% (0.004) green / 0.4–1.2% yellow / > 1.2% (0.012) red
- **Notes:** Tracks 6-hour burn rate for sustained error elevation detection. Aligned with `ApplyPathBurnRate6h` alert in `intent_api_alerts.yml`.

#### Panel 21: Preview Path Error Budget Burn (3d)
- **Metric:** `intent_api_rebase_preview_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_preview_requests_total{status!="success"}[3d])) / sum(rate(intent_api_rebase_preview_requests_total[3d]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.3% (0.003) green / 0.3–0.8% yellow / > 0.8% (0.008) red
- **Notes:** Tracks 3-day burn rate for chronic degradation detection. Aligned with `PreviewPathBurnRate3d` alert in `intent_api_alerts.yml`.

#### Panel 22: Apply Path Error Budget Burn (3d)
- **Metric:** `intent_api_rebase_apply_requests_total` (counter with `status` label)
- **Query:** `sum(rate(intent_api_rebase_apply_requests_total{status!="success"}[3d])) / sum(rate(intent_api_rebase_apply_requests_total[3d]))`
- **Type:** Stat / gauge
- **Thresholds:** < 0.6% (0.006) green / 0.6–1.6% yellow / > 1.6% (0.016) red
- **Notes:** Tracks 3-day burn rate for chronic degradation detection. Aligned with `ApplyPathBurnRate3d` alert in `intent_api_alerts.yml`.

**Slice 7 scope (bounded/truthful):**
- Multi-window burn-rate panels (1h/6h/3d) for preview and apply paths ✅
- Multi-window burn-rate alerting rules (1h/6h/3d) for preview and apply paths ✅
- **NOT in scope:** Budget depletion forecasting, 30-day budget tracking panel, SLO composite panels, production Alertmanager

---

## Metrics That Require Instrumentation

The following metric names are referenced in panels. Status reflects whether emission is active (per Batch 2 Slice 3):

| Metric | Source | Status |
|--------|--------|--------|
| `intent_api_intent_version_created_total` | intent-api | ✅ Active (Slice 3) |
| `intent_api_rebase_preview_requests_total` | intent-api | ✅ Active (Slice 3) |
| `intent_api_rebase_apply_requests_total` | intent-api | ✅ Active (Slice 3) |
| `intent_api_diff_compute_duration_seconds` | rebase engine | ✅ Active (Slice 3) |
| `intent_api_rebase_preview_duration_seconds` | rebase engine | ✅ Active (Slice 3) |
| `intent_api_rebase_apply_duration_seconds` | rebase engine | ✅ Active (Slice 3) |
| `intent_api_audit_append_total` | audit service | Not instrumented |
| `intent_api_approval_wait_duration_seconds` | intent-api | Not instrumented |
| `intent_api_compensation_action_total` | compensation service | Not instrumented |
| `intent_api_compensation_execution_total` | compensation service | Not instrumented |
| `intent_api_side_effect_captured_total` | graph service | Not instrumented |
| `intent_api_side_effect_capture_errors_total` | graph service | Not instrumented |

Metrics marked ✅ Active are recorded via metrics-exporter-prometheus 0.18.1 + metrics 0.24. Panels for those metrics will render real data once the intent-api service is running with the instrumented paths.

---

## Grafana Provisioning

Dashboard provisioning via `grafana-dashboards.yaml` or JSON provisioning is **not in scope for Slice 1**. This scaffold document is for manual dashboard construction. Automated provisioning requires:

1. Grafana instance with Prometheus data source configured
2. JSON dashboard export or `grafana-dashboards.yaml` provisioning
3. Dashboard UID stable across imports

---

## Related Documents

- [04-sre-and-slos.md](./04-sre-and-slos.md) — SLO definitions and provisional targets
- [03-observability.md](./03-observability.md) — Golden signals, domain metrics, and log structure
- [05-runbooks.md](./05-runbooks.md) — Runbook placeholder (future)
- [Phase 3 Hardening Plan](../../10-delivery/05-phase-3-hardening.md) — Batch 2 scope and dependencies