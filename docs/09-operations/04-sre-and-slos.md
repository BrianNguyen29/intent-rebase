# SRE and SLOs

> **Status (Batch 2 Slice 1 + Slice 2 + Slice 3):** This document describes provisional SLO targets and a Grafana dashboard scaffold (Slice 1), bounded tracing foundation (Slice 2), and bounded alerting rules + runbook foundation (Slice 3).
> These targets are **not yet SRE-approved** and **no production telemetry is connected**.
> Batch 2 Slice 1 delivers the **SLO foundation only** — SLO definitions documented, dashboard scaffold written.
> Batch 2 Slice 2 delivers **tracing foundation only** — request-id extraction middleware and service method instrumentation; no full OTEL export or distributed trace across all boundaries.
> Batch 2 Slice 3 delivers **alerting rules + runbook foundation** — Alertmanager config, Prometheus alerting rules, Grafana provisioning, and runbook scenarios for common failure modes. Metric emission is active (metrics-exporter-prometheus 0.18.1 with metrics 0.24); metric definitions are scaffolded and actively recorded. Full metrics coverage across all flows is NOT claimed.

---

## Documented SLO Targets (Provisional — Awaiting SRE Confirmation)

These are the candidate targets from Batch 0 planning. They are concrete enough to drive dashboard panel definitions but require SRE sign-off before being treated as exit evidence.

### Availability SLOs

| SLO | Target | Notes |
|-----|--------|-------|
| Intent version creation success rate | 99.9% | All artifact-producing operations |
| Rebase preview availability | 99.5% | HTTP endpoint availability |
| Rebase apply path availability | 99.0% | Apply endpoint + runtime adapter chain |
| Audit append success | 99.9% | Audit event persistence |
| Compensation plan generation success | 99.0% | Once Batch 1 basic flow exists |
| Forensic bundle generation success | 99.0% | Once Batch 3 implementation data exists |

### Latency SLOs

| SLO | Target | Notes |
|-----|--------|-------|
| p95 diff compute (structured changes) | < 2s | Intent diff calculation |
| p95 rebase preview (medium graph) | < 10s | Graph size: 100–1000 nodes |
| p95 rebase apply (low/medium risk) | < 60s | Risk-classified apply path |
| p95 approval wait alert threshold | 30 min | Stale approval detection |
| p95 compensation execution | Define after Batch 1 | No baseline yet |
| p95 forensic bundle generation | Define after Batch 3 | No implementation data |

### Error Budgets

| Budget | Threshold | Policy |
|--------|-----------|--------|
| Preview path monthly budget | 0.1% of 43,200 min | 43.2 min/month budget |
| Apply path monthly budget | 0.5% of 43,200 min | 216 min/month budget |
| Critical path incidents | Consume budget at 5× rate | Approval stale, audit failure |

---

## Batch 2 Slice 1 + Slice 2 + Slice 3 — What Is and Is Not Implemented

### ✅ Delivered (Slice 1)

- **SLO definitions documented** — all candidate targets above are written with enough specificity to drive Grafana panel queries
- **Grafana dashboard scaffold** — see `06-slo-dashboard.md` for panel layout, metric names, and query structure

### ✅ Delivered (Slice 2 — bounded tracing foundation)

- **Request-ID extraction middleware** — extracts `X-Request-ID` header or generates UUID; stores in request extensions for downstream correlation
- **Service method instrumentation** — `#[tracing::instrument]` on key intent-service, rebase-engine, and compensation-service methods

### ✅ Delivered (Slice 3 — alerting rules + runbook foundation)

- **Alerting rules** — Prometheus alerting rules in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` targeting availability and latency SLOs
- **Alertmanager config** — `infrastructure/local/alertmanager/alertmanager.yml` with placeholder receivers (local dev only)
- **Grafana provisioning** — `infrastructure/local/grafana/provisioning/` with datasource and dashboard provisioning
- **Metrics instrumentation** — metric definitions scaffolded in intent-api:
  - `intent_api_intent_version_created_total` (counter with status label)
  - `intent_api_rebase_preview_requests_total` (counter with status label)
  - `intent_api_rebase_apply_requests_total` (counter with status label)
  - `intent_api_diff_compute_duration_seconds` (histogram)
  - `intent_api_rebase_preview_duration_seconds` (histogram with graph_size label)
  - `intent_api_rebase_apply_duration_seconds` (histogram with risk_class label)
  - ✅ **Metric emission is now enabled** — metrics-exporter-prometheus upgraded to 0.18.1 (from 0.12.2) which is compatible with workspace metrics 0.24; metrics are actively recorded for core intent operations.
- **Runbook scenarios** — see `05-runbooks.md` for RB6-RB10 covering: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn

### ⬜ Not Yet Implemented

- **Full metrics coverage** — Slice 3 instruments core intent operations only; other flows (compensation, audit, side effects) remain uninstrumented
- **Error budget tracking dashboard** — no burn-rate query or budget tracking panels
- **Distributed tracing** — no full OTEL instrumentation, no trace context propagation across all service boundaries (Slice 2 delivers foundation only)
- **Performance benchmarks** — no rebase latency p50/p95/p99 measurements
- **Production alerting** — Slice 3 delivers local dev infrastructure only; production Alertmanager configuration is future scope

---

## On-Call Considerations (Reference)

These are documented for future runbook development — alerting infrastructure exists locally via Slice 3 but production deployment requires SRE confirmation.

- Adapter failures — see RB3 (Runtime adapter failing apply)
- Queue backlogs — see RB2 (Queue lag high)
- Stuck compensations — see RB9 (Compensation timeout)
- Approval stale not triggering — see RB7 (Approval backlog)
- Audit append failures — see RB4 (Audit sink unavailable)

---

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
