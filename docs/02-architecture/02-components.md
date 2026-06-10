# Components

## 1. Intent Ingestion Service
### Responsibilities
- receive sources: chat, markdown spec, issue comment, webhook, API call
- normalize to Intent DTO
- validate schema
- enrich metadata: actor, source, timestamps, tenant, workflow refs

### Interface
- REST API
- Git provider webhooks
- ticketing connectors
- policy system events

## 2. Intent Registry
### Responsibilities
- store current intent and version history
- manage lineage between versions
- support compare and snapshot retrieval

### Requirements
- immutable version records
- mutable "current head" pointer per workflow/session
- optimistic concurrency control

## 3. Semantic Diff Engine
### Responsibilities
- compare intent versions
- emit machine-readable change set
- assign severity and confidence
- separate low-risk vs high-risk changes

### Sample Output
- change_type
- affected_fields
- rationale
- confidence
- policy_relevance
- requires_human_confirmation

## 4. Trace Graph Service
### Responsibilities
- manage relationships between intent clauses and artifacts/actions
- query impact radius
- compute transitive dependencies

## 5. Impact Analysis Engine
### Responsibilities
- run propagation rules on the graph
- emit list:
  - still_valid
  - review_required
  - invalid
  - compensatable
  - restart_required

## 6. Rebase Planner
### Responsibilities
- create repair plan
- compute checkpoint resume point
- insert approval steps
- generate compensation tasks if needed

## 7. Runtime Adapter Layer
### Responsibilities
- translate rebase plan to specific workflow runtime
- pause/resume/cancel/branch execution
- attach intent_version metadata to runs

## 8. Policy / Approval Evaluator
### Responsibilities
- determine which approvals must be re-requested
- check authority scope, cost caps, forbidden actions
- evaluate under new policy snapshot

## 9. Artifact Service
### Responsibilities
- manage outputs, patches, summaries, test reports, decision docs
- store object payloads outside metadata

## 10. Side Effect Ledger
### Responsibilities
- classify actions:
  - pure read
  - internal write
  - external reversible
  - external irreversible
- attach compensation strategy

## 11. Audit and Replay Service
### Responsibilities
- record event log
- reconstruct timeline
- replay decisions
- export forensic bundle

## 12. Operator Console
### Responsibilities
- display intent diff
- impact map
- rebase preview
- approval UI
- incident timeline
