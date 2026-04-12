-- Migration: 009_add_rebase_apply_blocked_audit_event.sql
-- Description: Add RebaseApplyBlocked to audit_event_type enum for Phase 2b bounded slice
-- Created: Phase 2b PR - add RebaseApplyBlocked to existing enum
-- Target: Extend audit_event_type enum without recreating

-- Rollback:
-- ALTER TABLE audit_events DROP CONSTRAINT IF EXISTS audit_events_event_type_check;
-- ALTER TABLE audit_events ADD CONSTRAINT audit_events_event_type_check 
--   CHECK (event_type IN ('IntentCreated', 'IntentUpdated', 'IntentArchived', 'RebaseDetected', 
--                         'RebasePreviewGenerated', 'RebaseApplied', 'ApprovalRequired', 
--                         'ApprovalGranted', 'ApprovalRevoked', 'ArtifactProduced', 'ArtifactInvalidated'));

-- Add RebaseApplyBlocked to existing enum using ALTER TYPE
-- This is safe because it only adds a new value, existing rows remain valid
ALTER TYPE audit_event_type ADD VALUE IF NOT EXISTS 'RebaseApplyBlocked';

-- Post-migration validation notes:
-- 1. Verify enum has new value: SELECT enumlabel FROM pg_enum WHERE enumtypid = 'audit_event_type'::regtype ORDER BY enumlabel;
-- 2. Verify constraint: SELECT conname FROM pg_constraint WHERE conrelid = 'audit_events'::regclass AND conkey = ARRAY[3];
-- 3. Verify no rows violated: SELECT event_type, COUNT(*) FROM audit_events GROUP BY event_type;

-- Comments
COMMENT ON TYPE audit_event_type IS 'Updated: Added RebaseApplyBlocked for Phase 2b bounded slice - emitted when external rebase-apply hits blocked D/E path';
