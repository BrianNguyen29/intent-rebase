# Dependency Graph Specification

## Mục tiêu
Graph là lõi của impact analysis. Nếu không có graph đủ tốt, rebase sẽ hoặc:
- quá bảo thủ: invalidate quá nhiều
- quá lạc quan: bỏ sót hậu quả

## Node types
- IntentClause
- IntentVersion
- PlanNode
- TaskNode
- AgentRun
- ToolCall
- Artifact
- TestCase
- Approval
- PolicySnapshot
- SideEffect
- MemoryItem
- Checkpoint

## Edge types
- `defines`
- `depends_on`
- `generated_from`
- `validated_by`
- `approved_by`
- `governed_by`
- `derived_from`
- `stored_in`
- `supersedes`
- `compensates`
- `blocked_by`

## Ví dụ quan hệ
- `Artifact patch-42 depends_on IntentClause compatibility-must`
- `Approval appr-7 governed_by PolicySnapshot pol-14`
- `ToolCall deploy-1 blocked_by Approval appr-7`
- `Checkpoint cp-9 supersedes cp-8`

## Graph invariants
1. Mọi Artifact phải trace được về ít nhất một IntentVersion.
2. Mọi SideEffect phải trace được về:
   - initiating TaskNode
   - intent version
   - approval snapshot nếu có
3. Mọi Approval phải gắn policy snapshot và scope.

## Storage strategy
### OLTP relational
Cho metadata và edge tables có query đơn giản.

### Optional graph engine
Dùng khi:
- traversal sâu
- causal analysis nặng
- cross-artifact visualization phức tạp

Khuyến nghị production v1:
- Postgres với edge tables + recursive CTE
- chưa cần graph DB riêng trừ khi scale hoặc query patterns đòi hỏi

## Impact propagation rules
Ví dụ:
- Nếu `IntentClause` bị `tighten_constraint` và `Artifact depends_on clause`, artifact -> `review_required` hoặc `invalid` tùy domain.
- Nếu `Approval governed_by PolicySnapshot old` và policy domain bị đổi ở mức high, approval -> `stale`.
- Nếu `SideEffect` thuộc lớp irreversible và upstream change invalidates scope, trigger operator escalation.
