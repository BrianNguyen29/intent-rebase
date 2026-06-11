# Audit and Compliance

## Audit requirements
Record:
- who created intent/change
- which diff was calculated
- which rebase plan was created
- who approved/rejected/applied
- which side effect/compensation occurred
- which policy snapshot was in effect

## Audit event properties
- immutable id
- tenant scoped
- actor identity
- resource refs
- before/after states when appropriate
- rationale
- trace id

## Compliance readiness targets
By market:
- SOC 2 controls mapping
- ISO 27001 operational controls
- internal change management evidence
- customer audit export support

## Tamper resistance
- append-only write path
- periodic digesting/hashing
- optional external audit sink
