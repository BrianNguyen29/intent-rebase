# Intent Rebase Engine

> **Rebase in-flight agent work when the intent changes — instead of letting it drift on a stale promise.**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust: stable](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)
[![Docs](https://img.shields.io/badge/Docs-docs%2FREADME.md-blueviolet.svg)](docs/README.md)
[![Status: not production-ready](https://img.shields.io/badge/Status-not%20production--ready-critical.svg)](#safety)

[English](README.md) · [Tiếng Việt](README.vi.md)

[Quickstart](#quickstart) ·
[Documentation](#documentation) ·
[Architecture](#architecture) ·
[API](#api) ·
[Configuration](#configuration) ·
[Contributing](#contributing-security-and-support)

---

## What is Intent Rebase Engine?

**Intent Rebase Engine (IRE)** is a Rust control-plane layer for *intent change* in agent workflows. It versions the user's intent, computes a semantic diff between versions, models downstream impact, and **rebases** in-flight executions, approvals, and side effects onto the new intent — rather than resetting progress, ignoring the change, or letting the agent keep producing under a contradicted promise.

It is built for any system where humans and agents iterate together on a shared goal: coding copilots, support automation, research workflows, and policy-driven agents whose work product must remain consistent with the latest intent.

## Highlights

- **Versioned intent** — every meaningful change produces a new, immutable intent version with full lineage.
- **Semantic diff** — capture *what* changed and *what it means*, not just a textual patch.
- **Dependency graph** — link each intent clause to its artifacts, approvals, and side effects.
- **Impact-aware rebase** — classify invalidations, required reviews, and compensations automatically.
- **Repair planning** — generate a rebased execution plan, not a restart from zero.
- **Provenance by default** — every output traces back to the intent version that produced it.
- **Multi-tenant, audit-first** — tenant-scoped reads/writes, RLS wiring, and a forensic bundle for replay.
- **Bounded operator surface** — REST + OpenAPI, an operator CLI, and a runtime-adapter seam for the workflow engine of your choice.

## How it works

1. **Normalize** the intent into a versioned, validated structure.
2. **Diff** the new intent against the prior version semantically.
3. **Graph** the dependency between the intent and its artifacts, executions, and side effects.
4. **Classify** impact — invalidations, reviews required, compensations.
5. **Plan & trace** — emit a rebased execution plan and record provenance for every output.

## When IRE helps

| Scenario | What IRE does |
| --- | --- |
| Coding copilot's spec changes mid-implementation | Replays the plan, invalidates stale patches, and revalidates approvals under the new intent. |
| Support workflow's policy is updated | Rebuilds the dependency graph, marks affected cases, and proposes compensations. |
| Research workflow's budget shrinks | Reclassifies downstream tasks, surfaces review-required artifacts, and proposes a smaller plan. |
| Deployment freeze hits a running batch | Captures side effects, freezes apply, and produces a forensic bundle for later review. |

---

## Quickstart

> **Prerequisites.** A recent stable **Rust** toolchain (pinned via `rust-toolchain.toml`, installed through [rustup](https://rustup.rs)), **Git**, and optionally **Docker** + **Docker Compose v2** for the local Postgres / NATS / MinIO stack. **Node.js 20+** is only needed to run the OpenAPI spectral lint.

### 1. Clone and configure

```bash
git clone https://github.com/BrianNguyen29/intent-rebase.git
cd intent-rebase
cp .env.example .env       # local-dev defaults only — see Configuration below
```

`.env.example` ships with **local-dev-only placeholders** for `DATABASE_URL`, `JWT_SECRET`, and the S3/MinIO keys. Replace them with real values before any non-local use; see [Configuration](docs/getting-started/configuration.md) for the full reference.

### 2. Fast verify (no external services)

```bash
bash scripts/verify-fast.sh
# equivalent to:
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --lib --all-features
```

The fast loop is fully in-memory and is the **primary local source of truth** for the project.

### 3. Optional — local stack for live-integration tests

```bash
docker compose -f infrastructure/local/docker-compose.yml up -d
```

This brings up **Postgres 16**, **NATS 2.10 with JetStream**, and **MinIO**. Set the env vars the suite needs (see [`.env.example`](.env.example) and [Configuration](docs/getting-started/configuration.md)) and run the `#[ignore]`'d suites explicitly with `cargo test … -- --ignored`.

### 4. Run the API

```bash
cargo run -p intent-api
```

Smoke-check the running server:

```bash
curl -s http://localhost:8080/health
```

The default config uses in-memory repositories where possible. Set `DATABASE_URL` (and, optionally, `NATS_URL` / S3 env vars) to exercise the SQL-backed, NATS-backed, or S3-backed paths.

> **Heads-up.** A green fast-verify run is *not* a production-readiness signal. See [Safety](#safety).

---

## Architecture

IRE is a Cargo workspace of **11 crates** organized into four **planes**.

### Planes at a glance

| Plane | Responsibility |
| --- | --- |
| **Control** | Intent ingestion, versioning, semantic diff, impact analysis, repair planning, policy-aware decisions, audit. |
| **Execution** | Runtime adapters to workflow engines, agent runtimes, task schedulers, and side-effect dispatchers. |
| **Data** | OLTP metadata, event log, object store, graph store / relational edges, analytics store. |
| **Operator** | Console, approval UI, forensic replay, policy simulation, rebase previews. |

### Workspace crates

| Crate | Purpose |
| --- | --- |
| `intent-rebase-types` | Core type definitions and shared domain models. |
| `intent-service` | Intent persistence, semantic diff, and lifecycle (Postgres). |
| `intent-api` | HTTP API server (Axum) and middleware stack. |
| `rebase-engine` | Rebase decision engine — diff, impact, plan generation. |
| `graph-service` | Dependency graph service — nodes, edges, traversals. |
| `runtime-adapter` | Runtime execution adapter (mock by default; Temporal bounded). |
| `rebase-orchestrator` | Orchestration coordination, dry-run, single-shot runtime. |
| `compensation-service` | Compensation action lifecycle, executors, batch operations. |
| `forensic-service` | Forensic bundle generation, verification, export. |
| `tenant-service` | Multi-tenant onboarding, quota, and rule-pack isolation. |
| `intent-cli` | Operator CLI for orchestration runs and inspection. |

For the full component map, see the [System Overview](docs/02-architecture/01-system-overview.md) and the [Component Catalog](docs/02-architecture/02-components.md).

## API

- **REST + OpenAPI** — [`docs/04-api/openapi.yaml`](docs/04-api/openapi.yaml) is the canonical endpoint reference, paired with the [REST API notes](docs/04-api/01-rest-api.md).
- **Events** — see the [event contracts](docs/04-api/02-events.md) for topic and payload shapes.
- **Webhooks** — see the [webhook contract](docs/04-api/03-webhooks.md) for delivery, signing, and retry semantics.

## Configuration

IRE is configured entirely through environment variables; `.env.example` ships the local-dev defaults. See [Configuration](docs/getting-started/configuration.md) for the full reference — database, JWT, runtime adapter, NATS, S3/MinIO, forensic bundle, OpenTelemetry, CORS, and the `#[ignore]`'d test suites.

## Documentation

The full public documentation hub is at **[docs/README.md](docs/README.md)**.

| Topic | Document |
| --- | --- |
| Quickstart | [Quickstart](docs/getting-started/quickstart.md) |
| Configuration | [Configuration](docs/getting-started/configuration.md) |
| Development & verification | [Development & Verification](docs/getting-started/development.md) |
| System overview | [System Overview](docs/02-architecture/01-system-overview.md) |
| Components | [Component Catalog](docs/02-architecture/02-components.md) |
| OpenAPI spec (canonical) | [openapi.yaml](docs/04-api/openapi.yaml) |
| REST API notes | [REST API](docs/04-api/01-rest-api.md) |
| Events | [Events](docs/04-api/02-events.md) |
| Webhooks | [Webhooks](docs/04-api/03-webhooks.md) |
| Intent model | [Intent Model](docs/03-spec/01-intent-model.md) |
| Semantic diff | [Semantic Diff](docs/03-spec/02-semantic-diff.md) |
| Dependency graph | [Dependency Graph](docs/03-spec/03-dependency-graph.md) |
| Rebase engine | [Rebase Engine](docs/03-spec/04-rebase-engine.md) |
| Test strategy | [Test Strategy](docs/11-quality/01-test-strategy.md) |
| ADR pack | [ADR Index](docs/13-adrs/README.md) |
| Glossary | [Glossary](docs/01-product/05-glossary.md) |
| Rationale & external patterns | [Rationale & External Patterns](docs/99-reference/01-rationale-and-external-patterns.md) |

---

## Contributing, Security, and Support

- **Contributing** — see [CONTRIBUTING.md](CONTRIBUTING.md). Start with the local verification loop, follow the no-overclaim policy, and read the repository-specific rules in there.
- **Security** — see [SECURITY.md](SECURITY.md). IRE has not had external SRE, security, or penetration-testing sign-off; please report issues privately.
- **Support** — see [`.github/SUPPORT.md`](.github/SUPPORT.md). Best-effort, bounded by the project's scope; there is **no SLA**.
- **Code of Conduct** — see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- **Issues & PRs** — use the [bug report](.github/ISSUE_TEMPLATE/bug_report.md) and [feature request](.github/ISSUE_TEMPLATE/feature_request.md) templates, and the [PR template](.github/PULL_REQUEST_TEMPLATE.md).

## Safety

IRE is **not production-ready** and is not validated for production, sensitive, or customer-facing workloads. Use it only for local development, integration experimentation, and bounded study of the design. Do not treat a green local verification run as a production-readiness signal, and do not rely on any setting, command, or example on this site as production hardening guidance.

## License

Copyright © Intent Rebase Engine Team.

Licensed under the **Apache License, Version 2.0** (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at <https://www.apache.org/licenses/LICENSE-2.0>.

Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the [LICENSE](LICENSE) file for the full text.
