# Semantic Diff Specification

## Purpose

Text diff is insufficient for production. The IRE needs semantic diff to answer:
- what changed in meaning
- what is the risk level
- which parts of the workflow may be affected
- whether human confirmation is needed

## Inputs
- `IntentVersion N`
- `IntentVersion N+1`
- optional:
  - domain taxonomy
  - policy catalog
  - artifact dependency hints

## Output: ChangeSet

```json
{
  "diff_id": "uuid",
  "intent_id": "uuid",
  "from_version": 3,
  "to_version": 4,
  "changes": [
    {
      "change_id": "uuid",
      "change_type": "tighten_constraint",
      "semantic_domain": "compatibility",
      "severity": "high",
      "confidence": 0.93,
      "affected_clauses": ["uuid-a", "uuid-b"],
      "rationale": "Backward compatibility has changed from optional to mandatory",
      "human_confirmation_required": false,
      "policy_relevant": true
    }
  ]
}
```

## Semantic domains
- scope
- compatibility
- security
- quality
- cost
- latency
- compliance
- data handling
- approvals
- delivery timeline
- authority

## Severity heuristic
### Low
- clearer description but no change in meaning
- added detail that does not affect the execution path

### Medium
- changed trade-off or reporting expectations
- fixed criteria without touching side effects

### High
- changed constraints that may invalidate patch/test/approval
- changed authority scope
- changed budget/time cap affecting the runtime plan

### Critical
- changed policy/compliance
- added forbidden action
- invalidated legal/security assumptions
- revoked permissions for an already-scheduled action

## Human confirmation triggers
- confidence below threshold
- change touches policy/high-risk domain
- multiple conflicting changes
- diff leads to uncertain compensation

## Implementation note

The first version should be hybrid:
- rule-based deterministic diff for structured fields
- model-assisted classification for prose / ambiguity
- policy overlay to assign severity

## Acceptance criteria
- diff output must be stable for the same input
- the same change set must produce the same impact outcome under the same rule version
- diff can be replayed under a historical rule pack
