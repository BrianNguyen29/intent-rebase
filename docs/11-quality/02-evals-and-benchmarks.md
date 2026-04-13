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

## Criterion Benchmark Infrastructure (Bounded Slice)

The `crates/rebase-engine/benches/diff_latency.rs` harness uses [Criterion](https://bheisler.github.io/criterion.rs/book/criterion_rs.html) to measure `compute_diff_sync` latency. This is a **bounded infrastructure slice only** — it verifies the benchmark harness builds and runs.

**CI benchmark job:** `.github/workflows/ci.yml#bench` — runs `cargo bench -p rebase-engine` and uploads criterion HTML reports as artifacts.

**Baseline template:** [benchmark-baseline-results.md](./benchmark-baseline-results.md) — fill in after running benchmarks to capture p50/p95/p99 values.

**Status:** Actual performance targets (p95 < 60s for low/medium risk, etc.) and production load testing remain **gated on P2 completion** (Phase 3 Batch 2 full delivery). See [Phase 3 Checklist: SRE & Observability](../10-delivery/checklists/checklist-phase-3.md#3-sre--observability).

**Graph-path benchmark slice (P5 groundwork):** `crates/graph-service/benches/graph_ops.rs` measures `find_reachable`, `find_path`, and `detect_cycles` latency using criterion with deterministic in-memory fixtures. This is bounded groundwork establishing graph traversal baseline numbers before P5 full performance work.

**CI benchmark jobs:**
- `.github/workflows/ci.yml#bench` — runs `cargo bench -p rebase-engine` (diff latency)
- `cargo bench -p graph-service --bench graph_ops` (graph traversal/cycle detection)

**Status:** Graph traversal benchmarks are **bounded groundwork** — they verify the harness infrastructure and capture local baseline numbers. Actual performance targets and production load testing remain gated on P5 completion.

**Baseline template:** [benchmark-baseline-results.md](./benchmark-baseline-results.md)
