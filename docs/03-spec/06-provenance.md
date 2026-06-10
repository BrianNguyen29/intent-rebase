# Provenance Specification

## Purpose

Enables answering:
- when this output was produced
- under which intent version
- from which input/source
- under which policy snapshot
- by which agent/runtime
- after which rebase

## Provenance envelope

```yaml
provenance_id: uuid
artifact_id: uuid
tenant_id: uuid
workflow_id: uuid
intent_version_id: uuid
change_set_id: uuid|null
policy_snapshot_id: uuid|null
runtime_adapter: temporal|langgraph|custom
agent_identity: string
model_ref: string|null
source_refs:
  - type: spec|chat|ticket|webhook|policy
    id: string
created_at: timestamp
created_by_run_id: uuid
```

## Requirements
- immutable once written
- append-only updates via superseding artifact
- queryable in UI and APIs
- included in forensic export

## Provenance-aware policies

Can define:
- do not use an artifact produced before policy snapshot X
- do not allow merging output under a stale intent version
- do not reuse an approval more than N versions old
