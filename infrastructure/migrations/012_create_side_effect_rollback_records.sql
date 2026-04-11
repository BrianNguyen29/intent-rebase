-- Migration: 012_create_side_effect_rollback_records.sql
-- Description: Create side_effect_rollback_records table for Phase 3 Batch 1 side-effect rollback records
-- Created: Phase 3 Batch 1 PR - side effect rollback record slice
-- Storage: Postgres with UUID primary key and indexes for efficient lookup
-- See: docs/10-delivery/checklist-phase-3.md (item 7 in side effect ledger)
-- Rollback:
-- DROP INDEX IF EXISTS idx_side_effect_rollback_records_tenant_id ON side_effect_rollback_records;
-- DROP INDEX IF EXISTS idx_side_effect_rollback_records_compensation_action_id ON side_effect_rollback_records;
-- DROP INDEX IF EXISTS idx_side_effect_rollback_records_side_effect_id ON side_effect_rollback_records;
-- DROP INDEX IF EXISTS idx_side_effect_rollback_records_intent_id ON side_effect_rollback_records;
-- DROP INDEX IF EXISTS idx_side_effect_rollback_records_recorded_at ON side_effect_rollback_records;
-- DROP TABLE IF EXISTS side_effect_rollback_records;

-- Create rollback_record_result enum
DO $$ BEGIN
    CREATE TYPE rollback_record_result AS ENUM (
        'success',
        'failure',
        'waived'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main side_effect_rollback_records table
CREATE TABLE IF NOT EXISTS side_effect_rollback_records (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant isolation
    tenant_id UUID NOT NULL,

    -- Compensation action association (link back to the action that triggered this rollback)
    compensation_action_id UUID NOT NULL,

    -- Side effect being compensated
    side_effect_id UUID NOT NULL,

    -- Intent this rollback is scoped to
    intent_id UUID NOT NULL,

    -- Result of the compensation execution or waiver
    -- 'success': compensation executed successfully
    -- 'failure': compensation execution failed
    -- 'waived': compensation action was waived
    result rollback_record_result NOT NULL,

    -- Human-readable summary of what happened
    summary TEXT NOT NULL DEFAULT '',

    -- Error code if execution failed (null if success or waived)
    error_code VARCHAR(100),

    -- Detailed error message if execution failed
    error_detail TEXT,

    -- Who executed or waived (populated when record is created)
    recorded_by VARCHAR(255),

    -- Timestamp when this rollback record was created
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Lock version for optimistic concurrency
    lock_version INT NOT NULL DEFAULT 0
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_tenant_id ON side_effect_rollback_records(tenant_id);
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_compensation_action_id ON side_effect_rollback_records(compensation_action_id);
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_side_effect_id ON side_effect_rollback_records(side_effect_id);
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_intent_id ON side_effect_rollback_records(intent_id);

-- Composite indexes for common filter combinations
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_tenant_action ON side_effect_rollback_records(tenant_id, compensation_action_id);
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_tenant_side_effect ON side_effect_rollback_records(tenant_id, side_effect_id);
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_tenant_intent ON side_effect_rollback_records(tenant_id, intent_id);

-- Index for sorting by recorded_at
CREATE INDEX IF NOT EXISTS idx_side_effect_rollback_records_recorded_at ON side_effect_rollback_records(recorded_at DESC);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM side_effect_rollback_records;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'side_effect_rollback_records';
-- 3. Verify enum exists: SELECT typname FROM pg_type WHERE typname = 'rollback_record_result';
-- 4. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'side_effect_rollback_records';

-- Comments
COMMENT ON TABLE side_effect_rollback_records IS 'Phase 3 Batch 1: Side effect rollback records - records compensation execution results (success/failure/waived) for audit and replay';
COMMENT ON COLUMN side_effect_rollback_records.id IS 'Unique identifier for this rollback record';
COMMENT ON COLUMN side_effect_rollback_records.tenant_id IS 'Tenant this rollback record belongs to';
COMMENT ON COLUMN side_effect_rollback_records.compensation_action_id IS 'Reference to the compensation action that generated this rollback record';
COMMENT ON COLUMN side_effect_rollback_records.side_effect_id IS 'Reference to the side effect this rollback record is for';
COMMENT ON COLUMN side_effect_rollback_records.intent_id IS 'Reference to the intent this rollback record is scoped to';
COMMENT ON COLUMN side_effect_rollback_records.result IS 'Result of compensation: success, failure, or waived';
COMMENT ON COLUMN side_effect_rollback_records.summary IS 'Human-readable summary of what happened during compensation execution or waiver';
COMMENT ON COLUMN side_effect_rollback_records.error_code IS 'Error code if compensation execution failed';
COMMENT ON COLUMN side_effect_rollback_records.error_detail IS 'Detailed error message if compensation execution failed';
COMMENT ON COLUMN side_effect_rollback_records.recorded_by IS 'Who executed or waived this compensation action';
COMMENT ON COLUMN side_effect_rollback_records.recorded_at IS 'Timestamp when this rollback record was created';
COMMENT ON COLUMN side_effect_rollback_records.lock_version IS 'Lock version for optimistic concurrency';
