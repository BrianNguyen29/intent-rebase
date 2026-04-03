# Scaling Topology

## Scale dimensions

### 1. Tenants
Mỗi tenant có:
- policies riêng
- connectors riêng
- retention riêng
- encryption context riêng

### 2. Workflow count
Số workflow chạy đồng thời ảnh hưởng:
- write throughput vào event log
- graph updates
- console queries
- replay load

### 3. Artifact size
Patches, reports, transcripts, plans có thể lớn.
Metadata và payload phải tách.

### 4. Rebase frequency
Một số domains sẽ có nhiều intent changes trong một workflow.
Cần tối ưu incremental impact analysis.

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

Tối ưu:
- async message fan-out
- bounded queues
- priority lanes
- per-tenant backpressure

### Cold path
- replay
- analytics
- full graph scans
- historical audits

Tối ưu:
- warehouse / analytics db riêng
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

Không cache:
- mutable approval decisions nếu thiếu ETag/version
- high-risk action authorizations
