-- Migration: 022_add_webhook_outbox_version_fields.sql
-- Description: Add intent version tracking columns to webhook_outbox for D1 store-at-creation
-- Created: 2026-06-10 — bounded local-dev slice
-- Rationale: Stores intent version, version hash, and previous version in the outbox
--            record at creation time so the dispatcher can include accurate version
--            info in webhook payloads without re-querying the intent repository.
--            No production readiness claim; bounded local-dev only.

-- =============================================================================
-- WEBHOOK_OUTBOX: ADD VERSION FIELDS
-- =============================================================================

-- Intent version number at the time of outbox creation
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS version INT NOT NULL DEFAULT 0;

-- Content hash of the intent version at creation time
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS version_hash TEXT NULL;

-- Previous intent version number (for version chain continuity)
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS previous_version INT NULL;

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify columns were added:
-- SELECT column_name, data_type, column_default, is_nullable
-- FROM information_schema.columns
-- WHERE table_name = 'webhook_outbox'
--   AND column_name IN ('version', 'version_hash', 'previous_version');

-- =============================================================================
-- ROLLBACK
-- =============================================================================
-- Note: Drop in reverse order of creation
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS previous_version;
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS version_hash;
--   ALTER TABLE webhook_outbox DROP COLUMN IF EXISTS version;

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON COLUMN webhook_outbox.version IS 'Intent version number at the time of outbox creation (D1 store-at-creation)';
COMMENT ON COLUMN webhook_outbox.version_hash IS 'Content hash of the intent version at creation time';
COMMENT ON COLUMN webhook_outbox.previous_version IS 'Previous intent version number for version chain continuity';
