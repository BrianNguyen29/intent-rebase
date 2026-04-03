# Event Contracts

## Eventing principles
- tất cả events có `event_id`, `event_type`, `event_time`, `tenant_id`, `workflow_id`, `trace_id`
- schema versioned
- at-least-once delivery
- consumers phải idempotent

## Core events

### intent.created
### intent.version_created
### intent.diff_computed
### intent.change_classified
### graph.impact_computed
### rebase.plan_created
### rebase.plan_approved
### rebase.plan_applied
### rebase.plan_failed
### approval.stale_detected
### approval.revalidated
### side_effect.recorded
### side_effect.compensation_requested
### side_effect.compensation_executed
### artifact.invalidated
### artifact.quarantined
### workflow.rebased
### workflow.restart_required
### audit.export_generated

## Sample event

```json
{
  "event_id": "uuid",
  "event_type": "rebase.plan_created",
  "schema_version": 1,
  "event_time": "2026-04-03T10:00:00Z",
  "tenant_id": "uuid",
  "workflow_id": "uuid",
  "trace_id": "trace-123",
  "payload": {
    "rebase_plan_id": "uuid",
    "from_version": 5,
    "to_version": 6,
    "classification": "partial_repair",
    "invalid_count": 3,
    "review_required_count": 2,
    "approval_required_count": 1
  }
}
```

## Topics/streams đề xuất
- `intent-events`
- `rebase-events`
- `approval-events`
- `side-effect-events`
- `audit-events`

## DLQ rules
- poison events sau N retries
- schema incompatibility
- tenant routing failure
