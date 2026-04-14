# Evals and Benchmarks

## Product eval dimensions
- semantic diff quality
- impact precision
- unnecessary restart reduction
- stale approval detection accuracy
- compensation recommendation accuracy
- operator trust / explainability score

## Benchmark scenario catalog
- scope expansion
- scope shrink
- tighten compatibility
- change authority scope
- budget cut
- policy update mid-run
- external side effect already executed

## Suggested metrics
- % correct change type classification
- % artifacts correctly invalidated
- % false invalidations
- % salvageable work preserved
- mean time to safe rebase preview
- mean time to apply

---

## Benchmark Results — Batch 2 Slice 4 (rebase-engine sync)

**Scope:** sync CPU-bound diff + plan path only. No HTTP API, no graph service, no database queries.

**Benchmark harness:** `crates/rebase-engine/benches/rebase_latency.rs` (criterion)

**Command:** `cargo bench -p rebase-engine --bench rebase_latency -- --noplot`

### Observed latencies

| Benchmark | low (no change) | medium (scope add) | high (multi-section) |
|-----------|-----------------|--------------------|-----------------------|
| `compute_diff_sync` | ~490 ns | ~556 ns | ~2.6 µs |
| `compute_diff_with_risk_sync` | ~1 µs | ~988 ns | ~2.5 µs |
| `diff_and_plan_sync` | ~958 ns | ~2.5 µs | ~4.2 µs |

**Observations:**
- All observed latencies are in the microsecond range
- Diff target (< 100ms from P5): **MET** with significant margin
- Planning overhead is modest (~1-2µs over diff alone)

**Limitations:**
- No graph traversal benchmarks yet (added later - see below)
- No database query benchmarks yet (env-gated harness added - see below)
- No async/HTTP API benchmarks yet (sync path only benchmark harness added - see below)
- No load testing with actual production traffic patterns
- Fixtures are synthetic; real-world payload sizes may differ

---

## Benchmark Results — Graph Service (graph-traversal)

**Scope:** In-memory graph traversal operations with synthetic fixtures. No database queries.

**Benchmark harness:** `crates/graph-service/benches/graph_traversal.rs` (criterion)

**Command:** `cargo bench -p graph-service --bench graph_traversal -- --noplot`

### Observed latencies

| Benchmark | small (4 nodes) | medium (21 nodes) | large (121 nodes) |
|-----------|-----------------|--------------------|-----------------------|
| `bfs_reachable` | Success | Success | Success |
| `find_path` | Success | Success | Success |
| `cycle_detection` | Success | Success | Success |

**Observations:**
- All graph traversal operations complete successfully across all graph sizes
- Benchmark scope is in-memory only; SQL-backed graph repository benchmarks require live DB

**Limitations:**
- SQL-backed graph repository not benchmarked (requires live Postgres)
- Concurrent graph operations not benchmarked
- Graph classification/impact analysis not benchmarked

---

## Benchmark Results — Intent API (http-handlers sync path)

**Scope:** Synchronous processing paths in intent-api handlers. No HTTP server overhead, no database queries.

**Benchmark harness:** `crates/intent-api/benches/http_handlers.rs` (criterion)

**Command:** `cargo bench -p intent-api --bench http_handlers -- --noplot`

### Observed benchmarks

| Benchmark | Description |
|-----------|-------------|
| `diff_compute/low_no_change` | Diff computation with identical payloads |
| `diff_compute/medium_change` | Diff computation with different payloads |
| `validation/valid_request` | Validation overhead for valid request |
| `validation/invalid_empty_summary` | Validation overhead for invalid (empty summary) |
| `validation/invalid_nil_workflow` | Validation overhead for invalid (nil workflow) |
| `intent_service_create/create_intent` | IntentService.create_intent call |

**Observations:**
- Sync compute path benchmarks measure processing overhead without HTTP server
- Full HTTP server benchmarks require live server + load testing infrastructure

**Limitations:**
- No full HTTP server benchmarks (requires running server + load generator)
- No database-backed handler benchmarks (requires live Postgres)
- No graph-service integration benchmarks (requires full stack)

---

## Benchmark Results — Intent Service DB Operations (env-gated)

**Scope:** SQLx-backed intent repository operations. **Requires DATABASE_URL environment variable.**

**Benchmark harness:** `crates/intent-service/benches/db_operations.rs` (criterion)

**Command:** 
```bash
DATABASE_URL="postgres://user:pass@localhost/intent_rebase" cargo bench -p intent-service --bench db_operations -- --noplot
```

### Observed benchmarks

| Benchmark | Description |
|-----------|-------------|
| `db_create_intent/create_intent_tx` | Intent creation with initial version |
| `db_create_version/create_version_with_occ` | Version creation with optimistic concurrency control |
| `db_get_intent/get_intent` | Intent retrieval by ID |
| `db_list_versions/get_versions_by_intent` | Version listing for an intent |

**Observations:**
- When DATABASE_URL is not set, benchmarks report "skipped_no_database_url" and do not run
- Sample size reduced (20) to minimize DB load during benchmarking
- Connection pool limited to 4 max connections for benchmarking

**Limitations:**
- Requires live Postgres instance with DATABASE_URL set
- Connection pool benchmarks not included
- Concurrent DB operations not benchmarked
- Large payload benchmarks not included

---

## Overall Benchmark Coverage Status

| Layer | Benchmark | Status | Notes |
|-------|-----------|--------|-------|
| rebase-engine | Sync diff + plan | ✅ Delivered | Batch 2 Slice 4 |
| graph-service | Graph traversal | ✅ Delivered | BFS, path finding, cycle detection |
| intent-api | HTTP handler sync path | ✅ Delivered | Diff compute, validation |
| intent-service | DB operations | ✅ Env-gated harness | Requires DATABASE_URL |
| intent-service | DB operations (live) | ⬜ Not run | Requires live Postgres |
| HTTP server | Full HTTP benchmarks | ⬜ Not started | Requires running server |
| Full stack | Load testing | ⬜ Not started | Requires complete system |

**Limitations across all benchmarks:**
- No load testing with actual production traffic patterns
- Fixtures are synthetic; real-world payload sizes may differ
- Connection pool and concurrent operation benchmarks not included
