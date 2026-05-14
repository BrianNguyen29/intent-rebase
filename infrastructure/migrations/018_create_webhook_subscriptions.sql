-- Migration: 018_create_webhook_subscriptions.sql
-- Description: Create webhook_subscriptions table with indexes and RLS policy for Slice 3 webhook delivery prerequisite (B1)
-- Created: Propagation status Slice 3 prerequisite — webhook subscription registry
-- See: docs/10-delivery/19-propagation-status-implementation-plan.md (R3 B1, R4 D1)

-- =============================================================================
-- WEBHOOK_SUBSCRIPTIONS TABLE
-- =============================================================================

-- Webhook subscription registry for downstream systems.
-- B1 bounded: minimal subscription storage. Secret/HMAC keys, custom headers,
-- enabled/disabled flag, and subscription CRUD API remain deferred.

CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant scope (required for RLS)
    tenant_id UUID NOT NULL,

    -- Intent this subscription is for
    intent_id UUID NOT NULL,

    -- External subscription identifier
    subscription_id UUID NOT NULL,

    -- Target webhook URL
    webhook_url TEXT NOT NULL,

    -- Downstream system identifier (optional for labeling)
    downstream_system_id TEXT,

    -- Lifecycle timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, intent_id, subscription_id)
);

-- Index for dispatcher lookup by tenant+intent (primary access pattern)
CREATE INDEX IF NOT EXISTS idx_webhook_subscriptions_tenant_intent
    ON webhook_subscriptions(tenant_id, intent_id);

-- Index for subscription_id lookups
CREATE INDEX IF NOT EXISTS idx_webhook_subscriptions_subscription_id
    ON webhook_subscriptions(subscription_id);

-- =============================================================================
-- ENABLE RLS
-- =============================================================================

ALTER TABLE webhook_subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_subscriptions FORCE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICY FOR TENANT ISOLATION
-- =============================================================================

-- Policy: Allow access when current_tenant_id() is NULL (superuser/migration bypass)
--         OR when row's tenant_id matches the current session tenant.
-- This pattern matches migration 017's consistent RLS policy for all tenant-scoped tables.

CREATE POLICY tenant_isolation ON webhook_subscriptions
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled:
-- SELECT relrowsecurity::bool, relforcerowsecurity::bool
-- FROM pg_tables JOIN pg_class ON pg_tables.tablename = pg_class.relname
-- WHERE schemaname = 'public' AND tablename = 'webhook_subscriptions';

-- Verify policy exists:
-- SELECT policyname, permissive FROM pg_policies
-- WHERE schemaname = 'public' AND tablename = 'webhook_subscriptions';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   DROP POLICY tenant_isolation ON webhook_subscriptions;
--   ALTER TABLE webhook_subscriptions DISABLE ROW LEVEL SECURITY;
--   DROP INDEX IF EXISTS idx_webhook_subscriptions_tenant_intent;
--   DROP INDEX IF EXISTS idx_webhook_subscriptions_subscription_id;
--   DROP TABLE IF EXISTS webhook_subscriptions;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TABLE webhook_subscriptions IS 'Slice 3 prerequisite (B1): Webhook subscription registry. RLS enabled for tenant isolation. Secrets, CRUD API, and delivery log deferred.';
COMMENT ON COLUMN webhook_subscriptions.id IS 'Unique identifier for this subscription record';
COMMENT ON COLUMN webhook_subscriptions.tenant_id IS 'Tenant this subscription belongs to (RLS-scoped)';
COMMENT ON COLUMN webhook_subscriptions.intent_id IS 'Intent this subscription is for';
COMMENT ON COLUMN webhook_subscriptions.subscription_id IS 'External subscription identifier';
COMMENT ON COLUMN webhook_subscriptions.webhook_url IS 'Target webhook URL for delivery';
COMMENT ON COLUMN webhook_subscriptions.downstream_system_id IS 'Optional downstream system label';
