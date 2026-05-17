-- Migration: 021_add_webhook_outbox_replay_metadata.sql
-- Description: Add replay audit metadata columns to webhook_outbox for Phase 1.2
-- Created: Phase 1.2 replay audit metadata — bounded local-dev only
-- Rationale: Tracks DLQ replay count, timestamp, and actor for observability.
--            No production audit trail or signoff claim.

-- =============================================================================
-- WEBHOOK_OUTBOX: ADD REPLAY METADATA COLUMNS
-- =============================================================================

-- Number of times this record has been replayed from DLQ
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS replay_count INT NOT NULL DEFAULT 0;

-- Timestamp of the most recent DLQ replay
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS replayed_at TIMESTAMPTZ NULL;

-- Identity of the actor that performed the most recent replay (e.g. operator id, service name)
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS replayed_by TEXT NULL;

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify columns were added:
-- SELECT column_name, data_type, column_default, is_nullable
-- FROM information_schema.columns
-- WHERE table_name = 'webhook_outbox'
--   AND column_name IN ('replay_count', 'replayed_at', 'replayed_by');

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS replayed_by;
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS replayed_at;
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS replay_count;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON COLUMN webhook_outbox.replay_count IS 'Number of DLQ replays for this record';
COMMENT ON COLUMN webhook_outbox.replayed_at IS 'Timestamp of the most recent DLQ replay';
COMMENT ON COLUMN webhook_outbox.replayed_by IS 'Actor identity for the most recent DLQ replay (local-dev only)';
