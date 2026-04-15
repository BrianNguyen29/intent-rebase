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

## Limitations

### In-Memory Tests
- **In-memory repositories only** — no Postgres, NATS, or Temporal dependency
- **Dev profile** — unoptimized build; release profile would show lower latencies
- **Single-node** — no horizontal scaling or load balancing tested
- **No cold-start** — server is warm before test begins
- **Synthetic payloads** — small, fixed-size request bodies
- **No connection pool exhaustion test** — bounded concurrent clients only

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
