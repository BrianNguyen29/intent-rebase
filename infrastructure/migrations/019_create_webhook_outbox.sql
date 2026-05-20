-- Migration: 019_create_webhook_outbox.sql
-- Description: Create webhook_outbox table with indexes and RLS policy for Slice 1 webhook outbox foundation
-- Created: Phase 4a Slice 1 — bounded local-dev outbox table
-- See: docs/10-delivery/17-production-readiness-backlog.md (P2-6a), docs/10-delivery/22-phase-4-entry-plan.md (A-12)

-- =============================================================================
-- WEBHOOK_OUTBOX TABLE
-- =============================================================================

CREATE TABLE IF NOT EXISTS webhook_outbox (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant scope (required for RLS)
    tenant_id UUID NOT NULL,

    -- Intent this delivery is for (logical FK; enforced at application layer)
    intent_id UUID NOT NULL,

    -- Subscription this delivery targets (logical FK; enforced at application layer)
    subscription_id UUID NOT NULL,

    -- Event type
    event_type TEXT NOT NULL,

    -- Payload envelope
    payload JSONB NOT NULL DEFAULT '{}',

    -- Status: pending, claimed, delivered, failed
    status TEXT NOT NULL DEFAULT 'pending',

    -- Delivery attempt tracking
    attempt_count INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 3,

    -- Scheduling and locking
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_at TIMESTAMPTZ NULL,
    locked_by TEXT NULL,

    -- Outcome tracking
    delivered_at TIMESTAMPTZ NULL,
    last_error TEXT NULL,

    -- Optimistic locking
    lock_version INT NOT NULL DEFAULT 0,

    -- Lifecycle timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- INDEXES
-- =============================================================================

-- Pending due queue (primary worker polling index)
CREATE INDEX IF NOT EXISTS idx_webhook_outbox_pending_due
    ON webhook_outbox (scheduled_at, id)
    WHERE status = 'pending';

-- Tenant + intent lookup (support idempotency / replay queries)
CREATE INDEX IF NOT EXISTS idx_webhook_outbox_tenant_intent
    ON webhook_outbox (tenant_id, intent_id, created_at DESC);

-- Claimed rows (stale-claim recovery)
CREATE INDEX IF NOT EXISTS idx_webhook_outbox_claimed
    ON webhook_outbox (locked_at, locked_by)
    WHERE status = 'claimed';

-- =============================================================================
-- ENABLE RLS
-- =============================================================================

ALTER TABLE webhook_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_outbox FORCE ROW LEVEL SECURITY;

-- =============================================================================
-- RLS POLICY FOR TENANT ISOLATION
-- =============================================================================

CREATE POLICY tenant_isolation ON webhook_outbox
    USING (current_tenant_id() IS NULL OR tenant_id = current_tenant_id());

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify RLS is enabled:
-- SELECT relrowsecurity::bool, relforcerowsecurity::bool
-- FROM pg_tables JOIN pg_class ON pg_tables.tablename = pg_class.relname
-- WHERE schemaname = 'public' AND tablename = 'webhook_outbox';

-- Verify policy exists:
-- SELECT policyname, permissive FROM pg_policies
-- WHERE schemaname = 'public' AND tablename = 'webhook_outbox';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   DROP POLICY tenant_isolation ON webhook_outbox;
--   ALTER TABLE webhook_outbox DISABLE ROW LEVEL SECURITY;
--   DROP INDEX IF EXISTS idx_webhook_outbox_pending_due;
--   DROP INDEX IF EXISTS idx_webhook_outbox_tenant_intent;
--   DROP INDEX IF EXISTS idx_webhook_outbox_claimed;
--   DROP TABLE IF EXISTS webhook_outbox;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TABLE webhook_outbox IS 'Phase 4a Slice 1: Webhook outbox table. RLS enabled for tenant isolation. Background worker, HMAC signing, and subscription CRUD API remain deferred.';
