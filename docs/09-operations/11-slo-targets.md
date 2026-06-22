# SLO Targets — Intent Rebase (Private-Only)

> **Status:** 🟡 DRAFT — SLO targets defined, thresholds validated via k6, real Prometheus SLO rules blocked on missing app metrics.
> **Scope:** Internal/private-only operation. No public ingress. No production-ready claim.
> **Last updated:** 2026-06-22

## 1. Service Level Objectives (SLOs)

| SLO ID | Description | Target | Measurement Window | Current Status |
|--------|-------------|--------|-------------------|----------------|
| SLO-AVAIL-001 | Availability (intent-api health endpoint) | 99.9% | 30 days | 🟡 Measured via k6 only; no formal uptime SLI from Prometheus |
| SLO-LAT-001 | p95 latency (health + business-path endpoints) | < 100ms | 5-minute rolling | 🟡 Validated under 5 VU internal load; p95 848µs. No real Prometheus rule. |
| SLO-LAT-002 | p99 latency | < 200ms | 5-minute rolling | 🔴 Not measured |
| SLO-ERR-001 | HTTP error rate (5xx + timeout) | < 0.1% | 5-minute rolling | 🟡 Validated under 5 VU internal load; 0% failure. No real Prometheus rule. |
| SLO-ERR-002 | 4xx rate from client errors | < 1% | 5-minute rolling | 🟡 Not separately tracked |
| SLO-CAP-001 | Concurrent VU capacity | ≥ 5 VU | Per test run | 🟡 Validated at 5 VU. Saturation point unknown. |
| SLO-UP-001 | Target scrape availability (Prometheus `up`) | 100% | 1-minute | 🟢 `up{job="intent-api"}` == 1 continuously observed |

## 2. Error Budgets (Conceptual)

| SLO | Monthly Budget | Burn Rate Alert Threshold |
|-----|---------------|---------------------------|
| SLO-AVAIL-001 | 43.2 minutes downtime | 2% budget/day |
| SLO-LAT-001 | 0.1% of requests > 100ms | 5% budget/day |
| SLO-ERR-001 | 0.1% of requests 5xx | 5% budget/day |

> **Note:** Error budgets are conceptual only. No automated burn-rate calculation or alerting exists because app metrics are not exposed to Prometheus.

## 3. Measurement Methods

### 3.1 k6 Load Test (Primary)
- **Script:** `infrastructure/production/k6/business-path-smoke.js`
- **Job:** `infrastructure/production/k6/business-path-load-job.yaml`
- **Profile:** 5 VUs, ramp 1m + sustain 5m + ramp-down 1m = 7 minutes total
- **Thresholds:** `p(95) < 100ms`, `http_req_failed < 0.1%`
- **Endpoint:** `http://intent-api:8080` (internal ClusterIP)

### 3.2 Prometheus Rules (Blocked)
- **Availability:** Would use `up{job="intent-api"} == 0` with `for: 1m` → alerts on pod down.
- **Latency:** Would use `histogram_quantile(0.95, rate(http_request_duration_seconds_bucket{job="intent-api"}[5m])) > 0.1` → **BLOCKED**: app `/metrics` returns empty.
- **Error Rate:** Would use `rate(http_requests_total{job="intent-api",status=~"5.."}[5m]) / rate(http_requests_total{job="intent-api"}[5m]) > 0.001` → **BLOCKED**: app `/metrics` returns empty.

### 3.3 Manual Health Checks
- `kubectl exec` → `wget http://localhost:8080/health` → HTTP 200, `{"status":"ok"}`
- `kubectl exec` → `wget http://localhost:8080/ready` → HTTP 200, `{"status":"ready"}`

## 4. Current Blockers

| Blocker | Impact | Next Step |
|---------|--------|-----------|
| **App metrics endpoint empty** | Cannot define latency/error-rate SLO rules in Prometheus | Instrument `intent-api` with `prometheus-client` or `opentelemetry-prometheus` exporter; expose `http_request_duration_seconds` histogram and `http_requests_total` counter |
| **No node-exporter / kube-state-metrics** | Cannot define resource-based SLOs (CPU, memory, disk) | Deploy node-exporter DaemonSet and kube-state-metrics Deployment to Prometheus scrape targets |
| **No public ingress** | Cannot validate edge/CDN latency | Defer until public ingress is enabled and A-03/A-04/A-07 are closed |
| **Low VU capacity** | Saturation point unknown | Scale GKE node pool, run 20 VU / 50 VU tests, observe HPA behavior |
| **No formal SLA** | No committed penalties or compensating policies | Define SLA document with customer-facing penalties after SLOs are stable |

## 5. Validation Evidence

### 5.1 k6 Load Test (2026-06-21)

| Metric | Value | Threshold | Status |
|--------|-------|-----------|--------|
| Total checks | 1830 | — | ✅ |
| Check pass rate | 100.00% | > 99.9% | ✅ |
| HTTP failed rate | 0.00% (0/1830) | < 0.1% | ✅ |
| p95 latency | 848.42µs | < 100ms | ✅ |
| Average latency | 639.82µs | — | ✅ |
| Max latency | 3.83ms | — | ✅ |
| Max VUs | 5 | — | ✅ |
| Duration | 7m0s | — | ✅ |

### 5.2 k6 Load Test (2026-06-22 — Lane 4 Escalation)

| Metric | Value | Threshold | Status |
|--------|-------|-----------|--------|
| Total checks | 1829 | — | ✅ |
| Check pass rate | 100.00% ✓ 1829 / ✗ 0 | > 99.9% | ✅ |
| HTTP failed rate | 0.00% (0/1829) | < 0.1% | ✅ |
| p95 latency | 1.67ms | < 100ms | ✅ |
| Average latency | 1.25ms | — | ✅ |
| Max latency | 3.82ms | — | ✅ |
| Max VUs | 5 | — | ✅ |
| Duration | 7m0s (1m ramp + 5m sustain + 1m ramp-down) | — | ✅ |
| Throughput | 4.35 req/s | — | ✅ |
| Rule firing during load | `PrometheusSelfMetricValidation` (real metric) | `firing` | ✅ |
| App `/metrics` | HTTP 200, content-length 0 | — | 🔴 BLOCKER |

**Note:** Real app-level SLO breach validation (latency/error-rate via `http_request_duration_seconds`) remains blocked because `intent-api` `/metrics` returns empty. The temporary rule validated Prometheus→Alertmanager pipeline with a real scraped metric (`prometheus_build_info`), not a synthetic `vector(1)` expression.

### 5.3 Prometheus Synthetic Rule Validation (2026-06-21)
- Temporary rule `SLOValidationSyntheticRule` (`expr: vector(1)`, `for: 0s`) added to `prometheus-rules` ConfigMap.
- Rule verified firing (`state=firing`) during 7-minute load test.
- Alertmanager notification metrics confirmed (Slack + email deltas observed).
- Rule removed and ConfigMap restored after test.
- Confirmed 0 alerts firing after cleanup.

### 5.4 Prometheus Real Metric Rule Validation (2026-06-22)
- Temporary rule `PrometheusSelfMetricValidation` (`expr: prometheus_build_info > 0`, `for: 0s`) added to `prometheus-rules` ConfigMap.
- Rule verified firing because `prometheus_build_info` is always present.
- This validated that Prometheus rule evaluation works with real scraped metrics, not just synthetic `vector(1)` expressions.
- **Caveat:** This is a Prometheus self-metric, not an app-level metric. App-level SLO breach validation (latency, error rate) remains blocked because `intent-api` `/metrics` returns empty (HTTP 200, content-length 0). Real app-level SLO rules require `http_request_duration_seconds` histogram and `http_requests_total` counter instrumentation.
- Rule removed and ConfigMap restored after validation. Prometheus restarted; 0 alerts firing confirmed.

## 6. Forbidden Claims

| Claim | Actual Status |
|-------|-------------|
| Production-ready | ❌ Not claimed. Private-only solo operation with open gates. |
| Public ingress load tested | ❌ Not claimed. Internal ClusterIP only. |
| Real app SLO rules validated | ❌ Not claimed. Prometheus self-metric only; app metrics absent. |
| SLA committed | ❌ Not claimed. No error budgets or penalties defined. |

## 7. Next Steps

1. **Instrument app metrics** (highest priority): Add `http_request_duration_seconds` histogram and `http_requests_total` counter to `intent-api`.
2. **Deploy node-exporter + kube-state-metrics**: Enable resource and container-level SLOs.
3. **Add real Prometheus SLO rules**: Latency, error rate, availability rules once app metrics are available.
4. **Run higher-load tests**: 20 VU, 50 VU with HPA enabled; measure saturation point.
5. **Define formal SLA**: Error budgets, burn-rate alerts, customer-facing penalties after SLOs are stable.

---
> **Signed:** BrianNguyen (via authorized assistant fixer)
> **Date:** 2026-06-22
> **No production-ready claim. No external signoff claim. A-07 WAIVED-SOLO. A-03/A-04 SELF-ATTESTED-SOLO.**
