# SRE and SLOs

## Example SLOs
- 99.9% successful intent version creation
- 99.5% rebase preview availability
- 99.0% rebase apply path availability
- p95 diff compute < 2s for structured changes
- p95 rebase preview < 10s for medium graph size
- 99.9% audit append success

## Error budgets
- separate budgets for preview vs apply path
- critical path incidents consume budget faster

## On-call considerations
- adapter failures
- queue backlogs
- stuck compensations
- approval stale not triggering
- audit append failures

## Phase 3 provisional targets

These targets are Batch 0 planning inputs only.

- They are **not yet SRE-approved**.
- They should be confirmed or adjusted before Batch 2 alerting/dashboards are treated as exit evidence.

### Candidate service-level targets

- 99.9% successful intent version creation
- 99.5% rebase preview availability
- 99.0% rebase apply path availability
- 99.9% audit append success
- 99.0% compensation plan generation success once Batch 1 exists
- 99.0% forensic bundle generation success once Batch 3 exists

### Candidate latency targets

- p95 diff compute < 2s for structured changes
- p95 rebase preview < 10s for medium graph size
- p95 rebase apply < 60s for low/medium risk
- p95 approval wait alert threshold: 30 minutes
- p95 compensation execution target: define after Batch 1 basic flow exists
- p95 forensic bundle generation target: define after Batch 3 implementation data exists

### Batch 2 observability prep notes

- compensation and forensic targets should remain provisional until real implementations and benchmark baselines exist
- queue backlog and consumer lag alerts depend on production NATS/JetStream topology
- artifact quarantine failure alerts depend on a real artifact storage boundary
- forensic export/download alerts depend on Batch 3 API and storage implementation
