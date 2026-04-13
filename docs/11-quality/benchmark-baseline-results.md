# Benchmark Baseline Results

> **Template:** Fill in after running `cargo bench -p rebase-engine` locally or via CI benchmark job.
> **Status:** This document captures infrastructure baseline only — actual performance targets are gated on P2 completion.

---

## Benchmark Harness Verification

| Check | Status |
|-------|--------|
| `cargo bench -p rebase-engine` runs successfully | ⬜ |
| Criterion HTML reports generated in `target/criterion/` | ⬜ |
| CI benchmark job passes (builds + runs) | ⬜ |
| Benchmark artifacts uploaded to CI | ⬜ |

---

## Diff Latency Benchmarks (Bounded Slice — Infrastructure Only)

These benchmarks measure `compute_diff_sync` latency using criterion. They verify the harness
builds and runs in principle. **Production performance targets remain outstanding.**

| Benchmark | p50 | p95 | p99 | Unit | Notes |
|-----------|-----|-----|-----|------|-------|
| `diff_latency_no_change` | TBD | TBD | TBD | ns/μs/ms | Identical versions |
| `diff_latency_scope_change` | TBD | TBD | TBD | ns/μs/ms | Scope items added/removed |
| `diff_latency_constraints_change` | TBD | TBD | TBD | ns/μs/ms | Functional constraints added |
| `diff_latency_acceptance_criteria_change` | TBD | TBD | TBD | ns/μs/ms | AC items added |
| `diff_latency_all_sections_change` | TBD | TBD | TBD | ns/μs/ms | All sections modified |

---

## Environment

| Field | Value |
|-------|-------|
| Rust toolchain | stable |
| OS | ubuntu-latest (CI) |
| CPU | Standard GitHub Actions runner |
| Date | TBD |

---

## Next Steps

1. **P2 completion required** before setting production targets (p50/p95/p99 thresholds)
2. **Production load testing** (k6/Artillery) remains Phase 5 scope
3. **Performance regression gates** require P2 + observability baseline first

---

## References

- Benchmark harness: `crates/rebase-engine/benches/diff_latency.rs`
- CI benchmark job: `.github/workflows/ci.yml#bench`
- Phase 3 checklist: `docs/10-delivery/checklists/checklist-phase-3.md#sre--observability`
- Proposals tracker: `docs/10-delivery/09-completion-proposals-tracker.md#p2`
