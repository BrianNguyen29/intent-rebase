-- Migration: 015_create_orchestration_runs.sql
-- Description: Create orchestration_runs table with indexes and RLS policy for P1-S5i bounded slice
-- Created: P1-S5i orchestration_runs RLS transaction slice
-- See: docs/10-delivery/17-production-readiness-backlog.md (P1-S5i)

-- =============================================================================
-- ORCHESTRATION_RUNS TABLE
-- =============================================================================

-- Single-shot orchestration run handle for compensation action batch execution.
-- Phase 3 Batch 1 bounded slice: Persisted run handle for single-shot orchestration.
-- No queue polling, no distributed claiming/locking, no background scheduler.

CREATE TABLE IF NOT EXISTS orchestration_runs (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant scope (required for RLS)
    tenant_id UUID NOT NULL,

    -- Intent scope (optional - run may span multiple intents or be intent-agnostic)
    intent_id UUID,

    -- Compensation action IDs to process in this run
    action_ids JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Run status over its lifecycle
    status VARCHAR(30) NOT NULL DEFAULT 'pending',
    CONSTRAINT orchestration_runs_status_check CHECK (
        status IN ('pending', 'running', 'completed', 'completed_with_errors', 'failed')
    ),

    -- Who initiated this run
    initiated_by VARCHAR(255),

    -- Lifecycle timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Action counts
    succeeded_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,
    skipped_count INTEGER NOT NULL DEFAULT 0,
    not_found_count INTEGER NOT NULL DEFAULT 0,
    total_count INTEGER NOT NULL DEFAULT 0,

    -- Per-item results summary
    item_results JSONB NOT NULL DEFAULT '[]'::jsonb
);

-- Index for tenant-scoped queries (most common access pattern)
CREATE INDEX IF NOT EXISTS idx_orchestration_runs_tenant_id ON orchestration_runs(tenant_id);

-- Index for intent-scoped queries
CREATE INDEX IF NOT EXISTS idx_orchestration_runs_intent_id ON orchestration_runs(intent_id) WHERE intent_id IS NOT NULL;

-- Index for status filtering within a tenant
CREATE INDEX IF NOT EXISTS idx_orchestration_runs_tenant_status ON orchestration_runs(tenant_id, status);

-- Index for ordering by creation time (most recent first)
CREATE INDEX IF NOT EXISTS idx_orchestration_runs_created_at ON orchestration_runs(tenant_id, created_at DESC);

-- =============================================================================
-- ENABLE RLS
-- =============================================================================

ALTER TABLE orchestration_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE orchestration_runs FORCE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICY FOR TENANT ISOLATION
-- =============================================================================

-- Policy: Allow access when current_tenant_id() is NULL (superuser/migration bypass)
--         OR when row's tenant_id matches the current session tenant.
-- This pattern matches migration 013's consistent RLS policy for all tenant-scoped tables.

CREATE POLICY tenant_isolation ON orchestration_runs
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled:
-- SELECT relrowsecurity::bool, relforcerowsecurity::bool
-- FROM pg_tables JOIN pg_class ON pg_tables.tablename = pg_class.relname
-- WHERE schemaname = 'public' AND tablename = 'orchestration_runs';

-- Verify policy exists:
-- SELECT policyname, permissive FROM pg_policies
-- WHERE schemaname = 'public' AND tablename = 'orchestration_runs';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   DROP POLICY tenant_isolation ON orchestration_runs;
--   ALTER TABLE orchestration_runs DISABLE ROW LEVEL SECURITY;
--   DROP INDEX IF EXISTS idx_orchestration_runs_tenant_id;
--   DROP INDEX IF EXISTS idx_orchestration_runs_intent_id;
--   DROP INDEX IF EXISTS idx_orchestration_runs_tenant_status;
--   DROP INDEX IF EXISTS idx_orchestration_runs_created_at;
--   DROP TABLE IF EXISTS orchestration_runs;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TABLE orchestration_runs IS 'P1-S5i: Single-shot orchestration run handle for compensation action batch execution. RLS enabled for tenant isolation.';
COMMENT ON COLUMN orchestration_runs.id IS 'Unique identifier for this run';
COMMENT ON COLUMN orchestration_runs.tenant_id IS 'Tenant this run belongs to (RLS-scoped)';
COMMENT ON COLUMN orchestration_runs.intent_id IS 'Intent scope for this run (optional)';
COMMENT ON COLUMN orchestration_runs.action_ids IS 'List of compensation action IDs to process';
COMMENT ON COLUMN orchestration_runs.status IS 'Run status: pending, running, completed, completed_with_errors, failed';
COMMENT ON COLUMN orchestration_runs.initiated_by IS 'Who initiated this run';
COMMENT ON COLUMN orchestration_runs.created_at IS 'When the run was created';
COMMENT ON COLUMN orchestration_runs.started_at IS 'When the run started execution';
COMMENT ON COLUMN orchestration_runs.completed_at IS 'When the run completed';
COMMENT ON COLUMN orchestration_runs.succeeded_count IS 'Number of actions processed successfully';
COMMENT ON COLUMN orchestration_runs.failed_count IS 'Number of actions that failed';
COMMENT ON COLUMN orchestration_runs.skipped_count IS 'Number of actions skipped';
COMMENT ON COLUMN orchestration_runs.not_found_count IS 'Number of actions not found';
COMMENT ON COLUMN orchestration_runs.total_count IS 'Total number of actions in the run';
COMMENT ON COLUMN orchestration_runs.item_results IS 'Per-item outcome summary';
