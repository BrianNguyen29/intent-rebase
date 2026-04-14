-- Migration: 014_audit_immutability_trigger.sql
-- Description: Create immutability trigger on audit_events table for P6 audit trail integrity hardening
-- Created: P6 hardening - audit immutability
-- See: docs/10-delivery/phase-3-hardening.md (P6 audit immutability)

-- =============================================================================
-- IMMUTABILITY TRIGGER FUNCTION
-- =============================================================================

-- Trigger function: Prevents UPDATE and DELETE operations on audit_events.
-- This ensures audit trail integrity by making audit records truly immutable.
-- Any attempt to modify or delete audit events will raise an exception.
CREATE OR REPLACE FUNCTION enforce_audit_immutability()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'Audit events are immutable: % operation is not allowed on audit_events', TG_OP;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- =============================================================================
-- ATTACH TRIGGER TO AUDIT_EVENTS
-- =============================================================================

-- Trigger fires BEFORE UPDATE OR DELETE on each row.
-- The function always raises an exception, so no row modification ever occurs.
CREATE TRIGGER audit_events_immutable
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW
    EXECUTE FUNCTION enforce_audit_immutability();

-- =============================================================================
-- POST-MIGRATION VALIDATION
-- =============================================================================

-- Verify trigger exists:
-- SELECT trgname, tgtype, tgrelid::regclass, tgfoid::regproc
-- FROM pg_trigger WHERE tgrelid = 'audit_events'::regclass AND trgname = 'audit_events_immutable';

-- Verify trigger prevents updates (test in transaction, then rollback):
-- BEGIN;
--   UPDATE audit_events SET payload = payload WHERE id = (SELECT id FROM audit_events LIMIT 1);
-- -- Expected: ERROR: Audit events are immutable: UPDATE operation is not allowed
-- ROLLBACK;

-- Verify trigger prevents deletes (test in transaction, then rollback):
-- BEGIN;
--   DELETE FROM audit_events WHERE id = (SELECT id FROM audit_events LIMIT 1);
-- -- Expected: ERROR: Audit events are immutable: DELETE operation is not allowed
-- ROLLBACK;

-- =============================================================================
-- ROLLBACK
-- =============================================================================

-- To drop the immutability trigger (revert P6 audit hardening):
-- DROP TRIGGER IF EXISTS audit_events_immutable ON audit_events;
-- DROP FUNCTION IF EXISTS enforce_audit_immutability();

-- =============================================================================
-- COMMENTS
-- =============================================================================

COMMENT ON TRIGGER audit_events_immutable ON audit_events IS 
    'P6 Security: Prevents modification or deletion of audit events to maintain audit trail integrity';
COMMENT ON FUNCTION enforce_audit_immutability() IS 
    'P6: Trigger function that raises exception on any UPDATE or DELETE to audit_events';
