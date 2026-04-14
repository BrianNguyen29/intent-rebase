# REST API

> **Canonical source:** Full endpoint definitions, request/response schemas, and parameter details are maintained in [`openapi.yaml`](./openapi.yaml). This guide covers design principles and high-level resource layout only; example resources below may include target-state/planned surfaces that are not yet implemented.

## Design principles
- JSON over HTTPS
- idempotency cho create/update có side effects
- cursor-based pagination
- ETags/version preconditions
- explicit tenant scoping

## Base path

Conceptually `/v1`, but the current implementation contains a mix of legacy `/v1/...` and newer non-prefixed routes. Treat [`openapi.yaml`](./openapi.yaml) as the source of truth for live paths.

## Resources

> **Important:** The resource paths in this section are conceptual/design-time examples for API shape discussion. They are **not** a guaranteed inventory of implemented endpoints or exact live path shapes. For canonical implemented routes, always use [`openapi.yaml`](./openapi.yaml).

### POST /intents
Tạo intent mới.

Request:
```json
{
  "workflow_id": "uuid",
  "source_refs": [{"type":"spec","id":"spec://repo/path/spec.md"}],
  "payload": {
    "objective": {"summary":"Refactor auth module", "success_statement":"..."},
    "scope": {"in_scope":["auth service"], "out_of_scope":["public API"]},
    "constraints": {"functional":[], "non_functional":[], "policy":[], "budget":[], "time":[]},
    "acceptance_criteria": {"required":[], "optional":[]},
    "authority": {"allowed_actions":["open_pr"], "forbidden_actions":["merge_main"], "approval_requirements":["approve-code-change"]},
    "preferences": {"tradeoffs":[{"dimension":"quality","preference":"prioritize"}]},
    "references": {"specs":[], "tickets":[], "repos":[],"policies":[]},
    "assumptions": {"explicit":[]},
    "metadata": {"risk_tier":"medium","urgency":"medium","confidence":0.9}
  }
}
```

Response:
```json
{
  "intent_id": "uuid",
  "current_version": 1,
  "status": "active"
}
```

### POST /intents/{intent_id}/versions
Tạo version mới từ intent hiện có.

### GET /intents/{intent_id}
### GET /intents/{intent_id}/versions
### GET /intents/{intent_id}/versions/{version}

### POST /diffs
Tính semantic diff giữa hai versions.

### POST /rebases
Tạo rebase plan.

Request:
```json
{
  "intent_id": "uuid",
  "from_version": 3,
  "to_version": 4,
  "workflow_execution_ref": "wf-123",
  "mode": "preview"
}
```

Response:
```json
{
  "rebase_plan_id": "uuid",
  "classification": "partial_repair",
  "summary": {
    "still_valid": 17,
    "review_required": 4,
    "invalid": 3,
    "compensations": 1,
    "approvals_required": 2
  }
}
```

### POST /rebases/{rebase_plan_id}/apply
Áp dụng rebase plan.

### GET /rebases/{rebase_plan_id}
### GET /rebases/{rebase_plan_id}/timeline

### GET /workflows/{workflow_id}/impact-map
Trả impact graph rút gọn để hiển thị UI.

### POST /approvals/{approval_id}/revalidate
### GET /artifacts/{artifact_id}/provenance
### GET /side-effects/{side_effect_id}

## Implemented (Phase 3 P3-S4)

## Planned / target-state resources

The following resources are part of the broader design direction and may not exist yet in the current implementation. Check [`openapi.yaml`](./openapi.yaml) before integrating against them.

### POST /side-effects/{side_effect_id}/compensate
### GET /audit/events
Tenant-scoped audit event query. Returns all audit events for a tenant, ordered by occurred_at descending.

Query params: `tenant_id` (required), `limit` (optional, default 100, max 1000)

### GET /audit/events/{event_id}
Tenant-scoped single audit event query. Returns a specific audit event by ID.

Query params: `tenant_id` (required)

Returns 404 if event doesn't exist or belongs to a different tenant (enforces tenant isolation).

## Planned / target-state resources

The following resources are part of the broader design direction and may not exist yet in the current implementation. Check [`openapi.yaml`](./openapi.yaml) before integrating against them.

### POST /side-effects/{side_effect_id}/compensate
### GET /replays/{workflow_id}
### POST /replays/{workflow_id}/export

## Error model
```json
{
  "error": {
    "code": "STALE_VERSION_PRECONDITION_FAILED",
    "message": "Intent head changed while applying rebase plan",
    "retryable": true,
    "details": {}
  }
}
```

## Security requirements
- OAuth2 / OIDC access tokens
- service-to-service mTLS or signed workload identity
- per-request tenant binding
