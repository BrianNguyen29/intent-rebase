-- Migration: 003_create_intent_clauses.sql
-- Description: Create intent_clauses table for Phase 1 intent registry
-- Created: Phase 1 first slice

CREATE TABLE IF NOT EXISTS intent_clauses (
    clause_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    intent_version_id UUID NOT NULL REFERENCES intent_versions(intent_version_id) ON DELETE CASCADE,
    clause_type VARCHAR(20) NOT NULL CHECK (clause_type IN ('functional', 'non_functional', 'policy', 'budget', 'time')),
    semantic_domain VARCHAR(50) NOT NULL,
    key VARCHAR(255) NOT NULL,
    operator VARCHAR(20) NOT NULL CHECK (operator IN ('eq', 'neq', 'lt', 'lte', 'gt', 'gte', 'contains', 'not_contains', 'regex', 'custom')),
    value JSONB NOT NULL,
    priority VARCHAR(10) NOT NULL CHECK (priority IN ('must', 'should', 'could')),
    rationale TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_intent_clauses_version_id ON intent_clauses(intent_version_id);
CREATE INDEX IF NOT EXISTS idx_intent_clauses_type ON intent_clauses(clause_type);
CREATE INDEX IF NOT EXISTS idx_intent_clauses_priority ON intent_clauses(priority);
CREATE INDEX IF NOT EXISTS idx_intent_clauses_key ON intent_clauses(key);

-- Composite index for clause lookups
CREATE INDEX IF NOT EXISTS idx_intent_clauses_version_type ON intent_clauses(intent_version_id, clause_type);

-- Post-migration validation notes:
-- 1. Verify table exists: SELECT COUNT(*) FROM intent_clauses;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename = 'intent_clauses';
-- 3. Verify foreign key: SELECT conname FROM pg_constraint WHERE conrelid = 'intent_clauses'::regclass AND contype = 'f';
-- 4. Verify CHECK constraints: SELECT conname FROM pg_constraint WHERE conrelid = 'intent_clauses'::regclass AND contype = 'c';

-- Rollback:
-- DROP TABLE IF EXISTS intent_clauses;
-- (CASCADE will drop dependent objects due to ON DELETE CASCADE on FK from intent_clauses)

-- Comments
COMMENT ON TABLE intent_clauses IS 'Phase 1: Stores fine-grained intent clauses for traceability';
COMMENT ON COLUMN intent_clauses.semantic_domain IS 'Domain this clause applies to (e.g., scope, constraint, acceptance)';
COMMENT ON COLUMN intent_clauses.key IS 'Identifier/key for the clause within its semantic domain';
