-- Migration: 002_create_intent_versions.sql
-- Description: Create intent_versions table for Phase 1 intent registry
-- Created: Phase 1 first slice

CREATE TABLE IF NOT EXISTS intent_versions (
    intent_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    intent_id UUID NOT NULL REFERENCES intents(intent_id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL,
    parent_version_id UUID REFERENCES intent_versions(intent_version_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by_actor_type VARCHAR(50) NOT NULL,
    created_by_actor_id VARCHAR(255) NOT NULL,
    change_reason TEXT NOT NULL,
    change_channel VARCHAR(30) NOT NULL CHECK (change_channel IN ('user_edit', 'webhook', 'policy_update', 'system_normalization')),
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('draft', 'active', 'rejected', 'superseded')),
    hash VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL,
    
    -- Ensure version numbers are unique per intent
    UNIQUE (intent_id, version_number)
);

-- Indexes
CREATE INDEX idx_intent_versions_intent_id ON intent_versions(intent_id);
CREATE INDEX idx_intent_versions_version_number ON intent_versions(intent_id, version_number DESC);
CREATE INDEX idx_intent_versions_parent_id ON intent_versions(parent_version_id);
CREATE INDEX idx_intent_versions_status ON intent_versions(status);
CREATE INDEX idx_intent_versions_created_at ON intent_versions(created_at DESC);

-- Comments
COMMENT ON TABLE intent_versions IS 'Phase 1: Stores intent version snapshots';
COMMENT ON COLUMN intent_versions.hash IS 'SHA-256 hash of payload for integrity verification';
COMMENT ON COLUMN intent_versions.payload IS 'Full intent payload as JSONB per spec in 01-intent-model.md';
