-- Migration: 006_create_audit_events.sql
-- Description: Create audit_events table for Phase 2 audit logging workstream
-- Created: Phase 2 PR - audit event data model
-- Storage: Postgres with JSONB for flexible payload storage

-- Rollback:
-- DROP INDEX IF EXISTS idx_audit_events_tenant_id ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_intent_id ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_artifact_id ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_event_type ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_occurred_at ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_tenant_event_type ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_tenant_intent ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_tenant_artifact ON audit_events;
-- DROP INDEX IF EXISTS idx_audit_events_actor_id ON audit_events;
-- DROP TABLE IF EXISTS audit_events;
-- DROP TYPE IF EXISTS audit_event_type;

-- Create audit_event_type enum
DO $$ BEGIN
    CREATE TYPE audit_event_type AS ENUM (
        'IntentCreated',
        'IntentUpdated',
        'IntentArchived',
        'RebaseDetected',
        'RebasePreviewGenerated',
        'RebaseApplied',
        'ApprovalRequired',
        'ApprovalGranted',
        'ApprovalRevoked',
        'ArtifactProduced',
        'ArtifactInvalidated'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main audit_events table
CREATE TABLE IF NOT EXISTS audit_events (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant isolation
    tenant_id UUID NOT NULL,

    -- Event classification
    event_type audit_event_type NOT NULL,

    -- Actor who triggered the event
    actor_id VARCHAR(255) NOT NULL,

    -- Optional intent association
    intent_id UUID,

    -- Optional artifact association
    artifact_id UUID,

    -- Flexible event payload as JSONB
    payload JSONB NOT NULL DEFAULT '{}',

    -- Distributed tracing context
    trace_id VARCHAR(64),
    span_id VARCHAR(32),

    -- When the event occurred (from source system)
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_id ON audit_events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_events_intent_id ON audit_events(intent_id) WHERE intent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_events_artifact_id ON audit_events(artifact_id) WHERE artifact_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_events_actor_id ON audit_events(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_occurred_at ON audit_events(occurred_at DESC);

-- Composite indexes for common filter combinations
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_event_type ON audit_events(tenant_id, event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_intent ON audit_events(tenant_id, intent_id) WHERE intent_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_artifact ON audit_events(tenant_id, artifact_id) WHERE artifact_id IS NOT NULL;

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM audit_events;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'audit_events';
-- 3. Verify enum exists: SELECT typname FROM pg_type WHERE typname = 'audit_event_type';
-- 4. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'audit_events';
-- 5. Verify JSONB payload: SELECT jsonb_keys(payload) FROM audit_events LIMIT 1;

-- Comments
COMMENT ON TABLE audit_events IS 'Phase 2: Audit events for immutable audit logging - records all significant domain events';
COMMENT ON COLUMN audit_events.id IS 'Unique identifier for this audit event';
COMMENT ON COLUMN audit_events.tenant_id IS 'Tenant this audit event belongs to';
COMMENT ON COLUMN audit_events.event_type IS 'Type of audit event (from AuditEventType enum)';
COMMENT ON COLUMN audit_events.actor_id IS 'Identifier of the actor who triggered this event';
COMMENT ON COLUMN audit_events.intent_id IS 'Associated intent ID if applicable';
COMMENT ON COLUMN audit_events.artifact_id IS 'Associated artifact ID if applicable';
COMMENT ON COLUMN audit_events.payload IS 'Flexible JSONB payload containing event-specific data';
COMMENT ON COLUMN audit_events.trace_id IS 'Distributed trace ID for correlation';
COMMENT ON COLUMN audit_events.span_id IS 'Distributed span ID for correlation';
COMMENT ON COLUMN audit_events.occurred_at IS 'Timestamp when the event occurred in the source system';
