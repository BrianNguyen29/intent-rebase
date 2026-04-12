-- Migration: 001_create_intents.sql
-- Description: Create intents table for Phase 1 intent registry
-- Created: Phase 1 first slice

CREATE TABLE IF NOT EXISTS intents (
    intent_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    workflow_id UUID NOT NULL,
    current_version INTEGER NOT NULL DEFAULT 1,
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived', 'superseded')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by_actor_type VARCHAR(50) NOT NULL,
    created_by_actor_id VARCHAR(255) NOT NULL,
    source_refs JSONB NOT NULL DEFAULT '[]',
    tags TEXT[] NOT NULL DEFAULT '{}',
    
    -- Optimistic concurrency token
    row_version INTEGER NOT NULL DEFAULT 1
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_intents_tenant_id ON intents(tenant_id);
CREATE INDEX IF NOT EXISTS idx_intents_workflow_id ON intents(workflow_id);
CREATE INDEX IF NOT EXISTS idx_intents_status ON intents(status);
CREATE INDEX IF NOT EXISTS idx_intents_created_at ON intents(created_at DESC);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM intents;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'intents';
-- 3. Verify constraints: SELECT conname FROM pg_constraint WHERE conrelid = 'intents'::regclass;

-- Rollback:
-- DROP TABLE IF EXISTS intents;

-- Comments
COMMENT ON TABLE intents IS 'Phase 1: Intent registry - stores intent documents';
COMMENT ON COLUMN intents.row_version IS 'Optimistic concurrency control token';
COMMENT ON COLUMN intents.source_refs IS 'JSON array of source references [{type, id}]';
