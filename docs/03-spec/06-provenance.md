# Provenance Specification

## Mục tiêu
Cho phép trả lời:
- output này sinh khi nào
- dưới intent version nào
- từ input/source nào
- dưới policy snapshot nào
- bởi agent/runtime nào
- sau rebase nào

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
Có thể định nghĩa:
- không dùng artifact sinh trước policy snapshot X
- không cho merge output dưới intent version stale
- không tái sử dụng approval quá N versions cũ
