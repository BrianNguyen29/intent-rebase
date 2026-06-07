# Contributing to Intent Rebase Engine

Thanks for your interest in **Intent Rebase Engine (IRE)**. IRE is a personal,
bounded-slices project — the rules below are intentionally light and are meant
to keep the codebase and its public claims honest.

> **Read this first.** IRE is **not production-ready** and has had no
> external SRE / security sign-off, no production-scale load testing,
> and no penetration testing. Please do not contribute code, docs, or
> claims that imply otherwise. See
> [Status & Capabilities](docs/reference/status-and-capabilities.md) and
> the [Capability Support Matrix](docs/01-product/04-capability-support-matrix.md).

---

## Table of contents

- [Code of conduct](#code-of-conduct)
- [Project status & no-overclaim policy](#project-status--no-overclaim-policy)
- [Local setup](#local-setup)
- [Branching & PR expectations](#branching--pr-expectations)
- [Verification you must run locally](#verification-you-must-run-locally)
- [Documentation expectations](#documentation-expectations)
- [Repository-specific rules](#repository-specific-rules)
- [Reporting bugs & requesting features](#reporting-bugs--requesting-features)
- [Deeper contributor guidance](#deeper-contributor-guidance)

---

## Code of conduct

Everyone interacting with this project is expected to follow the
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Project status & no-overclaim policy

IRE is a Cargo workspace (Rust, stable, edition 2021) implementing a control
plane for intent change in agent workflows. It is delivered as **bounded
slices**, with **local verification** as the primary source of truth.

When contributing:

- **Do not** add badges, status indicators, or language that implies CI is
  green, the project is production-ready, or that external SRE / security
  review, load testing, or pen testing has been performed. They have not.
- **Do not** claim that external SRE / security sign-off, production-scale
  load testing, or penetration testing has been performed. None has been.
  See [Status & Capabilities](docs/reference/status-and-capabilities.md).
- **Do not** add fake production claims to docs, READMEs, comments, or commit
  messages. If you find pre-existing overclaims, open a PR to fix them and
  reference this policy in the PR body.
- **Do** keep "bounded" / "non-production" / "local-dev only" wording
  wherever it currently appears.

## Local setup

```bash
git clone https://github.com/BrianNguyen29/intent-rebase.git
cd intent-rebase
cp .env.example .env       # local-dev defaults only
```

Prerequisites:

- **Rust** (stable) — pinned via `rust-toolchain.toml`. Install via [rustup](https://rustup.rs).
- **Docker** + **Docker Compose** v2 — only for `#[ignore]`'d integration tests
  and the `observability` compose profile.
- **Node.js 20+** with `npx` — only for the OpenAPI spectral lint.

## Branching & PR expectations

- Branch off `main`. Use a short, descriptive branch name, e.g.
  `docs/readme-refresh`, `fix/rls-tenant-guard`, `feat/compensation-batch`.
- Keep PRs small and focused. One logical change per PR.
- Before opening a PR:
  1. Run the [local verification commands](#verification-you-must-run-locally).
  2. Read the [repository-specific rules](#repository-specific-rules) below —
     they apply to schema, API, graph, and apply-path changes.
  3. Fill in the [PR template](.github/PULL_REQUEST_TEMPLATE.md) completely,
     including the local-verification checklist and any "no production claim"
     confirmation.
- Commits should reference an issue or ADR where applicable.

## Verification you must run locally

Local verification is the source of truth. The GitHub Actions workflows are
narrow by design and are **not** a green-build guarantee.

```bash
# Fast verify — required for every PR
bash scripts/verify-fast.sh

# Conflict-marker check
git diff --check
```

The `verify-fast.sh` script runs:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-features`
- `cargo clippy --workspace --all-features -- -D warnings`
- `cargo test --workspace --lib --all-features`

If your change touches API, OpenAPI, or live-integration surfaces, also run the
relevant commands from the
[Verification](docs/11-quality/01-test-strategy.md) doc.

## Documentation expectations

IRE keeps documentation close to the code. When you change behavior, update
the matching docs in the same PR:

- **API changes** → update [`docs/04-api/openapi.yaml`](docs/04-api/openapi.yaml)
  **and** any affected event contract. See
  [REST API notes](docs/04-api/01-rest-api.md).
- **Intent schema changes** → update or add an ADR in
  [`docs/13-adrs/`](docs/13-adrs/README.md) **first**.
- **Graph rule changes** → include tests in the same PR.
- **Risky apply-path changes** → include replay tests.
- **Status / capability changes** → update
  [Status & Capabilities](docs/reference/status-and-capabilities.md) and the
  [Capability Support Matrix](docs/01-product/04-capability-support-matrix.md).

## Repository-specific rules

These are non-negotiable. They are also listed in
[AGENTS.md](AGENTS.md) for AI-agent contributors.

1. **Intent schema changes** must update an ADR first (see `docs/13-adrs/`).
2. **API changes** must update the OpenAPI spec and event contracts.
3. **Graph rule changes** must include tests.
4. **Risky apply-path changes** must include replay tests.
5. **S3/S4 side-effect auto-compensation** requires explicit approval before
   implementation.

## Reporting bugs & requesting features

- **Bugs** — use the
  [bug report template](.github/ISSUE_TEMPLATE/bug_report.md). Include
  environment, the exact local verification command you ran, and whether the
  behavior is local-dev only or claims a bounded production path.
- **Feature requests** — use the
  [feature request template](.github/ISSUE_TEMPLATE/feature_request.md).
  Please tie the request to an ADR or capability area where possible.

## Deeper contributor guidance

For the full contributor / AI-agent implementation guide, workstream
rules, and definition of done, see [AGENTS.md](AGENTS.md).

For security disclosures, see [SECURITY.md](SECURITY.md).
For support, see [.github/SUPPORT.md](.github/SUPPORT.md).
