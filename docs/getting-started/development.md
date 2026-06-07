# Development & Verification

> **Safety:** IRE is **not production-ready**. It is a bounded personal
> project intended for local development, integration experimentation,
> and study of the design. **Do not use it for production, sensitive, or
> customer-facing workloads** without independent validation. See
> [Status & Capabilities](../reference/status-and-capabilities.md) and
> the [Capability Support Matrix](../01-product/04-capability-support-matrix.md).

This page is the **public development reference**. It describes the
command-line loop and the split between smoke tests, heavy tests, and
manual / `#[ignore]`'d suites.

---

## TL;DR — the local loop

```bash
# Fast verify (required before every PR; ~minutes, no external services)
bash scripts/verify-fast.sh

# Conflict-marker check
git diff --check

# OpenAPI spectral lint (only if docs/04-api/openapi.yaml changed)
npx @stoplight/spectral-cli lint docs/04-api/openapi.yaml --ruleset .spectral.yml --fail-severity=error
```

If you change API surface, intent schema, graph rules, or apply-path
semantics, see [Test Strategy](../11-quality/01-test-strategy.md) for the
matching test policy.

---

## What `scripts/verify-fast.sh` actually runs

```bash
cargo fmt --all -- --check              # 1. format check
cargo check --workspace --all-features  # 2. type / dep-graph check
cargo clippy --workspace --all-features -- -D warnings  # 3. lint (deny warnings)
cargo test --workspace --lib --all-features             # 4. in-memory lib tests
```

These four checks are the **primary local source of truth**. The script does
**not** require Postgres, NATS, or any external service.

> **Caveat:** A green `verify-fast.sh` run is **not** production evidence. It
> is a bounded self-check that the code is internally consistent.

---

## Smoke vs heavy vs manual

IRE deliberately separates tests into three tiers. The split exists so the
default loop stays fast and free of external-service dependencies.

| Tier | Default? | Requires external services? | What it covers |
|------|----------|------------------------------|----------------|
| **Smoke** (`cargo test --workspace --lib --all-features`) | Yes (every PR + `verify-fast.sh`) | No | In-memory lib tests across all 11 crates. |
| **Heavy** (full test + release build + benchmarks + Docker build) | No (manual only) | Mixed | Full workspace test, release build, benchmarks, migration integration test, Docker build. |
| **Manual / `#[ignore]`'d** (`-- --ignored` suites) | No (opt-in, per-suite) | Postgres / NATS / MinIO depending on the suite | Live integration with real services, RLS, NATS JetStream, webhook, load harness. |

> **Caveat:** A green smoke run is **not** production evidence. The smoke
> workflow is intentionally narrow. See
> [Status & Capabilities](../reference/status-and-capabilities.md).

---

## Ignored tests — how and when to run them

Ignored suites are listed in
[Configuration](./configuration.md) with the exact env vars each suite
needs. The general flow is:

1. **Start the local stack**

   ```bash
   docker compose -f infrastructure/local/docker-compose.yml up -d
   ```

2. **Verify the services are healthy**

   ```bash
   docker compose -f infrastructure/local/docker-compose.yml ps
   ```

3. **Export the env var the suite needs** (see `.env.example` for the
   canonical local defaults).

4. **Run the suite with `-- --ignored`**

   ```bash
   cargo test -p intent-service --test migration_integration -- --ignored
   ```

> **Caveats:**
> - Ignored suites are **local-dev / manual evidence only** and are **not
>   production evidence**.
> - Some suites prefer a fresh database because existing ones can be stale
>   relative to the migration sequence.
> - The `#[ignore]`'d load tests are bounded local-only. Production-scale
>   load testing is not part of this project.
> - For a complete listing and the underlying rationale, see
>   [Test Strategy](../11-quality/01-test-strategy.md).

---

## Repository-specific rules

These are non-negotiable. They are also listed in
[`AGENTS.md`](../../AGENTS.md) for AI-agent contributors and in
[`CONTRIBUTING.md`](../../CONTRIBUTING.md).

1. **Intent schema changes** must update or add an ADR under
   `docs/13-adrs/` **first**.
2. **API changes** must update
   [`docs/04-api/openapi.yaml`](../04-api/openapi.yaml) **and** the
   relevant event contracts.
3. **Graph rule changes** must include tests in the same PR.
4. **Risky apply-path changes** must include replay tests.
5. **S3 / S4 side-effect auto-compensation** requires explicit approval
   before implementation.

---

If you are evaluating IRE for any sensitive, customer-facing, or production
workload, **do not** rely on it. Use it only for local development,
integration experimentation, and bounded study of the design. See
[`SECURITY.md`](../../SECURITY.md) for the full policy.
