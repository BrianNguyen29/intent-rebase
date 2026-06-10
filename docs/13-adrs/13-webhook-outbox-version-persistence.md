# ADR-13 — Webhook Outbox Version Persistence

## Status

**Accepted — Bounded implemented** — Store-at-creation option implemented for `WebhookOutboxRecord`. Migration `022` adds `version`, `version_hash`, `previous_version` columns. Dispatcher reads stored values. No production readiness claim.

## Context

The webhook payload schema (defined in `webhook_delivery.rs`) includes `version`, `version_hash`, and `previous_version` fields to help downstream receivers understand intent changes. However, at dispatch time the worker only has the `WebhookOutboxRecord`, which historically did not store version information. The TODO at `webhook_dispatcher.rs:79` flagged this gap.

### Design Question

How should version information reach the webhook dispatcher?

- **Option A: Store-at-creation** — Persist `version`, `version_hash`, and `previous_version` in the outbox record when it is created. The dispatcher reads these fields directly.
- **Option B: Re-query at dispatch** — The dispatcher re-queries the intent repository to fetch the current version info at dispatch time. This is more flexible but adds latency and coupling.
- **Option C: Embed in payload JSON** — Store the full payload (including version info) as a JSON blob in the outbox. Simple but loses structured queryability.

## Decision

**Option A (store-at-creation) is accepted and bounded implemented.**

Rationale:
1. **Determinism** — The version info at creation time is the version that triggered the webhook. Re-querying at dispatch might return a newer version, which is incorrect.
2. **Decoupling** — The dispatcher does not need access to the intent repository; it only needs the outbox record.
3. **Simplicity** — Three additive columns with default values (`0`, `NULL`, `NULL`) are backward-compatible and do not break existing queries.
4. **Auditability** — The outbox record itself becomes an auditable trace of what version was dispatched.

## Consequences

### Positive
- Dispatcher now emits accurate version info in webhook payloads
- Outbox records are self-contained; no intent-repo coupling in the worker
- Migration is additive and idempotent; existing records default to `version=0`

### Negative
- Version info is snapshotted at creation time. If the intent is modified before the webhook is delivered, the payload still reflects the original version. This is intentional (the webhook is about the change that triggered it).
- Callers that create outbox records must supply version info. Current caller (`webhook_delivery.rs`) passes `version` but leaves `version_hash` and `previous_version` as `None` until the upstream propagation path is enhanced to provide them.

### Neutral
- OpenAPI `WebhookOutboxRecord` schema updated with the three new fields
- In-memory and SQLx repositories both persist the new fields

## Implementation Notes

**Implemented (bounded):**
- `WebhookOutboxRecord` extended with `version: i32`, `version_hash: Option<String>`, `previous_version: Option<i32>`
- `new()` initializes defaults; builder methods `with_version`, `with_version_hash`, `with_previous_version` provided
- Migration `022_add_webhook_outbox_version_fields.sql` adds columns with `DEFAULT 0` / `NULL`
- `SqlxWebhookOutboxRepository` updated to insert/select the new columns
- `WebhookDeliveryDispatcher` reads `record.version`, `record.version_hash`, `record.previous_version` instead of hardcoded `0`/None
- `dispatch_webhooks_for_intent_with_outbox` calls `.with_version(version)` on outbox records

**Not implemented (out of scope):**
- Upstream propagation path does not yet provide `version_hash` or `previous_version` to the outbox creator
- Subscription CRUD and production secret management remain deferred
- Webhook delivery SLO, staging validation, and external review gates remain blocked

## Evidence

- Types and repository trait: `crates/intent-api/src/webhook_outbox_repo/types.rs`
- SQLx implementation: `crates/intent-api/src/webhook_outbox_repo/sqlx.rs`
- Dispatcher: `crates/intent-api/src/webhook_dispatcher.rs`
- Delivery wiring: `crates/intent-api/src/webhook_delivery.rs`
- Migration: `infrastructure/migrations/022_add_webhook_outbox_version_fields.sql`
- OpenAPI schema: `docs/04-api/openapi.yaml` (`WebhookOutboxRecord`)

## Related ADRs

- ADR-03 (External API): Webhook payload schema definition
- ADR-04 (Event Broker): Event publishing semantics

## Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-06-10 | (fixer) | ADR created; store-at-creation decision recorded; bounded implementation delivered |

---

**Next Step**: Upstream propagation path can be enhanced to provide `version_hash` and `previous_version` when available. Production readiness (secret manager, SLO, external review) remains Phase 4+ scope.
