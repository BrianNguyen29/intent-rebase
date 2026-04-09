# 04 — Approval Scope & Revalidation Specification

**Status:** Proposed  
**Phase:** Phase 1+  
**Owner:** Security Team

---

## Mục đích

Defines how approval scope is computed, how approvals are invalidated on intent changes, and how revalidation works. This ensures that intent changes cannot bypass approval requirements.

---

## Approval Scope Model

### Scope Types

| Type | Description | Trigger |
|------|-------------|---------|
| `full` | All downstream resources require re-approval | Critical risk change |
| `partial` | Only affected resources require re-approval | High/medium risk change |
| `none` | No approval required | Low risk change, or change outside scope |

### Scope Computation

```
1. Detect intent change (diff computed)
2. Classify risk (low/medium/high/critical)
3. Build affected resource set:
   - Start from changed intent nodes
   - BFS traversal of dependency graph
   - Include all downstream approval nodes
   - Stop at boundary defined by rule pack
4. Determine scope_type based on risk
5. Create approval_scope record
6. Create policy_snapshot for audit
```

---

## Approval Lifecycle

```
[Intent Created]
    ↓
[Approval Requested]
    ↓
[Pending Approval] ← → [Approval Granted] ← → [Approval Revoked]
                        ↓                       ↓
                 [Intent Executed]       [Compensation Triggered]
                        ↓
                 [Approval Valid] ← → [Approval Expired]
                        ↓
                 [Intent Changed] → [Scope Changed] → [Revalidation Required]
```

---

## Invalidation Rules

| Change Type | Risk Level | Invalidation Behavior |
|-------------|-----------|----------------------|
| `spec.target.delete` | Critical | Full invalidation — all approvals in scope |
| `spec.constraints.remove.protected` | Critical | Full invalidation |
| `spec.target.modify` | High | Partial invalidation — affected resources |
| `spec.constraints.change` | High | Partial invalidation |
| `spec.target.add` | Medium | Log + notify; no auto-invalidation unless scope expanded |
| `metadata.tags.add` | Low | No impact |
| `metadata.description.update` | Low | No impact |

---

## Revalidation Workflow

### Automatic Revalidation (Low/Medium Risk)

```
1. Intent change detected
2. Risk classified as low/medium
3. Scope compared to original
4. If scope unchanged → approval remains valid
5. If scope expanded → new approval required for expanded scope
6. Audit event logged
```

### Manual Revalidation (High/Critical Risk)

```
1. Intent change detected
2. Risk classified as high/critical
3. All affected approvals invalidated
4. New approval workflow initiated
5. Notification sent to required_approvers
6. Original approvers informed of invalidation
7. Block rebase apply until re-approval granted
```

### Revalidation API

**IMPLEMENTED (Phase 2b bounded):**
```yaml
GET /approval-requests/{id}/revalidate:
  description: Check if approval is still valid (read-only scope comparison)
  response:
    {
      "approval_id": "uuid",
      "valid": true|false,
      "reason": "Scope unchanged since approval was granted" | "Scope has changed since approval was granted",
      "approval_basis_scope_hash": "sha256...",
      "current_scope_hash": "sha256..." | null,
      "revalidation_required": true|false,
      "intent_id": "uuid",
      "approval_basis_version": 1
    }

POST /approval-requests/trigger-reapproval:
  description: Trigger re-approval workflow (bounded - creates approval record, returns queue intent)
  body:
    {
      "intent_id": "uuid",
      "original_version_from": 1,
      "current_version_to": 2,
      "original_scope_hash": "sha256...",
      "current_scope_hash": "sha256...",
      "reapproval_reason": "Scope has changed since approval was granted"
    }
  response:
    {
      "approval_request_id": "uuid",
      "intent_id": "uuid",
      "intent_version_from": 1,
      "intent_version_to": 2,
      "notification_intent": true,
      "status": "pending",
      "reason": "Scope has changed since approval was granted"
    }
  Note: notification_intent=true is advisory only. Does NOT send actual notifications.
        External notification systems integration is Phase 3 deferred work.
```

**NOT YET IMPLEMENTED (Future Phase 3):**
```yaml
POST /api/v1/approvals/{id}/revalidate:
  description: Full revalidation workflow with external notification delivery
  ...
```

**Bounded read-only behavior:**
- GET /approval-requests/{id}/revalidate: Compares approval-basis snapshot scope_hash with latest snapshot scope_hash
- Returns validity status without triggering workflow or modifying approval state
- If latest snapshot missing, returns valid=false (cannot determine current policy)
- Does NOT auto-invalidate approvals, queue notifications, or start re-approval workflows

**Bounded trigger behavior (Phase 2b):**
- POST /approval-requests/trigger-reapproval: Creates new pending approval request record
- Returns approval_request_id for queue/notification tracking
- Sets notification_intent=true to indicate notification SHOULD happen
- Does NOT send actual notifications (Phase 3 external notification system out of scope)
- Caller is responsible for actual notification delivery or Phase 3 integration

---

## Approval State Machine

```
                    ┌──────────────┐
         ┌──────────│   Pending    │
         │          └──────┬───────┘
         │                 │
    [Approval          [Approval
     Requested]         Granted]
         │                 │
         │                 ↓
         │          ┌──────────────┐
         │    ┌─────│   Valid      │─────┐
         │    │     └──────────────┘     │
         │    │                          │
    [Approval    [Intent            [Approval
     Expired]    Changed]            Revoked]
         │    │                          │
         │    ↓                          ↓
         │  ┌─────────────────────────────┐
         └─→│         Invalidated        │
            └─────────────────────────────┘
```

---

## Scope Change Detection

### Comparison Algorithm

```python
def scope_changed(old_snapshot, new_scope):
    old_scope = old_snapshot.approval_scope
    new_hash = compute_scope_hash(new_scope)
    return old_scope.scope_hash != new_hash

def compute_scope_hash(scope):
    # Deterministic hash of scope definition
    canonical = json.dumps(scope, sort_keys=True)
    return sha256(canonical.encode())
```

---

## Multi-Party Approvals

| Config | Behavior |
|--------|----------|
| `min_approvals: 1` | Any one approver grants approval |
| `min_approvals: 2` | Two different approvers required |
| `required_roles` | At least one approver from each role group |

---

## Audit Events

| Event | Trigger |
|-------|---------|
| `approval.requested` | New approval workflow initiated |
| `approval.granted` | Approval granted |
| `approval.revoked` | Approval manually revoked |
| `approval.expired` | Approval time-to-live exceeded |
| `approval.revalidated` | Approval revalidated after intent change |
| `approval.scope_changed` | Scope expanded/contracted |

---

## Related Documents

- [03 — Policy Snapshot Specification](./03-policy-snapshot-spec.md)
- [02 — Provenance Specification](./02-provenance-spec.md)
- [07 — Authorization Matrix](./07-authz-matrix.md)
- [11 — Incident Freeze](./11-incident-freeze.md)