# Webhooks

> **Status:** Design-only / future contract. This document describes the intended webhook event envelope and payload schemas for future integration. The current implementation (B3-B18 bounded slice) delivers only an env-gated, best-effort webhook dispatcher for `intent_changed` events with no production delivery guarantees, no outbox, no HMAC signing, no key rotation, no subscription CRUD API, and no event streaming. See `docs/10-delivery/19-propagation-status-implementation-plan.md` for the bounded Slice 3 scope.

> **Outbox Schema Design:** A draft outbox schema (`webhook_outbox`) is documented in the [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md). It is design-only — no migration or code has been implemented.

> **Background Worker Lifecycle:** A draft background delivery worker design (env gating, polling/claim loop, graceful shutdown, metrics) is documented in the [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md) as P2-6b. It is design-only — no implementation has been written.

> **HMAC Signing + Key Rotation:** A draft HMAC signing design (header format, canonical string, key rotation with dual-key grace window, consumer verification guidance) is documented in the [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md) as P2-6c. It is design-only — no implementation or secret material exists.

> **Subscription CRUD API:** A draft subscription management API design (endpoints, schemas, lifecycle states, tenant isolation, validation rules, secret redaction) is documented in the [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md) as P2-6d. It is design-only — no routes or handlers have been implemented.

> **Retry / Dead-Letter Semantics:** A draft retry and DLQ design (webhook outbox DLQ vs NATS DLQ, per-attempt log concept, replay/operator actions, metrics) is documented in the [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md) as P2-6e. It is design-only — no queue or worker implementation exists.

## Mục tiêu
Cho phép tích hợp IRE với:
- Git providers
- ticketing systems
- internal workflow engines
- policy engines
- approval tools
- chatops systems

## Outbound webhook events
- `rebase.plan_created`
- `rebase.manual_review_required`
- `approval.stale_detected`
- `workflow.restart_required`
- `compensation.manual_required`
- `audit.export_ready`

## Common Envelope

All outbound webhook deliveries use the same envelope structure:

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440000",
  "event_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "event_type": "rebase.plan_created",
  "occurred_at": "2026-04-05T10:30:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": { }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `delivery_id` | UUID | Unique identifier for this delivery attempt |
| `event_id` | UUID | Unique identifier for the event itself |
| `event_type` | string | One of the event types listed above |
| `occurred_at` | timestamp | ISO 8601 timestamp when the event occurred |
| `tenant_id` | UUID | The tenant that owns this workflow |
| `workflow_id` | string | The workflow instance that generated the event |
| `payload` | object | Event-specific payload (see below) |

## Event Payload Schemas

### `rebase.plan_created`

Fired when a rebase preview is available after calling `POST /intents/{id}/rebase-preview`.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440000",
  "event_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "event_type": "rebase.plan_created",
  "occurred_at": "2026-04-05T10:30:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "intent_id": "intent_xyz789",
    "rebase_preview_url": "/intents/intent_xyz789/rebase-preview",
    "conflicts_detected": true,
    "conflict_count": 3,
    "plan_summary": "Rebase onto main@latest, resolve 3 conflicts in src/auth/login.ts, src/api/users.ts, src/db/migrate.sql"
  }
}
```

### `rebase.manual_review_required`

Fired when the rebase planner recommends human review before proceeding.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440001",
  "event_id": "8d0f7780-8536-51e5-a755-f18fc2f01bf8",
  "event_type": "rebase.manual_review_required",
  "occurred_at": "2026-04-05T10:31:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "intent_id": "intent_xyz789",
    "review_reason": "high_conflict_risk",
    "review_reason_detail": "Rebase involves 5+ files with semantic conflicts in authentication logic",
    "rebase_preview_url": "/intents/intent_xyz789/rebase-preview",
    "auto_proceed_at": "2026-04-05T11:30:00.000Z"
  }
}
```

### `approval.stale_detected`

Fired when a graph-integrated approval becomes invalid due to upstream changes.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440002",
  "event_id": "9e1f8791-9647-62f6-b866-f29fd2f02cf9",
  "event_type": "approval.stale_detected",
  "occurred_at": "2026-04-05T10:32:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "intent_id": "intent_xyz789",
    "approval_id": "apr_stale456",
    "stale_reason": "upstream_dependencies_changed",
    "affected_approval_names": ["Security Review", "QA Sign-off"],
    "requires_resubmission": true
  }
}
```

### `workflow.restart_required`

Fired when a checkpoint-based restart signal is needed due to external state changes.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440003",
  "event_id": "af2f88a2-a758-73g7-c977-f30fe3f13c0a",
  "event_type": "workflow.restart_required",
  "occurred_at": "2026-04-05T10:33:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "intent_id": "intent_xyz789",
    "restart_from_checkpoint": "chk_after_approval_收集",
    "restart_reason": "deployment_freeze_lifted",
    "affected_spec_files": ["infra/k8s/deployment.yaml", "infra/k8s/service.yaml"]
  }
}
```

### `compensation.manual_required`

Fired when an irreversible side effect is detected and manual intervention is needed.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440004",
  "event_id": "bf3f99b3-b869-84h8-d088-f41ff4f24b1b",
  "event_type": "compensation.manual_required",
  "occurred_at": "2026-04-05T10:34:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "intent_id": "intent_xyz789",
    "side_effect_id": "se_ irreversible789",
    "side_effect_type": "database_migration_executed",
    "side_effect_detail": "Migration '20260301_add_user_preferences' was auto-executed during automated rebase",
    "compensation_action": "rollback_required",
    "rollback_script_url": "/intents/intent_xyz789/rollback/20260301_add_user_preferences"
  }
}
```

### `audit.export_ready`

Fired when a Phase 2 audit export is available for download.

```json
{
  "delivery_id": "550e8400-e29b-41d4-a716-446655440005",
  "event_id": "cf4faab4-c97a-95i9-e199-f52ff5f35c2c",
  "event_type": "audit.export_ready",
  "occurred_at": "2026-04-05T10:35:00.000Z",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "workflow_id": "wf_abc123def456",
  "payload": {
    "export_id": "exp_audit001",
    "export_url": "/audits/exports/exp_audit001/download",
    "export_format": "jsonl",
    "time_range_start": "2026-03-01T00:00:00.000Z",
    "time_range_end": "2026-04-01T00:00:00.000Z",
    "record_count": 1523,
    "expires_at": "2026-04-12T10:35:00.000Z"
  }
}
```

## Current Bounded Implementation Payload (B3-B18)

The currently implemented webhook delivery (env-gated, default disabled) sends an `intent_changed` event with this JSON payload:

```json
{
  "event_type": "intent_changed",
  "intent_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "version": 42,
  "version_hash": "sha256:abc123...",
  "previous_version": 41,
  "timestamp": "2026-05-13T12:00:00Z",
  "delivery_id": "550e8400-e29b-41d4-a716-446655440001",
  "attempt_number": 1,
  "subscription_id": "550e8400-e29b-41d4-a716-446655440002"
}
```

Headers:
- `Content-Type: application/json`
- `X-Idempotency-Key: <delivery_id>`

**Bounded scope:** No `X-Webhook-Signature` header (HMAC deferred). No payload compression. Delivery is best-effort with 3 attempts max and exponential backoff. See `crates/intent-api/src/webhook_delivery.rs` for the implementation.

## Future Design Envelope (not yet implemented)

The following describes the intended full webhook event envelope for future integration. It diverges from the current bounded `intent_changed` payload above.

### Webhook payload requirements
- signed secret
- delivery id
- event id
- retries with exponential backoff
- replay endpoint support

## Inbound webhook sources
- spec file changed
- issue updated
- policy updated
- approval revoked
- deployment freeze triggered

## Safety requirements
- verify signatures
- dedupe by delivery id
- source trust tier
- origin allowlist
