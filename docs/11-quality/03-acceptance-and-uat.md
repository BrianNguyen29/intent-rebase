# Acceptance and UAT

## MVP acceptance
- operator can create/read versions
- semantic diff can explain
- rebase preview for 3 main use cases
- full audit trail for core actions

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
- replay export supports investigation
- SLO dashboards operational
- runbooks have been dry-run

## UAT questions
- does the operator understand why an artifact was invalidated?
- does the preview provide enough information to decide?
- is the side-effect residual risk clear?
- is force override being abused?
