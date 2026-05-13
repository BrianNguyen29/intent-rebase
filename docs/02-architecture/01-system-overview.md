# System Overview

## Kiến trúc ở mức cao

```text
[User / Spec Source / Ticket / Policy Update]
                  |
                  v
         [Intent Ingestion Layer]
                  |
                  v
        [Intent Store + Version Graph]
                  |
                  v
          [Semantic Diff Engine]
                  |
                  v
         [Trace Graph / Impact Engine]
                  |
          +--------+---------+
          |                  |
          v                  v
  [Repair / Rebase Planner] [Policy / Approval Evaluator]
          |                  |
          +--------+---------+
                   |
                   v
    [Runtime Adapter / Workflow Integrations]
                   |
                   v
       [Agent Runtime / Planner / Workers]
                   |
                   v
    [Artifacts, Side Effects, Memory, Checkpoints]
                   |
                   v
     [Audit, Replay, Console, Webhooks, Analytics]
```

### Policy Snapshot → Impact Report Delegation Path

**Bounded MVP (ADR-11, implemented):**
```text
GET /policy-snapshots/{snapshot_id}/impact-report
  ├── Fetch policy snapshot by ID
  ├── Validate tenant_id matches snapshot.tenant_id
  ├── Extract snapshot.intent_id
  └── Delegate to build_impact_report_response(intent_id, tenant_id, from_version, to_version)
      └── Returns ImpactReportResponse (identical to GET /intents/{intent_id}/impact-report)
```

- **No new persistence** — reuses `policy_snapshots` table and existing intent repositories
- **No new executor** — delegates to existing ImpactReport builder (ADR-10)
- **No mutation** — endpoint is read-only
- **Full PolicyRebaseAdapter deferred to Phase 4+** — cross-intent policy lookup, synthetic `IntentVersionDiff` generation, and policy-specific preview/apply pipelines remain future design

## Các lớp chính

### 1. Control Plane
- Intent ingestion
- versioning
- diff
- impact analysis
- repair planning
- policy-aware decisions
- audit

### 2. Execution Plane
- adapters tới workflow runtimes
- agent runtimes
- task schedulers
- side effect dispatchers

### 3. Data Plane
- OLTP metadata
- event log
- object store
- graph store / relational edges
- analytics store

### 4. Operator Plane
- console
- approval UI
- forensic replay
- policy simulation
- rebase previews

## Mục tiêu kiến trúc

- composable
- auditable
- deterministic ở phần control logic
- eventually consistent ở phần integrations nơi cần
- multi-tenant
- failure-tolerant
