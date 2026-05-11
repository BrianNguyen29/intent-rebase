# Production Load Test Results

> Generated: 2026-04-15
> Test harness: `crates/intent-api/tests/load_test.rs`
> Repository: in-memory (no external dependencies) and SQLx-backed (docker-compose Postgres)
> Profile: dev (unoptimized)

---

## Section 1: In-Memory Load Test

**Run command:** `cargo test -p intent-api --features load-test --test load_test -- --nocapture test_load`

### Test Configuration

#### Traffic Mix
| Operation Type | Weight | Endpoints |
|---------------|--------|-----------|
| Read | 70% | GET /health, GET /intents/{id}, GET /intents/{id}/versions |
| Write | 20% | POST /intents, POST /intents/{id}/versions |
| Compute | 10% | POST /intents/{id}/diff, POST /intents/{id}/rebase-preview |

#### Load Levels
| Level | Concurrent Clients | Total Requests | Description |
|-------|-------------------|----------------|-------------|
| 1 | 10 | 1,000 | Normal load baseline |
| 2 | 50 | 5,000 | 5x normal (stress) |
| 3 | 100 | 10,000 | 10x normal (spike) |

### Results

#### Level 1 — Normal Load (10 clients, 1,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 1,000 |
| Successful | 1,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 2 ms |
| p90 Latency | 4 ms |
| **p95 Latency** | **5 ms** |
| p99 Latency | 7 ms |
| Max Latency | 15 ms |

#### Level 2 — 5x Stress (50 clients, 5,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 5,000 |
| Successful | 5,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 10 ms |
| p90 Latency | 18 ms |
| **p95 Latency** | **33 ms** |
| p99 Latency | 43 ms |
| Max Latency | 81 ms |

#### Level 3 — 10x Spike (100 clients, 10,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 10,000 |
| Successful | 10,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 21 ms |
| p90 Latency | 38 ms |
| **p95 Latency** | **60 ms** |
| p99 Latency | 77 ms |
| Max Latency | 132 ms |

### SLO Compliance (In-Memory)

| SLO Target | Threshold | Level 1 | Level 2 | Level 3 | Status |
|-----------|-----------|---------|---------|---------|--------|
| p95 Latency < 10s | 10,000 ms | 5 ms | 33 ms | 60 ms | ✅ PASS |
| Error Rate < 1% | 1.00% | 0.00% | 0.00% | 0.00% | ✅ PASS |

**All SLO targets met at all load levels.**

---

## Section 2: SQLx-Backed Load Test (Local Live Postgres)

**Run command:** `cd infrastructure/local && docker-compose up -d postgres && export DATABASE_URL="postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase" && cargo test -p intent-api --features load-test,sqlx-load-test --test load_test -- --nocapture test_load_sqlx`

**Infrastructure:** docker-compose Postgres, pool config: max_connections=20, min_connections=2, acquire_timeout=30s, idle_timeout=600s

### Test Configuration
| Test Case | Concurrent Clients | Total Requests | Description |
|-----------|-------------------|----------------|-------------|
| SQLx-L1 | 5 | 500 | Light load against live Postgres |
| SQLx-L2 | 10 | 1,000 | Normal load against live Postgres |

### Results

#### SQLx-L1 — Light Load (5 clients, 500 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 500 |
| Successful | 500 |
| Failed | 0 |
| Error Rate | 0.00% |
| **p95 Latency** | **5 ms** |
| Max Latency | ~12 ms |

#### SQLx-L2 — Normal Load (10 clients, 1,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 1,000 |
| Successful | 1,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| **p95 Latency** | **4 ms** |
| Max Latency | ~15 ms |

### SLO Compliance (SQLx)

| SLO Target | Threshold | SQLx-L1 | SQLx-L2 | Status |
|-----------|-----------|---------|---------|--------|
| p95 Latency < 10s | 10,000 ms | 5 ms | 4 ms | ✅ PASS |
| Error Rate < 1% | 1.00% | 0.00% | 0.00% | ✅ PASS |

**All SQLx SLO targets met. Local live Postgres load test passed.**

---

## Section 3: Evidence Collection Attempt — 2026-05-11

**Run command:** `cargo test -p intent-api --features load-test --test load_test -- --nocapture test_load`

**Infrastructure:** docker-compose services started (`docker compose -f infrastructure/local/docker-compose.yml up -d`)

**Service health (`docker compose ps`):**
| Service | Status |
|---------|--------|
| postgres | healthy |
| minio | healthy |
| prometheus | healthy (started explicitly) |
| grafana | healthy |
| nats | unhealthy (compose healthcheck reported unhealthy) |

**Test harness:** In-memory repositories (same harness as Section 1). This is a local in-memory run against a warm server — not equivalent to staging or production load testing.

### Results

#### Level 1 — Normal Load (10 clients, 1,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 1,000 |
| Successful | 1,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 2 ms |
| p90 Latency | 3 ms |
| **p95 Latency** | **4 ms** |
| p99 Latency | 7 ms |
| Max Latency | 14 ms |
| SLO | ✅ PASS |

#### Level 2 — 5x Stress (50 clients, 5,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 5,000 |
| Successful | 5,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 12 ms |
| p90 Latency | 20 ms |
| **p95 Latency** | **23 ms** |
| p99 Latency | 33 ms |
| Max Latency | 67 ms |
| SLO | ✅ PASS |

#### Level 3 — 10x Spike (100 clients, 10,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 10,000 |
| Successful | 10,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 24 ms |
| p90 Latency | 37 ms |
| **p95 Latency** | **42 ms** |
| p99 Latency | 62 ms |
| Max Latency | 125 ms |
| SLO | ✅ PASS |

### Prometheus Metrics Query Results

| Query | Result |
|-------|--------|
| `intent_api_requests_total` (non-existent aggregate) | `{"status":"success","data":{"resultType":"vector","result":[]}}` — empty vector |
| `intent_api_request_duration_seconds` (non-existent aggregate) | `{"status":"success","data":{"resultType":"vector","result":[]}}` — empty vector |

**Interpretation:** Prometheus returned empty vectors because the queries used non-existent aggregate metric names. The actual recorded metrics use granular names:
- `intent_api_rebase_preview_requests_total`
- `intent_api_rebase_apply_requests_total`
- `intent_api_intent_version_created_total`
- `intent_api_rebase_preview_duration_seconds`
- `intent_api_rebase_apply_duration_seconds`

Possible additional causes:
- Metrics endpoint not yet scraped at query time (scrape interval vs test duration mismatch)
- In-memory harness does not expose Prometheus metrics on the same port used by the query

**No overclaim:** Empty Prometheus results mean observability evidence is incomplete. This run validates the in-memory load harness only, not full L3 observability integration.

### NATS Healthcheck Gap

NATS container reported `unhealthy` via docker-compose healthcheck during the evidence collection window. The root cause was that the NATS server command in `infrastructure/local/docker-compose.yml` did not enable the monitoring port (`-m 8222`), so the healthcheck against `http://localhost:8222/healthz` failed. This has been corrected by adding `-m 8222` to the NATS command. JetStream config validation (G2) was performed separately on 2026-04-28 and is not invalidated by this transient healthcheck state. Full stack integration (NATS + load test) was not achieved in this run.

### Evidence Strength

| Criterion | Status |
|-----------|--------|
| Load harness functional | ✅ YES (in-memory) |
| L1/L2/L3 SLO pass | ✅ YES (in-memory thresholds) |
| Prometheus metrics visible | 🔴 NO (empty vectors) |
| NATS healthy during run | 🔴 NO (unhealthy at collection time) |
| SQLx-backed run | 🔴 NO (in-memory only) |
| Production equivalence | 🔴 NO |

### Follow-Up — NATS Fix and SQLx-Backed Run (2026-05-11)

**NATS healthcheck fix verified:**
- `infrastructure/local/docker-compose.yml` NATS command updated to `["--js", "-m", "8222"]`
- `docker compose -f infrastructure/local/docker-compose.yml up -d --force-recreate nats` succeeded
- `docker compose -f infrastructure/local/docker-compose.yml ps nats` showed `intent-rebase-nats ... Up ... (healthy)` with ports 4222/6222/8222 mapped

**Postgres recreation:**
- Postgres container recreated and showed healthy with `0.0.0.0:5432->5432/tcp`
- Migrations applied via `docker exec`; one duplicate trigger error for `graph_edges_validate_tenant_workflow` during migration 005 on already-initialized DB, but later migrations continued and SQLx test succeeded

**SQLx-backed load test run:**
- **Command:** `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase cargo test -p intent-api --features load-test,sqlx-load-test --test load_test -- --nocapture test_load_sqlx`
- **Result:** `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out`

#### SQLx-L1 — Light Load (5 clients, 500 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 500 |
| Successful | 500 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 2 ms |
| p90 Latency | 7 ms |
| **p95 Latency** | **10 ms** |
| p99 Latency | 22 ms |
| Max Latency | 32 ms |
| SLO | ✅ PASS |

#### SQLx-L2 — Normal Load (10 clients, 1,000 requests)
| Metric | Value |
|--------|-------|
| Total Requests | 1,000 |
| Successful | 1,000 |
| Failed | 0 |
| Error Rate | 0.00% |
| p50 Latency | 5 ms |
| p90 Latency | 12 ms |
| **p95 Latency** | **15 ms** |
| p99 Latency | 24 ms |
| Max Latency | 35 ms |
| SLO | ✅ PASS |

**Prometheus actual metric queries (follow-up):**
- `intent_api_rebase_preview_requests_total`: empty vector
- `intent_api_rebase_apply_requests_total`: empty vector
- `intent_api_intent_version_created_total`: empty vector

**Interpretation:** Even with correct metric names, Prometheus returned empty vectors. Likely causes:
- Metrics are recorded by the intent-api binary, but the load test harness runs in-process and may not expose the metrics endpoint on the port Prometheus scrapes
- Prometheus scrape target may not be configured to scrape the test process
- Scrape interval vs test duration mismatch

### L4 Observability Bounded Follow-Up — 2026-05-11

**Goal:** Validate that Prometheus can scrape at least one real metric from a running intent-api binary.

**Setup:**
- intent-api binary run via `cargo run -p intent-api`, default bind `0.0.0.0:8080`
- `DATABASE_URL` unset → in-memory repositories
- Server log: `DATABASE_URL not set — using in-memory repositories`; `Intent API server starting on 0.0.0.0:8080`
- Health check: `curl -s http://localhost:8080/health` → `{"status":"ok","uptime_seconds":54}`
- Prometheus `up{job="intent-api"}` query returned vector with `up=1` for `host.docker.internal:8080`

**Traffic generation:**
- 10 valid `POST /intents` requests with full valid payload → HTTP 201 with intent IDs

**Metrics validation:**

| Step | Result |
|------|--------|
| Local `/metrics` after traffic | `intent_api_intent_version_created_total{status="success"} 10` |
| Prometheus query after 15s scrape delay | `intent_api_intent_version_created_total` returned vector value `10` with labels `instance="host.docker.internal:8080"`, `job="intent-api"`, `status="success"` |
| Prometheus query `intent_api_rebase_preview_requests_total` | empty vector (no rebase-preview traffic generated — expected, not a failure) |

**Conclusion:** One real metric (`intent_api_intent_version_created_total`) successfully scraped by Prometheus from a running intent-api binary. This validates the basic Prometheus → intent-api metrics pipeline. It does NOT validate all metrics, all code paths, alerting rules, or production-equivalent observability.

**No overclaim:** This is a bounded L4 observability slice — one metric, one code path, local docker-compose only. Full L4 observability (all metrics, all paths, alerting validation, production scrape config) remains future scope.

### L4 Multi-Path Follow-Up — 2026-05-11

**Goal:** Attempt to validate additional metric paths (diff, rebase-preview, rebase-apply) by generating traffic across multiple endpoints.

**Setup:**
- intent-api binary run via `cargo run -p intent-api --no-default-features` (disabled default `jwt-auth` feature to rule out JWT middleware as the cause)
- Server bound `0.0.0.0:8080`, in-memory repositories
- `POST /intents` succeeded and created intent with ID `23e9748e-7c11-4316-a7e1-e63b675a480d`
- Total `intent_api_intent_version_created_total` incremented to 11

**Multi-path traffic attempt:**

| Endpoint | Method | Payload | Result | Notes |
|----------|--------|---------|--------|-------|
| `/intents/{intent_id}/versions` | POST | Valid CreateVersionRequest | 404 | Parameterized route not matched |
| `/intents/{intent_id}/diff` | POST | `{"from_version":1,"to_version":2}` | 404 | Parameterized route not matched |
| `/intents/{intent_id}/rebase-preview` | POST | `{"from_version":1,"to_version":2}` | 404 | Parameterized route not matched |
| `/intents/{intent_id}/rebase-apply` | POST | `{"from_version":1,"to_version":2}` | 404 | Parameterized route not matched |
| `/intents/{intent_id}/versions` | GET | — | 404 | Parameterized route not matched |
| `/intents/{intent_id}/versions/{version_number}` | GET | — | 404 | Parameterized route not matched |
| `/v1/graph/nodes` | POST | `{}` | 422 | Route matched (missing field error) |
| `/v1/graph/nodes/{node_id}` | GET | — | 404 | Parameterized route not matched |
| `/approval-requests/pending` | GET | — | 400 | Route matched (missing query param) |
| `/forensic/verify` | POST | `{}` | 422 | Route matched (missing field error) |

**Root cause analysis:**
- All non-parameterized routes (`/health`, `/metrics`, `/intents`, `/v1/graph/nodes`, `/approval-requests/pending`, `/forensic/verify`) match correctly and return expected status codes
- All parameterized routes (`/intents/{intent_id}`, `/intents/{intent_id}/versions`, `/intents/{intent_id}/diff`, etc.) return 404
- This behavior persists regardless of `jwt-auth` feature state
- Hypothesis: The `build_inmemory_router()` → `build_router()` chain may have a route registration issue specifically for parameterized paths when running as a standalone binary, or there is a subtle axum route-matching behavior not present in the test harness
- This is a runtime validation blocker, not a metrics implementation issue

**Metrics validated:**
- ✅ `intent_api_intent_version_created_total` — validated via 11 successful `POST /intents` requests

**Metrics blocked:**
- 🔴 `intent_api_diff_compute_duration_seconds` — blocked by 404 on `/intents/{intent_id}/diff`
- 🔴 `intent_api_rebase_preview_requests_total` — blocked by 404 on `/intents/{intent_id}/rebase-preview`
- 🔴 `intent_api_rebase_preview_duration_seconds` — blocked by 404 on `/intents/{intent_id}/rebase-preview`
- 🔴 `intent_api_rebase_apply_requests_total` — blocked by 404 on `/intents/{intent_id}/rebase-apply`
- 🔴 `intent_api_rebase_apply_duration_seconds` — blocked by 404 on `/intents/{intent_id}/rebase-apply`

**No overclaim:** Only one of six core metrics was validated. The remaining five are blocked by a parameterized route 404 issue in the standalone binary. Full L4 multi-path observability validation remains incomplete.

### L4 Post-Fix Multi-Path Validation — 2026-05-11

**Fix applied:** Commit `36bc548` changed axum route parameter syntax from `{param}` to `:param` in `crates/intent-api/src/router.rs`, restoring parameterized route matching in the standalone binary.

**Setup:**
- intent-api binary run via `cargo run -p intent-api --no-default-features`
- Server bound `0.0.0.0:8080`, in-memory repositories

**Traffic generation:**

| Step | Endpoint | Result |
|------|----------|--------|
| 1 | `POST /intents` | ✅ HTTP 201 — intent created |
| 2 | `POST /intents/{id}/versions` | ✅ HTTP 201 — version 2 created |
| 3 | `POST /intents/{id}/diff` | ✅ HTTP 200 — diff computed |
| 4 | `POST /intents/{id}/rebase-preview` | ✅ HTTP 200 — preview generated |
| 5 | `POST /intents/{id}/rebase-apply` | ✅ HTTP 200 — apply completed |

**Local `/metrics` after traffic:**

| Metric | Value |
|--------|-------|
| `intent_api_intent_version_created_total{status="success"}` | 2 |
| `intent_api_diff_compute_duration_seconds_count` | 1 |
| `intent_api_rebase_preview_requests_total{status="success"}` | 1 |
| `intent_api_rebase_preview_duration_seconds_count{graph_size="unknown"}` | 1 |
| `intent_api_rebase_apply_requests_total{status="success"}` | 1 |
| `intent_api_rebase_apply_duration_seconds_count{risk_class="medium"}` | 1 |

**Prometheus scrape after 15s delay:**

| Metric | Value |
|--------|-------|
| `intent_api_intent_version_created_total` | 2 |
| `intent_api_diff_compute_duration_seconds_count` | 1 |
| `intent_api_rebase_preview_requests_total` | 1 |
| `intent_api_rebase_preview_duration_seconds_count` | 1 |
| `intent_api_rebase_apply_requests_total` | 1 |
| `intent_api_rebase_apply_duration_seconds_count` | 1 |

**Conclusion:** All six core metrics successfully scraped by Prometheus from the running intent-api binary after the route fix. This validates the Prometheus → intent-api metrics pipeline for multiple code paths.

**No overclaim:** This is bounded local docker-compose evidence — one binary, in-memory repos, no load testing, no alerting validation, no production scrape config. Full L4 observability (alerting rules, sustained load, production-equivalent config) remains future scope.

---

## Limitations

### In-Memory Tests
- **In-memory repositories only** — no Postgres, NATS, or Temporal dependency
- **Dev profile** — unoptimized build; release profile would show lower latencies
- **Single-node** — no horizontal scaling or load balancing tested
- **No cold-start** — server is warm before test begins
- **Synthetic payloads** — small, fixed-size request bodies
- **No connection pool exhaustion test** — bounded concurrent clients only
- **Prometheus metrics empty (initial/SQLx runs)** — 2026-05-11 initial and SQLx runs returned empty vectors for both non-existent aggregate names and actual metric names because the in-process test harness did not expose a scrapeable metrics endpoint
- **Prometheus one-metric validated (L4 bounded follow-up)** — 2026-05-11 L4 follow-up successfully scraped `intent_api_intent_version_created_total` from a running intent-api binary
- **L4 multi-path blocked (pre-fix)** — 2026-05-11 multi-path follow-up attempted diff, rebase-preview, and rebase-apply traffic but all parameterized routes returned 404 in the standalone binary; only 1 of 6 core metrics validated
- **L4 multi-path validated (post-fix)** — 2026-05-11 post-fix validation successfully scraped all 6 core metrics from running intent-api binary after route parameter syntax fix (commit 36bc548); alerting and sustained load remain unvalidated
- **NATS unhealthy (initial run)** — 2026-05-11 initial run showed NATS container as unhealthy; fixed in follow-up by adding `-m 8222` to NATS command

### SQLx Tests
- **Local docker-compose Postgres only** — not equivalent to production RDS/high-performance managed Postgres
- **Dev profile** — unoptimized build
- **Single-node** — no replica read scaling tested
- **Pool config fixed** — max_connections=20; production may need higher

---

## Recommendations for Production

1. Run equivalent tests against a staging environment with production-grade Postgres (RDS/CloudSQL)
2. Test with release profile builds for realistic latency numbers
3. Add connection pool saturation tests (gradually increase clients until errors start)
4. Test with realistic payload sizes (large intents, many graph nodes)
5. Add sustained load test (30min+ at normal traffic levels) for memory leak detection
6. Validate SQLx pool config (max_connections, min_connections) against production load patterns
