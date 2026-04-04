# Data Schema

## Implementation Status

**Phase 1 First Slice**: Migration baseline created in `infrastructure/migrations/`:
- `001_create_intents.sql` ✅
- `002_create_intent_versions.sql` ✅
- `003_create_intent_clauses.sql` ✅

**Phase 1 PR #9 Graph Storage Baseline**: Graph nodes/edges tables for dependency tracking:
- `004_create_graph_nodes.sql` ✅
- `005_create_graph_edges.sql` ✅

Remaining tables planned for Phase 2+.

## OLTP tables

### intents ✅ (Phase 1 - Migration 001)
- intent_id PK (UUID)
- tenant_id (UUID)
- workflow_id (UUID)
- current_version (INTEGER)
- status (VARCHAR - active/archived/superseded)
- created_at (TIMESTAMPTZ)
- created_by (actor_ref split into actor_type and actor_id)
- source_refs (JSONB)
- tags (TEXT[])
- row_version (INTEGER) - optimistic concurrency token

### intent_versions ✅ (Phase 1 - Migration 002)
- intent_version_id PK (UUID)
- intent_id FK (UUID)
- version_number (INTEGER)
- parent_version_id (UUID, nullable)
- created_at (TIMESTAMPTZ)
- created_by (actor_ref split)
- change_reason (TEXT)
- change_channel (VARCHAR - user_edit/webhook/policy_update/system_normalization)
- status (VARCHAR - draft/active/rejected/superseded)
- payload (JSONB)
- hash (VARCHAR(64) - SHA-256 for integrity)

### intent_clauses ✅ (Phase 1 - Migration 003)
- clause_id PK (UUID)
- intent_version_id FK (UUID)
- clause_type (VARCHAR - functional/non_functional/policy/budget/time)
- semantic_domain (VARCHAR)
- key (VARCHAR)
- operator (VARCHAR - eq/neq/lt/lte/gt/gte/contains/not_contains/regex/custom)
- value (JSONB)
- priority (VARCHAR - must/should/could)
- rationale (TEXT, nullable)
- created_at (TIMESTAMPTZ)

### diffs 🔜 (Phase 2+)
- diff_id PK
- intent_id FK
- from_version
- to_version
- created_at
- classifier_version
- output_jsonb

### diff_changes 🔜 (Phase 2+)
- change_id PK
- diff_id FK
- change_type
- semantic_domain
- severity
- confidence
- affected_clause_ids jsonb
- rationale
- human_confirmation_required

### graph_nodes ✅ (Phase 1 - Migration 004)
- node_id PK (UUID)
- tenant_id (UUID)
- workflow_id (UUID)
- node_type (VARCHAR - intent/intent_version/artifact/approval/policy_snapshot/side_effect/checkpoint/workflow/generic)
- external_ref_type (VARCHAR, nullable - references meta_ref_types)
- external_ref_id (UUID, nullable, but only valid when external_ref_type is also set)
- label (VARCHAR)
- state (VARCHAR - active/stale/invalid/archived)
- properties (JSONB)
- created_at (TIMESTAMPTZ)
- updated_at (TIMESTAMPTZ)
- **Invariant**: external_ref_type and external_ref_id must be both NULL or both present (enforced by CHECK constraint)
- Unique constraint on (tenant_id, workflow_id, external_ref_type, external_ref_id)

### graph_edges ✅ (Phase 1 - Migration 005)
- edge_id PK (UUID)
- tenant_id (UUID)
- workflow_id (UUID)
- from_node_id (UUID FK to graph_nodes)
- to_node_id (UUID FK to graph_nodes)
- edge_type (VARCHAR - depends_on/produces/approves/triggers/defines/generated_from/validated_by/governed_by/derived_from/stored_in/supersedes/blocks/compensates)
- properties (JSONB)
- created_at (TIMESTAMPTZ)
- Unique constraint on (tenant_id, from_node_id, to_node_id, edge_type)
- **Integrity constraint**: `from_node` and `to_node` must have the same `tenant_id` and `workflow_id` as the edge row itself. Enforced by `graph_edges_validate_tenant_workflow` trigger (migration 005). This prevents silent cross-tenant/workflow edges.

### artifacts 🔜 (Phase 2+)
- artifact_id PK
- tenant_id
- workflow_id
- artifact_type
- storage_uri
- status
- checksum
- created_at
- created_by_run_id

### provenance_records 🔜 (Phase 2+)
- provenance_id PK
- artifact_id FK
- intent_version_id
- policy_snapshot_id
- runtime_adapter
- agent_identity
- model_ref
- created_at
- metadata_jsonb

### approvals 🔜 (Phase 2+)
- approval_id PK
- tenant_id
- workflow_id
- approval_scope_jsonb
- policy_snapshot_id
- status
- issued_at
- expires_at
- approver_actor

### side_effects 🔜 (Phase 2+)
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

### compensations 🔜 (Phase 2+)
- compensation_id PK
- side_effect_id FK
- feasibility
- strategy_type
- status
- requested_at
- executed_at
- metadata_jsonb

### checkpoints 🔜 (Phase 2+)
- checkpoint_id PK
- tenant_id
- workflow_id
- runtime_ref
- checkpoint_ref
- created_at
- state_hash

### audit_events 🔜 (Phase 2+)
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

## Migration Files

Located in `infrastructure/migrations/`:
- `001_create_intents.sql` - Creates intents table with indexes
- `002_create_intent_versions.sql` - Creates intent_versions table with indexes
- `003_create_intent_clauses.sql` - Creates intent_clauses table with indexes
- `004_create_graph_nodes.sql` - Creates graph_nodes table with indexes (Phase 1 PR #9)
- `005_create_graph_edges.sql` - Creates graph_edges table with indexes (Phase 1 PR #9)
