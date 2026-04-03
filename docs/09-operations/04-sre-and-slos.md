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
