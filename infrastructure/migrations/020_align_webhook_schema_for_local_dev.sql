-- Migration: 020_align_webhook_schema_for_local_dev.sql
-- Description: Align webhook_subscriptions and webhook_outbox schema for Slice 4a local-dev planning
-- Created: Slice 4a schema alignment — adds subscription lifecycle columns and outbox webhook_url
-- Rationale: Single migration is cleaner than separate 020/021 because both changes are bounded
--            local-dev schema alignment with no production data to migrate.
-- See: docs/10-delivery/22-phase-4-entry-plan.md (A-12 Slice 4a, WEB-DESIGN-1)

-- =============================================================================
-- WEBHOOK_SUBSCRIPTIONS: ADD LIFECYCLE COLUMNS
-- =============================================================================

-- Subscription status for active/paused/disabled lifecycle
ALTER TABLE webhook_subscriptions
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';

-- Max delivery attempts for this subscription (overrides outbox default)
ALTER TABLE webhook_subscriptions
    ADD COLUMN IF NOT EXISTS max_attempts INT NOT NULL DEFAULT 3;

-- Event types this subscription receives (e.g., intent_changed)
ALTER TABLE webhook_subscriptions
    ADD COLUMN IF NOT EXISTS event_types TEXT[] NOT NULL DEFAULT ARRAY['intent_changed'];

-- =============================================================================
-- WEBHOOK_SUBSCRIPTIONS: ACTIVE LOOKUP INDEX
-- =============================================================================

-- Index for worker/resolver lookup of active subscriptions by tenant+intent
CREATE INDEX IF NOT EXISTS idx_webhook_subscriptions_active_tenant_intent
    ON webhook_subscriptions (tenant_id, intent_id)
    WHERE status = 'active';

-- =============================================================================
-- WEBHOOK_OUTBOX: ADD WEBHOOK_URL
-- =============================================================================

-- Target webhook URL stored at outbox creation time so worker can dispatch
-- without joining back to subscriptions (bounded local-dev; production may
-- resolve URLs from subscription CRUD or secret manager instead).
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS webhook_url TEXT NULL;

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify columns were added:
-- SELECT column_name, data_type, column_default
-- FROM information_schema.columns
-- WHERE table_name IN ('webhook_subscriptions', 'webhook_outbox')
--   AND column_name IN ('status', 'max_attempts', 'event_types', 'webhook_url');

-- Verify index exists:
-- SELECT indexname, indexdef
-- FROM pg_indexes
-- WHERE tablename = 'webhook_subscriptions'
--   AND indexname = 'idx_webhook_subscriptions_active_tenant_intent';

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS webhook_url;
--   DROP INDEX IF EXISTS idx_webhook_subscriptions_active_tenant_intent;
--   ALTER TABLE webhook_subscriptions DROP COLUMN IF EXISTS event_types;
--   ALTER TABLE webhook_subscriptions DROP COLUMN IF EXISTS max_attempts;
--   ALTER TABLE webhook_subscriptions DROP COLUMN IF EXISTS status;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON COLUMN webhook_subscriptions.status IS 'Subscription lifecycle status: active, paused, disabled';
COMMENT ON COLUMN webhook_subscriptions.max_attempts IS 'Max delivery attempts for this subscription';
COMMENT ON COLUMN webhook_subscriptions.event_types IS 'Event types this subscription receives (TEXT array)';
COMMENT ON COLUMN webhook_outbox.webhook_url IS 'Target webhook URL at time of outbox record creation';
