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
CREATE INDEX idx_intents_tenant_id ON intents(tenant_id);
CREATE INDEX idx_intents_workflow_id ON intents(workflow_id);
CREATE INDEX idx_intents_status ON intents(status);
CREATE INDEX idx_intents_created_at ON intents(created_at DESC);

-- Comments
COMMENT ON TABLE intents IS 'Phase 1: Intent registry - stores intent documents';
COMMENT ON COLUMN intents.row_version IS 'Optimistic concurrency control token';
COMMENT ON COLUMN intents.source_refs IS 'JSON array of source references [{type, id}]';
