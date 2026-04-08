# Acceptance and UAT

## MVP acceptance
- operator có thể tạo/đọc versions
- semantic diff có thể giải thích
- rebase preview cho 3 use cases chính
- audit trail đầy đủ cho các hành động cốt lõi

## Phase 2a acceptance (internal groundwork)
- internal apply pipeline wired end-to-end with mock adapter
- checkpoint alignment logic passes alignment tests
- graph state updater transitions validated
- runtime readiness gating functional

## Phase 2b acceptance (external/integrated — prerequisite to Phase 3 full execution)
- low/medium risk apply endpoint operational
- TemporalAdapter external implementation delivered and integrated
- risk classification applied to all intent classes
- approvals revalidation triggered on intent change
- artifact invalidation + quarantine path functional
- graph nodes and edges updated on rebase
- replay API functional with checkpoint support
- event streaming operational (NATS/Kafka)

## Phase 3 acceptance
- compensation flows usable
- replay export điều tra được
- SLO dashboards hoạt động
- runbooks đã dry-run

## UAT questions
- operator có hiểu vì sao artifact bị invalid không?
- preview có đủ thông tin để quyết định không?
- side effect residual risk có rõ không?
- force override có bị lạm dụng không?
