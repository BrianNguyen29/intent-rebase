# CI/CD

> **Last Updated:** May 2026

## Free-Safe CI Policy

This repo enforces a **100% free-safe CI policy** for automatic runs:

- **Automatic runs** (push to `main`, pull requests): **lightweight jobs only** — no paid-resource risk
- **Heavy jobs** (build, benchmarks, DB integration tests, Docker builds): **manual-only** via `workflow_dispatch`
- **Concurrency**: redundant runs are cancelled automatically
- **Permissions**: all workflows use `contents: read` only (no write access)

### Why Free-Safe?

Public GitHub repos get free GitHub Actions minutes, but:
- Accidental heavy resource use can trigger cost alerts or throttling
- Unnecessary compute on every PR wastes resources
- Some jobs (benchmarks, Docker builds) have no value in fast feedback loops

The free-safe policy ensures **every push/PR gets fast, cheap checks** while **heavy jobs run only on explicit request**.

---

## Workflows

### Smoke Test (`.github/workflows/smoke.yml`)

**Automatic on push to `main` / manual via `workflow_dispatch`**

Lightweight sanity check only. Runs instantly, uses minimal resources.

### CI (`.github/workflows/ci.yml`)

**Split into two tiers:**

#### Automatic Tier (runs on every push/PR to `main`)

| Job | Command | Notes |
|-----|---------|-------|
| Rust Format | `cargo fmt --all -- --check` | Style check |
| Clippy Lints | `cargo clippy --workspace --all-targets -- -D warnings` | Lint + type check |
| Cargo Check | `cargo check --workspace` | Fast type verification |
| OpenAPI Validate | `npx spectral lint docs/04-api/openapi.yaml` | API contract check |

**Total automatic runtime:** ~3-5 minutes (parallel jobs)

#### Manual-Only Tier (requires `workflow_dispatch` with inputs)

All five inputs default to `false` — no heavy job runs unless you explicitly enable it.

| Job | Input | Notes |
|-----|-------|-------|
| Cargo Test | `run_tests` | Unit tests (some require NATS/DB; use local for fast iteration) |
| Cargo Build | `run_build` | Full release build |
| Benchmark | `run_bench` | Criterion benchmarks |
| Migration Integration Test | `run_test_db` | Postgres-backed integration tests |
| Docker Build | `run_docker_build` | Docker image build (no GHA cache write) |

**To trigger manual heavy CI:**
1. Go to the **Actions** tab in GitHub
2. Select **CI** workflow
3. Click **Run workflow**
4. Enable the inputs for the heavy jobs you want (all default to false)
5. Click **Run workflow**

### Running Heavy Jobs Locally

Equivalent local commands for heavy jobs:

```bash
# Full release build
cargo build --workspace --release

# Benchmarks
cargo bench -p rebase-engine

# Migration integration test (requires Postgres)
cargo test -p intent-service --test migration_integration -- --ignored

# Docker build (requires Docker)
docker build -t intent-api:latest .
```

---

## CI/CD Truths

1. **Free-safe policy enforced** — automatic CI is lightweight only
2. **Heavy jobs require manual trigger** — no accidental resource use
3. **Concurrency cancels redundant runs** — no wasted minutes on superseded commits
4. **Smoke workflow is lightweight** — runs in seconds on every push
5. **Local equivalents exist** — all CI jobs have direct `cargo` / `docker` equivalents

---

## Related Documents

- [Current Status](../10-delivery/00-current-status.md)
- [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md)
- [Solo Ops Evidence Plan](../10-delivery/16-solo-ops-evidence-plan.md)
