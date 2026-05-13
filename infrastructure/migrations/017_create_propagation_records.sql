-- Migration: 017_create_propagation_records.sql
-- Description: Create propagation_records table with indexes and RLS policy for propagation-status Slice 1 bounded implementation
-- Created: Propagation status Slice 1 — downstream system registry persistence
-- See: docs/10-delivery/19-propagation-status-implementation-plan.md (Slice 1)

-- =============================================================================
-- PROPAGATION_RECORDS TABLE
-- =============================================================================

-- Downstream system propagation status records.
-- Slice 1 bounded: SQL-backed repository with tenant RLS.
-- Webhook delivery, event streaming, and cross-workflow lineage remain deferred.

CREATE TABLE IF NOT EXISTS propagation_records (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant scope (required for RLS)
    tenant_id UUID NOT NULL,

    -- Intent this propagation record is for
    intent_id UUID NOT NULL,

    -- Downstream system identifier
    downstream_system_id TEXT NOT NULL,

    -- Acknowledgment state
    status VARCHAR(30) NOT NULL DEFAULT 'pending',
    CONSTRAINT propagation_records_status_check CHECK (
        status IN ('pending', 'acknowledged', 'failed')
    ),

    -- Last intent version the downstream system has processed
    last_seen_version INTEGER NOT NULL DEFAULT 0,

    -- Lifecycle timestamps
    signaled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    acknowledged_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,

    -- Failure metadata
    failure_reason TEXT,

    -- Delivery metadata
    delivery_attempt_count INTEGER NOT NULL DEFAULT 0,
    last_delivery_attempt_at TIMESTAMPTZ,

    -- Optimistic locking
    lock_version INTEGER NOT NULL DEFAULT 1,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, intent_id, downstream_system_id)
);

-- Index for tenant+intent scoped queries (most common access pattern)
CREATE INDEX IF NOT EXISTS idx_propagation_records_tenant_intent
    ON propagation_records(tenant_id, intent_id, updated_at DESC);

-- Index for system status filtering within a tenant
CREATE INDEX IF NOT EXISTS idx_propagation_records_tenant_system_status
    ON propagation_records(tenant_id, downstream_system_id, status);

-- =============================================================================
-- ENABLE RLS
-- =============================================================================

ALTER TABLE propagation_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE propagation_records FORCE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICY FOR TENANT ISOLATION
-- =============================================================================

-- Policy: Allow access when current_tenant_id() is NULL (superuser/migration bypass)
--         OR when row's tenant_id matches the current session tenant.
-- This pattern matches migration 013's consistent RLS policy for all tenant-scoped tables.

CREATE POLICY tenant_isolation ON propagation_records
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled:
-- SELECT relrowsecurity::bool, relforcerowsecurity::bool
-- FROM pg_tables JOIN pg_class ON pg_tables.tablename = pg_class.relname
-- WHERE schemaname = 'public' AND tablename = 'propagation_records';

-- Verify policy exists:
-- SELECT policyname, permissive FROM pg_policies
-- WHERE schemaname = 'public' AND tablename = 'propagation_records';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   DROP POLICY tenant_isolation ON propagation_records;
--   ALTER TABLE propagation_records DISABLE ROW LEVEL SECURITY;
--   DROP INDEX IF EXISTS idx_propagation_records_tenant_intent;
--   DROP INDEX IF EXISTS idx_propagation_records_tenant_system_status;
--   DROP TABLE IF EXISTS propagation_records;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TABLE propagation_records IS 'Slice 1: Downstream system propagation status. RLS enabled for tenant isolation. Webhook delivery and event streaming deferred.';
COMMENT ON COLUMN propagation_records.id IS 'Unique identifier for this propagation record';
COMMENT ON COLUMN propagation_records.tenant_id IS 'Tenant this record belongs to (RLS-scoped)';
COMMENT ON COLUMN propagation_records.intent_id IS 'Intent this propagation record is for';
COMMENT ON COLUMN propagation_records.downstream_system_id IS 'Identifier of the downstream system';
COMMENT ON COLUMN propagation_records.status IS 'Propagation status: pending, acknowledged, failed';
COMMENT ON COLUMN propagation_records.last_seen_version IS 'Last intent version the downstream system has processed';
COMMENT ON COLUMN propagation_records.signaled_at IS 'When the change was signaled to the downstream system';
COMMENT ON COLUMN propagation_records.acknowledged_at IS 'When the downstream system acknowledged';
COMMENT ON COLUMN propagation_records.failed_at IS 'When delivery failed';
COMMENT ON COLUMN propagation_records.failure_reason IS 'Reason for failure';
COMMENT ON COLUMN propagation_records.delivery_attempt_count IS 'Number of delivery attempts';
COMMENT ON COLUMN propagation_records.last_delivery_attempt_at IS 'Timestamp of last delivery attempt';
COMMENT ON COLUMN propagation_records.lock_version IS 'Optimistic locking version';
