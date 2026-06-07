# Configuration

> **Safety:** IRE is **not production-ready**. It is a bounded personal
> project intended for local development, integration experimentation,
> and study of the design. **Do not use it for production, sensitive, or
> customer-facing workloads** without independent validation. Do not use
> any setting on this page as production hardening guidance. See
> [Status & Capabilities](../reference/status-and-capabilities.md) and
> the [Capability Support Matrix](../01-product/04-capability-support-matrix.md).

IRE is configured through environment variables, with `.env.example` shipping
the **local-dev-only defaults**. The page below is the **public support
reference** for what each variable does and what is enabled by default.

---

## How configuration is loaded

IRE reads env vars directly through `std::env` and (for the local dev
helpers) through a `.env` file you create with `cp .env.example .env`. The
API server does **not** perform environment validation at startup beyond
specific guard flags — it tolerates missing optional variables and falls
back to in-memory defaults.

> **Caveat:** This means a missing secret will silently fall back to a
> dev-only stub. For local dev that is intentional; for any non-local use
> it is a bug, not a feature.

---

## Categories

The vars below are grouped by capability. Each row says what it does, what
the default is, and the **non-production caveat** the maintainer wants
visitors to remember.

### Database (Postgres)

| Variable | Default (local-dev) | Purpose | Caveat |
|----------|---------------------|---------|--------|
| `DATABASE_URL` | see `.env.example` | SQLx connection string used by the Postgres-backed repositories. | When unset, the service falls back to **in-memory repositories** for tests and dev. The fallback is dev-only. |

### Authentication (JWT)

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `JWT_SECRET` | placeholder in `.env.example` | HS256 signing key. | **Local-dev placeholder.** Replace with a strong value (`≥32` bytes, no `dev` / `secret` / `password`) before any non-local use. The placeholder is **not** a secret. |
| `INTENT_API_REQUIRE_JWT` | `false` | Strict guard. When `true`, the server fails to start if `JWT_SECRET` is missing / weak. | Kept **off by default** so the local dev loop does not require you to set a real secret. |

> **Local fallback caveat:** When `INTENT_API_REQUIRE_JWT=false` (the
> default) the API accepts requests without a token, so it is convenient for
> curl / OpenAPI smoke testing. Do not expose that instance outside
> localhost. Full authentication / authorization is **partial** — see the
> [Capability Support Matrix](../01-product/04-capability-support-matrix.md).

### Runtime adapter

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `INTENT_API_RUNTIME_ADAPTER` | `mock` | Selects the runtime adapter used for rebase operations. `mock` is the in-process dev adapter; `temporal` requires the `temporal` feature compiled in and the Temporal env vars below. | The default `mock` is dev/testing only. |
| `TEMPORAL_ADDRESS` | unset | Temporal frontend address. | Only consulted when the runtime adapter is `temporal`. |
| `TEMPORAL_NAMESPACE` | `default` | Temporal namespace. | Only consulted when the runtime adapter is `temporal`. |
| `TEMPORAL_TASK_QUEUE` | `intent-rebase` | Temporal task queue. | Only consulted when the runtime adapter is `temporal`. |
| `TEMPORAL_IDENTITY` | `intent-rebase-runtime-adapter` | Worker identity string. | Only consulted when the runtime adapter is `temporal`. |

### Event broker (NATS with JetStream)

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `NATS_URL` | `nats://localhost:4222` | NATS connection URL. | When unset, the in-memory event bus is used. |

### Object store (S3 / MinIO, local dev)

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `AWS_ACCESS_KEY_ID` | `CHANGE_ME_local_minio_access_key` | MinIO / S3 access key. | Local-dev placeholder. Replace with a real credential for any non-local use. |
| `AWS_SECRET_ACCESS_KEY` | `CHANGE_ME_local_minio_secret_key` | MinIO / S3 secret key. | Local-dev placeholder. Replace with a real credential for any non-local use. |
| `AWS_DEFAULT_REGION` | `us-east-1` | AWS region. | Local-dev default. |
| `S3_ENDPOINT` | `http://localhost:9000` | S3 / MinIO endpoint. | Local-dev default. |
| `S3_REGION` | `us-east-1` | S3 region. | Local-dev default. |
| `S3_BUCKET` | `intent-rebase-artifacts` | Default S3 bucket. | Local-dev default. |

### Forensic bundle storage

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `FORENSIC_BUNDLE_STORAGE` | unset (→ in-memory) | When set to `s3`, forensic bundles are written to S3 / MinIO. Any other value falls back to **in-memory** dev storage. | S3 wiring is bounded and not production-validated. |
| `S3_ACCESS_KEY` | unset | Explicit MinIO / S3 access key for forensic bundles. | Optional override of `AWS_ACCESS_KEY_ID`. |
| `S3_SECRET_KEY` | unset | Explicit MinIO / S3 secret key for forensic bundles. | Optional override of `AWS_SECRET_ACCESS_KEY`. |
| `FORENSIC_BUNDLE_BUCKET` | unset | Bucket for forensic bundles. | Defaults to `S3_BUCKET` when unset. |

### Observability

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTLP endpoint for traces / metrics. | Local-dev default. |
| `OTEL_SERVICE_NAME` | `intent-rebase` | OTLP `service.name` attribute. | Local-dev default. |

### CORS

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `INTENT_API_CORS_ALLOWED_ORIGINS` | `http://localhost:3000,http://127.0.0.1:3000` | Comma-separated list of allowed origins. | Local-dev defaults. Tighten to your real origin in any non-local deployment. |

### App

| Variable | Default | Purpose | Caveat |
|----------|---------|---------|--------|
| `RUST_LOG` | `info` | `tracing` / `tracing-subscriber` filter. | Standard library / app log filter. |
| `RUST_BACKTRACE` | `1` | Backtrace on panic. | Local-dev default. Set to `0` for cleaner release-mode logs. |

---

## Default-off workers

A few environment variables gate bounded workers (NATS checkpoint consumer,
DLQ metrics, DLQ replay). They look like production knobs but are
**intentionally off by default** and are **not production-validated**. They
are listed here so visitors do not mistake "the env var exists" for "we
recommend turning it on." See `.env.example` for the exact names and the
[Test Strategy](../11-quality/01-test-strategy.md) for the policy and
prerequisites.

---

## `#[ignore]`'d test suites

Several `#[ignore]`'d suites in this repo are the bridge between "the code
compiles" and "the code actually talks to Postgres / NATS / MinIO." They
are **not** part of the default loop and **not** run on every PR. They are
**local-dev / manual evidence only** and are **not production evidence**.

For the exact list, required env vars, and prerequisites per suite, see the
[Test Strategy](../11-quality/01-test-strategy.md) and the
`infrastructure/local/docker-compose.yml` stack. Generally, each suite
follows the pattern:

```bash
docker compose -f infrastructure/local/docker-compose.yml up -d
# set the env var(s) the suite needs (see .env.example for the canonical local defaults)
cargo test <suite-path> -- --ignored
```

> **Caveat:** Load-test results from these suites are bounded local-only
> runs. Production-scale load / staging / production infra are not part of
> this project.
