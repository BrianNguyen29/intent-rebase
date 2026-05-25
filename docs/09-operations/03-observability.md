# Observability

## Golden signals
- request latency
- queue lag
- diff compute latency
- rebase apply success rate
- approval stale detection latency
- compensation success rate
- propagation signal creation success rate (Slice 2 bounded — see RB12)
- webhook delivery success rate (Slice 3 bounded — see RB13; local dev only, default disabled)

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
- webhook delivery rate (attempted / succeeded / failed / retry-exhausted) — Slice 3 bounded; metrics instrumented in `send_webhook_with_retries`; local dev only, default disabled via `INTENT_API_WEBHOOK_DELIVERY`
- downstream system acknowledgment rate (pending → acknowledged / failed) — **not instrumented; deferred to Phase 4+**
- webhook outbox DLQ depth / age / replay rate — **not instrumented; design-only (P2-6e). No queue or worker exists.**

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

Local Alertmanager is configured in `infrastructure/local/alertmanager/alertmanager.yml` with placeholder webhook routes.

> **Panic alerting:** The panic hook (`crates/intent-api/src/panic_hardening.rs`) logs sanitized panic payloads but does **not** emit Prometheus metrics. The intended panic alerting path is documented in [`12-panic-alerting-integration.md`](12-panic-alerting-integration.md) (S7 design-only; production blocked).

**Standalone (host):**
```bash
python3 infrastructure/local/alertmanager/webhook_receiver.py
```
The standalone script listens on `http://localhost:9094/webhook` and prints received alert JSON to stdout.

**Docker Compose (observability profile):**
```bash
docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d
```
When running via docker-compose, Alertmanager routes internally to the `alert-receiver` service at `http://alert-receiver:9094/webhook`, and host port 9094 is exposed by the `alert-receiver` container.

In both cases the helper is local/manual-only, does not persist alerts, and does not route to external systems. Press `Ctrl+C` to stop the standalone script.

### Smoke test (Alertmanager → alert-receiver)

> **Local/manual-only.** This validates the alert delivery path on a developer workstation; it is not a production readiness check.

1. **Validate compose config** before starting services:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml --profile observability config
   ```

2. **Start alert-receiver and alertmanager** (preserves existing Postgres/NATS/MinIO):
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d alert-receiver alertmanager
   ```

3. **Post a test alert** using the helper:
   ```bash
   python3 infrastructure/local/alertmanager/smoke_test_alert_receiver.py
   ```

   Or manually with `curl`:
   ```bash
   curl -X POST http://localhost:9093/api/v1/alerts \
     -H "Content-Type: application/json" \
     -d '[{"labels":{"alertname":"TestAlert","severity":"warning","slo":"propagation","instance":"smoke-test","source":"local-smoke-helper"},"annotations":{"summary":"Smoke test alert"}}]'
   ```

4. **Inspect alert-receiver logs** for the `TestAlert` payload:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml --profile observability logs alert-receiver
   ```
   You should see the alert JSON printed by `webhook_receiver.py`.

5. **Clean up only alert-receiver and Alertmanager** while preserving any pre-existing Grafana/Prometheus/core services:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml --profile observability stop alert-receiver alertmanager
   docker compose -f infrastructure/local/docker-compose.yml --profile observability rm -f alert-receiver alertmanager
   ```
   To stop the whole observability profile, including Grafana/Prometheus if running:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml --profile observability down
   ```
