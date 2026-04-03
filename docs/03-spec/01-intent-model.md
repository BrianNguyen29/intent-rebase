# Intent Model Specification

## Mục tiêu
Định nghĩa cấu trúc chuẩn cho intent, để:
- có thể version
- có thể diff
- có thể trace sang artifacts
- có thể đánh giá policy/approval
- có thể tái dựng lịch sử

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
Để trace chính xác, các phần quan trọng nên có `clause_id`.

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

## Phân loại intent changes

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

## Quy tắc modeling

1. Phần nào ảnh hưởng execution phải tách được thành clause.
2. Không nhét mọi thứ vào prose.
3. Source refs phải immutable và truy hồi được.
4. Change reason là bắt buộc.
5. Không overwrite version cũ.
