# Observability

## Golden signals
- request latency
- queue lag
- diff compute latency
- rebase apply success rate
- approval stale detection latency
- compensation success rate
- propagation signal creation success rate (Slice 2 bounded — see RB12)

## Domain metrics
- intent changes per workflow
- rebase preview to apply rate
- percentage work salvaged
- full restart rate
- invalidation distribution
- operator override rate
- false positive invalidations
- incident count tied to stale intent
- propagation signal creation rate (attempted / succeeded / failed / no-downstream)
- downstream system acknowledgment rate (pending → acknowledged / failed) — **not instrumented; deferred to Phase 4+**

## Logs
Structured logs với:
- trace_id
- tenant_id
- workflow_id
- intent_id
- intent_version
- rebase_plan_id
- actor_ref

## Tracing
- ingestion -> diff -> graph -> rebase -> adapter -> runtime
- propagate correlation ids end-to-end

## Dashboards
- control plane health
- tenant risk dashboard
- approval/side effect health
- adapter health

## Local alerting (manual inspection only)

> **Not production-ready.** Real receivers (Slack, PagerDuty, email) remain blocked/deferred.

Local Alertmanager is configured in `infrastructure/local/alertmanager/alertmanager.yml` with placeholder webhook routes to `http://localhost:9001/webhook`. To inspect alert payloads manually during local development, run the lightweight helper:

```bash
python3 infrastructure/local/alertmanager/webhook_receiver.py
```

This starts a local HTTP server on port 9001 that prints received alert JSON to stdout. It does not persist alerts or route to external systems. Press `Ctrl+C` to stop.
