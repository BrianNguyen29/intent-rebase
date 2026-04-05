# REST API

## Implementation Status

**Phase 1 First Slice** - Intent Registry endpoints are fully implemented via axum HTTP transport layer.
- HTTP framework: axum 0.7 with tower-http middleware (CORS only — no tracing middleware in this PR)
- Routes manually wired to match OpenAPI spec
- Error responses mapped to OpenAPI Error schema
- Base path: routes mount directly (e.g., `POST /intents`), intended to be served under `/v1` prefix in production

Other endpoints (rebases, graph, etc.) are planned for Phase 2+. Diff endpoint is implemented in this phase.

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

### POST /intents/{intent_id}/diff ✅ (Phase 1 Diff Preview)
Compute semantic diff between two versions.

**Request:**
```json
{
  "from_version": 1,
  "to_version": 2
}
```

**Response:**
```json
{
  "intent_id": "uuid",
  "from_version": { /* IntentVersion */ },
  "to_version": { /* IntentVersion */ },
  "diff": {
    "scope": { "in_scope": { "added": [], "removed": [] }, "out_of_scope": { "added": [], "removed": [] } },
    "constraints": { "functional": [], "non_functional": [], "policy": [], "budget": [], "time": [] },
    "acceptance_criteria": { "required": [], "optional": [] },
    "authority": { "allowed_actions": [], "forbidden_actions": [], "approval_requirements": [] }
  },
  "risk": {
    "severity": "low",
    "confidence": 1.0,
    "manual_review": false,
    "manual_review_reasons": [],
    "section_risks": [],
    "rationale": "No semantic changes detected..."
  }
}
```

**Error responses:**
- `400 Bad Request`: Invalid version ordering (from_version >= to_version)
- `404 Not Found`: Intent or version not found

### POST /intents/{intent_id}/rebase-preview ✅ (Phase 1 PR #16: Graph-Integrated)
Generate rebase preview plan between two versions with graph-integrated affected items.

**Request:**
```json
{
  "from_version": 1,
  "to_version": 2
}
```

**Response:**
```json
{
  "intent_id": "uuid",
  "from_version": { /* IntentVersion */ },
  "to_version": { /* IntentVersion */ },
  "decision_class": "B",
  "rationale": "medium severity with 80% confidence",
  "section_decisions": [
    {
      "section": "scope",
      "change_summary": "+1 in_scope",
      "recommended_action": "Review scope deltas before proceeding"
    }
  ],
  "affected_items": {
    "status": "available",
    "affected_artifacts": [
      {
        "node_id": "uuid",
        "label": "artifact-label",
        "impact": "direct",
        "reason": "directly depends on",
        "external_ref": { "ref_type": "artifact", "ref_id": "uuid" }
      }
    ],
    "affected_approvals": [],
    "side_effects": []
  },
  "manual_review_recommended": true,
  "risk_level": 2
}
```

**Affected Items Status:**
- `available`: Graph data was found and affected items were classified
- `unavailable`: Graph node not found or graph service unavailable

**Reliability:** The endpoint remains functional even when graph coverage is incomplete. A `status: unavailable` does NOT cause a 500 error.

**Decision Classes:**
- `A`: No semantic changes — no rebase needed
- `B`: Soft review recommended — no immediate invalidation
- `C`: Partial repair candidate — limited scope changes
- `D`: Compensation and repair needed — manual review advised
- `E`: Hard restart required — manual handoff needed

**Phase 1 PR #16 Notes:**
- `affected_items` now includes graph-integrated classification when available
- `side_effects` identifies items that may need compensation review (Phase 2)
- Does NOT expose `deferred` fields (Phase 2)

**Error responses:**
- `400 Bad Request`: Invalid version ordering (from_version >= to_version)
- `404 Not Found`: Intent or version not found

## Not Yet Implemented (Phase 2+)

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
