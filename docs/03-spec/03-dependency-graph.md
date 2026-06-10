# Dependency Graph Specification

## Purpose

The graph is the core of impact analysis. Without a good-enough graph, rebase will either:
- be too conservative: invalidate too much
- be too optimistic: miss consequences

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

## Example Relationships
- `Artifact patch-42 depends_on IntentClause compatibility-must`
- `Approval appr-7 governed_by PolicySnapshot pol-14`
- `ToolCall deploy-1 blocked_by Approval appr-7`
- `Checkpoint cp-9 supersedes cp-8`

## Graph invariants
1. Every Artifact must be traceable to at least one IntentVersion.
2. Every SideEffect must be traceable to:
   - the initiating TaskNode
   - the intent version
   - an approval snapshot if applicable
3. Every Approval must be attached to a policy snapshot and scope.

## Storage strategy
### OLTP relational
For metadata and edge tables with simple queries.

### Optional graph engine
Use when:
- deep traversal
- heavy causal analysis
- complex cross-artifact visualization

Production v1 recommendation:
- Postgres with edge tables + recursive CTE
- no separate graph DB needed unless scale or query patterns demand it

## Impact propagation rules
Examples:
- If an `IntentClause` is `tighten_constraint` and an `Artifact depends_on` that clause, the artifact becomes `review_required` or `invalid` depending on domain.
- If an `Approval` is `governed_by` an old `PolicySnapshot` and the policy domain is changed at high severity, the approval becomes `stale`.
- If a `SideEffect` is of the irreversible class and an upstream change invalidates its scope, trigger operator escalation.
