-- Migration: 010_create_side_effects_ledger.sql
-- Description: Create side_effects table for Phase 3 Batch 1 side effect ledger groundwork
-- Created: Phase 3 Batch 1 PR - side effect ledger
-- Storage: Postgres with UUID primary key and indexes for efficient lookup
-- See: docs/10-delivery/05-phase-3-hardening.md (item 1-1)

-- Rollback:
-- DROP INDEX IF EXISTS idx_side_effects_tenant_idempotency_key_unique ON side_effects;
-- DROP INDEX IF EXISTS idx_side_effects_tenant_id ON side_effects;
-- DROP INDEX IF EXISTS idx_side_effects_intent_id ON side_effects;
-- DROP INDEX IF EXISTS idx_side_effects_idempotency_key ON side_effects;
-- DROP INDEX IF EXISTS idx_side_effects_occurred_at ON side_effects;
-- DROP INDEX IF EXISTS idx_side_effects_effect_class ON side_effects;
-- DROP TABLE IF EXISTS side_effects;

-- Create effect_class enum
DO $$ BEGIN
    CREATE TYPE effect_class AS ENUM (
        's0_pure_read',
        's1_internal_reversible',
        's2_external_reversible',
        's3_external_partially_reversible',
        's4_irreversible'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main side_effects table
CREATE TABLE IF NOT EXISTS side_effects (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Tenant isolation
    tenant_id UUID NOT NULL,
    
    -- Intent association
    intent_id UUID NOT NULL,
    intent_version INT NOT NULL,
    
    -- Effect classification
    effect_class effect_class NOT NULL DEFAULT 's0_pure_read',
    
    -- Effect details
    effect_type VARCHAR(100) NOT NULL,
    target VARCHAR(500) NOT NULL,
    
    -- Timestamp when the effect occurred
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Idempotency key to prevent duplicate compensation (unique per tenant)
    idempotency_key VARCHAR(255)
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_side_effects_tenant_id ON side_effects(tenant_id);
CREATE INDEX IF NOT EXISTS idx_side_effects_intent_id ON side_effects(intent_id);
CREATE INDEX IF NOT EXISTS idx_side_effects_intent_tenant ON side_effects(intent_id, tenant_id);

-- Index for idempotency key lookups (unique per tenant)
CREATE INDEX IF NOT EXISTS idx_side_effects_idempotency_key 
    ON side_effects(tenant_id, idempotency_key) 
    WHERE idempotency_key IS NOT NULL;

-- Partial unique index for tenant-scoped idempotency keys
CREATE UNIQUE INDEX IF NOT EXISTS idx_side_effects_tenant_idempotency_key_unique
    ON side_effects(tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- Indexes for filtering and sorting
CREATE INDEX IF NOT EXISTS idx_side_effects_occurred_at ON side_effects(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_side_effects_effect_class ON side_effects(effect_class);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM side_effects;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'side_effects';
-- 3. Verify enum exists: SELECT typname FROM pg_type WHERE typname = 'effect_class';
-- 4. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'side_effects';

-- Comments
COMMENT ON TABLE side_effects IS 'Phase 3 Batch 1: Side effect ledger - records effects emitted by artifact-producing operations for compensation planning';
COMMENT ON COLUMN side_effects.id IS 'Unique identifier for this side effect record';
COMMENT ON COLUMN side_effects.tenant_id IS 'Tenant this side effect belongs to';
COMMENT ON COLUMN side_effects.intent_id IS 'Intent that produced this side effect';
COMMENT ON COLUMN side_effects.intent_version IS 'Intent version at time of effect emission';
COMMENT ON COLUMN side_effects.effect_class IS 'Severity class (S0-S4) of the side effect';
COMMENT ON COLUMN side_effects.effect_type IS 'Effect type identifier (e.g. email_sent, pr_opened, ticket_created)';
COMMENT ON COLUMN side_effects.target IS 'Target of the effect (e.g. email address, PR URL, ticket ID)';
COMMENT ON COLUMN side_effects.occurred_at IS 'Timestamp when the effect occurred';
COMMENT ON COLUMN side_effects.idempotency_key IS 'Optional idempotency key to prevent duplicate compensation';
