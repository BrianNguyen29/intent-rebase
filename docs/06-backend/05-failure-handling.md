# Failure Handling

## Failure classes

### F1. Ingestion failure
- schema invalid
- untrusted source
- duplicate delivery

Actions:
- reject + audit
- optionally create quarantined draft

### F2. Diff failure
- classifier unavailable
- low confidence
- malformed prior version

Actions:
- fallback rules-only
- manual review required

### F3. Graph inconsistency
- missing nodes
- orphan edges
- stale projection

Actions:
- graph repair job
- unsafe auto-apply disabled

### F4. Runtime adapter failure
- pause timeout
- checkpoint lookup failed
- resume rejected

Actions:
- rollback apply if possible
- manual intervention
- mark rebase blocked

### F5. Compensation failure
- external system unavailable
- action non-idempotent
- insufficient permissions

Actions:
- retry if safe
- escalate operator
- annotate residual risk

## Incident policy
Critical incidents require:
- freeze affected workflow
- capture forensic snapshot
- create operator task
- emit high severity audit event
