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
# Run ignored NATS live integration tests (requires live NATS with JetStream)
export NATS_URL=nats://localhost:4222
cargo test -p intent-api --lib nats_jetstream -- --ignored

# Run ignored RLS integration tests (requires local Postgres via docker-compose)
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase
cargo test --test rls_integration -- --ignored

# Run ignored SQLx repository smoke tests (requires local Postgres)
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase
cargo test -p intent-api --lib sqlx_repo_smoke -- --ignored

# Run ignored webhook integration test (requires local Postgres + in-process HTTP receiver)
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix
cargo test -p intent-api --test webhook_integration -- --ignored

# Run ignored I3 JWT→RLS→DML integration test (requires local Postgres + jwt-auth feature)
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix
cargo test -p intent-api --test rls_integration test_i3 -- --ignored --test-threads=1 --nocapture

# Run ignored tests that require Postgres (broad filter)
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase
cargo test -p intent-api -- --ignored
```

### Backup/restore validation (I6)

Non-destructive local validation that a `pg_dump` archive can be restored and application tests pass against the restored database:

```bash
# 1. Start Postgres
docker compose -f infrastructure/local/docker-compose.yml up -d postgres

# 2. Dump the source database
docker exec intent-rebase-postgres pg_dump -U intent_rebase -Fc -d intent_rebase_phase1_fix -f /tmp/i6_restore_test.dump

# 3. Create a fresh restore target
docker exec intent-rebase-postgres createdb -U intent_rebase intent_rebase_i6_restore

# 4. Restore
docker exec intent-rebase-postgres pg_restore -U intent_rebase -d intent_rebase_i6_restore /tmp/i6_restore_test.dump

# 5. Verify migrations are present
docker exec intent-rebase-postgres psql -U intent_rebase -d intent_rebase_i6_restore -c "SELECT COUNT(*) AS migrations FROM _sqlx_migrations"

# 6. Run integration tests against the restored database
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_i6_restore
cargo test -p intent-service --test migration_integration -- --ignored
cargo test -p intent-api --test webhook_integration -- --ignored
```

> **Caveat:** This is local docker-compose `pg_dump`/`pg_restore` only. It does not validate production PITR, WAL archiving, basebackup, or offsite replication. RPO/RTO were not measured.

**Prerequisites for ignored tests:**
1. Start local services: `docker compose -f infrastructure/local/docker-compose.yml up -d`
2. Verify Postgres is healthy: `docker compose -f infrastructure/local/docker-compose.yml ps`
3. Verify NATS is healthy (for NATS tests): check `NATS_URL` is set and NATS JetStream is available
4. Set `DATABASE_URL` and/or `NATS_URL` as shown above

**Current environment caveat (2026-05-20):** `scripts/verify-fast.sh` completes fmt/check/clippy but may time out during the test phase (~10 min). Running `cargo test --workspace --lib --all-features` separately passes. Ignored live tests (RLS, NATS, SQLx smoke) pass when the above prerequisites are met and environment variables are set, but they remain **local-dev/manual evidence only** and are **not production evidence**.

### Known local caveats and blockers (Phase 1 execution results)

The following issues were discovered by actually running the ignored suites against a local docker-compose stack (Postgres, NATS, MinIO). They are recorded here so future runs are not surprised and so blockers are not hidden.

| Suite | Symptom | Status | Likely Cause | Workaround / Next Step |
|-------|---------|--------|--------------|------------------------|
| RLS integration (fresh DB) | 22 passed | 🟢 FIXED | RLS harness fixed; fresh DB migrations apply cleanly | Use fresh DB (`intent_rebase_phase1_fix`) for canonical RLS integration evidence |
| Migration integration (fresh DB) | 1 passed | 🟢 FIXED | Migration 19 syntax fixed in `infrastructure/migrations/019_create_webhook_outbox.sql` | Fresh DB path is now the canonical migration evidence source |
| Audit SQLx smoke (fresh DB) | 1 passed | 🟢 FIXED | Audit enum insert/read alignment fixed in `crates/intent-rebase-types/src/audit_repo.rs` | Fresh DB audit smoke is green |
| NATS JetStream ignored suite | 14 passed | 🟢 FIXED | NATS uniqueness fix in `crates/intent-api/src/nats_jetstream/tests_live_integration.rs` | All live NATS tests pass after code fix |
| RLS integration (existing DB) | 3 passed, 19 failed | 🔴 OPEN | Missing relations `propagation_records`, `webhook_subscriptions`; RLS not enabled → `RowNotFound`; tenant isolation assertions fail | Existing DB is stale relative to migration sequence; fresh DB is the canonical source |
| RLS integration (existing DB, `RLS_TEST_RUN_MIGRATIONS=true`) | 1 passed, 21 failed | 🔴 OPEN | Migration 9 checksum mismatch: "previously applied but has been modified" | Existing DB has a modified migration 9; fresh DB path avoids this |
| Webhook integration (fresh DB) | 1 passed | 🟢 FIXED | S4 delivered: SQLx outbox repo + subscription repo + real HTTP receiver + DB status verification | Fresh DB (`intent_rebase_phase1_fix`) required for clean schema |
| Load tests (L1-L3) | 2 passed | 🟡 LOCAL ONLY | L1 1000/1000, L2 5000/5000, L3 10000/10000; sustained 90s 4505/4505, 50.05 req/s, p95 3ms, p99 8ms | **Local load-test harness only**; not production/staging evidence. L4-L5 remain blocked until staging/production infra exists. |

> **Ground truth rule:** The pass/fail counts above are the ground truth from the most recent execution. Update this table after each re-run. Fixed items are kept in the table with their resolution status so historical baseline (Phase 1 failures) is preserved.

This policy keeps `cargo test --workspace --lib --all-features` fast and free of external-service dependencies.

## RLS Structural Audit (S2)

Run the local structural audit script to verify handler-level RLS wrapping invariants:

```bash
bash scripts/audit-rls-dml.sh
```

This script checks that all mutation handlers in `crates/intent-api/src` use `begin_with_tenant` or delegated `_with_rls` methods, and documents known gaps (webhook handlers, query handler residuals). See `docs/11-quality/03-rls-audit.md` for full details, limitations, and expected warnings.

## Completed audits (P2)
- Module-level documentation (`//!` headers) for extracted modules and all handler modules — completed
- Cross-link consistency between test strategy and delivery docs — completed
- RLS DML structural audit (`scripts/audit-rls-dml.sh`) — completed (S2)

## Deferred audits (P2 / future)
- Benchmark integration (`cargo bench` in `verify-fast.sh` or CI) — no benchmarks exist yet
- Dev-experience verification (optional `justfile` alternative to `verify-fast.sh`)
- Router Stage 2 route-group split evaluation (deferred to P2)

> **Non-production caveat:** This test strategy supports bounded non-production feature delivery. Full production readiness requires additional external gates (SRE, Security, load/pen testing) tracked in P3 and explicitly NOT claimed here.
