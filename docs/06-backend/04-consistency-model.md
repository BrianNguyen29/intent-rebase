# Consistency Model

## Principle
Strong consistency is not needed everywhere. But some points require stronger/serializable consistency:
- create intent version
- apply rebase plan
- approval status transitions
- side effect dispatch preflight

## Proposed model

### Stronger consistency areas
- `intents.current_version`
- `rebases.apply`
- `approvals.status`
- `side_effects.status`

### Eventual consistency areas
- analytics dashboards
- search indexes
- non-critical graph projections
- operator insights summaries

## Techniques
- optimistic concurrency with version numbers
- transactional outbox
- idempotency keys
- compare-and-swap for apply rebase
- saga patterns for multi-step external effects

## Critical race conditions to handle
1. Intent head changes between preview and apply
2. Approval is revoked while workflow is preparing side effect
3. Compensation runs while operator force-restarts
4. Runtime state changes while graph snapshot is stale

## Rule
Apply rebase must check:
- current intent head == rebase_plan.to_version
- runtime execution state hash matches or falls within allowed window
