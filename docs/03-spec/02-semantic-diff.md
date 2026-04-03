# Semantic Diff Specification

## Mục tiêu
Text diff không đủ cho production. IRE cần semantic diff để trả lời:
- thay đổi là gì về mặt ý nghĩa
- mức rủi ro bao nhiêu
- phần nào của workflow có khả năng bị ảnh hưởng
- có cần con người xác nhận không

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
- mô tả rõ hơn nhưng không đổi nghĩa
- thêm chi tiết không ảnh hưởng execution path

### Medium
- thay trade-off hoặc reporting expectations
- sửa criteria không đụng side effects

### High
- thay constraints có thể làm invalid patch/test/approval
- thay authority scope
- thay budget/time cap ảnh hưởng runtime plan

### Critical
- thay policy/compliance
- thêm forbidden action
- invalidate legal/security assumptions
- revoke permissions của hành động đã scheduled

## Human confirmation triggers
- confidence thấp hơn threshold
- change chạm policy/high-risk domain
- multiple conflicting changes
- diff dẫn đến compensation không chắc chắn

## Implementation note
Bản đầu nên là hybrid:
- rule-based deterministic diff cho fields có cấu trúc
- model-assisted classification cho prose / ambiguity
- policy overlay để gán severity

## Acceptance criteria
- diff output phải ổn định cùng input
- cùng change set phải cho ra cùng impact outcome trong cùng rule version
- có thể replay diff dưới historical rule pack
