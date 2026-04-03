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
