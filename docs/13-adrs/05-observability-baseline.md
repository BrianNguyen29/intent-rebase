# ADR-05 — Observability Baseline

**Status:** Proposed  
**Date:** 2026-04-03  
**Authors:** Intent Rebase Engine Team  
**Phase:** Phase 0–P1  

---

## Context

IRE requires comprehensive observability for:
- **Debugging** — understanding intent diff computation, rebase decisions, graph propagation
- **Operational health** — monitoring system throughput, latency, error rates
- **Audit compliance** — tracing which user/action triggered which event
- **SLO tracking** — measuring rebase latency, intent processing time, approval wait time

Observability scope includes: logs, metrics, traces, and audit events.

---

## Decision

**Adopt OpenTelemetry (OTel) as the unified observability standard, with structured JSON logs to stdout.**

### Observability Stack

| Pillar | Tool/Standard | Rationale |
|--------|--------------|-----------|
| **Traces** | OpenTelemetry + Jaeger/Zipkin | Distributed trace context for rebase flows |
| **Metrics** | OTLP → Prometheus → Grafana | SLO/SLA dashboards, alerting |
| **Logs** | Structured JSON → stdout → Loki | Centralized log aggregation |
| **Audit** | Structured events → NATS → PostgreSQL | Immutable audit trail (separate from application logs) |

### Instrumentation Requirements

All IRE services must emit:

```
Traces:
  - trace_id, span_id propagated across service boundaries
  - Span attributes: intent_id, intent_version, tenant_id, operation_type
  -关键 spans: intent.create, diff.compute, rebase.plan, rebase.apply, approval.check

Metrics (Prometheus):
  - intent_versions_total (counter, labels: tenant_id, operation)
  - rebase_detected_total (counter, labels: intent_id, severity)
  - rebase_apply_duration_seconds (histogram)
  - graph_propagation_duration_seconds (histogram)
  - approval_wait_time_seconds (histogram, labels: intent_id, approval_type)
  - active_intents (gauge, labels: tenant_id)
  - artifact_invalidated_total (counter)

Logs:
  - JSON structured, keys: timestamp, level, trace_id, span_id, tenant_id, intent_id, message
  - No PII in logs; use tenant_id references, not user email/name
```

### Alerting

| Alert | Condition | Severity |
|-------|-----------|---------|
| Rebase latency high | p95 rebase_apply_duration > 60s | warning |
| Approval backlog | approval_wait_time > 30min for >50% of pending | warning |
| Audit lag | audit event age > 5min before processing | critical |
| NATS consumer lag | consumer lag > 10000 messages | warning |

---

## Consequences

### Positive
- OpenTelemetry standard avoids vendor lock-in
- Unified trace context across all services simplifies debugging
- Structured logs enable efficient log queries without parsing
- Prometheus + Grafana ecosystem is mature and widely understood

### Negative
- Requires OTel SDK integration in all services
- Audit events are high-volume; storage sizing must account for this
- Multiple backends (Loki, Prometheus, Jaeger) add operational complexity

### Neutral
- Phase 1 baseline: logs + basic metrics only; full tracing in Phase 2
- OpenTelemetry collector sidecar for export transformation

---

## Implementation Notes

### Phase 0
- Define OTel trace/span naming conventions
- Define metric naming convention and label schema
- Define log schema (JSON keys, prohibited fields)
- Set up local OTel collector for development

### Phase 1
- Instrument intent CRUD operations
- Instrument rebase detection
- Instrument approval workflow

### Phase 2
- Full distributed tracing across runtime adapter boundary
- Add SLO dashboards

---

## Related ADRs

- [ADR-04](./04-event-broker.md) — NATS for event transport
- [ADR-02](./02-data-plane.md) — Audit storage

---

## References

- OpenTelemetry: https://opentelemetry.io/
- Prometheus metric naming: https://prometheus.io/docs/practices/naming/
- Structured logging: `../09-operations/03-observability.md`