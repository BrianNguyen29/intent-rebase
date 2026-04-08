-- Migration: 007_create_checkpoints.sql
-- Description: Create checkpoints table for Phase 2 Temporal checkpoint mapping workstream
-- Created: Phase 2 PR - checkpoint data model
-- Storage: Postgres with JSONB for flexible state storage
-- See: docs/04-phase-2/02-checkpoint-mapping.md (checkpoint specification)

-- Rollback:
-- DROP INDEX IF EXISTS idx_checkpoints_workflow_state ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_expires_at ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_created_at ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_status ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_checkpoint_type ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_intent_version ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_intent_id ON checkpoints;
-- DROP INDEX IF EXISTS idx_checkpoints_tenant_workflow ON checkpoints(tenant_id, workflow_id);
-- DROP INDEX IF EXISTS idx_checkpoints_tenant_id ON checkpoints;
-- DROP TABLE IF EXISTS checkpoints;

-- Create checkpoint_type enum
DO $$ BEGIN
    CREATE TYPE checkpoint_type AS ENUM (
        'initial',
        'pre_flight',
        'intent_received',
        'intent_validated',
        'rebase_started',
        'rebase_completed',
        'final',
        'custom'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create checkpoint_status enum
DO $$ BEGIN
    CREATE TYPE checkpoint_status AS ENUM (
        'pending',
        'created',
        'active',
        'superseded',
        'expired',
        'invalidated'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main checkpoints table
CREATE TABLE IF NOT EXISTS checkpoints (
    -- Primary key
    checkpoint_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Intent association
    intent_id UUID NOT NULL,
    intent_version INT NOT NULL,
    
    -- Workflow association
    workflow_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    
    -- Workflow state at checkpoint time (serialized as JSONB)
    workflow_state JSONB NOT NULL DEFAULT '{}',
    
    -- Checkpoint classification
    checkpoint_type checkpoint_type NOT NULL DEFAULT 'initial',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- NULL means never expires
    
    -- Status tracking
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending', 'created', 'active', 'superseded', 'expired', 'invalidated'
    )),
    
    -- Additional metadata as JSONB for flexibility
    metadata JSONB NOT NULL DEFAULT '{}'
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_checkpoints_tenant_id ON checkpoints(tenant_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_tenant_workflow ON checkpoints(tenant_id, workflow_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_intent_id ON checkpoints(intent_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_intent_version ON checkpoints(intent_id, intent_version DESC);

-- Indexes for filtering and sorting
CREATE INDEX IF NOT EXISTS idx_checkpoints_checkpoint_type ON checkpoints(checkpoint_type);
CREATE INDEX IF NOT EXISTS idx_checkpoints_status ON checkpoints(status);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created_at ON checkpoints(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_checkpoints_expires_at ON checkpoints(expires_at) WHERE expires_at IS NOT NULL;

-- Index for workflow state queries (GIN index for JSONB)
CREATE INDEX IF NOT EXISTS idx_checkpoints_workflow_state ON checkpoints USING GIN (workflow_state);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM checkpoints;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'checkpoints';
-- 3. Verify enums exist: SELECT typname FROM pg_type WHERE typname IN ('checkpoint_type', 'checkpoint_status');
-- 4. Verify CHECK constraints: SELECT conname FROM pg_constraint WHERE conrelid = 'checkpoints'::regclass AND contype = 'c';
-- 5. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'checkpoints';

-- Comments
COMMENT ON TABLE checkpoints IS 'Phase 2: Checkpoints for Temporal workflow checkpoint mapping - enables replay from specific intent versions';
COMMENT ON COLUMN checkpoints.checkpoint_id IS 'Unique identifier for this checkpoint';
COMMENT ON COLUMN checkpoints.intent_id IS 'Intent this checkpoint is associated with';
COMMENT ON COLUMN checkpoints.intent_version IS 'Version of the intent at checkpoint time';
COMMENT ON COLUMN checkpoints.workflow_id IS 'Workflow this checkpoint belongs to';
COMMENT ON COLUMN checkpoints.tenant_id IS 'Tenant this checkpoint belongs to';
COMMENT ON COLUMN checkpoints.workflow_state IS 'Serialized workflow state at checkpoint time (JSONB)';
COMMENT ON COLUMN checkpoints.checkpoint_type IS 'Type of checkpoint indicating position in workflow lifecycle';
COMMENT ON COLUMN checkpoints.created_at IS 'Timestamp when checkpoint was created';
COMMENT ON COLUMN checkpoints.expires_at IS 'Timestamp when checkpoint expires (NULL = never expires)';
COMMENT ON COLUMN checkpoints.status IS 'Current status of the checkpoint';
COMMENT ON COLUMN checkpoints.metadata IS 'Additional metadata as JSONB for flexibility';
