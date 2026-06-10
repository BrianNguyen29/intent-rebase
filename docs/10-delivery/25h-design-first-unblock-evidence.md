# Design-First Unblock Evidence

> **Status:** INTERNAL EVIDENCE — local design and bounded implementation only
> **Date:** 2026-06-10
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** Evidence for the design-first unblock slice executed under task `unblock-design-first-items`. Covers webhook D1, forensic chain-hash, and NATS per-tenant streams.
> **Non-Production Caveat:** This document is an internal planning and implementation evidence artifact. It does not claim production readiness, CI-green status, or external sign-off. All external and production evidence gates remain blocked or deferred.

---

## 1. Webhook D1 — Outbox Version Persistence

### ADR

- **ADR-13** created and accepted: `docs/13-adrs/13-webhook-outbox-version-persistence.md`
- Decision: Option A (store-at-creation)

### Implementation

| File | Change |
|------|--------|
| `crates/intent-api/src/webhook_outbox_repo/types.rs` | Added `version: i32`, `version_hash: Option<String>`, `previous_version: Option<i32>` to `WebhookOutboxRecord`; builder methods `with_version`, `with_version_hash`, `with_previous_version` |
| `infrastructure/migrations/022_add_webhook_outbox_version_fields.sql` | Additive migration with `DEFAULT 0` / `NULL`; idempotent |
| `crates/intent-api/src/webhook_outbox_repo/sqlx.rs` | `map_row` and `create` INSERT updated to read/write new columns |
| `crates/intent-api/src/webhook_dispatcher.rs` | Dispatcher reads `record.version`, `record.version_hash`, `record.previous_version` instead of hardcoded `0`/None |
| `crates/intent-api/src/webhook_delivery.rs` | Outbox record creation calls `.with_version(version)` |
| `docs/04-api/openapi.yaml` | `WebhookOutboxRecord` schema updated with `version`, `version_hash`, `previous_version` |

### Verification

- `cargo test -p intent-api --lib`: 442 passed, 0 failed, 17 ignored
- `cargo check -p intent-api`: clean

### Limitations

- `version_hash` and `previous_version` are not yet populated at creation time; upstream propagation path enhancement is future work
- Production secret manager, key rotation, delivery SLO, and external validation remain blocked

---

## 2. Forensic Chain-Hash — Local Algorithm

### ADR

- **ADR-14** created and accepted: `docs/13-adrs/14-forensic-chain-hash.md`
- Decision: Option A (linear chain hash)

### Implementation

| File | Change |
|------|--------|
| `crates/forensic-service/src/chain_hash.rs` | New module — `ChainHashLink`, `compute_chain_hash()`, `verify_chain()`, `ChainVerificationFailure`; pure algorithm, no infra dependencies |
| `crates/forensic-service/src/bundle.rs` | `BundleIntegrity` extended with `previous_bundle_hash: Option<String>` |
| `crates/forensic-service/src/lib.rs` | Module registration and re-export |

### Verification

- `cargo test -p forensic-service --lib`: 125 passed, 0 failed, 1 ignored
- `cargo check -p forensic-service`: clean

### Limitations

- Chain-hash computation is not yet wired into bundle generation (no automatic `previous_bundle_hash` population)
- Object Lock, S3 retention enforcement, and external witness (signed final hash) remain Phase 4+ deferred scope
- Production security review and infrastructure gates (A-05, A-04) remain blocked

---

## 3. NATS Per-Tenant Streams — Design Only

### ADR

- **ADR-15** created (Proposed): `docs/13-adrs/15-nats-per-tenant-streams.md`
- Staged migration documented (Stage 1 readiness ✅, Stage 2–4 proposed)
- Duplicate storage guardrail defined: subject prefix `v2` with publisher-side env gate
- Rollback plan documented

### Implementation

- **No code changes.** The shared `audit_events` stream remains the sole stream.

### Blockers

- A-03 (external SRE sign-off)
- A-05 (staging/production NATS topology)
- A-04 (security review of per-tenant isolation)

---

## 4. S3/S4 Side-Effect Auto-Compensation

- **No changes.** Remains blocked pending explicit user approval per AGENTS.md constraint #5.
- No code written; no design doc created.

---

## 5. External Gates Status

| Gate | Status | Notes |
|------|--------|-------|
| A-03 | 🔴 Blocked | External SRE sign-off required |
| A-04 | 🔴 Blocked | External security review required |
| A-05 | 🔴 Blocked | Production infrastructure required |
| A-06 | 🔴 Blocked | Load test evidence required |
| A-07 | 🔴 Blocked | External pen test required |
| A-10 | 🔴 Blocked | Production NATS topology + DLQ validation |
| A-11 | 🔴 Deferred | SDK-blocked |
| A-12 | 🔴 Blocked | Production secret manager + SLO evidence |
| A-13 | 🔴 Blocked | Object Lock + retention enforcement; local chain-hash algorithm only |

---

## 6. Sign-Off

**Internal-only sign-off:** BrianNguyen29 (Backend Lead)

- [x] Webhook D1 bounded implementation reviewed and accepted for local-dev scope
- [x] Forensic chain-hash local algorithm reviewed and accepted for local-dev scope
- [x] NATS per-tenant stream migration design reviewed and accepted as design-only
- [x] Explicit non-production caveats recorded
- [x] No external gates claimed closed
- [x] No S3/S4 auto-compensation code implemented

**Limitation statement:** This sign-off is internal-only and does not substitute for external SRE, security, or infrastructure review. Production readiness claims require independent third-party evidence.

---

## 7. Evidence Artifacts

| Artifact | Path |
|----------|------|
| ADR-13 | `docs/13-adrs/13-webhook-outbox-version-persistence.md` |
| ADR-14 | `docs/13-adrs/14-forensic-chain-hash.md` |
| ADR-15 | `docs/13-adrs/15-nats-per-tenant-streams.md` |
| Updated todo list | `docs/10-delivery/25-remaining-todo-list.md` |
| ADR index | `docs/13-adrs/README.md` |
| Webhook types | `crates/intent-api/src/webhook_outbox_repo/types.rs` |
| Webhook SQLx repo | `crates/intent-api/src/webhook_outbox_repo/sqlx.rs` |
| Webhook dispatcher | `crates/intent-api/src/webhook_dispatcher.rs` |
| Webhook delivery | `crates/intent-api/src/webhook_delivery.rs` |
| Migration 022 | `infrastructure/migrations/022_add_webhook_outbox_version_fields.sql` |
| OpenAPI update | `docs/04-api/openapi.yaml` |
| Chain hash module | `crates/forensic-service/src/chain_hash.rs` |
| Bundle integrity model | `crates/forensic-service/src/bundle.rs` |
| Forensic service lib | `crates/forensic-service/src/lib.rs` |
