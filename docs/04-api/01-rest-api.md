# REST API

## Implementation Status

**Phase 1 First Slice** - Intent Registry endpoints are fully implemented via axum HTTP transport layer.
- HTTP framework: axum 0.7 with tower-http middleware (CORS only — no tracing middleware in this PR)
- Routes manually wired to match OpenAPI spec
- Error responses mapped to OpenAPI Error schema
- Base path: routes mount directly (e.g., `POST /intents`), intended to be served under `/v1` prefix in production

Other endpoints (diffs, rebases, graph, etc.) are planned for Phase 2+.

## Design principles
- JSON over HTTPS
- idempotency cho create/update có side effects
- cursor-based pagination (future)
- ETags/version preconditions
- explicit tenant scoping

## Base path
`/v1`

## Implemented Resources (Phase 1)

### POST /intents ✅
Tạo intent mới.

Request:
```json
{
  "workflow_id": "uuid",
  "source_refs": [{"type":"spec","id":"spec://repo/path/spec.md"}],
  "payload": {
    "objective": {"summary":"Refactor auth module", "success_statement":"Auth module refactored successfully", "domain":"backend"},
    "scope": {"in_scope":["auth service"], "out_of_scope":["public API"]},
    "constraints": {"functional":[], "non_functional":[], "policy":[], "budget":[], "time":[]},
    "acceptance_criteria": {"required":[], "optional":[]},
    "authority": {"allowed_actions":[{"action":"open_pr"}], "forbidden_actions":[{"action":"merge_main"}], "approval_requirements":[{"rule_id":"approve-code-change", "description":"Requires approval for code changes"}]},
    "preferences": {"tradeoffs":[{"dimension":"quality","preference":"prioritize"}]},
    "references": {"specs":[], "tickets":[], "repos":[],"policies":[]},
    "assumptions": {"explicit":[]},
    "metadata": {"risk_tier":"medium","urgency":"medium","confidence":0.9}
  },
  "created_by": {"actor_type": "user", "actor_id": "user@example.com"},
  "tags": ["auth", "refactor"]
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

### POST /intents/{intent_id}/versions ✅
Tạo version mới từ intent hiện có.

**OCC Headers (optional):**
- `X-Expected-Version`: version number client believes is current
- `X-Expected-Row-Version`: row_version client last observed

Nếu provided và không match, trả về `409 Conflict`.
Nếu header present nhưng malformed (không phải integer), trả về `400 Bad Request`.

**Headers:**
```
X-Expected-Version: 3
X-Expected-Row-Version: 5
```

**Response 400 (malformed header):**
```json
{
  "error": {
    "code": "INVALID_HEADER",
    "message": "X-Expected-Version header must be an integer, got: not-a-number",
    "retryable": false
  }
}
```

**Response 409 (conflict):**
```json
{
  "error": {
    "code": "CONCURRENCY_CONFLICT",
    "message": "intent {uuid} has been modified",
    "retryable": true
  }
}
```

### GET /intents/{intent_id} ✅
Get intent head (current version).

### GET /intents/{intent_id}/versions ✅
List all versions of an intent.

### GET /intents/{intent_id}/versions/{version_number} ✅
Get specific version by version number.

## Not Yet Implemented (Phase 2+)

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
### POST /side-effects/{side_effect_id}/compensate

### GET /audit/events
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
- OAuth2 / OIDC access tokens (Phase 2+)
- service-to-service mTLS or signed workload identity (Phase 2+)
- per-request tenant binding (Phase 2+)

## OpenAPI

Full OpenAPI 3.0 specification available at `openapi.yaml`.
