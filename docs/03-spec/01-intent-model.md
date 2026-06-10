# Intent Model Specification

## Purpose

Define the standard structure for an intent, so that it can:
- be versioned
- be diffed
- be traced to artifacts
- be evaluated against policy/approval
- be used to reconstruct history

## Entity: IntentDocument

```yaml
intent_id: uuid
tenant_id: uuid
workflow_id: uuid
current_version: integer
status: active | archived | superseded
created_at: timestamp
created_by: actor_ref
source_refs: [source_ref]
tags: [string]
```

## Entity: IntentVersion

```yaml
intent_version_id: uuid
intent_id: uuid
version_number: integer
parent_version_id: uuid|null
created_at: timestamp
created_by: actor_ref
change_reason: string
change_channel: user_edit | webhook | policy_update | system_normalization
status: draft | active | rejected | superseded
hash: string
payload:
  objective:
    summary: string
    success_statement: string
    domain: string
  scope:
    in_scope: [string]
    out_of_scope: [string]
  constraints:
    functional: [constraint]
    non_functional: [constraint]
    policy: [constraint]
    budget: [constraint]
    time: [constraint]
  acceptance_criteria:
    required: [criterion]
    optional: [criterion]
  authority:
    allowed_actions: [action_ref]
    forbidden_actions: [action_ref]
    approval_requirements: [approval_rule_ref]
  preferences:
    tradeoffs:
      - dimension: speed|cost|quality|risk|compatibility|latency
        preference: prioritize|balance|minimize|maximize
  references:
    specs: [doc_ref]
    tickets: [doc_ref]
    repos: [doc_ref]
    policies: [doc_ref]
  assumptions:
    explicit: [string]
  metadata:
    risk_tier: low|medium|high|critical
    urgency: low|medium|high|critical
    confidence: float
```

## Intent Clause Model

To enable precise tracing, important parts should have a `clause_id`.

```yaml
constraint:
  clause_id: uuid
  type: functional|non_functional|policy|budget|time
  key: string
  operator: eq|neq|lt|lte|gt|gte|contains|not_contains|regex|custom
  value: any
  rationale: string
  priority: must|should|could
```

## Intent Change Classification

- `add_detail`
- `remove_detail`
- `tighten_constraint`
- `relax_constraint`
- `expand_scope`
- `shrink_scope`
- `change_acceptance`
- `change_authority`
- `change_budget`
- `change_priority`
- `invalidate_assumption`
- `source_update`

## Modeling Rules

1. Anything that affects execution must be separable into a clause.
2. Do not stuff everything into prose.
3. Source refs must be immutable and retrievable.
4. Change reason is mandatory.
5. Do not overwrite the old version.
