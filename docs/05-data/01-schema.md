# Data Schema

## OLTP tables

### intents
- intent_id PK
- tenant_id
- workflow_id
- current_version
- status
- created_at
- created_by

### intent_versions
- intent_version_id PK
- intent_id FK
- version_number
- parent_version_id
- created_at
- created_by
- change_reason
- change_channel
- status
- payload_jsonb
- hash

### intent_clauses
- clause_id PK
- intent_version_id FK
- clause_type
- semantic_domain
- key
- operator
- value_jsonb
- priority

### diffs
- diff_id PK
- intent_id FK
- from_version
- to_version
- created_at
- classifier_version
- output_jsonb

### diff_changes
- change_id PK
- diff_id FK
- change_type
- semantic_domain
- severity
- confidence
- affected_clause_ids jsonb
- rationale
- human_confirmation_required

### graph_nodes
- node_id PK
- tenant_id
- workflow_id
- node_type
- external_ref
- state
- metadata_jsonb

### graph_edges
- edge_id PK
- tenant_id
- workflow_id
- from_node_id
- to_node_id
- edge_type
- metadata_jsonb

### artifacts
- artifact_id PK
- tenant_id
- workflow_id
- artifact_type
- storage_uri
- status
- checksum
- created_at
- created_by_run_id

### provenance_records
- provenance_id PK
- artifact_id FK
- intent_version_id
- policy_snapshot_id
- runtime_adapter
- agent_identity
- model_ref
- created_at
- metadata_jsonb

### approvals
- approval_id PK
- tenant_id
- workflow_id
- approval_scope_jsonb
- policy_snapshot_id
- status
- issued_at
- expires_at
- approver_actor

### side_effects
- side_effect_id PK
- tenant_id
- workflow_id
- task_ref
- effect_type
- reversibility_class
- target_ref
- status
- executed_at
- metadata_jsonb

### compensations
- compensation_id PK
- side_effect_id FK
- feasibility
- strategy_type
- status
- requested_at
- executed_at
- metadata_jsonb

### checkpoints
- checkpoint_id PK
- tenant_id
- workflow_id
- runtime_ref
- checkpoint_ref
- created_at
- state_hash

### audit_events
- audit_event_id PK
- tenant_id
- workflow_id
- actor_ref
- event_type
- payload_jsonb
- created_at

## Storage split
- Postgres: metadata, transactional state
- S3: large artifacts
- ClickHouse/OpenSearch: analytics/search
