# 11 — Pen Test & Load Test Execution Packet Template

**Status:** `DOCUMENTED — Template Only; No External Testing Conducted`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** April 2026

---

## Purpose

This document provides **execution packet templates** for:
1. **Penetration testing** (L1-L5 scope)
2. **Load/performance testing** (L1-L5 scope)

These are **planning and execution artifacts** — they document how testing should be conducted, what evidence to collect, and how to track findings. They do **not** represent that testing has been executed or that findings have been resolved.

> **⚠️ Evidence Strength Disclaimer**
>
> This is a **template for test execution**. No penetration testing or load testing has been conducted by an external party. Do not represent these templates as evidence of test completion. Local load test evidence (L1/L2) is documented separately in `docs/11-quality/load-test-results.md`.

---

## Part 1: Penetration Testing Execution Packet

### PT-1: Pre-Engagement

```markdown
## PT-1: Pre-Engagement Checklist

**Pen Test Type:** [ ] External [ ] Internal [ ] Combined
**Engagement Lead:** <Name>
**Target Environment:** [ ] Dev [ ] Staging [ ] Production (PROHIBITED for active exploitation)
**Date:** <YYYY-MM-DD>

### Pre-Engagement Authorization

| Item | Status | Notes |
|------|--------|-------|
| Written authorization obtained | [ ] Yes [ ] No | Required before any testing |
| Scope document reviewed and approved | [ ] Yes [ ] No | See `docs/08-security/06-pen-test-scope.md` |
| Rules of engagement signed | [ ] Yes [ ] No | Includes: no data destruction, no lateral movement beyond IRE |
| Emergency contact established | [ ] Yes [ ] No | Who to call if issues arise |
| Kill switch procedure defined | [ ] Yes [ ] No | How to stop test if needed |

### Environment Readiness

| Item | Status | Notes |
|------|--------|-------|
| Dev environment accessible | [ ] Yes [ ] No | For initial reconnaissance |
| Staging environment accessible | [ ] Yes [ ] No | For full exploitation attempts |
| Test accounts created | [ ] Yes [ ] No | API keys, JWT credentials |
| Production isolated | [ ] Yes [ ] No | No active exploitation on production |

### Tools Readiness

| Tool | Purpose | Ready |
|------|---------|-------|
| Burp Suite / OWASP ZAP | Web API testing | [ ] |
| SQLMap | SQL injection testing | [ ] |
| ffuf / dirb | Directory enumeration | [ ] |
| nmap | Network reconnaissance | [ ] |
| curl / httpie | Manual API testing | [ ] |
| JWT tool | JWT manipulation | [ ] |
| Custom scripts | Application-specific testing | [ ] |

---
```

### PT-2: Reconnaissance Evidence

```markdown
## PT-2: Reconnaissance Evidence

**Date:** <YYYY-MM-DD>
**Tester:** <Name>
**Environment:** <dev|staging>

### API Endpoint Enumeration

```bash
# Endpoints discovered:
# <list endpoints with method and path>

# Notable findings:
# <any interesting endpoints or behaviors>
```

### Technology Stack Identification

```bash
# Technology fingerprinting results:
# Server: <server type and version>
# Frameworks: <detected frameworks>
# Libraries: <detected libraries>
# Headers: <interesting headers>
```

### Attack Surface Mapping

```markdown
| Endpoint | Auth Required | Input Fields | Risk Level |
|----------|--------------|-------------|------------|
| <endpoint> | [ ] Yes [ ] No | <fields> | <LOW/MED/HIGH/CRIT> |
| | | | |
```

### Evidence

```bash
# Attach screenshots, curl commands, or tool output here
```

---
```

### PT-3: Vulnerability Discovery Evidence

```markdown
## PT-3: Vulnerability Discovery Evidence

**Date:** <YYYY-MM-DD>
**Tester:** <Name>
**Environment:** <dev|staging>

### PT-3.1: Authentication Testing

| Test | Command/Method | Result | Severity |
|------|---------------|--------|----------|
| API key brute force | | PASS/FAIL | |
| JWT manipulation | | PASS/FAIL | |
| JWT alg=none bypass | | PASS/FAIL | |
| Credential stuffing | | PASS/FAIL | |
| Default credentials | | PASS/FAIL | |

**Evidence:**
```bash
# <commands and output>
```

### PT-3.2: Authorization Testing

| Test | Command/Method | Result | Severity |
|------|---------------|--------|----------|
| IDOR on intents | | PASS/FAIL | |
| IDOR on versions | | PASS/FAIL | |
| Cross-tenant access | | PASS/FAIL | |
| Privilege escalation | | PASS/FAIL | |

**Evidence:**
```bash
# <commands and output>
```

### PT-3.3: Input Validation Testing

| Test | Payload | Result | Severity |
|------|---------|--------|----------|
| SQL injection (POST /intents) | | DETECTED/SAFE | |
| SQL injection (POST /artifacts) | | DETECTED/SAFE | |
| XSS (console) | | DETECTED/SAFE | |
| Command injection | | DETECTED/SAFE | |

**Evidence:**
```bash
# <commands and output>
```

### PT-3.4: Cross-Tenant Isolation Testing (RR-09 Priority)

| Test | Command/Method | Result | Severity |
|------|---------------|--------|----------|
| Tenant A reads Tenant B intents | | PASS/FAIL | |
| Tenant A modifies Tenant B intents | | PASS/FAIL | |
| Tenant A accesses Tenant B audit | | PASS/FAIL | |
| Tenant A approves Tenant B workflow | | PASS/FAIL | |

**Evidence:**
```bash
# <commands and output>
```

---
```

### PT-4: Exploitation Evidence

```markdown
## PT-4: Exploitation Evidence

**Date:** <YYYY-MM-DD>
**Tester:** <Name>
**Environment:** <dev|staging>

### PT-4.1: Privilege Escalation

| Target | Method | Result | Impact |
|--------|--------|--------|--------|
| Standard user -> privileged | | SUCCESS/FAIL | |
| Read-only -> write | | SUCCESS/FAIL | |
| Tenant-local -> cross-tenant | | SUCCESS/FAIL | |

**Steps to reproduce:**
```bash
# 1. <step>
# 2. <step>
```

### PT-4.2: Cross-Tenant Data Access

| Target | Method | Result | Data Accessed |
|--------|--------|--------|---------------|
| Tenant B intents | | SUCCESS/FAIL | <type and volume> |
| Tenant B audit events | | SUCCESS/FAIL | <type and volume> |
| Tenant B policy snapshots | | SUCCESS/FAIL | <type and volume> |

**Steps to reproduce:**
```bash
# 1. <step>
# 2. <step>
```

### PT-4.3: Audit Trail Tampering

| Test | Method | Result | Detected |
|------|--------|--------|----------|
| Delete audit event | | SUCCESS/FAIL | [ ] Yes [ ] No |
| Modify audit event | | SUCCESS/FAIL | [ ] Yes [ ] No |
| Suppress audit event | | SUCCESS/FAIL | [ ] Yes [ ] No |

**Steps to reproduce:**
```bash
# 1. <step>
# 2. <step>
```

### PT-4.4: Approval Bypass

| Test | Method | Result | Impact |
|------|--------|--------|--------|
| Skip approval step | | SUCCESS/FAIL | |
| Modify approval after grant | | SUCCESS/FAIL | |
| Fake approval timestamp | | SUCCESS/FAIL | |

**Steps to reproduce:**
```bash
# 1. <step>
# 2. <step>
```

---
```

### PT-5: Findings Report

```markdown
## PT-5: Findings Report

| Finding ID | Title | CVSS | Severity | Status | Remediation |
|------------|-------|------|----------|--------|-------------|
| PT-FIND-001 | | /10 | CRIT/HIGH/MED/LOW | OPEN | |
| PT-FIND-002 | | | | | |
| PT-FIND-003 | | | | | |

### PT-FIND-001: <Title>

**CVSS Score:** <score> (<vector>)
**Severity:** <CRIT/HIGH/MED/LOW>
**Status:** <OPEN/IN_PROGRESS/RESOLVED/DISMISSED>

**Description:**
<detailed description of the finding>

**Steps to Reproduce:**
```bash
# 1. <step>
# 2. <step>
```

**Impact:**
<what an attacker could achieve if this vulnerability is exploited>

**Remediation:**
<recommended fix>

**References:**
<related CWE, OWASP, or other references>

---
```

### PT-6: Pen Test Sign-Off

```markdown
## PT-6: Pen Test Sign-Off

**Pen Test Lead:** _______________________
**Organization:** _______________________
**Date of Test:** _______________________
**Report Date:** _______________________

| Deliverable | Status | Notes |
|-------------|--------|-------|
| Scope document | [ ] Complete | |
| Reconnaissance evidence | [ ] Complete | |
| Vulnerability findings | [ ] Complete | |
| Exploitation evidence | [ ] Complete | |
| Final report | [ ] Complete | |

**Overall Assessment:** [ ] APPROVED [ ] APPROVED WITH CONDITIONS [ ] NOT APPROVED

**Signature:** _______________________
```

---

## Part 2: Load Testing Execution Packet

### LT-1: Load Test Plan

```markdown
## LT-1: Load Test Plan

**Test Level:** [ ] L1 [ ] L2 [ ] L3 [ ] L4 [ ] L5
**Environment:** <local|staging-like|staging|production>
**Date:** <YYYY-MM-DD>

### Test Objectives

| Objective | Target | Measurement Method |
|-----------|--------|-------------------|
| Measure throughput | <N> req/s | k6/telegraf metrics |
| Measure latency | p95 < <X> ms | histogram_quantile |
| Verify error rate | < <Y>% | error counter |
| Verify SLO compliance | 99.9% availability | burn rate alert |

### Load Profile

| Stage | Duration | VUs | RPS Target | Description |
|-------|----------|-----|------------|-------------|
| Ramp-up | <M> min | 0 -> <N> | | Gradual increase |
| Steady-state | <M> min | <N> | <R> | Normal load |
| Stress | <M> min | <N> * <X> | | Spike load |
| Ramp-down | <M> min | <N> -> 0 | | Cool down |

### Test Scenarios

| Scenario | Weight | Endpoint | Method | Payload |
|----------|--------|----------|--------|---------|
| Read intents | 70% | GET /api/v1/intents/{id} | GET | Small payload |
| Create intent | 10% | POST /api/v1/intents | POST | Medium payload |
| Create version | 10% | POST /api/v1/intents/{id}/versions | POST | Medium payload |
| Diff compute | 5% | POST /api/v1/intents/{id}/diff | POST | Large payload |
| Rebase preview | 5% | POST /api/v1/intents/{id}/rebase-preview | POST | Large payload |

### Success Criteria

| Criterion | Threshold | Measurement |
|-----------|-----------|-------------|
| p95 latency | < 100ms | histogram_quantile(0.95, latency_bucket) |
| Error rate | < 0.1% | errors / total * 100 |
| SLO availability | 99.9% | 1 - (errors / total) |

---
```

### LT-2: L1/L2 Load Test Evidence (Local)

```markdown
## LT-2: L1/L2 Load Test Evidence (Local — Already Executed)

**Test Level:** L1 (in-memory) and L2 (SQLx-backed)
**Environment:** local docker-compose
**Date:** 2026-04-15
**Status:** ✅ COMPLETED

### L1: In-Memory Load Test

**Configuration:**
- Concurrent clients: 10 / 50 / 100
- Total requests: 1,000 / 5,000 / 10,000
- Environment: In-memory repositories only

**Results:**

| Level | Clients | Total Requests | p95 Latency | Error Rate |
|-------|---------|----------------|-------------|------------|
| L1-Normal | 10 | 1,000 | 5 ms | 0.00% |
| L1-Stress | 50 | 5,000 | 33 ms | 0.00% |
| L1-Spike | 100 | 10,000 | 60 ms | 0.00% |

**SLO Compliance:** ✅ PASS (p95 < 10s, error rate < 0.1% at all levels)

**Evidence:** `docs/11-quality/load-test-results.md`

### L2: SQLx-Backed Load Test

**Configuration:**
- Concurrent clients: 5 / 10
- Total requests: 500 / 1,000
- Environment: docker-compose Postgres

**Results:**

| Level | Clients | Total Requests | p95 Latency | Error Rate |
|-------|---------|----------------|-------------|------------|
| L2-Light | 5 | 500 | 5 ms | 0.00% |
| L2-Normal | 10 | 1,000 | 4 ms | 0.00% |

**SLO Compliance:** ✅ PASS (p95 < 10s, error rate < 0.1% at all levels)

**Evidence:** `docs/11-quality/load-test-results.md`

---
```

### LT-3: L3 Load Test Evidence (Staging-Like)

```markdown
## LT-3: L3 Load Test Evidence (Staging-Like — Pending)

**Test Level:** L3 (full stack with NATS + Postgres)
**Environment:** infrastructure/staging/docker-compose.yml
**Date:** <YYYY-MM-DD>
**Status:** 🟡 PENDING — staging scaffold exists; execution pending

### Prerequisites

| Prerequisite | Status | Notes |
|--------------|--------|-------|
| Staging docker-compose running | [ ] Yes [ ] No | `docker compose -f infrastructure/staging/docker-compose.yml up -d` |
| intent-api deployed to staging | [ ] Yes [ ] No | cargo run or docker deploy |
| Load test harness ready | [ ] Yes [ ] No | k6 or custom test harness |
| Prometheus metrics accessible | [ ] Yes [ ] No | http://localhost:9091 |
| Grafana dashboard ready | [ ] Yes [ ] No | http://localhost:3001 |

### Test Execution Template

```bash
#!/bin/bash
# lt-l3-load-test.sh — L3 Load Test Execution Script

set -euo pipefail

STAGING_STACK="infrastructure/staging/docker-compose.yml"
API_URL="http://localhost:8081"  # Staging port offset
PROMETHEUS_URL="http://localhost:9091"

echo "=== L3 Load Test (Staging-Like) ==="
echo "Date: $(date -Iseconds)"
echo "API: ${API_URL}"
echo "Prometheus: ${PROMETHEUS_URL}"
echo ""

# 1. Verify services healthy
echo "1. Verifying services..."
curl -s "${API_URL}/health" | jq . || { echo "API not healthy"; exit 1; }

# 2. Run k6 load test
echo "2. Running k6 load test..."
k6 run \
  --out influxdb=http://localhost:8086/k6 \
  --summary-export=lt-l3-results.json \
  scripts/load_test_l3.js

# 3. Wait for metrics to be scraped
sleep 30

# 4. Query Prometheus for metrics
echo "3. Collecting Prometheus metrics..."
# Query granular request counters (no aggregate intent_api_requests_total exists)
curl -s "${PROMETHEUS_URL}/api/v1/query?query=intent_api_intent_version_created_total" | jq .
curl -s "${PROMETHEUS_URL}/api/v1/query?query=intent_api_rebase_preview_requests_total" | jq .
curl -s "${PROMETHEUS_URL}/api/v1/query?query=intent_api_rebase_apply_requests_total" | jq .
# Query a valid latency histogram
curl -s "${PROMETHEUS_URL}/api/v1/query?query=histogram_quantile(0.95, intent_api_rebase_preview_duration_seconds_bucket)" | jq .

# 5. Check for errors
echo "4. Checking error rate..."
# Aggregate error rate across granular counters (no intent_api_requests_total exists)
ERROR_RATE=$(curl -s "${PROMETHEUS_URL}/api/v1/query?query=sum(rate(intent_api_intent_version_created_total{status!=\"success\"}[5m]))+sum(rate(intent_api_rebase_preview_requests_total{status!=\"success\"}[5m]))+sum(rate(intent_api_rebase_apply_requests_total{status!=\"success\"}[5m]))" | jq '.data.result | length')
echo "Error rate queries: ${ERROR_RATE}"

echo ""
echo "=== L3 Load Test Complete ==="
echo "Results: lt-l3-results.json"
```

### Expected L3 Results Template

| Level | Clients | Duration | Total Requests | p50 Latency | p95 Latency | p99 Latency | Error Rate | SLO Pass |
|-------|---------|----------|----------------|-------------|-------------|-------------|------------|----------|
| L3-Normal | 50 | 5 min | <N> | <ms> | <ms> | <ms> | <%> | [ ] |
| L3-Stress | 100 | 5 min | <N> | <ms> | <ms> | <ms> | <%> | [ ] |
| L3-Spike | 200 | 2 min | <N> | <ms> | <ms> | <ms> | <%> | [ ] |

### Evidence Collection Checklist

```markdown
| Item | Status | Evidence |
|------|--------|----------|
| k6 output | [ ] Collected | lt-l3-results.json |
| Prometheus metrics | [ ] Collected | curl output |
| Grafana dashboard screenshot | [ ] Captured | lt-l3-grafana.png |
| Application logs | [ ] Collected | lt-l3-logs.txt |
| Resource utilization | [ ] Collected | docker stats output |
```

---
```

### LT-4: L4 Load Test Evidence (Observability-Integrated)

```markdown
## LT-4: L4 Load Test Evidence (Observability-Integrated — Pending)

**Test Level:** L4 (full stack + Prometheus + Grafana + Alertmanager)
**Environment:** infrastructure/staging/docker-compose.yml --profile observability
**Date:** <YYYY-MM-DD>
**Status:** 🔴 PENDING — requires observability stack deployment

### Prerequisites

| Prerequisite | Status | Notes |
|--------------|--------|-------|
| L3 complete | [ ] Yes [ ] No | |
| Observability profile running | [ ] Yes [ ] No | `docker compose --profile observability up -d` |
| Prometheus scraping all targets | [ ] Yes [ ] No | Verify targets at http://localhost:9091/targets |
| Grafana dashboards accessible | [ ] Yes [ ] No | Verify at http://localhost:3001 |
| Alertmanager accessible | [ ] Yes [ ] No | Verify at http://localhost:9094 |

### Test Execution Template

```bash
#!/bin/bash
# lt-l4-load-test.sh — L4 Load Test with Observability

set -euo pipefail

STAGING_STACK="infrastructure/staging/docker-compose.yml"
API_URL="http://localhost:8081"
PROMETHEUS_URL="http://localhost:9091"
GRAFANA_URL="http://localhost:3001"
ALERTMANAGER_URL="http://localhost:9094"

echo "=== L4 Load Test (Observability-Integrated) ==="
echo "Date: $(date -Iseconds)"

# 1. Verify observability stack
echo "1. Verifying observability stack..."
curl -s "${PROMETHEUS_URL}/-/healthy" || { echo "Prometheus not healthy"; exit 1; }
curl -s "${ALERTMANAGER_URL}/-/healthy" || { echo "Alertmanager not healthy"; exit 1; }

# 2. Clear any firing alerts
echo "2. Silencing any existing alerts..."
curl -X POST "${ALERTMANAGER_URL}/api/v1/silences" \
  -H "Content-Type: application/json" \
  -d '{"matchers":[],"comment":"L4 load test","createdBy":"load-test"}'

# 3. Run load test
echo "3. Running L4 load test..."
k6 run \
  --out influxdb=http://localhost:8086/k6 \
  scripts/load_test_l4.js

# 4. Monitor alerts during test
echo "4. Monitoring alerts during test..."
for i in {1..30}; do
  ALERTS=$(curl -s "${ALERTMANAGER_URL}/api/v1/alerts" | jq '.data | length')
  echo "  Active alerts: ${ALERTS}"
  if [ "${ALERTS}" -gt 0 ]; then
    curl -s "${ALERTMANAGER_URL}/api/v1/alerts" | jq '.data[] | {name: .labels.alertname, state: .status}'
  fi
  sleep 10
done

# 5. Verify no alerts persisted after cool-down
echo "5. Verifying alert recovery..."
sleep 60
curl -s "${ALERTMANAGER_URL}/api/v1/alerts" | jq .

echo ""
echo "=== L4 Load Test Complete ==="
```

### L4-Specific Validation

| Check | Command | Expected | Status |
|-------|---------|----------|--------|
| Prometheus scraping intent-api | `curl -s ${PROMETHEUS_URL}/api/v1/targets \| jq '.data.targets[].labels.job'` | intent-api present | [ ] |
| Metrics being recorded | `curl -s ${PROMETHEUS_URL}/api/v1/query?query=sum(intent_api_intent_version_created_total)+sum(intent_api_rebase_preview_requests_total)+sum(intent_api_rebase_apply_requests_total)'` | > 0 | [ ] |
| Grafana SLO dashboard populated | Grafana UI | Panels showing data | [ ] |
| No alerts firing during normal load | `curl -s ${ALERTMANAGER_URL}/api/v1/alerts'` | 0 active | [ ] |
| Alerts fire under stress | k6 stress phase | Critical/Warning alerts | [ ] |
| Alerts resolve after cool-down | Post-test silence | 0 active | [ ] |

---
```

### LT-5: L5 Load Test Evidence (Production — External)

```markdown
## LT-5: L5 Load Test Evidence (Production — Blocked)

**Test Level:** L5 (production load)
**Environment:** Production infrastructure
**Date:** N/A
**Status:** 🔴 BLOCKED — requires external SRE engagement and production deployment

### Why L5 Is Blocked

L5 (production load testing) requires:
1. Production infrastructure deployment
2. External SRE sign-off on operational readiness
3. Production data protection (no synthetic data in production)
4. External load testing tooling (k6 Cloud, Artillary, or similar)
5. Coordinated maintenance window

### What L5 Would Validate

| Validation | Purpose | Why It Matters |
|-----------|---------|---------------|
| Real production throughput | Actual capacity under production load | Capacity planning |
| Production latency percentiles | Real p50/p95/p99 under production | SLO compliance |
| Production error rate | Real error rate with production data | SLO compliance |
| CDN/load balancer behavior | Performance under distributed load | Infrastructure |
| Real user payload mix | Production traffic patterns | Realistic testing |
| Third-party dependency performance | API calls to external services | Dependency validation |

### Pre-requisites Before L5

| Prerequisite | Owner | Status |
|--------------|-------|--------|
| Production deployment pipeline | SRE | 🔴 |
| External SRE sign-off | SRE | 🔴 |
| Production data protection plan | Security | 🔴 |
| Load test tooling (external) | SRE | 🔴 |
| Maintenance window | Operations | 🔴 |
| Rollback plan | SRE | 🔴 |

### L5 Evidence Template

```markdown
**Test Date:** <YYYY-MM-DD>
**Environment:** Production
**Load Tool:** <k6 Cloud | Artillery | custom>
**Test Duration:** <duration>
**Peak RPS:** <value>

**Results:**

| Metric | Target | Actual | SLO Pass |
|--------|--------|--------|----------|
| Peak RPS | <target> | <actual> | [ ] |
| p50 Latency | <target> | <actual> | [ ] |
| p95 Latency | <target> | <actual> | [ ] |
| p99 Latency | <target> | <actual> | [ ] |
| Error Rate | <target> | <actual> | [ ] |

**Evidence:** <links to test reports, screenshots, metrics dashboards>

**SRE Sign-Off:** [ ] APPROVED [ ] NOT APPROVED — <reason>
```

---
```

### LT-6: Load Test Sign-Off

```markdown
## LT-6: Load Test Sign-Off

### L1/L2 Sign-Off (Local — Completed)

**Status:** ✅ COMPLETED — Local evidence exists in `docs/11-quality/load-test-results.md`
**Reviewed By:** Brian Nguyen (solo practitioner)
**Date:** 2026-04-15

### L3 Sign-Off (Staging-Like)

**Status:** 🟡 PENDING — staging scaffold exists; execution pending
**Sign-Off Required:** External SRE (preferred) or solo practitioner

### L4 Sign-Off (Observability-Integrated)

**Status:** 🔴 PENDING — requires observability stack deployment
**Sign-Off Required:** External SRE

### L5 Sign-Off (Production)

**Status:** 🔴 BLOCKED — requires production infrastructure and external SRE
**Sign-Off Required:** External SRE + Production Operations

---
```

---

## Findings Tracker

```markdown
## Combined Findings Tracker

### Penetration Testing Findings

| Finding ID | Title | CVSS | Severity | Status | Remediation |
|------------|-------|------|----------|--------|-------------|
| PT-FIND-001 | | /10 | | | |
| | | | | | |

### Load Testing Findings

| Finding ID | Title | Severity | Status | Remediation |
|------------|-------|----------|--------|-------------|
| LT-FIND-001 | | | | |
| | | | | |

---
```

---

## Deferred Items

| Item | Reason Deferred | Phase |
|------|----------------|-------|
| L3 execution | Requires staging environment running | Future |
| L4 execution | Requires observability stack deployment | Future |
| L5 execution | Requires production infrastructure + external SRE | Future |
| External pen test | Requires external pen test team engagement | Future |

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Pen test completed by external tester` | `Pen test packet template exists; external pen test pending engagement` |
| `L3-L5 load tests passed` | `L1/L2 local load tests passed; L3-L5 pending staging/production execution` |
| `Production load test passed` | `L1/L2 local evidence exists; production load test blocked pending external SRE and production infrastructure` |
| `SRE-approved load test results` | `Local load test results documented; SRE approval pending external review` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/08-security/06-pen-test-scope.md` | Pen test scope definition |
| `docs/11-quality/load-test-results.md` | L1/L2 load test results (already executed) |
| `docs/10-delivery/16-solo-ops-evidence-plan.md` | References this packet for L3-L5 evidence collection |
| `infrastructure/staging/docker-compose.yml` | Staging scaffold for L3/L4 execution |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (fixer) | Initial creation — pen test execution packet (PT-1 through PT-6) and load test execution packet (LT-1 through LT-6); L1/L2 marked completed, L3-L5 and external pen test marked pending/blocked |
