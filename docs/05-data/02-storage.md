# Storage Strategy

## 1. Postgres
Dùng cho:
- metadata có tính giao dịch
- intent versions
- diff outputs
- graph edges ở v1
- approvals
- audit trail chính

Lý do:
- transaction tốt
- JSONB linh hoạt
- recursive CTE đủ cho graph vừa
- row-level security khả thi

## 2. Object Store
Dùng cho:
- patch bundles
- full reports
- transcript chunks
- forensic exports
- replay bundles

## 3. Stream/Event Store
Dùng cho:
- event fan-out
- async processing
- durable decoupling giữa services

## 4. Analytics Store
Dùng cho:
- SLA dashboards
- rebase metrics
- incident analytics
- tenant usage reports

## Retention
- OLTP operational state: 90–365 ngày tùy plan
- audit logs: theo compliance
- forensic exports: immutable retention policy
- artifacts lớn: warm/cold tiers
