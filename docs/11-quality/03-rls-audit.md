# RLS DML Structural Audit (S2)

> **Status:** DELIVERED — local structural audit only. No production-readiness claim.
> **Date:** 2026-05-20
> **Scope:** Handler modules in `crates/intent-api/src`

## Purpose

This document describes the local-verifiable structural audit for Row-Level Security (RLS) sensitive DML paths in handler modules. It is Phase 2 Slice S2.

The audit checks that mutation handlers either:
- Use `begin_with_tenant` for direct RLS transaction wrapping, or
- Call delegated `_with_rls` service methods (e.g., `create_intent_with_rls`), or
- Are documented as known gaps/residuals.

> **Non-production caveat:** This is a structural source-code audit. It does **not** prove runtime DB-level RLS enforcement. Live `rls_integration` tests remain the ground truth for runtime behavior.

## What the audit checks

The script `scripts/audit-rls-dml.sh` inspects `crates/intent-api/src/*handlers.rs` and verifies:

1. **RLS-wrapped mutation handlers** contain either `begin_with_tenant` or `_with_rls`.
2. **Read-only handlers** do not contain RLS transaction patterns.
3. **Known webhook gaps** are flagged as warnings, not failures.
4. **Known residuals** (e.g., `query_handlers.rs` `ingest_propagation_signal`) are flagged as warnings.
5. **Delegated RLS methods** (`create_intent_with_rls`, `create_version_with_rls`) exist in `crates/intent-service/src/intent_service.rs`.
6. **No unclassified handler modules** have appeared since the baseline.

## Limitations

- **Structural only:** Checks source code patterns, not runtime DB RLS policy enforcement.
- **Handler scope only:** Repositories, workers, and helper modules (e.g., `propagation_signals.rs`, `webhook_delivery.rs`, `webhook_subscription_repo.rs`) are outside the handler audit boundary. Their gaps are documented as known residuals.
- **In-memory repos excluded:** Handlers that fall back to in-memory repositories are not required to use `begin_with_tenant`.
- **Read-only handlers excluded:** GET/list/query handlers do not need RLS transactions.
- **Webhook gaps are local-known residuals:** Webhook subscription and outbox handlers do not use `begin_with_tenant` or `OptionalRlsTenantClaims`. This is a documented Phase 4+ gap.
- **Live tests are ground truth:** `cargo test --test rls_integration -- --ignored` with a fresh DB is the canonical runtime evidence.

## Expected warnings

| Module | Warning | Reason |
|--------|---------|--------|
| `webhook_subscription_handlers.rs` | No RLS wrapping | Webhook Slice 4b is local-dev only; no production readiness claim |
| `webhook_outbox_dlq_handlers.rs` | No RLS wrapping | Webhook Slice 5b is local-dev only; no production readiness claim |
| `webhook_delivery.rs` | SQLx DML without RLS | Dispatcher scaffolding uses `sqlx::query` directly; not wired into production flow |
| `query_handlers.rs` | `OptionalRlsTenantClaims` but no `begin_with_tenant` | `ingest_propagation_signal` uses application-layer tenant scoping; DB-level RLS tx wrapping is deferred (S8) |

## Verification commands

Run the structural audit:

```bash
bash scripts/audit-rls-dml.sh
```

Run format check:

```bash
cargo fmt --all -- --check
```

Run type check:

```bash
cargo check -p intent-api --all-features
```

Run live RLS integration tests (requires local Postgres):

```bash
export DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix
cargo test -p intent-api --test rls_integration -- --ignored --test-threads=1
```

## When to update this audit

- After adding a new handler module.
- After adding `begin_with_tenant` to a previously unwrapped handler.
- After moving a module out of "known residual" status.
