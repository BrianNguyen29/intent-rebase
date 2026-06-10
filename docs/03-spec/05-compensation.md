# Compensation Model

## Why Compensation Is Needed

Not every task is purely computational. Many workflows have side effects:
- sending email
- opening a PR
- deploying
- creating a ticket
- modifying the DB
- posting a message to a channel
- approving / rejecting a transaction

If the intent changes after a side effect has occurred, merely invalidating the artifact is not enough.

## Side effect classes

### S0 — Pure read
No compensation needed.

### S1 — Internal reversible
Example: internal metadata writes that can be rolled back transactionally.

### S2 — External reversible
Example: creating a ticket that can later be closed/cancelled; opening a PR that can later be closed.

### S3 — External partially reversible
Example: sending an email that can be followed up with a correction, but cannot be absolutely recalled.

### S4 — Irreversible
Example: transferring money, making something public, deleting data without backup.

## Compensation record

```yaml
compensation_id: uuid
side_effect_id: uuid
feasibility: automatic|semi_automatic|manual_only|not_possible
strategy_type: rollback|counter_action|followup_notice|quarantine|escalation
required_approvals: [approval_rule_ref]
generated_at: timestamp
status: pending|approved|executed|failed|waived
```

## Rules
- S0: skip
- S1: auto if policy permits
- S2: auto or semi-auto depending on risk
- S3: operator review by default
- S4: mandatory escalation

## UI requirements

The operator must see:
- which side effects have occurred
- which intent change made it problematic
- the proposed compensation plan
- the residual risk after compensation
