# Storage Strategy

## 1. Postgres
Used for:
- transactional metadata
- intent versions
- diff outputs
- graph edges at v1
- approvals
- primary audit trail

Rationale:
- strong transactions
- flexible JSONB
- recursive CTE sufficient for moderate graphs
- row-level security feasible

## 2. Object Store
Used for:
- patch bundles
- full reports
- transcript chunks
- forensic exports
- replay bundles

## 3. Stream/Event Store
Used for:
- event fan-out
- async processing
- durable decoupling between services

## 4. Analytics Store
Used for:
- SLA dashboards
- rebase metrics
- incident analytics
- tenant usage reports

## Retention
- OLTP operational state: 90–365 days depending on plan
- audit logs: per compliance requirements
- forensic exports: immutable retention policy
- large artifacts: warm/cold tiers
