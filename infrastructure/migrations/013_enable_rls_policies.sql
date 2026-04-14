-- Migration: 013_enable_rls_policies.sql
-- Description: Enable Row Level Security (RLS) on all tenant-scoped tables for P6 tenant isolation hardening
-- Created: P6 hardening - tenant isolation
-- See: docs/10-delivery/phase-3-hardening.md (P6 tenant isolation)

-- =============================================================================
-- HELPER FUNCTIONS
-- =============================================================================

-- Helper function: Returns the current tenant_id from session settings.
-- Returns NULL if no tenant context is set (allows superuser/migration access).
-- This is the primary mechanism for RLS policy evaluation.
CREATE OR REPLACE FUNCTION current_tenant_id() RETURNS uuid AS $$
BEGIN
    RETURN NULLIF(current_setting('app.current_tenant_id', TRUE), '')::uuid;
EXCEPTION WHEN OTHERS THEN
    RETURN NULL;
END;
$$ LANGUAGE plpgsql STABLE;

-- Helper function: Fallback tenant_id extraction for application-layer queries.
-- Returns the current_tenant_id if set, otherwise returns the null uuid as a sentinel.
-- This should only be used as a fallback when session setting cannot be passed.
CREATE OR REPLACE FUNCTION default_tenant_id() RETURNS uuid AS $$
BEGIN
    RETURN current_tenant_id();
EXCEPTION WHEN OTHERS THEN
    RETURN '00000000-0000-0000-0000-000000000000'::uuid;
END;
$$ LANGUAGE plpgsql STABLE;

-- =============================================================================
-- ENABLE RLS ON ALL TENANT-SCOPED TABLES
-- =============================================================================

-- Core intent tables
ALTER TABLE intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_requests ENABLE ROW LEVEL SECURITY;

-- Graph storage tables
ALTER TABLE graph_nodes ENABLE ROW LEVEL SECURITY;
ALTER TABLE graph_edges ENABLE ROW LEVEL SECURITY;

-- Side effect and compensation tables
ALTER TABLE side_effects ENABLE ROW LEVEL SECURITY;
ALTER TABLE compensation_actions ENABLE ROW LEVEL SECURITY;
ALTER TABLE side_effect_rollback_records ENABLE ROW LEVEL SECURITY;

-- Policy governance tables
ALTER TABLE policy_snapshot ENABLE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICIES FOR TENANT ISOLATION
-- =============================================================================

-- Policy: Allow access when current_tenant_id() is NULL (superuser/migration bypass)
--         OR when row's tenant_id matches the current session tenant.
-- This pattern is applied consistently across all tenant-scoped tables.

-- intents: Core intent registry
CREATE POLICY tenant_isolation ON intents
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- audit_events: Immutable audit log entries
CREATE POLICY tenant_isolation ON audit_events
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- checkpoints: Workflow checkpoint mappings
CREATE POLICY tenant_isolation ON checkpoints
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- approval_requests: Pending approval queue
CREATE POLICY tenant_isolation ON approval_requests
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- graph_nodes: Dependency graph nodes
CREATE POLICY tenant_isolation ON graph_nodes
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- graph_edges: Dependency graph edges
CREATE POLICY tenant_isolation ON graph_edges
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- side_effects: Side effect ledger
CREATE POLICY tenant_isolation ON side_effects
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- compensation_actions: Compensation action ledger
CREATE POLICY tenant_isolation ON compensation_actions
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- side_effect_rollback_records: Compensation execution records
CREATE POLICY tenant_isolation ON side_effect_rollback_records
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- policy_snapshot: Policy approval snapshots
CREATE POLICY tenant_isolation ON policy_snapshot
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled on all tenant-scoped tables:
-- SELECT tablename FROM pg_tables WHERE schemaname = 'public'
--   AND relrowsecurity = true ORDER BY tablename;

-- Verify policies exist:
-- SELECT schemaname, tablename, policyname, permissive
-- FROM pg_policies WHERE schemaname = 'public' ORDER BY tablename, policyname;

-- Verify helper functions exist:
-- SELECT proname, pronargs, prorettype::regtype
-- FROM pg_proc WHERE proname IN ('current_tenant_id', 'default_tenant_id');

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: RLS policies and helper functions can be dropped individually.
-- Example rollback for a single table:
--   DROP POLICY tenant_isolation ON intents;
--   ALTER TABLE intents DISABLE ROW LEVEL SECURITY;
--
-- Full rollback (drop all P6 RLS objects):
--   DROP POLICY tenant_isolation ON intents;
--   DROP POLICY tenant_isolation ON audit_events;
--   DROP POLICY tenant_isolation ON checkpoints;
--   DROP POLICY tenant_isolation ON approval_requests;
--   DROP POLICY tenant_isolation ON graph_nodes;
--   DROP POLICY tenant_isolation ON graph_edges;
--   DROP POLICY tenant_isolation ON side_effects;
--   DROP POLICY tenant_isolation ON compensation_actions;
--   DROP POLICY tenant_isolation ON side_effect_rollback_records;
--   DROP POLICY tenant_isolation ON policy_snapshot;
--   ALTER TABLE intents DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE audit_events DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE checkpoints DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE approval_requests DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE graph_nodes DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE graph_edges DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE side_effects DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE compensation_actions DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE side_effect_rollback_records DISABLE ROW LEVEL SECURITY;
--   ALTER TABLE policy_snapshot DISABLE ROW LEVEL SECURITY;
--   DROP FUNCTION IF EXISTS current_tenant_id();
--   DROP FUNCTION IF EXISTS default_tenant_id();

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON FUNCTION current_tenant_id() IS 'P6: Returns current tenant_id from session setting app.current_tenant_id. NULL means no tenant context (superuser/migration access).';
COMMENT ON FUNCTION default_tenant_id() IS 'P6: Fallback tenant extraction. Returns current_tenant_id() or null uuid sentinel on error.';
COMMENT ON TABLE intents IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE audit_events IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE checkpoints IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE approval_requests IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE graph_nodes IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE graph_edges IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE side_effects IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE compensation_actions IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE side_effect_rollback_records IS 'P6: RLS enabled for tenant isolation';
COMMENT ON TABLE policy_snapshot IS 'P6: RLS enabled for tenant isolation';
