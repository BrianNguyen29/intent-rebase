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
- No graph traversal benchmarks yet
- No database query benchmarks yet
- No async/HTTP API benchmarks yet
- No load testing with actual production traffic patterns
- Fixtures are synthetic; real-world payload sizes may differ
