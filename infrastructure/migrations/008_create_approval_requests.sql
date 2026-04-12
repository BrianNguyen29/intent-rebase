-- Migration: 008_create_approval_requests.sql
-- Description: Create approval_requests table for Phase 2b bounded external apply slice
-- Created: Phase 2b PR - DB-backed pending approval queue stub for blocked D/E external rebase apply
-- Storage: Postgres with JSONB for flexible metadata storage
-- Scope: Bounded slice - only external POST /intents/{intent_id}/rebase-apply blocked D/E path

-- Rollback:
-- DROP INDEX IF EXISTS idx_approval_requests_intent_id ON approval_requests;
-- DROP INDEX IF EXISTS idx_approval_requests_tenant_id ON approval_requests;
-- DROP INDEX IF EXISTS idx_approval_requests_status ON approval_requests;
-- DROP INDEX IF EXISTS idx_approval_requests_workflow_id ON approval_requests;
-- DROP INDEX IF EXISTS idx_approval_requests_requestor_id ON approval_requests;
-- DROP TABLE IF EXISTS approval_requests;

-- Create approval_request_status enum
DO $$ BEGIN
    CREATE TYPE approval_request_status AS ENUM (
        'pending',
        'approved',
        'rejected',
        'expired',
        'cancelled'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Main approval_requests table
CREATE TABLE IF NOT EXISTS approval_requests (
    -- Primary key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Intent association (which rebase apply triggered this approval request)
    intent_id UUID NOT NULL,
    intent_version_from INT NOT NULL,
    intent_version_to INT NOT NULL,

    -- Workflow and tenant context
    workflow_id UUID NOT NULL,
    tenant_id UUID NOT NULL,

    -- Requestor (who triggered the external rebase apply - best effort from API context)
    requestor_id VARCHAR(255) NOT NULL DEFAULT 'external-api/unknown',
    requestor_type VARCHAR(50) NOT NULL DEFAULT 'external-api',

    -- Approval details
    decision_class VARCHAR(10) NOT NULL,  -- 'D' or 'E'
    reason TEXT NOT NULL,  -- human-readable reason for blocking
    metadata JSONB NOT NULL DEFAULT '{}',

    -- Status tracking (only pending status is in scope for Phase 2b bounded slice)
    status approval_request_status NOT NULL DEFAULT 'pending',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- NULL means never expires

    -- Approval resolution (populated when status changes from pending)
    resolved_at TIMESTAMPTZ,
    resolved_by VARCHAR(255),
    resolution_notes TEXT
);

-- Indexes for primary lookups
CREATE INDEX IF NOT EXISTS idx_approval_requests_intent_id ON approval_requests(intent_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_tenant_id ON approval_requests(tenant_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
CREATE INDEX IF NOT EXISTS idx_approval_requests_workflow_id ON approval_requests(workflow_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_requestor_id ON approval_requests(requestor_id);

-- Composite indexes for common filter combinations
CREATE INDEX IF NOT EXISTS idx_approval_requests_tenant_status ON approval_requests(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_approval_requests_intent_version ON approval_requests(intent_id, intent_version_from, intent_version_to);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM approval_requests;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'approval_requests';
-- 3. Verify enum exists: SELECT typname FROM pg_type WHERE typname = 'approval_request_status';
-- 4. Verify column types: SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'approval_requests';

-- Comments
COMMENT ON TABLE approval_requests IS 'Phase 2b: Bounded approval queue for blocked D/E external rebase-apply - only pending status stub';
COMMENT ON COLUMN approval_requests.id IS 'Unique identifier for this approval request';
COMMENT ON COLUMN approval_requests.intent_id IS 'Intent this approval request is associated with';
COMMENT ON COLUMN approval_requests.intent_version_from IS 'Source version of the rebase apply';
COMMENT ON COLUMN approval_requests.intent_version_to IS 'Target version of the rebase apply';
COMMENT ON COLUMN approval_requests.workflow_id IS 'Workflow this approval request belongs to';
COMMENT ON COLUMN approval_requests.tenant_id IS 'Tenant this approval request belongs to';
COMMENT ON COLUMN approval_requests.requestor_id IS 'Identifier of who triggered the external rebase apply (fallback: external-api/unknown)';
COMMENT ON COLUMN approval_requests.requestor_type IS 'Type of requestor (fallback: external-api)';
COMMENT ON COLUMN approval_requests.decision_class IS 'Decision class that triggered blocking (D or E)';
COMMENT ON COLUMN approval_requests.reason IS 'Human-readable reason for blocking';
COMMENT ON COLUMN approval_requests.metadata IS 'Additional metadata as JSONB for flexibility';
COMMENT ON COLUMN approval_requests.status IS 'Current status of the approval request';
COMMENT ON COLUMN approval_requests.created_at IS 'Timestamp when approval request was created';
COMMENT ON COLUMN approval_requests.updated_at IS 'Timestamp when approval request was last updated';
COMMENT ON COLUMN approval_requests.expires_at IS 'Timestamp when approval request expires (NULL = never expires)';
COMMENT ON COLUMN approval_requests.resolved_at IS 'Timestamp when approval was resolved';
COMMENT ON COLUMN approval_requests.resolved_by IS 'Who resolved the approval request';
COMMENT ON COLUMN approval_requests.resolution_notes IS 'Notes from the approval resolution';
