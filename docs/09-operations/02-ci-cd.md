# CI/CD

> **Last Updated:** May 2026

## Free-Safe CI Policy

This repo enforces a **100% free-safe CI policy** for automatic runs:

- **Automatic runs disabled** — no CI runs on push or pull_request to avoid GitHub Actions costs
- **All jobs are manual-only** via `workflow_dispatch` — user explicitly triggers when needed
- **Concurrency**: redundant runs are cancelled automatically
- **Permissions**: all workflows use `contents: read` only (no write access)

### Why Manual-Only?

Public GitHub repos get free GitHub Actions minutes, but:
- Accidental heavy resource use can trigger cost alerts or throttling
- CI is not needed for a personal project with no collaborators
- All CI jobs have direct `cargo` / `docker` equivalents for local use

The manual-only policy means **CI runs only when explicitly triggered** via the Actions tab.

---

## Workflows

### Smoke Test (`.github/workflows/smoke.yml`)

**Manual-only via `workflow_dispatch`**

Lightweight sanity check. Runs instantly, uses minimal resources. Trigger manually from the Actions tab when needed.

### CI (`.github/workflows/ci.yml`)

**Manual-only via `workflow_dispatch` with inputs**

All inputs default to `false` — no job runs unless you explicitly enable it.

| Job | Input | Notes |
|-----|-------|-------|
| Rust Format | (none — runs on trigger) | Style check |
| Clippy Lints | (none — runs on trigger) | Lint + type check |
| Cargo Check | (none — runs on trigger) | Fast type verification |
| OpenAPI Validate | (none — runs on trigger) | API contract check |
| Cargo Test | `run_tests` | Unit tests (some require NATS/DB; use local for fast iteration) |
| Cargo Build | `run_build` | Full release build |
| Benchmark | `run_bench` | Criterion benchmarks |
| Migration Integration Test | `run_test_db` | Postgres-backed integration tests |
| Docker Build | `run_docker_build` | Docker image build (no GHA cache write) |

**To trigger manual CI:**
1. Go to the **Actions** tab in GitHub
2. Select **CI** or **Smoke Test** workflow
3. Click **Run workflow**
4. For CI, enable the inputs for the jobs you want (all default to false)
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

1. **Automatic CI is disabled** — no runs on push or pull_request
2. **All jobs require manual trigger** — user explicitly triggers via workflow_dispatch
3. **Concurrency cancels redundant runs** — no wasted minutes on superseded commits
4. **Smoke workflow is manual-only** — trigger from Actions tab when needed
5. **Local equivalents exist** — all CI jobs have direct `cargo` / `docker` equivalents

---

## Related Documents

- [Current Status](../10-delivery/00-current-status.md)
- [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md)
- [Solo Ops Evidence Plan](../10-delivery/16-solo-ops-evidence-plan.md)
