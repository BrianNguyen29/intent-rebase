# AGENTS.md — Intent Rebase Engine

## Workspace Root
- **Real workspace root**: `/home/uong_guyen/work/intent-rebase/intent-rebase`
- The outer `/home/uong_guyen/work/intent-rebase` is a shell containing only an orphan `crates/` dir; all real project files are inside `intent-rebase/`.

## Build & Test Commands
```bash
cargo fmt --all -- --check          # format check
cargo clippy --workspace --all-targets -- -D warnings  # lint (fails on warnings)
cargo check --workspace             # type/check only
cargo test --workspace              # run all tests (inline in lib.rs, use tokio::test + in-memory mocks)
cargo build --workspace --release   # release build
```
- **No Makefile/justfile**: use `cargo` directly.
- CI also runs: `spectral lint docs/04-api/openapi.yaml --fail-on-severity=error` after tests.

## Local Services
```bash
docker compose -f infrastructure/local/docker-compose.yml up -d
```
- Services: **Postgres 16**, **NATS with JetStream**, **MinIO** (S3-compatible).
- Copy `.env.example` to `.env` and update credentials for local dev.

## Crates (7 members)
`intent-rebase-types`, `intent-service`, `intent-api`, `rebase-engine`, `graph-service`, `runtime-adapter`, `forensic-service`

## Repo-Specific Change Constraints
1. **Intent schema changes** → must update ADR first (see `docs/13-adrs/`).
2. **API changes** → must update OpenAPI spec (`docs/04-api/openapi.yaml`) AND event contracts.
3. **Graph rule changes** → must include tests.
4. **Risky apply-path changes** → must include replay tests.
5. **S3/S4 side-effect auto-compensation** → requires explicit approval before implementation.

## Key Docs to Read First
- `docs/12-agents/01-agent-implementation-guide.md` — agent rules, workstreams, definition of done.
- `docs/README.md` — recommended reading order for the full documentation set.
- `docs/02-architecture/01-system-overview.md` — system architecture.
- `docs/11-quality/01-test-strategy.md` — test approach.

## Architecture Planes
Control, Execution, Data, Operator. See `docs/02-architecture/` for details.

## Rust Toolchain
- Stable (from `dtolnay/rust-toolchain@stable` in CI).
- Edition 2021.
