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
CREATE INDEX idx_intent_clauses_version_id ON intent_clauses(intent_version_id);
CREATE INDEX idx_intent_clauses_type ON intent_clauses(clause_type);
CREATE INDEX idx_intent_clauses_priority ON intent_clauses(priority);
CREATE INDEX idx_intent_clauses_key ON intent_clauses(key);

-- Composite index for clause lookups
CREATE INDEX idx_intent_clauses_version_type ON intent_clauses(intent_version_id, clause_type);

-- Comments
COMMENT ON TABLE intent_clauses IS 'Phase 1: Stores fine-grained intent clauses for traceability';
COMMENT ON COLUMN intent_clauses.semantic_domain IS 'Domain this clause applies to (e.g., scope, constraint, acceptance)';
COMMENT ON COLUMN intent_clauses.key IS 'Identifier/key for the clause within its semantic domain';
