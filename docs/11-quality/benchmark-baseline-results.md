# Benchmark Baseline Results

> **Template:** Fill in after running `cargo bench -p rebase-engine` locally or via CI benchmark job.
> **Status:** Local benchmark baseline captured. Production targets remain gated on P2 completion.

---

## Benchmark Harness Verification

| Check | Status |
|-------|--------|
| `cargo bench -p rebase-engine` runs successfully | ✅ |
| Criterion HTML reports generated in `target/criterion/` | ✅ |
| CI benchmark job passes (builds + runs) | ✅ |
| Benchmark artifacts uploaded to CI | ✅ |

---

## Diff Latency Benchmarks (Local Baseline — April 2026)

These benchmarks measure `compute_diff_sync` latency using criterion v0.5.1.
**This is a local baseline on developer hardware — NOT production SLA validation.**

### Environment

| Field | Value |
|-------|-------|
| Rust toolchain | 1.94.0 (4a4ef493e 2026-03-02) |
| Cargo | 1.94.0 (85eff7c80 2026-01-15) |
| OS | Linux (local dev machine) |
| CPU | Local development environment |
| Date | April 2026 |
| Criterion version | 0.5.1 |
| Sample size | 10 samples per benchmark |
| Measurement | release build, optimized |

### Measured Values

| Benchmark | Mean | CI Low | CI High | Unit | Notes |
|-----------|------|--------|---------|------|-------|
| `diff_latency_no_change` | 4.74 µs | 4.57 µs | 4.86 µs | µs | Identical versions |
| `diff_latency_scope_change` | 5.27 µs | 5.03 µs | 5.47 µs | µs | Scope items added/removed |
| `diff_latency_constraints_change` | 4.77 µs | 4.38 µs | 5.42 µs | µs | Functional constraints added |
| `diff_latency_acceptance_criteria_change` | 3.78 µs | 3.68 µs | 3.91 µs | µs | AC items added |
| `diff_latency_all_sections_change` | 6.09 µs | 5.49 µs | 6.57 µs | µs | All sections modified |

> **Caveats:**
> - Single measurement run (10 samples) on local dev hardware — not CI-averaged
> - Values are criterion mean estimates with 95% confidence intervals — **not** percentile statistics (p50/p95/p99)
> - No warmup variance control beyond criterion defaults
> - Single-threaded, no concurrent load
> - Production targets and load testing remain Phase 5 scope (P2 completion required)

### Raw Output

```
Benchmarking diff_latency_no_change
Benchmarking diff_latency_no_change: Warming up for 3.0000 s
Benchmarking diff_latency_no_change: Collecting 10 samples in estimated 5.0001 s (1.1M iterations)
Benchmarking diff_latency_no_change: Analyzing
diff_latency_no_change  time:   [4.5725 µs 4.7386 µs 4.8590 µs]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild

Benchmarking diff_latency_scope_change
Benchmarking diff_latency_scope_change: Warming up for 3.0000 s
Benchmarking diff_latency_scope_change: Collecting 10 samples in estimated 5.0002 s (944k iterations)
Benchmarking diff_latency_scope_change: Analyzing
diff_latency_scope_change
                        time:   [5.0263 µs 5.2732 µs 5.4679 µs]

Benchmarking diff_latency_constraints_change
Benchmarking diff_latency_constraints_change: Warming up for 3.0000 s
Benchmarking diff_latency_constraints_change: Collecting 10 samples in estimated 5.0002 s (937k iterations)
Benchmarking diff_latency_constraints_change: Analyzing
diff_latency_constraints_change
                        time:   [4.3769 µs 4.7691 µs 5.4247 µs]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) low mild

Benchmarking diff_latency_acceptance_criteria_change
Benchmarking diff_latency_acceptance_criteria_change: Warming up for 3.0000 s
Benchmarking diff_latency_acceptance_criteria_change: Collecting 10 samples in estimated 5.0001 s (1.4M iterations)
Benchmarking diff_latency_acceptance_criteria_change: Analyzing
diff_latency_acceptance_criteria_change
                        time:   [3.6819 µs 3.7771 µs 3.9050 µs]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild

Benchmarking diff_latency_all_sections_change
Benchmarking diff_latency_all_sections_change: Warming up for 3.0000 s
Benchmarking diff_latency_all_sections_change: Collecting 10 samples in estimated 5.0001 s (916k iterations)
Benchmarking diff_latency_all_sections_change: Analyzing
diff_latency_all_sections_change
                        time:   [5.4887 µs 6.0935 µs 6.5696 µs]
```

---

## Next Steps

1. **P2 completion required** before setting production targets (Mean/CI thresholds)
2. **Production load testing** (k6/Artillery) remains Phase 5 scope
3. **Performance regression gates** require P2 + observability baseline first
4. **CI-averaged benchmarks** — capture multiple CI runs to establish statistically meaningful baseline

---

## References

- Benchmark harness: `crates/rebase-engine/benches/diff_latency.rs`
- CI benchmark job: `.github/workflows/ci.yml#bench`
- Phase 3 checklist: `docs/10-delivery/checklists/checklist-phase-3.md#sre--observability`
- Proposals tracker: `docs/10-delivery/09-completion-proposals-tracker.md#p2`