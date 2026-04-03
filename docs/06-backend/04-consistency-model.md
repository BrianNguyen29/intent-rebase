# Consistency Model

## Nguyên tắc
Không cần strong consistency mọi nơi. Nhưng một số điểm bắt buộc strong/serializable hơn:
- tạo intent version
- apply rebase plan
- approval status transitions
- side effect dispatch preflight

## Model đề xuất

### Stronger consistency areas
- `intents.current_version`
- `rebases.apply`
- `approvals.status`
- `side_effects.status`

### Eventual consistency areas
- analytics dashboards
- search indexes
- non-critical graph projections
- operator insights summaries

## Techniques
- optimistic concurrency với version numbers
- transactional outbox
- idempotency keys
- compare-and-swap cho apply rebase
- saga patterns cho multi-step external effects

## Critical race conditions cần xử lý
1. Intent head đổi giữa preview và apply
2. Approval bị revoke trong lúc workflow chuẩn bị side effect
3. Compensation chạy trong khi operator force restart
4. Runtime state đổi trong lúc graph snapshot đã cũ

## Rule
Apply rebase phải kiểm tra:
- current intent head == rebase_plan.to_version
- runtime execution state hash khớp hoặc nằm trong allowed window
