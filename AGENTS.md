# AGENTS.md — Intent Rebase Engine

> Compact repo-specific guide for future OpenCode sessions.
> Every line answers: “would an agent likely miss this?”

## Workspace
- Root: `/home/uong_guyen/work/intent-rebase`
- 11 crates under `crates/*`, resolver 2, Rust stable, edition 2021
- 2 binaries: `intent-api` (HTTP server), `intent-cli` (CLI); rest are libraries

## Verified Commands

| Intent | Command |
|--------|---------|
| Fast local verification (no services) | `scripts/verify-fast.sh` or `just verify-fast` |
| Format | `cargo fmt --all -- --check` |
| Type check | `cargo check --workspace --all-features` |
| Lint (CI style) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Lint (local fast) | `cargo clippy --workspace --all-features -- -D warnings` |
| Lib tests (fast) | `cargo test --workspace --lib --all-features` |
| Full tests (needs services) | `cargo test --workspace --all-features` |
| Bench compile check | `cargo bench --workspace --no-run` |
| Release build | `cargo build --workspace --release` |
| OpenAPI lint | `npx --yes @stoplight/spectral-cli lint docs/04-api/openapi.yaml --ruleset .spectral.yml --fail-severity=error` |

**Note:** CI `clippy` uses `--all-targets`; local `verify-fast` uses `--all-features`. If in doubt, run the project script.

## Integration / Live Tests
- Services: `docker compose -f infrastructure/local/docker-compose.yml up -d` → Postgres 16, NATS JetStream, MinIO
- Copy `.env.example` to `.env`; placeholders only, never commit real values.
- RLS tests: fresh DB + `--test-threads=1` required
- NATS ignored test: `NATS_URL=nats://localhost:4222` + `-- --ignored`
- Webhook / migration ignored tests: fresh DB + `DATABASE_URL=...` + `-- --ignored`
- Load test: requires `load-test` feature flag
- **Do not claim integration completeness from lib tests alone.**

## Feature / Env Quirks
- `intent-api` default feature includes `jwt-auth`; `temporal` is opt-in
- `INTENT_API_REQUIRE_JWT=true` enforces JWT_SECRET presence/strength
- `JWT_SECRET_PREVIOUS` supports dual-key grace window (validated 2026-06-21; grace window closed 2026-06-22)
- NATS / webhook / forensic S3 paths are env-gated; see `.env.example`

## CI / Workflows
- `smoke.yml`: PR + dispatch, free-tier-safe fmt/check/clippy/lib tests
- `ci.yml`: manual-only (`workflow_dispatch`) full CI; includes fmt/clippy/check/openapi plus manual-gated heavy tests/build/bench/docker. Internal comments reference auto lightweight jobs, but trigger is manual-only to avoid costs.
- `audit-trail.yml`: manual-only quality + SBOM artifact; signing deferred (no registry push, no OIDC)
- **Local verification is source of truth for this solo project.** Do not over-rely on remote CI.

## Repo Change Constraints
1. Intent schema changes → ADR first (`docs/13-adrs/`)
2. API changes → OpenAPI spec (`docs/04-api/openapi.yaml`) + event contracts
3. Graph rule changes → include tests
4. Risky apply-path changes → include replay tests
5. S3/S4 side-effect auto-compensation → explicit approval required

## Infra / Ops Gotchas (Private-Only)
- **No public ingress.** GCP/GKE/GSM/ESO stack is private-only. Do not enable or claim public production without A-03/A-04/A-07 artifacts.
- GCS forensic bucket: versioning, 30-day retention, public access prevention enforced. Bucket Lock deferred (unlocked). No S3 Object Lock compliance. App-layer hash detection works; storage-layer immutability is not fully enforced.
- NATS JetStream pilot: single-node, internal ClusterIP, token auth enforced (shell substitution workaround for NATS config env var limitation), no TLS/HA. App consumer enabled via env gates.
- JWT and DB URL rotations validated 2026-06-21; `JWT_SECRET_PREVIOUS` grace window closed 2026-06-22.
- A-07: waived / not approved (no external pen test). A-03/A-04: private-only conditional only. **No production-ready claim.**

## Key Docs
- `docs/README.md` — reading order
- `docs/02-architecture/01-system-overview.md` — architecture
- `docs/11-quality/01-test-strategy.md` — test approach
- `docs/getting-started/configuration.md` — env/feature flags
- `docs/12-agents/01-agent-implementation-guide.md` — agent rules, workstreams, DOD
- `docs/10-delivery/26-post-signoff-execution-plan.md` — current execution state
- `docs/09-operations/12-authorization-signoff-packet.md` — gate statuses
