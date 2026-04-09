-- Migration: 011_create_compensation_actions.sql
-- Description: Create compensation_actions table for Phase 3 Batch 1 compensation action persistence
-- Created: Phase 3 Batch 1 PR - compensation action ledger
-- Storage: Postgres with UUID primary key and indexes for efficient lookup
-- See: docs/10-delivery/05-phase-3-hardening.md (item 1-2)

-- Rollback:
-- DROP INDEX IF EXISTS idx_compensation_actions_tenant_id ON compensation_actions;
-- DROP INDEX IF EXISTS idx_compensation_actions_side_effect_id ON compensation_actions;
-- DROP INDEX IF EXISTS idx_compensation_actions_status ON compensation_actions;
-- DROP INDEX IF EXISTS idx_compensation_actions_intent_id ON compensation_actions;
-- DROP INDEX IF EXISTS idx_compensation_actions_tenant_intent ON compensation_actions;
-- DROP INDEX IF EXISTS idx_compensation_actions_generated_at ON compensation_actions;
-- DROP TABLE IF EXISTS compensation_actions;

-- Create compensation_status enum (mirrors CompensationStatus in Rust)
DO $$ BEGIN
    CREATE TYPE compensation_status AS ENUM (
        'pending',
        'approved',
        'executed',
        'failed',
        'waived'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create strategy_type enum
DO $$ BEGIN
    CREATE TYPE strategy_type AS ENUM (
        'rollback',
        'counter_action',
        'followup_notice',
        'quarantine',
        'escalation'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create compensation_feasibility enum
DO $$ BEGIN
    CREATE TYPE compensation_feasibility AS ENUM (
        'automatic',
        'semi_automatic',
        'manual_only',
        'not_possible'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main compensation_actions table
CREATE TABLE IF NOT EXISTS compensation_actions (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Tenant isolation
    tenant_id UUID NOT NULL,
    
    -- Side effect association
    side_effect_id UUID NOT NULL,
    
    -- Intent linkage for direct intent-scoped queries
    -- This allows querying all compensation actions for an intent without joining through side_effects
    intent_id UUID NOT NULL,
    
    -- Trigger context: minimal rebase context that caused this compensation action (JSONB)
    -- Stores RebaseContext: { intent_id, from_version, to_version, workflow_id }
    -- Used by planner/executor to reason about what triggered the compensation
    trigger_context JSONB NOT NULL DEFAULT '{}',
    
    -- Result payload: execution result context (JSONB)
    -- Stores ExecutionResult: { success, summary, error_code, error_detail, completed_at }
    -- Captures what happened during execution for retry/audit reasoning
    execution_result_payload JSONB DEFAULT NULL,
    
    -- Feasibility and strategy
    feasibility compensation_feasibility NOT NULL DEFAULT 'manual_only',
    strategy_type strategy_type NOT NULL DEFAULT 'quarantine',
    
    -- Human-readable rationale for the chosen strategy
    rationale TEXT NOT NULL DEFAULT '',
    
    -- Status tracking
    status compensation_status NOT NULL DEFAULT 'pending',
    
    -- Who approved / executed / waived (populated when status changes)
    approved_by VARCHAR(255),
    waived_by VARCHAR(255),
    executed_by VARCHAR(255),
    
    -- Timestamps
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    approved_at TIMESTAMPTZ,
    waived_at TIMESTAMPTZ,
    executed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    
    -- Execution attempt counter for idempotency/retry tracking
    attempt_count INT NOT NULL DEFAULT 0,
    
    -- Lock version for optimistic concurrency during status transitions
    lock_version INT NOT NULL DEFAULT 0
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_compensation_actions_tenant_id ON compensation_actions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_compensation_actions_side_effect_id ON compensation_actions(side_effect_id);
CREATE INDEX IF NOT EXISTS idx_compensation_actions_status ON compensation_actions(status);
CREATE INDEX IF NOT EXISTS idx_compensation_actions_intent_id ON compensation_actions(intent_id);

-- Composite indexes for common filter combinations
CREATE INDEX IF NOT EXISTS idx_compensation_actions_tenant_status ON compensation_actions(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_compensation_actions_tenant_side_effect ON compensation_actions(tenant_id, side_effect_id);
CREATE INDEX IF NOT EXISTS idx_compensation_actions_tenant_intent ON compensation_actions(tenant_id, intent_id);

-- Index for sorting by generation time
CREATE INDEX IF NOT EXISTS idx_compensation_actions_generated_at ON compensation_actions(generated_at DESC);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM compensation_actions;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'compensation_actions';
-- 3. Verify enums exist: SELECT typname FROM pg_type WHERE typname IN ('compensation_status', 'strategy_type', 'compensation_feasibility');
-- 4. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'compensation_actions';

-- Comments
COMMENT ON TABLE compensation_actions IS 'Phase 3 Batch 1: Compensation action ledger - records compensation actions generated from side effects for rollback/counter-action planning';
COMMENT ON COLUMN compensation_actions.id IS 'Unique identifier for this compensation action';
COMMENT ON COLUMN compensation_actions.tenant_id IS 'Tenant this compensation action belongs to';
COMMENT ON COLUMN compensation_actions.side_effect_id IS 'Reference to the side effect this action compensates';
COMMENT ON COLUMN compensation_actions.intent_id IS 'Reference to the intent this compensation action is scoped to (for direct intent-scoped queries)';
COMMENT ON COLUMN compensation_actions.trigger_context IS 'JSONB: minimal rebase context that triggered compensation planning (intent_id, from_version, to_version, workflow_id)';
COMMENT ON COLUMN compensation_actions.execution_result_payload IS 'JSONB: execution result context captured after executor runs (success, summary, error_code, error_detail, completed_at)';
COMMENT ON COLUMN compensation_actions.feasibility IS 'Feasibility level of compensating this effect';
COMMENT ON COLUMN compensation_actions.strategy_type IS 'Chosen compensation strategy';
COMMENT ON COLUMN compensation_actions.rationale IS 'Human-readable rationale for the chosen strategy';
COMMENT ON COLUMN compensation_actions.status IS 'Current status of the compensation action';
COMMENT ON COLUMN compensation_actions.approved_by IS 'Who approved this compensation action';
COMMENT ON COLUMN compensation_actions.waived_by IS 'Who waived this compensation action';
COMMENT ON COLUMN compensation_actions.executed_by IS 'Who executed this compensation action';
COMMENT ON COLUMN compensation_actions.generated_at IS 'Timestamp when this action was generated';
COMMENT ON COLUMN compensation_actions.approved_at IS 'Timestamp when compensation was approved';
COMMENT ON COLUMN compensation_actions.waived_at IS 'Timestamp when compensation was waived';
COMMENT ON COLUMN compensation_actions.executed_at IS 'Timestamp when compensation was executed';
COMMENT ON COLUMN compensation_actions.failed_at IS 'Timestamp when compensation failed';
COMMENT ON COLUMN compensation_actions.execution_result_payload IS 'Execution outcome payload for executed or failed compensation actions as JSONB';
COMMENT ON COLUMN compensation_actions.attempt_count IS 'Execution attempt counter for idempotency/retry tracking';
COMMENT ON COLUMN compensation_actions.lock_version IS 'Lock version for optimistic concurrency during status transitions';
