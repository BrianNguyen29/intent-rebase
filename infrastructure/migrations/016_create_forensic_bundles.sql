-- Migration: 016_create_forensic_bundles.sql
-- Description: Create forensic_bundles table with indexes and RLS policy for T-08 bounded slice
-- Created: T-08 forensic bundle SQL repository slice
-- See: docs/10-delivery/17-production-readiness-backlog.md (T-08)

-- =============================================================================
-- FORENSIC_BUNDLES TABLE
-- =============================================================================

-- Forensic bundle manifest storage.
-- T-08 bounded slice: SQL-backed repository with tenant RLS.
-- S3 Object Lock, retention enforcement, and forensic replay remain deferred.

CREATE TABLE IF NOT EXISTS forensic_bundles (
    -- Primary key
    bundle_id UUID PRIMARY KEY,

    -- Tenant scope (required for RLS)
    tenant_id UUID NOT NULL,

    -- Bundle format version
    bundle_version VARCHAR(30) NOT NULL,

    -- Lifecycle timestamps
    created_at TIMESTAMPTZ NOT NULL,

    -- Actor who triggered bundle generation
    created_by VARCHAR(255) NOT NULL,

    -- Time range covered by the bundle
    time_range_start TIMESTAMPTZ NOT NULL,
    time_range_end TIMESTAMPTZ NOT NULL,

    -- Purpose of the bundle
    purpose VARCHAR(50) NOT NULL,

    -- Generation status
    status VARCHAR(30) NOT NULL DEFAULT 'pending',
    CONSTRAINT forensic_bundles_status_check CHECK (
        status IN ('pending', 'generating', 'ready', 'failed')
    ),

    -- Summary of contents (JSONB)
    contents JSONB NOT NULL,

    -- Integrity verification result (JSONB)
    integrity JSONB NOT NULL,

    -- Retention policy metadata (JSONB, nullable)
    retention JSONB
);

-- Index for tenant-scoped queries (most common access pattern)
CREATE INDEX IF NOT EXISTS idx_forensic_bundles_tenant_id ON forensic_bundles(tenant_id);

-- Index for status filtering within a tenant
CREATE INDEX IF NOT EXISTS idx_forensic_bundles_tenant_status ON forensic_bundles(tenant_id, status);

-- Index for ordering by creation time (most recent first)
CREATE INDEX IF NOT EXISTS idx_forensic_bundles_tenant_created_at ON forensic_bundles(tenant_id, created_at DESC);

-- =============================================================================
-- ENABLE RLS
-- =============================================================================

ALTER TABLE forensic_bundles ENABLE ROW LEVEL SECURITY;
ALTER TABLE forensic_bundles FORCE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICY FOR TENANT ISOLATION
-- =============================================================================

-- Policy: Allow access when current_tenant_id() is NULL (superuser/migration bypass)
--         OR when row's tenant_id matches the current session tenant.
-- This pattern matches migration 013's consistent RLS policy for all tenant-scoped tables.

CREATE POLICY tenant_isolation ON forensic_bundles
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled:
-- SELECT relrowsecurity::bool, relforcerowsecurity::bool
-- FROM pg_tables JOIN pg_class ON pg_tables.tablename = pg_class.relname
-- WHERE schemaname = 'public' AND tablename = 'forensic_bundles';

-- Verify policy exists:
-- SELECT policyname, permissive FROM pg_policies
-- WHERE schemaname = 'public' AND tablename = 'forensic_bundles';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   DROP POLICY tenant_isolation ON forensic_bundles;
--   ALTER TABLE forensic_bundles DISABLE ROW LEVEL SECURITY;
--   DROP INDEX IF EXISTS idx_forensic_bundles_tenant_id;
--   DROP INDEX IF EXISTS idx_forensic_bundles_tenant_status;
--   DROP INDEX IF EXISTS idx_forensic_bundles_tenant_created_at;
--   DROP TABLE IF EXISTS forensic_bundles;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TABLE forensic_bundles IS 'T-08: Forensic bundle manifest storage. RLS enabled for tenant isolation. S3/replay deferred.';
COMMENT ON COLUMN forensic_bundles.bundle_id IS 'Unique identifier for this bundle';
COMMENT ON COLUMN forensic_bundles.tenant_id IS 'Tenant this bundle belongs to (RLS-scoped)';
COMMENT ON COLUMN forensic_bundles.bundle_version IS 'Bundle format version';
COMMENT ON COLUMN forensic_bundles.created_at IS 'When the bundle was created';
COMMENT ON COLUMN forensic_bundles.created_by IS 'Actor who triggered bundle generation';
COMMENT ON COLUMN forensic_bundles.time_range_start IS 'Start of time range covered by the bundle';
COMMENT ON COLUMN forensic_bundles.time_range_end IS 'End of time range covered by the bundle';
COMMENT ON COLUMN forensic_bundles.purpose IS 'Purpose: incident_investigation, compliance_audit, legal';
COMMENT ON COLUMN forensic_bundles.status IS 'Generation status: pending, generating, ready, failed';
COMMENT ON COLUMN forensic_bundles.contents IS 'Summary of contents included in the bundle (JSONB)';
COMMENT ON COLUMN forensic_bundles.integrity IS 'Integrity verification result (JSONB)';
COMMENT ON COLUMN forensic_bundles.retention IS 'Retention policy metadata (JSONB, nullable)';
