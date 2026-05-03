-- Migration: 009_add_rebase_apply_blocked_audit_event.sql
-- Description: Add RebaseApplyBlocked to audit_event_type enum AND create policy_snapshot table
-- Created: Phase 2b PR - add RebaseApplyBlocked to existing enum
-- Target: Extend audit_event_type enum without recreating
-- Policy Snapshot: Phase 2 governance bounded slice - see docs/14-governance/03-policy-snapshot-spec.md
-- Note: This file was consolidated from two original 009 migrations to resolve duplicate version conflict.
-- Migration 013 depends on policy_snapshot table.

-- Rollback:
-- ALTER TABLE audit_events DROP CONSTRAINT IF EXISTS audit_events_event_type_check;
-- ALTER TABLE audit_events ADD CONSTRAINT audit_events_event_type_check
--   CHECK (event_type IN ('IntentCreated', 'IntentUpdated', 'IntentArchived', 'RebaseDetected',
--                         'RebasePreviewGenerated', 'RebaseApplied', 'ApprovalRequired',
--                         'ApprovalGranted', 'ApprovalRevoked', 'ArtifactProduced', 'ArtifactInvalidated'));
-- DROP INDEX IF EXISTS idx_policy_snapshot_hash ON policy_snapshot;
-- DROP INDEX IF EXISTS idx_policy_snapshot_intent_version ON policy_snapshot;
-- DROP TABLE IF EXISTS policy_snapshot;

-- =============================================================================
-- Part 1: Add RebaseApplyBlocked to existing enum
-- =============================================================================

-- Add RebaseApplyBlocked to existing enum using ALTER TYPE
-- This is safe because it only adds a new value, existing rows remain valid
ALTER TYPE audit_event_type ADD VALUE IF NOT EXISTS 'RebaseApplyBlocked';

-- Post-migration validation notes:
-- 1. Verify enum has new value: SELECT enumlabel FROM pg_enum WHERE enumtypid = 'audit_event_type'::regtype ORDER BY enumlabel;
-- 2. Verify constraint: SELECT conname FROM pg_constraint WHERE conrelid = 'audit_events'::regclass AND conkey = ARRAY[3];
-- 3. Verify no rows violated: SELECT event_type, COUNT(*) FROM audit_events GROUP BY event_type;

-- Comments
COMMENT ON TYPE audit_event_type IS 'Updated: Added RebaseApplyBlocked for Phase 2b bounded slice - emitted when external rebase-apply hits blocked D/E path';

-- =============================================================================
-- Part 2: Create policy_snapshot table for Phase 2 governance bounded slice
-- =============================================================================

-- Main policy_snapshot table
CREATE TABLE IF NOT EXISTS policy_snapshot (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant and intent association
    tenant_id UUID NOT NULL,
    intent_id UUID NOT NULL REFERENCES intents(intent_id),
    intent_version INT NOT NULL,

    -- Policy state at snapshot time
    rule_pack_version TEXT NOT NULL,

    -- Scope definition (what requires approval)
    scope_type TEXT NOT NULL CHECK (scope_type IN ('full', 'partial', 'none')),
    affected_resources JSONB NOT NULL DEFAULT '[]',
    required_approvers JSONB NOT NULL DEFAULT '[]',
    min_approvals INT NOT NULL DEFAULT 1,

    -- Integrity hash of scope_definition
    scope_hash TEXT NOT NULL,

    -- Snapshot URI (placeholder - actual S3 URI populated when S3 upload is implemented)
    -- For now, this is a memory:// URI indicating the snapshot exists but blob is not yet persisted
    snapshot_uri TEXT NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    canonicalized_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique constraint: one snapshot per intent version
    UNIQUE(intent_id, intent_version)
);

-- Index for intent version lookups (most common query)
CREATE INDEX IF NOT EXISTS idx_policy_snapshot_intent_version
    ON policy_snapshot(tenant_id, intent_id, intent_version DESC);

-- Index for scope hash lookups (used during revalidation - when implemented)
CREATE INDEX IF NOT EXISTS idx_policy_snapshot_hash
    ON policy_snapshot(tenant_id, scope_hash);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM policy_snapshot;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'policy_snapshot';
-- 3. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'policy_snapshot';

-- Comments
COMMENT ON TABLE policy_snapshot IS 'Phase 2 governance: Policy snapshots for point-in-time approval policy records - bounded slice (S3/canonicalization/revalidation out of scope)';
COMMENT ON COLUMN policy_snapshot.id IS 'Unique identifier for this policy snapshot';
COMMENT ON COLUMN policy_snapshot.tenant_id IS 'Tenant this snapshot belongs to';
COMMENT ON COLUMN policy_snapshot.intent_id IS 'Intent this snapshot is associated with';
COMMENT ON COLUMN policy_snapshot.intent_version IS 'Intent version this snapshot was created for';
COMMENT ON COLUMN policy_snapshot.rule_pack_version IS 'Rule pack version active at snapshot creation time';
COMMENT ON COLUMN policy_snapshot.scope_type IS 'Type of scope: full, partial, or none';
COMMENT ON COLUMN policy_snapshot.affected_resources IS 'Resources affected by the intent at approval time';
COMMENT ON COLUMN policy_snapshot.required_approvers IS 'Approvers required for approval at snapshot time';
COMMENT ON COLUMN policy_snapshot.min_approvals IS 'Minimum number of approvals required';
COMMENT ON COLUMN policy_snapshot.scope_hash IS 'SHA256 hash of scope_definition for integrity verification';
COMMENT ON COLUMN policy_snapshot.snapshot_uri IS 'URI to immutable snapshot blob (S3 URI when S3 upload implemented, memory:// placeholder for now)';
COMMENT ON COLUMN policy_snapshot.created_at IS 'Timestamp when snapshot was created';
COMMENT ON COLUMN policy_snapshot.canonicalized_at IS 'Timestamp when scope was canonicalized (placeholder - canonicalization out of scope)';
