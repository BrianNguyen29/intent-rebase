# Test Strategy

## Test pyramid

### Unit tests
- diff rules
- graph propagation rules
- rebase classifier
- approval revalidation logic
- side effect class mapping

### Integration tests
- API + DB
- event flows
- adapter contracts
- object store + metadata consistency

### Scenario tests
- coding agent rebase cases
- support workflow policy change
- research workflow budget change
- deployment freeze case

### Replay tests
- historical event streams replay under new code/rules
- validate compatibility and deterministic control decisions

### Chaos / resilience
- queue failures
- adapter partial outages
- audit sink degradation
- duplicate webhooks

## Quality gates
- contract tests pass
- replay tests pass before prod deploys
- no critical security findings
- operator workflows manually validated

## Local verification

Use `scripts/verify-fast.sh` for rapid pre-commit validation. It runs:
- `cargo fmt --all -- --check`
- `cargo check --workspace --all-features`
- `cargo clippy --workspace --all-features -- -D warnings`
- `cargo test --workspace --lib --all-features`

This script does NOT require Postgres, NATS, or any external services.

## Test extraction patterns

### Handler test extraction
When a handler file grows large enough to contain substantial inline `#[cfg(test)] mod tests` blocks, extract them to a sibling file named `{module}_tests.rs` and register it in `lib.rs` with `#[cfg(test)] mod {module}_tests;`. Preserves feature-gating semantics (e.g., `#[cfg(all(test, feature = "jwt-auth"))]`) exactly.

Examples:
- `auth.rs` inline tests → `auth_tests.rs`
- `error_response.rs` inline tests → `error_response_tests.rs`
- `panic_hardening.rs` inline tests → `panic_hardening_tests.rs`

### NATS sibling test module pattern
The `nats_jetstream.rs` module was decomposed into submodules (`dlq.rs`, `stream.rs`, `consumer.rs`, etc.) and its test blocks were relocated to sibling files under `nats_jetstream/`:
- `tests_unit.rs` — in-memory unit tests (no live NATS required)
- `tests_lifecycle.rs` — in-memory lifecycle tests (no live NATS required)
- `tests_live_integration.rs` — live NATS integration tests (all `#[ignore]` by default)

## Live integration test policy

Tests that require external services (NATS with JetStream, Postgres, etc.) are marked `#[ignore]` and are NOT run by default. Run them explicitly when the service is available:

```bash
# Run ignored NATS live integration tests
cargo test -p intent-api --lib nats_jetstream -- --ignored

# Run ignored tests that require Postgres
cargo test -p intent-api -- --ignored
```

This policy keeps `cargo test --workspace --lib --all-features` fast and free of external-service dependencies.

## Deferred audits (P2 / future)
- Benchmark integration (`cargo bench` in `verify-fast.sh` or CI) — no benchmarks exist yet
- Dev-experience verification (optional `justfile` alternative to `verify-fast.sh`)
- Cross-link consistency between test strategy and delivery docs
- Router Stage 2 route-group split evaluation (deferred to P2)

> **Non-production caveat:** This test strategy supports bounded non-production feature delivery. Full production readiness requires additional external gates (SRE, Security, load/pen testing) tracked in P3 and explicitly NOT claimed here.
