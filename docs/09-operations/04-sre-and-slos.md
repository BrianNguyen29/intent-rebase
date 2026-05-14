# SRE and SLOs

# SRE and SLOs

> **Status (Batch 2 Slice 1 + Slice 2 + Slice 3 + Slice 5 + Slice 7):** This document describes provisional SLO targets and a Grafana dashboard scaffold (Slice 1), bounded tracing foundation (Slice 2), bounded alerting rules + runbook foundation (Slice 3), error budget tracking panels (Slice 5), and multi-window burn-rate alerting rules (Slice 7).
> These targets are **not yet SRE-approved** and **no production telemetry is connected**.
> Batch 2 Slice 1 delivers the **SLO foundation only** — SLO definitions documented, dashboard scaffold written.
> Batch 2 Slice 2 delivers **bounded OTEL propagation** — optional OTLP export (when OTEL_EXPORTER_OTLP_ENDPOINT is set), W3C trace-context extraction from inbound requests, traceparent/tracestate response headers, and span propagation into spawned background work. Cross-process trace propagation beyond this service remains future scope.
> Batch 2 Slice 3 delivers **alerting rules + runbook foundation** — Alertmanager config, Prometheus alerting rules, Grafana provisioning, and runbook scenarios for common failure modes. Metric emission is active (metrics-exporter-prometheus 0.18.1 with metrics 0.24); metric definitions are scaffolded and actively recorded. Full metrics coverage across all flows is NOT claimed.
> Batch 2 Slice 5 delivers **error budget tracking panels** — preview and apply path 1-hour burn-rate stat panels backed by intent_api_rebase_preview_requests_total and intent_api_rebase_apply_requests_total.
> Batch 2 Slice 7 delivers **multi-window burn-rate alerting** — Prometheus alerting rules covering 1h/6h/3d windows for preview and apply paths. Grafana dashboard updated with 6h and 3d burn-rate panels. Budget depletion forecasting and 30-day budget tracking remain future scope.

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

### ✅ Delivered (Slice 2 — bounded OTEL propagation)

- **Request-ID extraction middleware** — extracts `X-Request-ID` header or generates UUID; stores in request extensions for downstream correlation
- **Service method instrumentation** — `#[tracing::instrument]` on key intent-service, rebase-engine, and compensation-service methods
- **Optional OTLP export** — OTLP tracing enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` env var is set; JSON logging fallback when not set
- **W3C trace-context propagation** — extracts `traceparent` and `tracestate` headers from inbound requests; adds traceparent/tracestate to responses
- **Background task span propagation** — spawned background work inherits current span context via `tracing::Instrument`

### ✅ Delivered (Phase 3 Slice — bounded trace continuity for audit/events)

- **Trace context in audit events** — `AuditEvent` now carries `trace_id` and `span_id` fields populated when an active trace exists
- **Trace context in published event envelopes** — `EventEnvelope` and `PublishedEvent` now carry `trace_id` and `span_id`
- **Database columns exist** — `audit_events` table already has `trace_id` and `span_id` columns
- **Bounded scope** — trace context capture is implemented for in-process audit/event boundaries; cross-process propagation via Temporal gRPC metadata/traceparent injection, sqlx connection context, or NATS headers remains future scope

### ✅ Delivered (Phase 3 Batch 2 Slice 8 — bounded Temporal adapter tracing)

- **Local span correlation around Temporal adapter methods** — `#[tracing::instrument]` on `connect`, `get_checkpoints`, `send_rebase_signal`, `map_intent_to_checkpoint`, `replay_from_checkpoint`, `is_adapter_ready`
- **Span fields** — relevant context captured per method (intent_id, workflow_id, checkpoint_id, namespace, target_url, etc.)
- **Bounded scope** — this delivers local tracing span correlation only; gRPC metadata/traceparent injection into Temporal wire protocol is NOT implemented; cross-process trace propagation via Temporal gRPC remains future scope

### ✅ Delivered (Slice 3 — alerting rules + runbook foundation)

- **Alerting rules** — Prometheus alerting rules in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` targeting availability, latency, and propagation signal SLOs
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
- **Runbook scenarios** — see `05-runbooks.md` for RB6-RB13 covering: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout, error-budget-burn, propagation-signal-failures (RB12), webhook-delivery-failures (RB13)

### ⬜ Not Yet Implemented

- **Full metrics coverage** — Slice 3 instruments core intent operations only; other flows (compensation, audit, side effects) remain uninstrumented
- **Error budget tracking dashboard** — Slice 5 delivers preview + apply 1h burn-rate stat panels; Slice 7 delivers 6h and 3d burn-rate panels and multi-window alerting rules; budget depletion forecasting and 30-day budget tracking remain future scope
- **Cross-process trace propagation** — Slice 2 delivers bounded in-process OTEL propagation (optional OTLP export, W3C trace-context headers, background task span propagation); Slice 8 delivers bounded Temporal adapter local span correlation; full distributed trace across service boundaries via Temporal gRPC metadata/traceparent injection, sqlx connection context, or NATS headers remains future scope
- **Performance benchmarks** — local baseline captured (rebase-engine diff+plan: p50 3.78–6.09 µs); CI-averaged p50/p95/p99 targets and production load testing remain gated on P2 completion
- **Production alerting** — Slice 3 + Slice 7 deliver local dev infrastructure only; production Alertmanager configuration is future scope; SRE approval for production deployment is still open

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

---

## SLO Definitions (Phase 3 Batch 2 - P2-S2)

**P2-S2 delivered:** SLO definitions, alerting rules, error-budget dashboard, metrics infrastructure, observability stack.

> **Note:** P2-S2 is a bounded slice covering items 2-1, 2-2, 2-3. Items 2-4, 2-5, 2-6 remain open.

**P2-S2 delivered:** Real metrics instrumentation on existing `/metrics` endpoint via `metrics-exporter-prometheus`. Full alerting/dashboard/OTel propagation/runbooks are P2-S2+ scope.

**P2-S3 delivered (bounded distributed tracing slice):** Trace context propagated into audit/event surfaces via existing `trace_id`/`span_id` fields on `AuditEvent`. The `trace_context::current_trace_context()` helper extracts span IDs from the current tracing span for propagation into audit recording calls. Six event types now carry trace context: `RebaseApplied`, `RebaseApplyBlocked`, `ApprovalGranted`, `ApprovalRevoked`, `ApprovalExpired`, `ReplayInitiated`. Full OTLP/cross-service trace propagation remains P2-S4+ scope.

**Instrumented metrics (real code paths, verified by cargo check/test):**
- `intent_api_intent_version_created_total{status="success|error"}` — intent version creation handler
- `intent_api_rebase_preview_requests_total{status="success|error"}` — rebase_preview handler
- `intent_api_rebase_apply_requests_total{status="success|error"}` — rebase_apply handler
- `intent_api_diff_compute_duration_seconds_bucket` — diff compute latency histogram
- `intent_api_rebase_preview_duration_seconds_bucket` — rebase preview latency histogram
- `intent_api_rebase_apply_duration_seconds_bucket` — rebase apply latency histogram
- `intent_api_propagation_signals_attempted_total` — propagation signal creation attempts (Slice 2 bounded)
- `intent_api_propagation_signals_succeeded_total` — successful propagation record updates
- `intent_api_propagation_signals_failed_total` — failed propagation record updates or list errors
- `intent_api_propagation_signals_no_downstream_total` — apply trigger ran but no downstream records found
- `intent_api_webhook_deliveries_attempted_total` — webhook delivery attempts (Slice 3 bounded; env-gated, default disabled)
- `intent_api_webhook_deliveries_succeeded_total` — successful webhook deliveries
- `intent_api_webhook_deliveries_failed_total` — failed webhook deliveries (non-retryable or exhausted)
- `intent_api_webhook_deliveries_retry_exhausted_total` — deliveries where all retries were exhausted

> **Note:** Earlier `intent_rebase_*` metric names and `intent_api_version_created_total` / `intent_api_rebase_preview_total` / `intent_api_rebase_apply_total` were documented but are not the actual emitted names. Documentation has been updated to match the metrics currently instrumented in intent-api.
> **Propagation metrics:** Instrumented in `rebase_apply_handlers.rs` post-commit, Proceed-only, best-effort. See RB12 for runbook guidance.
> **Webhook delivery metrics:** Instrumented in `webhook_delivery.rs` within `send_webhook_with_retries`, incremented per send attempt. See RB13 for runbook guidance. Delivery is env-gated (`INTENT_API_WEBHOOK_DELIVERY`, default disabled) and best-effort — does not affect apply outcomes.

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

**Key metrics (currently instrumented):**
- `intent_api_intent_version_created_total{status="success|error"}`
- `intent_api_rebase_preview_requests_total{status="success|error"}`
- `intent_api_rebase_apply_requests_total{status="success|error"}`
- `intent_api_diff_compute_duration_seconds_bucket`
- `intent_api_rebase_preview_duration_seconds_bucket`
- `intent_api_rebase_apply_duration_seconds_bucket`
- `intent_api_propagation_signals_attempted_total` — propagation signal creation attempts (Slice 2 bounded)
- `intent_api_propagation_signals_succeeded_total` — successful propagation record updates
- `intent_api_propagation_signals_failed_total` — failed propagation record updates or list errors
- `intent_api_propagation_signals_no_downstream_total` — apply trigger ran but no downstream records found

> **Not instrumented:** `intent_api_audit_append_total`, `intent_api_approval_wait_duration_seconds`, `intent_api_error_budget_remaining`, `compensation_action_executed_total`. Dashboards and alerts referencing these metrics are stale and have been removed.

### Alerting Rules

Prometheus alerting rules defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml`:

**Availability alerts:**
- `IntentVersionCreationLowSuccessRate` (< 99.0%)
- `RebasePreviewLowAvailability` (< 99.0%)
- `RebaseApplyLowAvailability` (< 98.5%)

**Latency alerts:**
- `DiffComputeHighLatency` (> 2s)
- `RebasePreviewHighLatency` (> 10s)
- `RebaseApplyHighLatency` (> 60s)

**Error budget burn-rate alerts:**
- `PreviewPathBurnRate1h` / `6h` / `3d`
- `ApplyPathBurnRate1h` / `6h` / `3d`

**Propagation signal alerts (Slice 2 bounded — local dev only):**
- `PropagationSignalFailureRate` (> 10% failed/attempted ratio with meaningful traffic) — see RB12

**Webhook delivery alerts (Slice 3 bounded — local dev only):**
- `WebhookDeliveryFailureRate` (> 10% failed/attempted ratio with meaningful traffic) — see RB13

> **Not instrumented:** Approval wait, audit append, compensation execution, DLQ, and error-budget-remaining alerts are not yet backed by real metrics and have been removed from local rules.
> **Webhook outbox DLQ metrics:** `intent_api_outbox_dlq_*` metrics are design-only (P2-6e). No queue, table, or worker is implemented.
> **Propagation alerts:** `PropagationSignalFailureRate` is instrumented and defined in local rules but is **local dev scaffolding only**. Production deployment requires SRE sign-off and receiver configuration.
> **Webhook delivery alerts:** `WebhookDeliveryFailureRate` is instrumented and defined in local rules but is **local dev scaffolding only**. No production delivery guarantees, outbox, HMAC, or subscription CRUD are in scope.
