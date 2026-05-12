# ImpactReport Examples

> **Scope:** Bounded MVP read-only projection examples. These illustrate the four canonical ImpactReport shapes returned by `GET /intents/{intent_id}/impact-report`.
> **Non-production:** Examples are illustrative; actual field values depend on live service state.

---

## Example 1 — No-Impact / Empty Rebase

A version bump that touches only metadata (e.g., urgency label change) with no scope changes.

```json
{
  "intent_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "660e8400-e29b-41d4-a716-446655440001",
  "trigger": {
    "change_summary": "Minor metadata update: urgency changed from Medium to Low",
    "risk_tier": "Low",
    "decision_class": "AutoProceeded"
  },
  "scope": {
    "affected_artifacts_count": 0,
    "affected_approvals_count": 0,
    "affected_side_effects_count": 0
  },
  "invalidation": {
    "invalidated_artifacts_count": 0,
    "invalidated_approvals_count": 0
  },
  "compensation": {
    "total_actions": 0,
    "eligible_count": 0,
    "blocked_count": 0,
    "manual_review_required_count": 0,
    "dlq_candidate_count": 0
  },
  "safety_gates": {
    "open_gates": 0,
    "blocked_gates": 0,
    "manual_review_gates": 0
  },
  "provenance": {
    "generated_at": "2026-05-12T10:00:00Z",
    "from_version": 1,
    "to_version": 2
  },
  "unsupported_items": [
    "propagation-status downstream tracking",
    "cross-workflow lineage impact",
    "checkpoint alignment recommendations"
  ]
}
```

**Interpretation:** Safe to apply automatically. No approvals invalidated, no compensation required, no manual review gates.

---

## Example 2 — Approval Invalidation

A scope change that removes a previously-approved artifact, causing the existing approval to become invalid.

```json
{
  "intent_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "660e8400-e29b-41d4-a716-446655440001",
  "trigger": {
    "change_summary": "Scope reduced: removed 'payment-gateway-v1' from in_scope",
    "risk_tier": "Medium",
    "decision_class": "AutoProceeded"
  },
  "scope": {
    "affected_artifacts_count": 3,
    "affected_approvals_count": 2,
    "affected_side_effects_count": 1
  },
  "invalidation": {
    "invalidated_artifacts_count": 1,
    "invalidated_approvals_count": 1
  },
  "compensation": {
    "total_actions": 1,
    "eligible_count": 1,
    "blocked_count": 0,
    "manual_review_required_count": 0,
    "dlq_candidate_count": 0
  },
  "safety_gates": {
    "open_gates": 1,
    "blocked_gates": 0,
    "manual_review_gates": 0
  },
  "provenance": {
    "generated_at": "2026-05-12T10:05:00Z",
    "from_version": 2,
    "to_version": 3
  },
  "unsupported_items": [
    "propagation-status downstream tracking",
    "cross-workflow lineage impact",
    "checkpoint alignment recommendations"
  ]
}
```

**Interpretation:** One approval is invalidated because its artifact is no longer in scope. A single compensation action (e.g., rollback or counter-action) is eligible and unblocked. Apply can proceed after the approval is re-validated or cancelled.

---

## Example 3 — Compensation Required

A functional change that introduces a new side effect requiring a compensating action.

```json
{
  "intent_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "660e8400-e29b-41d4-a716-446655440001",
  "trigger": {
    "change_summary": "Added new external dependency: third-party billing API",
    "risk_tier": "High",
    "decision_class": "AutoProceeded"
  },
  "scope": {
    "affected_artifacts_count": 5,
    "affected_approvals_count": 2,
    "affected_side_effects_count": 3
  },
  "invalidation": {
    "invalidated_artifacts_count": 0,
    "invalidated_approvals_count": 0
  },
  "compensation": {
    "total_actions": 2,
    "eligible_count": 2,
    "blocked_count": 0,
    "manual_review_required_count": 0,
    "dlq_candidate_count": 0
  },
  "safety_gates": {
    "open_gates": 2,
    "blocked_gates": 0,
    "manual_review_gates": 0
  },
  "provenance": {
    "generated_at": "2026-05-12T10:10:00Z",
    "from_version": 3,
    "to_version": 4
  },
  "unsupported_items": [
    "propagation-status downstream tracking",
    "cross-workflow lineage impact",
    "checkpoint alignment recommendations"
  ]
}
```

**Interpretation:** Two compensation actions are required (e.g., a follow-up notice and a rollback capability). Both are eligible and unblocked. The rebase can proceed; compensation actions are queued for execution.

---

## Example 4 — Manual Review Required

A high-risk change that triggers a policy gate requiring human approval before the rebase can be applied.

```json
{
  "intent_id": "550e8400-e29b-41d4-a716-446655440000",
  "tenant_id": "660e8400-e29b-41d4-a716-446655440001",
  "trigger": {
    "change_summary": "Breaking change: removed required constraint 'data-retention-days'",
    "risk_tier": "Critical",
    "decision_class": "BlockedManualReview"
  },
  "scope": {
    "affected_artifacts_count": 8,
    "affected_approvals_count": 4,
    "affected_side_effects_count": 2
  },
  "invalidation": {
    "invalidated_artifacts_count": 2,
    "invalidated_approvals_count": 2
  },
  "compensation": {
    "total_actions": 3,
    "eligible_count": 1,
    "blocked_count": 1,
    "manual_review_required_count": 1,
    "dlq_candidate_count": 0
  },
  "safety_gates": {
    "open_gates": 1,
    "blocked_gates": 1,
    "manual_review_gates": 1
  },
  "provenance": {
    "generated_at": "2026-05-12T10:15:00Z",
    "from_version": 4,
    "to_version": 5
  },
  "unsupported_items": [
    "propagation-status downstream tracking",
    "cross-workflow lineage impact",
    "checkpoint alignment recommendations"
  ]
}
```

**Interpretation:**
- **Manual review gate is closed:** One policy gate requires human sign-off before apply.
- **Blocked compensation:** One compensation action is blocked pending the manual review.
- **Invalidated approvals:** Two approvals are invalidated due to scope changes.
- **Recommendation:** Do NOT auto-apply. Route to manual review workflow. After approval, re-run ImpactReport to verify gate status.

---

## Using These Examples

These examples are used for:
- **Frontend mock data** during UI development
- **API consumer documentation** to explain ImpactReport semantics
- **Test fixture references** for handler-level assertions

All examples reflect the **bounded MVP** shape:
- `unsupported_items` lists deferred features (propagation-status, cross-workflow lineage, checkpoint alignment)
- No historical reports; each request generates a fresh projection
- No persistence; these are transient response illustrations only
