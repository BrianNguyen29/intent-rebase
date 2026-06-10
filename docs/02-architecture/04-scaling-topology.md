# Scaling Topology

## Scale dimensions

### 1. Tenants
Each tenant has:
- own policies
- own connectors
- own retention
- own encryption context

### 2. Workflow count
Number of concurrently running workflows affects:
- write throughput into event log
- graph updates
- console queries
- replay load

### 3. Artifact size
Patches, reports, transcripts, and plans can be large.
Metadata and payload must be separated.

### 4. Rebase frequency
Some domains will have many intent changes within a single workflow.
Requires optimizing incremental impact analysis.

## Logical topology

```text
Edge/API -> Ingestion Pods -> Intent Registry
                        -> Event Bus
Event Bus -> Diff Workers -> Graph Workers -> Rebase Workers
                                 |                 |
                                 v                 v
                              OLTP DB          Runtime Adapters
                                 |
                                 v
                           Analytics Sink
```

## Scaling strategy

### Hot path
- ingestion
- version creation
- diff
- critical impact classification

Optimization:
- async message fan-out
- bounded queues
- priority lanes
- per-tenant backpressure

### Cold path
- replay
- analytics
- full graph scans
- historical audits

Optimization:
- separate warehouse / analytics db
- background indexing
- archival tiers

## Partitioning

### Primary partition keys
- tenant_id
- workflow_id
- session_id

### Secondary keys
- intent_family_id
- actor_id
- domain

## Caching
Cache:
- current intent head
- policy snapshots
- adapter capabilities
- artifact metadata summaries

Do not cache:
- mutable approval decisions if ETag/version is missing
- high-risk action authorizations
