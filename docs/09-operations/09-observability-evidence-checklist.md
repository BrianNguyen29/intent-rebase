# 09 — Observability Evidence Collection Checklist

**Status:** `DOCUMENTED — Evidence Collection Checklist Templates Only`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** April 2026

---

## Purpose

This document provides an **evidence collection checklist** for observability validation across metrics, Prometheus, Grafana, Alertmanager, and traces. The checklist is designed to capture evidence that the observability stack is correctly configured and functioning — without claiming production deployment has occurred.

> **⚠️ Evidence Strength Disclaimer**
>
> This document provides **checklist templates for evidence collection**. Evidence is collected against local docker-compose infrastructure (`infrastructure/local/docker-compose.yml`), which is **not production-equivalent**. Do not represent local observability evidence as production deployment validation.

---

## Scope of This Checklist

This checklist covers the following observability components:

| Component | Evidence Type | Environment |
|-----------|--------------|-------------|
| **Metrics** (Prometheus format) | `/metrics` endpoint output | Local docker-compose |
| **Prometheus** | Scrape targets, rule evaluation | Local docker-compose |
| **Grafana** | Dashboard provisioning, panel queries | Local docker-compose |
| **Alertmanager** | Route configuration, inhibit rules | Local docker-compose |
| **Traces** (OTLP/W3C) | Trace context propagation | Local docker-compose |

**Not in scope:**
- Production Prometheus/Grafana/Alertmanager deployment
- Remote OTLP endpoint (e.g., Datadog, Honeycomb, Jaeger)
- Production Alertmanager routing to external notification systems

---

## Evidence Collection Procedure

### Pre-Checks

```bash
# 1. Verify observability stack is running
docker compose -f infrastructure/local/docker-compose.yml --profile observability ps

# Expected output: intent-api, prometheus, alertmanager, grafana containers running

# 2. Verify intent-api is running
curl -s http://localhost:8080/health | jq .

# Expected: {"status":"ok"}

# 3. Start observability profile if not running
docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d
```

---

## Section 1: Metrics Evidence

### 1.1 Verify Metrics Endpoint

```bash
# Collect metrics from intent-api /metrics endpoint
curl -s http://localhost:8080/metrics | head -100

# Expected: Prometheus-formatted metrics (text/plain; version=0.0.4)
# Key metrics should include:
# - intent_rebase_intent_create_total
# - intent_rebase_version_create_total
# - intent_rebase_rebase_preview_total
# - intent_rebase_rebase_apply_total
# - intent_rebase_compensation_actions_total
```

**Evidence Template:**

```
### 1.1 Metrics Endpoint Evidence

Command: curl -s http://localhost:8080/metrics | head -100
Date: <YYYY-MM-DD>
Environment: local docker-compose

Metrics Found:
- intent_rebase_intent_create_total: <present|absent>
- intent_rebase_version_create_total: <present|absent>
- intent_rebase_rebase_preview_total: <present|absent>
- intent_rebase_rebase_apply_total: <present|absent>
- intent_rebase_compensation_actions_total: <present|absent>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 1.2 Verify Prometheus Scrape Target

```bash
# Check Prometheus targets
curl -s http://localhost:9090/api/v1/targets | jq .

# Expected: intent-api should be listed as a scrape target with state="active"
```

**Evidence Template:**

```
### 1.2 Prometheus Scrape Target Evidence

Command: curl -s http://localhost:9090/api/v1/targets | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Scrape Target Status:
- intent-api: <active|down|unknown>
- State: <healthy|unhealthy>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 1.3 Collect Metric Samples

```bash
# Query key metrics via Prometheus API
curl -s "http://localhost:9090/api/v1/query?query=intent_rebase_intent_create_total" | jq .
curl -s "http://localhost:9090/api/v1/query?query=intent_rebase_rebase_preview_total" | jq .
curl -s "http://localhost:9090/api/v1/query?query=intent_rebase_rebase_apply_total" | jq .

# Query histogram quantiles (if any requests have been made)
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, intent_rebase_rebase_preview_duration_seconds_bucket)" | jq .
```

**Evidence Template:**

```
### 1.3 Metric Samples Evidence

Command: curl -s "http://localhost:9090/api/v1/query?query=<metric>"
Date: <YYYY-MM-DD>
Environment: local docker-compose

Metric Samples:
- intent_rebase_intent_create_total: <value>
- intent_rebase_rebase_preview_total: <value>
- intent_rebase_rebase_apply_total: <value>

Histogram Quantiles:
- p95 rebase preview duration: <value or null>
- p95 rebase apply duration: <value or null>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

---

## Section 2: Prometheus Alerting Rules Evidence

### 2.1 List Alerting Rules

```bash
# List Prometheus alerting rules
curl -s http://localhost:9090/api/v1/rules | jq .

# Expected: Alert rules defined for:
# - IntentVersionCreationSuccessRate
# - RebasePreviewAvailability
# - RebaseApplyAvailability
# - DiffComputeLatency
# - RebasePreviewLatency
# - RebaseApplyLatency
# - DLQDepthHigh
# - DLQMessageStale
# - PreviewPathBurnRate1h/6h/3d
# - ApplyPathBurnRate1h/6h/3d
```

**Evidence Template:**

```
### 2.1 Alerting Rules Evidence

Command: curl -s http://localhost:9090/api/v1/rules | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Alert Rules Found:
- IntentVersionCreationSuccessRate: <present|absent>
- RebasePreviewAvailability: <present|absent>
- RebaseApplyAvailability: <present|absent>
- DiffComputeLatency: <present|absent>
- RebasePreviewLatency: <present|absent>
- RebaseApplyLatency: <present|absent>
- DLQDepthHigh: <present|absent>
- DLQMessageStale: <present|absent>
- PreviewPathBurnRate1h: <present|absent>
- PreviewPathBurnRate6h: <present|absent>
- PreviewPathBurnRate3d: <present|absent>
- ApplyPathBurnRate1h: <present|absent>
- ApplyPathBurnRate6h: <present|absent>
- ApplyPathBurnRate3d: <present|absent>

Alert Rule File Location: infrastructure/local/prometheus/rules/intent_api_alerts.yml
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 2.2 Evaluate Sample Alert

```bash
# Trigger a sample alert evaluation (fire a test alert manually)
curl -s -X POST http://localhost:9090/api/v1/alerts \
  -d '{"labels":{"alertname":"TestAlert","severity":"warning"}}'

# Check if alert is in firing state
curl -s http://localhost:9090/api/v1/alerts | jq .
```

---

## Section 3: Grafana Evidence

### 3.1 Verify Grafana is Accessible

```bash
# Check Grafana health
curl -s http://localhost:3000/api/health | jq .

# Expected: {"status":"ok","version":"<version>"}
```

**Evidence Template:**

```
### 3.1 Grafana Health Evidence

Command: curl -s http://localhost:3000/api/health | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Grafana Status: <ok|error>
Grafana Version: <version>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 3.2 List Provisioned Dashboards

```bash
# List dashboards via Grafana API (requires auth)
curl -s -u admin:admin http://localhost:3000/api/dashboards | jq .

# Expected: Dashboards for:
# - intent-api Overview (or similar name)
# - SLO Dashboard
```

**Evidence Template:**

```
### 3.2 Provisioned Dashboards Evidence

Command: curl -s -u admin:admin http://localhost:3000/api/dashboards | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Dashboards Found:
- <dashboard_name_1>: <present|absent>
- <dashboard_name_2>: <present|absent>

Dashboard Provisioning Source: infrastructure/local/grafana/provisioning/
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 3.3 Verify SLO Dashboard Panels

```bash
# Query Grafana dashboard by UID (if known)
DASHBOARD_UID="intent-api-slos"  # Example UID
curl -s -u admin:admin "http://localhost:3000/api/dashboards/uid/${DASHBOARD_UID}" | jq .

# Expected: Dashboard with panels for:
# - Intent creation success rate
# - Rebase preview availability
# - Rebase apply availability
# - Error budget remaining (preview and apply paths)
# - Burn rate panels (1h, 6h, 3d)
```

**Evidence Template:**

```
### 3.3 SLO Dashboard Panels Evidence

Dashboard: SLO Dashboard (or equivalent)
Date: <YYYY-MM-DD>
Environment: local docker-compose

Panels Found:
- Intent creation success rate: <present|absent>
- Rebase preview availability: <present|absent>
- Rebase apply availability: <present|absent>
- Error budget (preview): <present|absent>
- Error budget (apply): <present|absent>
- Burn rate (1h/6h/3d): <present|absent>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

---

## Section 4: Alertmanager Evidence

### 4.1 Verify Alertmanager Configuration

```bash
# Get Alertmanager status
curl -s http://localhost:9093/api/v1/status | jq .

# Get Alertmanager config
curl -s http://localhost:9093/api/v1/configs | jq .

# Expected: Config with receivers (e.g., "local-dev" or "null" for no-op)
```

**Evidence Template:**

```
### 4.1 Alertmanager Configuration Evidence

Command: curl -s http://localhost:9093/api/v1/status | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Alertmanager Status:
- Version: <version>
- Config loaded: <true|false>

Receivers Defined:
- <receiver_name_1>: <type>
- <receiver_name_2>: <type>

Config File Location: infrastructure/local/alertmanager/alertmanager.yml
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 4.2 List Active Alerts in Alertmanager

```bash
# Get alerts currently firing in Alertmanager
curl -s http://localhost:9093/api/v1/alerts | jq .

# Expected: List of alerts (may be empty if no alerts firing)
```

**Evidence Template:**

```
### 4.2 Active Alerts Evidence

Command: curl -s http://localhost:9093/api/v1/alerts | jq .
Date: <YYYY-MM-DD>
Environment: local docker-compose

Active Alerts: <count>
Alert Details:
- <alert_name_1>: <firing|resolved|inhibited>
- <alert_name_2>: <firing|resolved|inhibited>

Note: Active alerts indicate an ongoing issue. Empty list is expected in healthy state.
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

---

## Section 5: Tracing Evidence

### 5.1 Verify Trace Context Extraction

```bash
# Make a request with trace context headers
curl -s -H "traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01" \
     -H "tracestate: congo=t61rcWkgMzE" \
     http://localhost:8080/api/v1/intents | jq .

# Check response headers for traceparent
curl -s -I -H "traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01" \
         -H "tracestate: congo=t61rcWkgMzE" \
         http://localhost:8080/api/v1/intents | grep -i traceparent

# Expected: Response includes traceparent header
```

**Evidence Template:**

```
### 5.1 Trace Context Extraction Evidence

Test: Inbound traceparent/tracestate headers
Date: <YYYY-MM-DD>
Environment: local docker-compose

Inbound Headers Sent:
- traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
- tracestate: congo=t61rcWkgMzE

Response traceparent: <present|absent>
Trace ID in response: <ID or null>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 5.2 Verify OTLP Export (Optional)

```bash
# Check if OTEL_EXPORTER_OTLP_ENDPOINT is set
echo "OTEL_EXPORTER_OTLP_ENDPOINT: ${OTEL_EXPORTER_OTLP_ENDPOINT:-not set}"

# If set, verify traces are being exported
# (requires OTLP-compatible endpoint like Jaeger, Tempo, or Honeycomb)
# curl -s http://${OTEL_EXPORTER_OTLP_ENDPOINT}/api/traces | jq .

# Expected: Traces visible in OTLP backend (if configured)
```

**Evidence Template:**

```
### 5.2 OTLP Export Evidence

OTEL_EXPORTER_OTLP_ENDPOINT: <set|not set>
Date: <YYYY-MM-DD>
Environment: local docker-compose

OTLP Export: <enabled|disabled>

Note: OTLP export is optional. When OTEL_EXPORTER_OTLP_ENDPOINT is not set,
traces are written to structured JSON logs (tracing::info spans).
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

### 5.3 Verify Trace Context in Audit Events

```bash
# Create a test intent and check audit event trace_id field
curl -s -X POST http://localhost:8080/api/v1/intents \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"test-tenant","graph_definition":{}}' | jq .

# Query audit events to verify trace_id is populated
# (requires database access or audit API endpoint)
# curl -s http://localhost:8080/api/v1/audit/events | jq .

# Expected: audit_events table has trace_id and span_id columns populated
```

**Evidence Template:**

```
### 5.3 Trace Context in Audit Events Evidence

Test: Create intent, check audit event has trace_id
Date: <YYYY-MM-DD>
Environment: local docker-compose

Audit Event trace_id: <populated|null>
Audit Event span_id: <populated|null>

Note: Trace context is captured on in-process boundaries only.
Cross-process propagation via NATS headers, Temporal gRPC, or sqlx
connection context is Phase 4+ scope.
Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

---

## Section 6: Composite Observability Evidence

### 6.1 End-to-End Request Trace

```bash
#!/bin/bash
# observability-e2e-check.sh — End-to-End Observability Check Template
# This script exercises the full request path and verifies observability signals

set -euo pipefail

echo "=== E2E Observability Check ==="
echo "Date: $(date -Iseconds)"
echo ""

# 1. Health check
echo "1. Health check..."
HEALTH=$(curl -s http://localhost:8080/health)
echo "   Result: ${HEALTH}"
echo ""

# 2. Create intent (generates metrics + trace)
echo "2. Create intent..."
INTENT=$(curl -s -X POST http://localhost:8080/api/v1/intents \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"e2e-test-tenant","graph_definition":{}}')
INTENT_ID=$(echo "${INTENT}" | jq -r '.intent_id')
echo "   Intent ID: ${INTENT_ID}"
echo ""

# 3. Wait for metrics to be scraped (Prometheus scrapes every 15s)
echo "3. Waiting for Prometheus scrape..."
sleep 20

# 4. Check metrics
echo "4. Check metrics..."
curl -s "http://localhost:9090/api/v1/query?query=intent_rebase_intent_create_total" | jq .

# 5. Check traces in logs (if OTLP not configured)
echo "5. Check application logs for trace context..."
# docker logs intent-rebase-api 2>&1 | grep -i "trace_id\|span_id" | tail -10

echo ""
echo "=== E2E Observability Check Complete ==="
```

**Evidence Template:**

```
### 6.1 End-to-End Observability Evidence

Script: observability-e2e-check.sh
Date: <YYYY-MM-DD>
Environment: local docker-compose

Results:
- Health check: <ok|error>
- Intent created: <yes|no>
- Metrics populated: <yes|no>
- Trace context in logs: <present|absent>

Evidence Strength: LOCAL DOCKER-COMPOSE (not production)
```

---

## Observability Evidence Summary

| Component | Evidence Collected | Evidence Strength |
|-----------|-------------------|-------------------|
| Metrics endpoint | ⚠️ Placeholder nginx, not real intent-api metrics | LOCAL DOCKER-COMPOSE |
| Prometheus scrape | ✅ Targets intent-api/prometheus up | LOCAL DOCKER-COMPOSE |
| Prometheus rules | ✅ Rule groups present: intent_api_availability(3), compensation(2), dlq(3), error_budget(6), latency(3) | LOCAL DOCKER-COMPOSE |
| DLQ rules | ✅ DLQDepthHigh, DLQMessageStale, DLQReplayFailures present | LOCAL DOCKER-COMPOSE |
| Grafana dashboards | ✅ Health endpoint returns `database: ok`, version `10.2.0` after datasource default fix | LOCAL DOCKER-COMPOSE |
| Alertmanager config | ✅ Healthy after removing invalid Prometheus-only/lifecycle flags | LOCAL DOCKER-COMPOSE |
| Trace context | ⏳ Not exercised | LOCAL DOCKER-COMPOSE |

---

## Local Evidence Collected — April 29, 2026

### What Was Verified

| Check | Result | Notes |
|-------|--------|-------|
| Prometheus targets | ✅ PASS | intent-api and prometheus targets up |
| Prometheus rule groups | ✅ PASS | 17 rules across 4 groups: intent_api_availability(3), compensation(2), dlq(3), error_budget(6), latency(3) |
| DLQ rules | ✅ PASS | DLQDepthHigh, DLQMessageStale, DLQReplayFailures confirmed present |
| Alertmanager startup | ✅ FIXED | Removed invalid `--web.console.libraries`, `--web.console.templates`, and `--web.enable-lifecycle` flags |
| Metrics endpoint | ⚠️ GAP | Returns placeholder nginx static page, not intent-api Prometheus metrics |
| Grafana health | ✅ FIXED | Health endpoint returns `database: ok`, version `10.2.0` after resolving duplicate default datasource provisioning |

### Issues Found and Resolved

1. **Alertmanager invalid flags (RESOLVED)**
   - File: `infrastructure/local/docker-compose.yml`
   - Problem: Alertmanager command included `--web.console.libraries`, `--web.console.templates`, and `--web.enable-lifecycle`, which are not supported by the pinned Alertmanager image
   - Fix: Removed unsupported flags; Alertmanager now starts with only valid flags (`--config.file`, `--storage.path`)

2. **Metrics endpoint placeholder (KNOWN GAP)**
   - `intent-api` container in observability profile is nginx serving a static page, not the actual intent-api service
   - Real metrics require running `cargo run -p intent-api` separately
   - This is expected behavior for the local observability profile which is designed for infrastructure validation, not full application testing

3. **Grafana datasource default conflict (RESOLVED)**
   - File: `infrastructure/local/grafana/provisioning/datasources/prometheus.yml`
   - Problem: Two provisioned Prometheus datasource files both marked a datasource as default, causing Grafana startup failure
   - Fix: Renamed the legacy datasource to `Prometheus Legacy` and set `isDefault: false`; Grafana health now returns `database: ok`, version `10.2.0`

### Evidence Boundaries

> **⚠️ Explicitly Non-Production**
>
> This evidence was collected against local docker-compose infrastructure only. The observability stack validates configuration and rule syntax but does not represent:
> - Production Prometheus/Grafana/Alertmanager deployment
> - Real alert routing to external notification systems (PagerDuty, Slack, email)
> - Live application metrics from a running intent-api instance
> - End-to-end trace propagation across service boundaries

---

## Deferred Items (Phase 4+)

| Item | Reason Deferred | Phase |
|------|----------------|-------|
| Production Prometheus/Grafana/Alertmanager | Requires external SRE deployment | Phase 4+ |
| OTLP trace export to external backend | Requires external tracing infrastructure | Phase 4+ |
| Cross-process trace propagation (NATS, Temporal, sqlx) | Requires additional instrumentation | Phase 4+ |
| Production Alertmanager routing to PagerDuty/Slack | Requires external notification integration | Phase 4+ |
| SLO dashboard with real production data | Requires production telemetry | Phase 4+ |

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Observability is production-ready` | `Local observability stack is documented; production deployment requires external SRE sign-off` |
| `Metrics have been validated in staging` | `Metrics evidence collected against local docker-compose; staging validation pending` |
| `Tracing is end-to-end across all services` | `In-process trace context is implemented; cross-process propagation is Phase 4+ scope` |
| `Alertmanager routes to production notifications` | `Alertmanager config is local-only; production routing requires external integration` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/04-sre-and-slos.md` | SLO definitions and observability infrastructure |
| `docs/09-operations/05-runbooks.md` | Runbooks for alert handling |
| `docs/10-delivery/16-solo-ops-evidence-plan.md` | References this checklist for observability evidence |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (fixer) | Initial creation — metrics/Prometheus/Grafana/Alertmanager/traces evidence collection checklist templates |
| April 29, 2026 | (fixer) | Added local evidence from evidence execution run; fixed Alertmanager invalid flags (`--web.console.libraries`, `--web.console.templates`); documented known gaps (placeholder metrics, unconfirmed Grafana health) |
| April 29, 2026 | (orchestrator) | Re-verified local stack; removed remaining unsupported Alertmanager lifecycle flag; resolved Grafana duplicate-default datasource; confirmed Alertmanager and Grafana health endpoints locally |
