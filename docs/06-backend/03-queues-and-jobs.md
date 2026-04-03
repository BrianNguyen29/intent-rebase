# Queues and Jobs

## Queue classes

### Q1. Real-time control
- intent ingestion
- diff compute
- approval stale detection

SLO: low latency

### Q2. Rebase planning
- graph traversal
- repair planning
- checkpoint candidate selection

SLO: medium latency

### Q3. Heavy background
- replay exports
- analytics materialization
- graph compaction
- cold artifact moves

## Job design rules
- idempotent consumers
- explicit retry policy
- poison message handling
- correlation ids
- tenant-aware backpressure

## Suggested workers
- diff-worker
- graph-worker
- rebase-worker
- policy-worker
- compensation-worker
- replay-worker
- metrics-materializer
