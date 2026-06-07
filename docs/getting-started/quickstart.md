# Quickstart

> **Safety:** IRE is **not production-ready**. It is a bounded personal
> project intended for local development, integration experimentation,
> and study of the design. **Do not use it for production, sensitive, or
> customer-facing workloads** without independent validation. See
> [Status & Capabilities](../reference/status-and-capabilities.md) and
> the [Capability Support Matrix](../01-product/04-capability-support-matrix.md).

This page walks a new visitor from a clean clone to a running local API in
roughly five minutes. For deeper reference material, see the linked docs
at the end of this page.

---

## 1. Prerequisites

| Tool | Version | Why |
|------|---------|-----|
| **Rust** | stable (pinned via `rust-toolchain.toml`) | Compiles the workspace |
| **Cargo** | bundled with rustup | Build, test, lint |
| **Git** | any recent | Clone |
| **Docker** + **Docker Compose v2** | any recent | Local Postgres / NATS / MinIO stack (optional for the fast loop) |
| **Node.js** | 20+ with `npx` | OpenAPI spectral lint (only if you edit `docs/04-api/openapi.yaml`) |
| **OpenSSL** (or `libssl-dev`) | system | Some crate dependencies |

Install Rust via [rustup](https://rustup.rs); the workspace already pins the
toolchain through `rust-toolchain.toml`, so `rustup` will pick it up
automatically on first `cargo` invocation.

> **Optional for the default loop:** You only need Docker if you want to run
> the `#[ignore]`'d live-integration tests (Postgres, NATS, MinIO). The fast
> verify loop is fully in-memory and dependency-free.

---

## 2. Clone and configure

```bash
git clone https://github.com/BrianNguyen29/intent-rebase.git
cd intent-rebase
cp .env.example .env       # local-dev defaults only
```

`.env.example` ships with **local-dev-only** placeholders. Replace
`JWT_SECRET`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and any S3 / NATS
secrets with real values before any non-local use. See
[Configuration](./configuration.md) for the full env-var reference.

---

## 3. Fast verify (no external services)

The fast verify loop is the **primary local source of truth** for this
project. It runs fmt, type check, clippy, and the in-memory lib tests in
sequence:

```bash
bash scripts/verify-fast.sh
```

`scripts/verify-fast.sh` is equivalent to:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --lib --all-features
```

> **Caveat:** A green fast-verify run is **not** a production-readiness
> signal. See [Status & Capabilities](../reference/status-and-capabilities.md)
> for the full safety note.

---

## 4. Optional: full local stack for live-integration tests

The local stack brings up **Postgres 16**, **NATS 2.10 with JetStream**, and
**MinIO** (S3-compatible).

```bash
# Core stack (Postgres + NATS + MinIO)
docker compose -f infrastructure/local/docker-compose.yml up -d
```

Once the stack is up, you can run the opt-in `#[ignore]`'d suites. Set the
required env vars (see [Configuration](./configuration.md) for the full
list) and pass `-- --ignored` to the relevant `cargo test` command.

`#[ignore]`'d tests are **not** part of the default loop — they require real
infrastructure and are deliberately opt-in. Their results are **local-dev /
manual evidence only** and are **not production evidence**. See
[Development & Verification](./development.md) for the full policy and the
[Test Strategy](../11-quality/01-test-strategy.md) for the underlying rationale.

---

## 5. Run the API

```bash
cargo run -p intent-api
```

The default config uses in-memory repositories where possible. Set
`DATABASE_URL` (and, optionally, `NATS_URL` / S3 env vars per `.env.example`)
to exercise the SQL-backed, NATS-backed, or S3-backed paths.

Smoke-check the running server:

```bash
# health endpoint
curl -s http://localhost:8080/health
```

For the full REST surface and request/response shapes, use
[`docs/04-api/openapi.yaml`](../04-api/openapi.yaml) as the canonical source,
together with the [REST API notes](../04-api/01-rest-api.md).

---

## Caveats

- **Non-production only.** IRE is a bounded personal project. Do not use it
  for production, sensitive, or customer-facing workloads without
  independent validation. See
  [Status & Capabilities](../reference/status-and-capabilities.md).
- **Local-dev secrets.** `.env.example` placeholders are **not** secrets.
  Replace them with real values before any non-local use.
- **No live-integration by default.** Most `#[ignore]`'d suites need a real
  Postgres / NATS / MinIO. Do not treat a green local stack as production
  evidence.
- **API paths are not all versioned.** The current implementation contains a
  mix of legacy `/v1/...` and newer non-prefixed routes. Treat
  [`openapi.yaml`](../04-api/openapi.yaml) as the source of truth for live
  paths.
- **Boundaries are personal.** This is a personal-project repository; there
  is **no SLA** and **no on-call**. See
  [Support](../README.md#contributing-security-and-support) and
  [`SECURITY.md`](../../SECURITY.md).

---

## Where to go next

- [Configuration](./configuration.md) — env vars and the `#[ignore]`'d
  test suites.
- [Development & Verification](./development.md) — local commands and the
  smoke / heavy / manual split.
- [Status & Capabilities](../reference/status-and-capabilities.md) — concise
  non-production status and pointers to the capability matrix.
- [System Overview](../02-architecture/01-system-overview.md) — high-level
  architecture and planes.
- [REST API](../04-api/01-rest-api.md) and
  [OpenAPI spec](../04-api/openapi.yaml) — the canonical API surface.
- [Capability Support Matrix](../01-product/04-capability-support-matrix.md) —
  bounded support vs. production status per capability.
- [Test Strategy](../11-quality/01-test-strategy.md) — how to run the local
  verification commands and the `#[ignore]` policy.
- [ADR Pack](../13-adrs/README.md) — the architectural decisions driving the
  design.
- [Glossary](../01-product/05-glossary.md) — domain vocabulary.
- [Rationale and external patterns](../99-reference/01-rationale-and-external-patterns.md)
  — where the design ideas come from.
- [Contributing](../../CONTRIBUTING.md),
  [Security](../../SECURITY.md),
  [Support](../../.github/SUPPORT.md).
